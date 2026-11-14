# Rust 依赖许可证逐包清单（cargo-deny 口径，P10-4）

> 快照性质：结论反映 2026-11-14 批次工作树（未 commit）的 Cargo.lock 与 vendor/ 实装状态。
> 口径沿 telegram-bot `admin/DEPENDENCIES.md`（2026-08-22 基线）：逐包读实装值而非声明值，
> Rust 面实装值取 `vendor/<crate>/Cargo.toml` 的 `license` 字段（vendor 即构建实际使用的源），
> 合规裁定用 cargo-deny（EmbarkStudios 维护的成熟审计工具）0.20.2 实跑。
> 相关：deny.toml（合规配置）、docs/phase10/DEPENDENCIES-WEB.md（npm 面）、docs/phase10/P10-4-TASK.md。

## 一、方法与工具口径

1. 逐包实装值：Cargo.lock 解析出 182 个包（18 个 workspace 自有 crate + 164 个第三方包）；
   第三方逐包读 `vendor/` 内 Cargo.toml 的 `license` 字段，164/164 全部有显式 license 表达式
   （零 missing、零 vendor 缺失，脚本实测）。
2. 合规门禁：deny.toml（仓库根）+ `cargo deny check licenses`（exit 0，licenses ok）。
   工具经 `cargo install cargo-deny --locked` 装入用户目录 `~/.cargo/bin`（0.20.2），不属于
   workspace 依赖（Cargo.toml/Cargo.lock/vendor 零增量，--offline 构建口径不变）；该命令读
   Cargo.lock + vendor 实装源，离线可复跑。
3. fail-closed：第三方未声明许可（不匹配 allow 清单）即 deny；workspace 自有 crate 以
   `publish = false` + deny.toml `[licenses] private.ignore = true` 排除出第三方审计面
   （私有工程线不发布 crates.io，属元数据标注，不影响构建产物）。
4. 本仓自身许可状态：README.md 载明私有工程线，顶层无 LICENSE 文件（外部审阅现状项）。
   是否附开源许可证属维护者决策，本批不代决；第三方合规不受影响。

## 二、workspace 直接外部依赖（7 个）

| 依赖 | 版本要求 | License（vendor 实装值） | 用途 |
| --- | --- | --- | --- |
| rusqlite | 0.32 | MIT OR Apache-2.0 | SQLite（bundled + backup feature，P10-1 备份面） |
| serde | 1.0 | MIT OR Apache-2.0 | 序列化框架 |
| serde_json | 1.0 | MIT OR Apache-2.0 | JSON |
| toml | 1.1 | MIT OR Apache-2.0 | TOML（Manifest/配置） |
| serde_yaml_ng | 0.10 | MIT OR Apache-2.0 | YAML（serde_yaml 维护中 fork，P6-1 选型） |
| rmcp | =3.10.0 | Apache-2.0 | MCP 官方 Rust SDK（P9 rmcp 迁移） |
| tokio | 1 | MIT | rmcp 的 async 运行时面（最小 feature 子集） |

workspace 自有 18 个 crate 清单（publish = false，私有）：

| crate | 版本 | 说明 |
| --- | --- | --- |
| agent-engine | 0.2.0 | Agent 执行引擎（Phase 7） |
| audit-engine | 0.2.0 | 审计事件面 |
| context-broker | 0.2.0 | 上下文装配（Phase 4） |
| database | 0.2.0 | 数据库基础设施（连接池/迁移/备份/保留归档） |
| domain | 0.2.0 | 统一域模型（零依赖纯类型库） |
| mcp-server | 0.2.0 | MCP stdio 服务（Phase 9，rmcp SDK） |
| plugin-registry | 0.2.0 | 插件注册与信任级目录（Phase 10） |
| plugin-runtime | 0.2.0 | 插件生命周期运行时（Phase 6/10） |
| quality-engine | 0.2.0 | 知识治理评审（Phase 4/M6） |
| research-engine | 0.2.0 | 研究编排引擎 |
| resource-engine | 0.2.0 | 资源库与 Resolve（M0-M2） |
| rp-eri-parity | 0.2.0 |  |
| rp-mcp-parity | 0.2.0 | MCP 对账门禁（Phase 9） |
| runtime-gateway | 0.2.0 | Runner 网关与状态机（Phase 2/10） |
| search-engine | 0.2.0 | FTS 检索索引（M3） |
| tool-engine | 0.2.0 | 工具执行引擎（Phase 2/10） |
| tool-registry | 0.2.0 | 工具注册表与 Manifest 校验（Phase 1） |
| workflow-engine | 0.2.0 | Workflow 状态机（Phase 8） |

