# RT-0：Runtime 原生优先混合执行基线审计

- 审计日期：2026-11-18
- 审计范围：Runtime Manifest、Adapter/dispatch、Podman 存储、Cargo 构建产物
- 审计方式：只读；未运行构建/测试；未清理缓存、镜像、容器或卷
- 项目路径：`/media/emo/code/pensting/research-platform`
- 审计时 HEAD：`4e7b4c693762`

## 0. 结论摘要

1. `target/debug/deps` 约 **69 GiB**，其中约 **60.78 GiB** 是无扩展名 ELF 可执行产物；抽样 ELF 带完整 `.debug_info` 且未 strip。它是本轮磁盘问题的首要对象，不是容器镜像或 `vendor/`。
2. 同一批集成测试目标存在 **10–17 组历史 fingerprint**（跨 2026-11-14 至 2026-11-18），并出现多个 hash 版本。结合 Cargo 不自动回收旧 fingerprint 对应产物，历史重复构建残留是重要嫌疑；但本轮没有把任何文件判定为可安全删除。
3. 当前 Cargo 仅配置 `[profile.release] strip = "debuginfo"`，没有显式 `[profile.dev]` / `[profile.test]`。下一步应在独立 target-dir 做 `debug=1` A/B，不应直接改主配置或 `cargo clean`。
4. 当前 `manifests/tools/*.json` 共 19 个：16 个 `python_worker`、2 个 `native_rust`、1 个 `podman_host`。Manifest 校验支持的 Runtime 类型多于实际 Adapter/dispatch 能力。
5. 当前真正具备通用 Adapter 的是 `python_worker` 和 `podman_host`。未发现通用 `native_rust` Adapter；`podman_vm` 没有独立 VM 生命周期/执行 Adapter；`mcp_client`、`http_api`、`cli_adapter` 目前主要停留在 Manifest 校验词表。
6. Podman rootless 存储报告约 18 GiB；Podman 统计为镜像 4.597 GB、容器可写层 11.51 GB、卷 568 MB。当前 4 个容器全部已退出。两个最大的容器是 Distrobox Ubuntu/Kali，不应误算成 RIP 工具运行容器。
7. 本轮没有触碰 WSW-1 文件、Cargo 配置、数据库、Podman 资源或 target 产物。

## 1. 工作树保护状态

审计时：
- HEAD：`4e7b4c693762`
- WSW-1 未提交项仍存在：`Cargo.toml`、`Cargo.lock`、`crates/wsw-message/`、`migrations/023_wsw_body_blob.sql`。
- 同时可见此前 Runtime 策略文档变更：`AGENTS.md`、`docs/README.md`、`docs/phase0/PHASE-ROADMAP.md`、`docs/phase0/NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md`。

以上均按既有工作保留。本轮没有修改或暂存任何已有文件。

## 2. Cargo 构建产物基线

### 2.1 目录占用

| 路径 | 实测 |
|---|---:|
| `target/` | 约 70 GiB |
| `target/debug/deps/` | 约 69 GiB |
| `target/debug/incremental/` | 约 106 MiB |
| `target/debug/build/` | 约 516 MiB |
| `target/debug/.fingerprint/` | 约 66 MiB |
| `vendor/` | 约 386 MiB |

磁盘：`/media/emo` 所在文件系统约 477 GiB，总可用约 185 GiB。

### 2.2 deps 文件类型（按文件大小求和）

| 类型 | 文件数 | 占用 |
|---|---:|---:|
| 无扩展名（可执行产物） | 922 | 60.78 GiB |
| `.rlib` | 1,150 | 5.06 GiB |
| `.rmeta` | 3,016 | 1.64 GiB |
| `.so` | 104 | 0.96 GiB |
| `.d` | 4,049 | 约 0.01 GiB |
| `.txt` | 1 | 可忽略 |

最大的一批无扩展名文件是测试/二进制目标，单个约 130–170 MiB，例如多个版本的 `rp_mcp`、`rp_workflow`、`run_execution`、`trace_correlation_p7_5`。抽样 ELF 文件显示 `with debug_info, not stripped`，并包含 `.debug_info`、`.debug_line` 等调试段。

### 2.3 历史构建残留迹象

- 多个 Workspace 测试目标有 10–17 个不同 fingerprint 目录，例如 `agent-engine`、`context-broker`、`tool-engine`、`plugin-runtime`、`workflow-engine`。
- 同一目标记录到两个 profile hash；同一 profile 下仍有多个 fingerprint/hash 版本。
- 这些 fingerprint 的时间覆盖 2026-11-14 至 2026-11-18。
- `libjsonschema` 约 76 MiB 的 rlib 也存在多个 hash 版本；`libtokenizers` 约 60 MiB 的 rlib 存在多个版本。

判断：**完整调试信息 + 多轮构建产生的历史 fingerprint/二进制残留**，是 69 GiB 的主要解释。Cargo 为不同依赖指纹/编译配置保留产物；不能只看文件名相似就删。需要在 RT-5 用 Cargo freshness 证据和隔离 target-dir 做验证，精确估算可回收空间。

