# ============================================================================
# RIP Rust Core（M2）多阶段构建
# 构建：podman build -t rp-core .
# 冒烟：podman run --rm -v $(pwd)/migrations:/db:ro rp-core
#   （默认 CMD 对 /db/platform-dryrun.db 跑一次 resolve 冒烟并输出 JSON）
#
# 基础镜像说明（本机网络实测）：
#   * docker.io/library/rust:1.99 直连与 daocloud/1ms 镜像源均在大层 blob
#     传输阶段停滞（0 KB/s，与 M0 审计发现的大文件直连掐断同因）。
#   * 故 builder 采用 "debian:bookworm-slim + rustup(rsproxy.cn) 自建工具链"：
#     rustup 走 rsproxy.cn（与 cargo 依赖同源），装 1.99.1 minimal + clippy +
#     rustfmt，效果等同 rust:1.99 slim 且网络可用。deb.debian.org 实测可达。
#   * 依赖全部 vendor 进镜像，构建期 cargo 全程 --offline，无 crates.io 依赖。
# ============================================================================
FROM docker.io/library/debian:bookworm-slim AS toolchain
ENV RUSTUP_DIST_SERVER=https://rsproxy.cn \
    RUSTUP_UPDATE_ROOT=https://rsproxy.cn/rustup \
    RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH
RUN apt-get update -qq \
 && apt-get install -y -qq --no-install-recommends \
      ca-certificates curl gcc libc6-dev \
 && rm -rf /var/lib/apt/lists/* \
 && curl -sSf https://rsproxy.cn/rustup-init.sh -o /tmp/rustup-init.sh \
 && sh /tmp/rustup-init.sh -y --profile minimal --default-toolchain 1.99.1 \
      -c clippy -c rustfmt --no-modify-path \
 && rm /tmp/rustup-init.sh \
 && cargo --version && rustc --version && cargo clippy --version && cargo fmt --version

FROM toolchain AS builder
WORKDIR /build
ENV CARGO_NET_OFFLINE=true

# 依赖清单与 vendored 源先行 COPY，利用层缓存
COPY Cargo.toml Cargo.lock ./
COPY .cargo/config.toml .cargo/config.toml
COPY vendor ./vendor

# 源码与迁移（容器内测试需要 migrations/001_init.sql、002_tasks.sql、dryrun 库）
COPY crates ./crates
COPY tests ./tests
COPY migrations ./migrations

# 质量门 + 构建（全部 --offline）
# 说明：rp-eri-parity 的 eri_parity 对照测试需要 python3 + eri catalog（容器外跑），
#   同包的 m2_integration 无 python 依赖，单独在容器内执行。
RUN cargo fmt --all -- --check \
 && cargo clippy --offline --workspace --all-targets -- -D warnings \
 && cargo test --offline --workspace --exclude rp-eri-parity \
 && cargo test --offline -p rp-eri-parity --test m2_integration \
 && cargo build --offline --release -p resource-engine --bin rp-resolve

FROM docker.io/library/debian:bookworm-slim AS runtime
COPY --from=builder /build/target/release/rp-resolve /usr/local/bin/rp-resolve
ENV RUST_BACKTRACE=1
ENTRYPOINT ["/usr/local/bin/rp-resolve"]
CMD ["--db", "/db/platform-dryrun.db", "--keywords", "fuzzing", "--limit", "5", "--json"]
