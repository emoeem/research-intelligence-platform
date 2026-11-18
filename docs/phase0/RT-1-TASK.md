# RT-1 任务书：Native Rust Adapter 设计与最小实现

- 日期：2026-11-19
- 状态：任务书已定稿（RT-1 设计裁决员产出；实现批次开工即按本任务书执行）
- 依据：[NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md](NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md) §3.1/§4/§9 RT-1 行（223 行）；[RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md](RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md) §6/§7/§8；[RT-1-ADR-NATIVE-ADAPTER.md](RT-1-ADR-NATIVE-ADAPTER.md)（本批设计裁决，D1 到 D8）
- 前置：RT-0 基线已完成、RT-0.5 已收口（commit f4f3ebcf，2026-11-19 本机 `git status --short` 实测干净）
- 设计裁决：本任务书不重复论证，实现口径一律以 [RT-1-ADR-NATIVE-ADAPTER.md](RT-1-ADR-NATIVE-ADAPTER.md) D1 到 D8 为准

## 1. 目标

把 `native_rust` 从「仅 Manifest 可登记」补齐到受策略约束的真实 Adapter（NATIVE-FIRST §8.1:207 原文口径）：

1. `NativeRustAdapter`：受控原生子进程执行面，固定 binary/subcommand allowlist、环境白名单、工作区三目录、超时/取消/进程组回收、输出上限，统一 ToolRun 信封。
2. Capability Provider 最小接线：静态目录（[RT-0.5-C-CAPABILITY-CATALOG.md](RT-0.5-C-CAPABILITY-CATALOG.md) 16 条）作种子数据读取，数据结构与接口形状符合 [RT-0.5-F](RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md) §2/§3。
3. Manifest 校验增强：`tool-registry` validate.rs native_rust 分支从「两个非空字符串」升级为「绝对路径 + sha256 钉值 + 子命令白名单」形状（现状见 validate.rs:744-751，本任务书 §4 S2）。
4. 现状基线（2026-11-19 实测）：`mcp-server`/`runtime-gateway` 中 grep `native_rust` 零命中，现有两份 native_rust Manifest（rip.knowledge.source.sync.json:10-13、rip.knowledge.document.fetch.json）无任何 Adapter 消费路径；本批交付后该缺口由 allowlist 校验面补上。

不目标（本批不做）：完整探测面（L1/L2/L3）、缓存 TTL/失效事件实现（F §5 完整版留 DI-1 前置批）、工具 Manifest 逐项迁移（RT-4）、Runtime Policy Resolver（RT-3）、python_worker 原生环境治理（RT-2）。

## 2. 现状锚点（开工只读核验清单，全部为 2026-11-19 实测）

| 锚点 | 事实 | 出处 |
|---|---|---|
| native_rust 校验分支现状 | 只要求 `binary`/`subcommand` 两个非空字符串，无绝对路径/钉值/白名单校验 | crates/tool-registry/src/validate.rs:744-751 |
| 插件注册投影现状 | native_rust 投影只有 binary/subcommand 两字段 | crates/plugin-runtime/src/capability/tool_register.rs:265-269 |
| 现有 native_rust Manifest | binary=`rp-plugin`/`rp-knowledge-fetch`（名称形态，非绝对路径，无钉值） | manifests/tools/rip.knowledge.source.sync.json:9-13 |
| Adapter 消费现状 | mcp-server/runtime-gateway 零命中，无派发路径 | 本任务书写作会话 grep 实测 |
| 进程范式先例 | std::process::Command + 线程 + mpsc，零 tokio | crates/tool-engine/src/adapter/python_worker.rs:13-21、podman_host.rs:27-32 |
| 看门狗先例 | try_wait 轮询 deadline + TERM 优雅窗口 + KILL 兜底 | crates/tool-engine/src/adapter/podman_host.rs:483-512、622-632 |
| 取消先例 | 轮询 tool_run.cancel_requested_at（D-TC-04） | crates/tool-engine/src/adapter/podman_host.rs:595-608 |
| 截断先例 | envelope::truncate 全 runtime 共用 | crates/tool-engine/src/envelope.rs:296-327 |
| 错误码映射先例 | map_worker_code 码表到 §5.3 类别 | crates/tool-engine/src/adapter/mod.rs:90-102 |
| 中性终局先例 | AdapterOutcome 四态 | crates/tool-engine/src/adapter/mod.rs:32-64 |
| 帧协议先例 | stdin 一行 JSON 请求、stdout 一行 JSON 结果（D-TC-03） | crates/tool-engine/src/adapter/podman_host.rs:449-459、519-530 |
| 工作区先例 | `<root>/<run_id>/{in,out,tmp}` 三目录 | crates/tool-engine/src/adapter/podman_host.rs:279-303 |
| 资源强制锚 | systemd-run --user / prlimit 兜底；机制不可用即拒绝派发 | [RT-0.5-E](RT-0.5-E-RISK-AND-ISOLATION.md) §2 注 A（50 行） |
| 现有 rip.doc.parse | python_worker pooled，format 枚举 md/txt/html，输入 10 MiB/输出 1 MiB/60s | manifests/tools/rip.doc.parse.json:13-19、46-58、118-123 |