## 三、第三方逐包清单（164 个，vendor 实装值）

| 包 | 版本 | License（vendor 实装值） |
| --- | --- | --- |
| ahash | 0.8.12 | MIT OR Apache-2.0 |
| aho-corasick | 1.1.5 | Unlicense OR MIT |
| allocator-api2 | 0.2.21 | MIT OR Apache-2.0 |
| android_system_properties | 0.1.6 | MIT OR Apache-2.0 |
| autocfg | 1.5.1 | Apache-2.0 OR MIT |
| bit-set | 0.8.0 | Apache-2.0 OR MIT |
| bit-vec | 0.8.0 | Apache-2.0 OR MIT |
| bitflags | 2.13.2 | MIT OR Apache-2.0 |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 |
| block-buffer | 0.12.1 | MIT OR Apache-2.0 |
| borrow-or-share | 0.2.4 | MIT-0 |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 |
| bytecount | 0.6.9 | Apache-2.0/MIT |
| bytes | 1.12.1 | MIT |
| cc | 1.11.0 | MIT OR Apache-2.0 |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 |
| chrono | 0.4.45 | MIT OR Apache-2.0 |
| const-oid | 0.10.3 | Apache-2.0 OR MIT |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 |
| cpufeatures | 0.3.1 | MIT OR Apache-2.0 |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 |
| crypto-common | 0.2.2 | MIT OR Apache-2.0 |
| data-encoding | 2.11.1 | MIT |
| digest | 0.10.7 | MIT OR Apache-2.0 |
| digest | 0.11.3 | MIT OR Apache-2.0 |
| dyn-clone | 1.0.20 | MIT OR Apache-2.0 |
| email_address | 0.2.9 | MIT |
| equivalent | 1.0.3 | Apache-2.0 OR MIT |
| errno | 0.3.14 | MIT OR Apache-2.0 |
| fallible-iterator | 0.3.0 | MIT/Apache-2.0 |
| fallible-streaming-iterator | 0.1.9 | MIT/Apache-2.0 |
| fancy-regex | 0.19.2 | MIT |
| filetime | 0.2.29 | MIT/Apache-2.0 |
| find-msvc-tools | 0.1.15 | MIT OR Apache-2.0 |
| fluent-uri | 0.4.1 | MIT |
| foldhash | 0.2.0 | Zlib |
| fraction | 0.17.0 | MIT OR Apache-2.0 |
| futures | 0.3.34 | MIT OR Apache-2.0 |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 |
| futures-core | 0.3.34 | MIT OR Apache-2.0 |
| futures-executor | 0.3.34 | MIT OR Apache-2.0 |
| futures-io | 0.3.34 | MIT OR Apache-2.0 |
| futures-macro | 0.3.34 | MIT OR Apache-2.0 |
| futures-sink | 0.3.34 | MIT OR Apache-2.0 |
| futures-task | 0.3.34 | MIT OR Apache-2.0 |
| futures-util | 0.3.34 | MIT OR Apache-2.0 |
| generic-array | 0.14.7 | MIT |
| getrandom | 0.3.4 | MIT OR Apache-2.0 |
| getrandom | 0.4.3 | MIT OR Apache-2.0 |
| hashbrown | 0.14.5 | MIT OR Apache-2.0 |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 |
| hashlink | 0.9.1 | MIT OR Apache-2.0 |
| heck | 0.5.0 | MIT OR Apache-2.0 |
| hybrid-array | 0.4.15 | MIT OR Apache-2.0 |
| iana-time-zone | 0.1.65 | MIT OR Apache-2.0 |
| iana-time-zone-haiku | 0.1.2 | MIT OR Apache-2.0 |
| indexmap | 2.14.2 | Apache-2.0 OR MIT |
| itoa | 1.0.18 | MIT OR Apache-2.0 |
| js-sys | 0.3.107 | MIT OR Apache-2.0 |
| jsonschema | 0.56.0 | MIT |
| jsonschema-regex | 0.56.0 | MIT |
| jsonschema-value | 0.56.0 | MIT |
| libc | 0.2.191 | MIT OR Apache-2.0 |
| libsqlite3-sys | 0.30.1 | MIT |
| linux-raw-sys | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| lock_api | 0.4.14 | MIT OR Apache-2.0 |
| log | 0.4.35 | MIT OR Apache-2.0 |
| memchr | 2.8.3 | Unlicense OR MIT |
| micromap | 0.3.0 | MIT |
| mio | 1.2.5 | MIT |
| num | 0.4.3 | MIT OR Apache-2.0 |
| num-bigint | 0.4.8 | MIT OR Apache-2.0 |
| num-cmp | 0.1.0 | MIT/Apache-2.0 |
| num-complex | 0.4.6 | MIT OR Apache-2.0 |
| num-integer | 0.1.47 | MIT OR Apache-2.0 |
| num-iter | 0.1.46 | MIT OR Apache-2.0 |
| num-rational | 0.4.2 | MIT OR Apache-2.0 |
| num-traits | 0.2.19 | MIT OR Apache-2.0 |
| once_cell | 1.21.4 | MIT OR Apache-2.0 |
| outref | 0.5.2 | MIT |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 |
| pastey | 0.2.3 | MIT OR Apache-2.0 |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 |
| quote | 1.0.47 | MIT OR Apache-2.0 |
| r-efi | 5.3.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later |
| r-efi | 6.0.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later |
| redox_syscall | 0.5.18 | MIT |
| ref-cast | 1.0.27 | MIT OR Apache-2.0 |
| ref-cast-impl | 1.0.27 | MIT OR Apache-2.0 |
| referencing | 0.56.0 | MIT |
| regex | 1.13.1 | MIT OR Apache-2.0 |
| regex-automata | 0.4.18 | MIT OR Apache-2.0 |
| regex-syntax | 0.8.11 | MIT OR Apache-2.0 |
| rmcp | 3.10.0 | Apache-2.0 |
| rusqlite | 0.32.1 | MIT |
| rustix | 1.1.6 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| rustversion | 1.0.23 | MIT OR Apache-2.0 |
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 |
| schemars | 1.2.3 | MIT |
| schemars_derive | 1.2.3 | MIT |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 |
| serde | 1.0.229 | MIT OR Apache-2.0 |
| serde_core | 1.0.229 | MIT OR Apache-2.0 |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 |
| serde_derive_internals | 0.30.0 | MIT OR Apache-2.0 |
| serde_json | 1.0.154 | MIT OR Apache-2.0 |
| serde_spanned | 1.1.3 | MIT OR Apache-2.0 |
| serde_yaml_ng | 0.10.0 | MIT |
| sha2 | 0.10.9 | MIT OR Apache-2.0 |
| sha2 | 0.11.1 | MIT OR Apache-2.0 |
| shlex | 2.0.1 | MIT OR Apache-2.0 |
| slab | 0.4.12 | MIT |
| smallvec | 1.16.4 | MIT OR Apache-2.0 |
| socket2 | 0.6.5 | MIT OR Apache-2.0 |
| strum | 0.28.0 | MIT |
| strum_macros | 0.28.0 | MIT |
| syn | 2.0.119 | MIT OR Apache-2.0 |
| syn | 3.0.9 | MIT OR Apache-2.0 |
| tar | 0.4.46 | MIT OR Apache-2.0 |
| thiserror | 2.0.21 | MIT OR Apache-2.0 |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 |
| tokio | 1.54.1 | MIT |
| tokio-macros | 2.7.2 | MIT |
| tokio-util | 0.7.20 | MIT |
| toml | 1.2.1+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_datetime | 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_parser | 1.1.5+spec-1.1.0 | MIT OR Apache-2.0 |
| toml_writer | 1.2.0+spec-1.1.0 | MIT OR Apache-2.0 |
| tracing | 0.1.44 | MIT |
| tracing-attributes | 0.1.31 | MIT |
| tracing-core | 0.1.36 | MIT |
| typenum | 1.20.1 | MIT OR Apache-2.0 |
| unicode-general-category | 1.1.0 | Apache-2.0 |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| unsafe-libyaml | 0.2.11 | MIT |
| uuid | 1.29.2 | Apache-2.0 OR MIT |
| uuid-simd | 0.8.0 | MIT |
| vcpkg | 0.2.15 | MIT/Apache-2.0 |
| version_check | 0.9.5 | MIT/Apache-2.0 |
| vsimd | 0.8.0 | MIT |
| wasi | 0.11.1+wasi-snapshot-preview1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| wasip2 | 1.0.4+wasi-0.2.12 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| wasm-bindgen | 0.2.130 | MIT OR Apache-2.0 |
| wasm-bindgen-macro | 0.2.130 | MIT OR Apache-2.0 |
| wasm-bindgen-macro-support | 0.2.130 | MIT OR Apache-2.0 |
| wasm-bindgen-shared | 0.2.130 | MIT OR Apache-2.0 |
| windows-core | 0.62.2 | MIT OR Apache-2.0 |
| windows-implement | 0.60.2 | MIT OR Apache-2.0 |
| windows-interface | 0.59.3 | MIT OR Apache-2.0 |
| windows-link | 0.2.1 | MIT OR Apache-2.0 |
| windows-result | 0.4.1 | MIT OR Apache-2.0 |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 |
| winnow | 1.0.5 | MIT |
| wit-bindgen | 0.57.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| xattr | 1.7.0 | MIT OR Apache-2.0 |
| zerocopy | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT |
| zerocopy-derive | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT |
| zmij | 1.0.23 | MIT |

