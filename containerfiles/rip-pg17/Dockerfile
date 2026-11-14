# ============================================================================
# rip-pg17：RIP3 情报核 PostgreSQL 17 镜像族（P1-1 批交付物，ADR-0001）
# ============================================================================
# 基座：paradedb/paradedb:pg17（pg_search BM25 + pgvector 预装，Debian 13 trixie
# + PostgreSQL 17.11）。基座 digest 钉死（受控联网拉取一次，产物镜像构建后离线
# 可重建：third_party/ 内置 pg_jieba 源码含 cppjieba 子模块，COPY 进构建层，
# 零构建期网络）。
# 本镜像族承载 RIP3 情报核全部数据面（ADR-0001 绞杀者路线）；迁移链
# migrations_pg/ 由 refinery 执行（crates/intel-db），扩展装载：
#   CREATE EXTENSION vector; pg_search 自带; CREATE EXTENSION pg_jieba;
# 构建命令（宿主机零安装，镜像属运行时数据，ADR §3.2 口径；构建期 apt/cmake
# 为受控联网一次性，产物 digest 钉死后离线可复跑）：
#   podman build -t localhost/rip-pg17:latest -f containerfiles/rip-pg17/Dockerfile containerfiles/rip-pg17/
# 约束落点：PG 服务以容器服务形态运行（podman run/quadlet，digest 钉死），
# 数据卷挂载宿主目录；本文件不设 USER 指令（基座同款，官方镜像自带处理）。
# 工具链：builder 层 apt 安装 build-essential/cmake/postgresql-server-dev-17
# （Debian 13 trixie PGDG 源，基座自带）；runtime 层零构建工具。
FROM docker.io/paradedb/paradedb:pg17@sha256:7cb29053a77c33f93ac79f8f9f44dfb75df54208fb803c3a4e45d55c8ebdd8c8 AS builder

RUN apt-get update \
  && apt-get install -y --no-install-recommends --allow-downgrades \
       build-essential cmake ca-certificates \
       libpq5=17.11-0+deb13u1 libpq-dev=17.11-0+deb13u1 \
       postgresql-server-dev-17=17.11-0+deb13u1 \
  && rm -rf /var/lib/apt/lists/*

COPY third_party/pg_jieba /tmp/pg_jieba
RUN cd /tmp/pg_jieba \
  && mkdir build && cd build \
  && cmake .. -DCMAKE_BUILD_TYPE=Release \
       -DPostgreSQL_TYPE_INCLUDE_DIR=/usr/include/postgresql/17/server \
  && make -j"$(nproc)" && make install

FROM docker.io/paradedb/paradedb:pg17@sha256:7cb29053a77c33f93ac79f8f9f44dfb75df54208fb803c3a4e45d55c8ebdd8c8

COPY --from=builder /usr/lib/postgresql/17/lib/pg_jieba.so /usr/lib/postgresql/17/lib/
COPY --from=builder /usr/share/postgresql/17/extension/pg_jieba* /usr/share/postgresql/17/extension/
COPY --from=builder /usr/share/postgresql/17/tsearch_data/jieba* /usr/share/postgresql/17/tsearch_data/