## 3. 范围

### 3.1 允许改动（新增/修改落点）

| 落点 | 内容 | 步骤 |
|---|---|---|
| `crates/tool-engine/src/adapter/native_rust.rs`（新增） | NativeRustAdapter + NativeRustConfig + allowlist 解析 + 进程面 | S4 |
| `crates/tool-engine/src/adapter/mod.rs` | 注册 `pub mod native_rust` + pub use | S4 |
| `crates/tool-engine/src/capability.rs`（新增） | RT-0.5-F §2 数据结构 + StaticCatalogProvider 最小面 | S3 |
| `crates/tool-engine/src/lib.rs` | `pub mod capability` | S3 |
| `crates/tool-registry/src/validate.rs` | native_rust 分支增强（绝对路径/钉值/白名单/argv 模板） | S2 |
| `crates/tool-registry/tests/sk3_tool_manifests.rs` | native_rust 分支正负例断言扩展 | S2 |
| `schemas/tool-manifest/v1.json` | native_rust 分支 additive 可选字段（binary_sha256/subcommands/argv_template）；词表零新增值 | S2 |
| `manifests/capabilities/host-catalog-seed.json`（新增） | RT-0.5-C 16 条转录种子（全量 sha256 仅登记实测值，16 位前缀照录不补造） | S3 |
| `crates/tool-engine/tests/native_rust_e2e.rs`（新增） | §5 八项判据测试载具 | S6 |
| `crates/tool-engine/tests/capability_catalog.rs`（新增） | 种子加载/五态/指纹边界测试 | S6 |
| `docs/phase0/` 三份（本批已落）+ `docs/README.md` 登记行 | 任务书 + ADR + DI-0 ADR | 已完成 |

### 3.2 不修改

- 现有 19+1 份工具 Manifest 语义字段（含两份 native_rust：钉值补录属 RT-4 逐项迁移，本批执行面强制、登记面兼容，见 ADR D3）。
- `python_worker.rs`/`podman_host.rs`/`egress_proxy.rs`/`structured_output.rs` 既有行为（回归由 §6 测试面守护）。
- `migrations/`、`Cargo.toml` 依赖面、`Cargo.lock`（399 条目）、`vendor/`。
- 生产 `platform*.db`、`platform-dryrun.db`、`integrations/rtis/`。
- `schemas/packs/planned/*` 存根（DI-0 ADR §4 只改登记文档，存根修订随 DI-1）。

## 4. 实施步骤（S1 → S8，串行，每步独立可验证）