## 四、许可证直方图与结论

| license 表达式（vendor 实装值） | 包数 |
| --- | --- |
| MIT OR Apache-2.0 | 99 |
| MIT | 33 |
| Apache-2.0 OR MIT | 8 |
| MIT/Apache-2.0 | 6 |
| Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | 5 |
| MIT OR Apache-2.0 OR LGPL-2.1-or-later | 2 |
| BSD-2-Clause OR Apache-2.0 OR MIT | 2 |
| Apache-2.0 | 2 |
| MIT-0 | 1 |
| Apache-2.0/MIT | 1 |
| Apache-2.0 OR BSL-1.0 | 1 |
| (MIT OR Apache-2.0) AND Unicode-3.0 | 1 |
| Unlicense OR MIT | 2 |
| Zlib | 1 |

结论：

1. 全树 164 个第三方包均为许可型（permissive）许可证的 SPDX 表达式，无 GPL/AGPL 强著作权
   许可证；LGPL-2.1-or-later 仅作为 `r-efi@5.3.0 / 6.0.0` 三选一 OR 的可选项出现
   （MIT 与 Apache-2.0 同列，实际取 MIT/Apache-2.0 分支，cargo-deny 判定通过）。
2. 特殊表达式逐包核对（cargo-deny 判定通过的 OR/AND 组合）：LLVM-exception 组为
   linux-raw-sys/rustix/wasi/wasip2/wit-bindgen（Rust std 与 target 侧基础库）；
   `(MIT OR Apache-2.0) AND Unicode-3.0` 为 unicode-ident；MIT-0 为 borrow-or-share
   （MIT 无署名变体）；Zlib 为 foldhash；Unlicense OR MIT 为 aho-corasick/memchr；
   Apache-2.0 OR BSL-1.0 为 ryu。
3. deny.toml allow 清单与实装集合精确对齐（无未命中条目）：MIT、MIT-0、Apache-2.0、
   Unicode-3.0、Zlib 五类，confidence-threshold 0.93。
4. 维护约定：新增依赖先过 `cargo deny check licenses`（deny 即拒绝引入）；workspace
   依赖变更后 Cargo.lock/vendor 增量入库，本清单按批次快照更新。

## 五、验收命令与输出

~~~bash
$ /home/emo/.cargo/bin/cargo-deny check licenses
# licenses ok（exit 0，零警告；Cargo.lock 182 包，第三方 164 包全过 allow 清单）
$ cargo test --offline --workspace   # publish = false 元数据标注后全量门复验（见批次报告）
~~~

工具安装口径（用户级，零系统写入）：

~~~bash
cargo install cargo-deny --locked   # 装入 ~/.cargo/bin（cargo-deny 0.20.2），repo 外目录
~~~