### 2.4 当前 Cargo 配置

- `.cargo/config.toml` 仅将 crates.io source 指向仓库 `vendor/`。
- 根 `Cargo.toml` 当前只显式设置 `[profile.release] strip = "debuginfo"`。
- 未发现显式 `[profile.dev]`、`[profile.test]`、`target-dir` 或全局 `RUSTFLAGS` 配置。

RT-5 建议：在独立、稳定 SSD 路径做代表性 crate 的 `[profile.dev] debug = 1` / `[profile.test] debug = 1` A/B；记录磁盘、冷/热构建时间、测试时间、backtrace/诊断可用性。没有 A/B 结果前不改主 Cargo.toml，不缩减 CI，不执行 `cargo clean`，不把 target 放入 tmpfs。

## 3. Runtime Manifest 盘点

当前 `manifests/tools/*.json` 共 19 个（YAML 文件是配套表示，不重复计数）：

| Runtime | 数量 | Manifest |
|---|---:|---|
| `python_worker` | 16 | `rip.doc.parse`、`rip.evidence.rtk.auth`、`rip.evidence.rtk.ledger`、`rip.evidence.rtk.lint`、`rip.evidence.rtk.mcp`、`rip.evidence.rtk.scaffold`、`rip.kb.eri`、`rip.kb.evaluate`、`rip.kb.inventory`、`rip.kb.keyword_search`、`rip.kb.rtis`、`rip.knowledge.catalog.extract`、`rip.ops.etl_status`、`rip.ops.worker-runtime`、`rip.security.extension.audit`、`rip.text.embed` |
| `native_rust` | 2 | `rip.knowledge.document.fetch`、`rip.knowledge.source.sync` |
| `podman_host` | 1 | `rip.security.nuclei` |

补充：`containerfiles/rip-tool-webresearch/tool-manifest.json` 另有 `rip.http.inspect` 的 Podman 工具 Manifest 示例；它不在上述 `manifests/tools/` 19 个正式 JSON 计数中。Containerfiles 还包含 parser、embed、gov、rerank、tool-base、tool-source、tool-coreutils、nuclei、pg17 等构建定义。

## 4. Adapter 与实际派发能力矩阵

| Runtime / 模块 | 当前实现状态 | RT-0 判断 |
|---|---|---|
| `python_worker` | 有通用 Adapter；支持 pooled/per_run、initialize/get_capabilities 握手、execute/cancel、stdout 协议与 stderr 日志分流 | 可继续作为可信 Python Worker 的原生宿主进程基础 |
| `podman_host` | 有通用 rootless Podman Adapter；镜像 digest 复核、只读 rootfs、cap-drop、CPU/内存/PID 限额、tmpfs、任务工作区挂载、超时/取消/清理 | 保留为按风险隔离的工具后端 |
| `podman_vm` | Registry 接受该类型；MCP dispatch 将其与 `podman_host` 共用容器 Gateway 分支；未发现独立 VM 启动、VM 生命周期或专用 VM Adapter | 不能宣称 VM Runner 已落地；需在后续设计中明确真正的 VM 边界 |
| `native_rust` | Registry 只校验 `binary` / `subcommand`；有 `rp-knowledge-fetch` 等具体 Rust CLI 入口，但未发现通用 Native Rust Adapter 和 MCP 通用派发分支 | 当前属于“可登记/有具体二进制”，不是完整通用 Runtime |
| `mcp_client` | Registry 校验 endpoint/tool_name；未发现对应通用 Adapter/dispatch | 暂不启用 |
| `http_api` | Registry 校验 base_url/method/auth；未发现对应通用 Adapter/dispatch | 暂不启用 |
| `cli_adapter` | Registry 校验 argv_template；未发现独立通用 Adapter/dispatch | 暂不启用 |

代码证据位置：
- Adapter 模块目录：`crates/tool-engine/src/adapter/`，当前只有 `egress_proxy.rs`、`mod.rs`、`podman_host.rs`、`python_worker.rs`、`structured_output.rs`。
- Runtime 校验：`crates/tool-registry/src/validate.rs` 的 runtime 分支。
- MCP 派发：`crates/mcp-server/src/dispatch.rs`；当前真实分流是 `python_worker` 与 `podman_host | podman_vm` 两条路径。
- Python Worker 启动：`crates/tool-engine/src/adapter/python_worker.rs`；通过 `Command` 直接启动 argv，不经过 shell。

Python 环境缺口：Manifest 中的 command 通常是 `python3`，Adapter 会继承宿主环境并叠加 `extra_env`；目前未看到统一的项目级 venv Resolver、lockfile 哈希/解释器版本指纹或 Worker 启动环境审计。后续应优先让可信 Worker 显式使用项目 venv 的解释器路径，保持 Worker Protocol v1 不变。

其他实际调用点：`intel-db` 的 parser/embed/gov/search，以及 `verification-engine`、`workflow-engine` 中有直接复用 `PodmanHostAdapter` 的路径。它们应逐项判断是否确实需要隔离/重型依赖，不能仅按 Manifest 类型做批量迁移。WSW Proxy/MITM 属 RIP 平台自身可信服务，保持 Rust 原生服务定位。