- **S1 门禁基线核验**：开工复跑 `grep -c '^\[\[package\]\]' Cargo.lock`（期望 399）、`grep -n tokio crates/tool-engine/Cargo.toml`（期望零命中）、`git status --short`（期望干净）；复跑 §6 两条测试面命令确认基线绿（本任务书写作时点 2026-11-19 已实测：定向过滤 0 tests 全套件编译通过、clippy 零告警，输出见 §6 基线记录）。
- **S2 Manifest 校验增强**：validate.rs native_rust 分支（744-751 行）升级：`binary` 必须绝对路径（`/` 前缀）；新增可选 `binary_sha256`（64hex 钉值）、`subcommands`（非空字符串数组白名单）、`argv_template`（字符串数组，与 cli_adapter 同形状 validate.rs:784-790 先例）；`subcommand` 单值字段保留兼容。负例（相对路径/坏 hash/越界字段）与正例断言进 sk3_tool_manifests.rs；schema additive 字段同步。
- **S3 Capability Provider 最小接线**：`crates/tool-engine/src/capability.rs` 按 RT-0.5-F §2（18-93 行）落 CapabilityId/CapabilityStatus/ProbeErrorKind/ProbeError/BinaryIdentity/PackageProvenance/ProbedFeature/Constraint/SecurityProfile/CapabilityEntry/CachedEntry 数据结构；StaticCatalogProvider 实现 `get/list/fingerprint/probe(L0)` 最小面（ADR D2）：种子加载 + 受信前缀路径存在性 + sha256 对照；完整探测/缓存/失效留续作。种子 `manifests/capabilities/host-catalog-seed.json` 从 RT-0.5-C §3 十六条转录，不编造指纹值（C §5.1:269 口径）。
- **S4 Adapter 进程面**：`crates/tool-engine/src/adapter/native_rust.rs`：NativeRustConfig（workspace_root/默认白名单 env/取消轮询间隔/kill 窗口）；allowlist 解析（binary 形态 = 绝对路径 + binary_sha256；subcommand 形态 = 绝对路径 + subcommands 白名单 + argv_template）；`Command::env_clear()` + 显式白名单（ADR D4）；`process_group(0)` + try_wait 轮询 + killpg TERM→KILL（ADR D5）；stdout 逐块帽计数 + stderr 尾部采集（ADR D8）。mod.rs 注册。
- **S5 引擎驱动接线**：`execute_native_rust_via_engine`（python_worker.rs:504-790 / podman_host.rs:701-1023 同形态）：QUEUED→PREPARING（allowlist/钉值/能力校验，失败按 ADR D7 五态映射落 FAILED）→RUNNING（子进程 + 看门狗 + 取消轮询）→终局（output_schema 校验 + envelope::truncate 复用 + build_envelope，runtime_type="native_rust"）。四路径（成功/失败/超时/取消）信封统一。
- **S6 测试面落地**：`tests/native_rust_e2e.rs` + `tests/capability_catalog.rs` 按 §5 八项判据逐项落测试；fixture 工具用测试内编译的极小 Rust bin 或 shell 脚本固定 sha256（fixture 与 sha256 一并入库，测试自持）。
- **S7 收口门**：§6 两条测试面命令 + `cargo fmt --all -- --check` + `cargo test --offline --workspace`（跨 crate 回归）+ lock/vendor 零增量复核 + `git status` 复核（只含 §3.1 落点）。
- **S8 批次收口**：RT-1 批次验收报告落 `docs/phase0/`（命令原文 + 输出摘要 + 八项判据对照表）；docs/README.md 索引补 RT-1 批次报告行；DI-1 前置条件登记（消费本批 adapter + RT-0.5-F 契约，见 [DI-0-P34-PREMISE-REVISION-ADR.md](DI-0-P34-PREMISE-REVISION-ADR.md) §5）。

## 5. 完成标准（八项判据，S6 逐项有测试、S8 逐项有实测输出）

| # | 判据 | 测试要求（负例为硬性红线） |
|---|---|---|
| 1 | allowlist 正例 | 钉值 sha256 命中 + 白名单子命令命中 → run SUCCEEDED，stdout 帧解析为结构化结果 |
| 2 | allowlist 负例 | sha256 与钉值不符 / binary 文件缺失 / 子命令不在白名单 → PREPARING 拒绝，错误信封按 ADR D7 落 DEPENDENCY/POLICY_DENIED，零子进程拉起 |
| 3 | 任意路径拒绝 | Manifest binary 为相对路径 / PATH 名解析 / argv 模板注入 `../` 或 shell 元字符 → 登记校验或派发面拒绝；`sh -c`/`bash -c` 形态在代码层无构造入口（grep 断言） |
| 4 | env 泄漏断言 | fixture 工具回显自身环境 → 断言子进程环境 = 显式白名单集合；SSH Agent / HTTP_PROXY(宿主值) / 数据库路径 / 浏览器 Profile 变量零继承（`env_clear` 效果实证） |
| 5 | 超时击杀 | 预算到期 → 进程组 TERM → 2s 窗口 → killpg KILL；fixture 派生孙进程，断言 run TIMED_OUT 且进程组零残留（kill 后 wait 重aping） |
| 6 | 取消 | cancel_requested_at 轮询命中 → 协作取消窗口 → CANCELLED 终态信封（-32004 语义），部分输出不伪装成功 |
| 7 | 输出帽 | 结构化结果超 max_output_bytes → envelope::truncate 降级 + Artifact 登记 + usage.truncated=true；流式读帽触顶 → 进程组终止 + 按截断/失败语义交付，不整读 OOM |
| 8 | 统一信封 | 四路径信封全部过 envelope::validate；状态跃迁 transcript 与 tool_run 行一致；runtime_type="native_rust" 写入 environment |