## 5. Podman 资源基线

### 5.1 存储与总体用量

- Podman：6.2.0，rootless=true。
- GraphRoot：`/media/emo/containers/storage`；RunRoot：`/run/user/1000/containers`。
- `du -sh` 对 GraphRoot 的可见统计约 18 GiB；部分 overlay/volume 路径权限不足，因此该数值存在低估风险。
- `podman system df`：
  - Images：8 个，4.597 GB；其中 1.078 GB 被标记为可回收。
  - Containers：4 个，11.51 GB；当前 0 个运行中，Podman 标记 11.51 GB 可回收。
  - Local Volumes：13 个，568 MB；其中 410.7 MB 被标记为可回收。

“可回收”只是 Podman 的引用统计，不等于数据可丢弃。特别是数据库卷和恢复卷，未经归属核验禁止清理。

### 5.2 镜像清单（摘要）

- `localhost/ubuntu-zcode:26.04`：约 2.345 GB。
- `localhost/rip-pg17:latest`：约 1.13 GB。
- `docker.io/kalilinux/kali-rolling:latest`：约 125 MB。
- `docker.io/library/python`：约 124 MB。
- 另有若干无 tag 镜像（约 124 MB、228 MB、1.067 GB、1.067 GB）；需要通过 image inspect 与构建记录确认来源，不按 dangling 直接删除。

### 5.3 容器与卷归属

| 容器 | 状态 | 可写层大小 | 归属/备注 |
|---|---|---:|---|
| `ubuntu` | Exited | 7.22 GB | Distrobox Ubuntu/ZCode 开发环境 |
| `kali` | Exited | 4.29 GB | Distrobox Kali 开发环境 |
| `rip-pg17` | Exited | 约 11.5 KB | PostgreSQL；数据目录 bind mount 到项目 `rip-data/pg17` |
| `rip-p17-restore` | Exited | 约 44.5 KB | PostgreSQL 恢复容器；挂载一个约 157.3 MB 的 named volume |

Distrobox 的 inspect 显示其挂载包含宿主 `/home/emo`、`/`、`/dev`、`/sys`、`/tmp` 等宽范围路径。因此它们是开发环境容器，不是可用于执行不受信工具的安全沙箱。

13 个卷中有 5 个被容器引用、8 个显示 links=0；部分卷对应 Distrobox 的日志/pts 挂载，另有约 157.3 MB 的 PostgreSQL restore 卷。卷名为自动生成 hash，必须结合 inspect、容器标签和数据目录继续归因。**本轮未删除任何容器、镜像或卷，也未读取/修改 PostgreSQL 数据文件。**

## 6. RT-0 后续决策输入

1. **RT-1 Native Rust Adapter**：基于现有 Tool Engine/ToolRun/Artifact/Evidence/Trace 语义实现；必须有固定 binary allowlist、subcommand allowlist、参数 Schema、环境白名单、工作区约束、超时/取消、进程组回收和输出限制。不能开放任意 Shell。
2. **RT-2 Python Worker**：项目级 venv 与锁文件；Manifest/启动探测记录解释器路径、版本和依赖指纹；复用 Worker Protocol v1，不使用全局 pip。
3. **RT-3 Policy Resolver**：根据工具信任等级、依赖复杂度、文件/网络隔离需求选择 native/python/podman/VM；不允许高风险工具降级到原生执行。
4. **RT-4 Manifest 逐项迁移**：先评估 `intel-db` 与 `verification-engine` 等直接 Podman 调用，再处理正式工具 Manifest；不做一刀切。
5. **RT-5 Cargo A/B**：独立 target-dir 验证 `debug=1`，评估磁盘节省、编译时间和诊断能力；主工作树配置不在本批修改。
6. **RT-6 Podman 治理**：将 RIP 工具镜像、Distrobox、PostgreSQL、恢复卷、未知镜像分别归属；有明确数据保留策略后再讨论清理。
7. **RT-7 CI**：保持完整 Workspace 测试、Clippy 与 GitHub CI 质量门；容器仅用于确实需要的工具环境。

## 7. 本轮操作与验收记录

- [x] 核对 HEAD、工作树、WSW-1 文件状态。
- [x] 统计 Cargo target、deps 文件类型和代表性大文件。
- [x] 检查 Cargo profile 与 `.cargo/config.toml`。
- [x] 盘点 Runtime Manifest、Adapter 模块、Registry 校验与 MCP dispatch。
- [x] 盘点 Podman images/containers/volumes 和主要挂载归属。
- [x] 复核工作树：WSW-1 文件仍在，未被本轮改动。
- [x] 未执行 cargo build/test/clippy、cargo clean 或任何 target 删除。
- [x] 未执行 podman rm/rmi/prune、未删除卷、未触碰数据库。
- [x] 未 commit/push。
- [ ] 尚未进行 RT-1 至 RT-7 的实现工作；这些不属于 RT-0。

备注：审计时发现一个先前启动的只读磁盘扫描进程（PID 12372）仍存在，CPU 使用率为 0；本轮没有终止或干预该进程。