能力面（S3/S6 附带判据）：种子 16 条加载成功（15 available + 1 missing pdf.mutool，RT-0.5-C §4:249-250 实测口径）；probe(L0) 对种子路径 stat/sha256 校验；指纹仅在实测全量 sha256 后计算，16 位前缀不升格为指纹（C §5.1:269「不预生成、不编造」）。

## 6. 测试面（收口命令，与 CI 同门）

```bash
# 定向测试（native_rust + capability 全量，单线程防端口/工作区竞争）
cargo test --manifest-path /media/emo/code/pensting/research-platform/Cargo.toml --offline -p tool-engine native_rust -- --test-threads=1

# 静态检查门
cargo clippy -p tool-engine --all-targets -- -D warnings

# 格式门
cargo fmt --all -- --check

# 跨 crate 回归（S7）
cargo test --offline --workspace
```

基线记录（本任务书写作时点 2026-11-19 实测）：

- `cargo test --manifest-path /media/emo/code/pensting/research-platform/Cargo.toml --offline -p tool-engine native_rust -- --test-threads=1`：全部套件编译通过，`native_rust` 过滤命中 0 tests（现状无该名测试，S6 落地后由此命令承载），退出码 0。
- `cargo clippy -p tool-engine --all-targets -- -D warnings`：零告警，退出码 0（1.50s）。
- `grep -c '^\[\[package\]\]' Cargo.lock`：399。
- `grep -n tokio crates/tool-engine/Cargo.toml`：零命中（依赖面仅 database/domain/tool-registry/serde/serde_json/serde_yaml_ng）。

## 7. 红线（八条，违反任一即停批上报）

1. **零 vendor 增量**：`vendor/` 目录零改动，Cargo.lock 条目数恒 399（S1/S7 双点复核）；禁新增任何依赖（含 dev-dependencies）。
2. **tool-engine 零 tokio**：现状依赖面不改；进程模型一律 `std::process::Command` + 线程 + mpsc（python_worker.rs/podman_host.rs 同范式）；禁引入 async runtime 或 tokio feature 面。
3. **禁任意 Shell**：可执行路径只来自 Manifest 固定绝对值或 Capability 目录校验（RT-0.5-F §6:235「entry.executable_path 绝对派发」）；禁 `sh -c`/`bash -c`/shell 字符串拼接；argv 只由 Manifest argv_template + input_schema 校验后参数构成；Agent/UI 无任何路径拼进 argv 的入口（NATIVE-FIRST §4.2:127）。
4. **生产库零触碰**：`platform*.db`/`platform-dryrun.db` 全程不写；测试用内存库/临时库；开发库副本操作同 ROADMAP §1.4 纪律。
5. **RTIS 零接触**：`integrations/rtis/` 零 diff。
6. **不建第二事实源**：ToolRun/Artifact/Evidence/Trace/预算/取消语义全部复用引擎既有面（NATIVE-FIRST §4.1:118-122）；错误信封只用 §5.3 九类（envelope.rs:14-24），五态映射不得发明第九类外词表值。
7. **既有 runtime 零回归**：python_worker/podman_host 行为不变；`cargo test --offline --workspace` 全绿为回归门。
8. **宿主软件零安装**：缺失能力只报告不安装（RT-0.5-C §3.9:147、NATIVE-FIRST §12:262）；本批不自动升级/卸载任何系统软件，probe 失败不触发任何写操作。

## 8. 回滚

- 每步独立 commit（S2/S3/S4+S5/S6/S8 各一）；`git revert <批次 commit>` 整体撤销；validate.rs 增强回滚后旧两份 Manifest 语义不受影响（增强为 additive）。
- 测试载具与 fixture 随代码 commit 回滚，无外部状态；种子 JSON 为只读数据面，回滚零残留。
- 生产库/RTIS/Podman 对象/宿主软件零操作，无生产回滚面。

## 9. 交付物

1. 代码：`native_rust.rs` + `capability.rs` + validate.rs 增强 + 引擎接线（S2-S5）。
2. 测试：八项判据测试 + 能力面测试（S6），收口命令输出全绿（S7）。
3. 数据：`manifests/capabilities/host-catalog-seed.json`（16 条转录）。
4. 文档：本任务书、[RT-1-ADR-NATIVE-ADAPTER.md](RT-1-ADR-NATIVE-ADAPTER.md)、[DI-0-P34-PREMISE-REVISION-ADR.md](DI-0-P34-PREMISE-REVISION-ADR.md)（本批已落）+ RT-1 批次验收报告（S8）+ docs/README.md 登记。
5. 前置输出：DI-1 可消费的 NativeRustAdapter + RT-0.5-F 形状 Provider（DI-0 ADR §5 依赖链兑现）。
