# RT-1 批次报告：Native Rust Adapter 设计与最小实现

- 日期：2026-11-19
- 批次范围：设计（任务书 + ADR + DI-0 前提修订）→ S1 基线 → S2 Manifest 校验增强 → S3 Capability Provider → S4+S5+S6 Adapter/接线/测试 → S7 收口门 → S8 报告收口 → 评审修订（6 applied / 0 rejected）→ 门禁复测
- 依据：[RT-1-TASK.md](RT-1-TASK.md)（任务书 S1→S8、八项判据、八条红线）、[RT-1-ADR-NATIVE-ADAPTER.md](RT-1-ADR-NATIVE-ADAPTER.md)（D1-D8 设计裁决）、[RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md](RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md)（Provider 契约）、[DI-0-P34-PREMISE-REVISION-ADR.md](DI-0-P34-PREMISE-REVISION-ADR.md)
- 提交链：`9c76f6f9`（S2 校验增强）→ `7146d8f0`（S3 Capability Provider）→ `ff70f6aa`（S4+S5+S6 Adapter/接线/测试）→ `1571a227`（S8 报告与登记）；评审修订 F1-F6 落于工作树（8 文件，未提交，见 C 节）
- 复核结论：八项判据 8/8；八条红线逐条实测通过；评审 6 条 issues（2 medium + 4 low）修订 **6 applied / 0 rejected**；门禁在本报告会话（批次报告员，2026-11-19）独立复跑全绿（E 节命令原文）

## A. 完成情况

### A.1 步骤逐项完成（S1→S8 + 评审修订）

| 步骤 | 内容 | 落点/提交 | 状态 |
|---|---|---|---|
| S1 | 门禁基线核验：lock 399 / tool-engine 零 tokio / git status 净 / 定向过滤编译过 + clippy 零告警 | 无 commit（核验步） | 完成 |
| S2 | Manifest 校验增强：validate.rs native_rust 分支升级 + sk3 +3 测试 + v1.json additive 五可选字段 | `9c76f6f9`（3 文件） | 完成 |
| S3 | Capability Provider 最小接线：capability.rs（RT-0.5-F §2 全字段数据结构 + StaticCatalogProvider L0）+ 种子 16 条 + lib.rs 注册 | `7146d8f0`（4 文件） | 完成 |
| S4+S5+S6 | NativeRustAdapter 进程面 + execute_native_rust_via_engine 引擎接线 + fixture + e2e 15 测试 + capability_catalog 11 测试（合为一笔提交，与任务书 §8 两笔相比合并——同一测试面互为验证载体，回滚单元不变） | `ff70f6aa`（5 文件） | 完成 |
| S7 | 收口门：定向测试 + clippy + scoped fmt + workspace 回归 + lock/vendor/migrations/integrations/rtis 零触碰复核 | 无 commit（门禁步） | 完成 |
| S8 | 批次收口：本报告落库 + docs/README.md 登记 + 三份设计文档随批入库 | `1571a227`（5 文件） | 完成 |
| 评审 | 6 issues（2 medium + 4 low）→ F1-F6 全部 applied；定向门禁两轮内全绿 | 工作树 8 文件（C 节） | 完成 |

### A.2 八项判据对照（任务书判据 1-8；测试名在本报告会话 17/17 复跑中全部命中，见 E 节）

| # | 判据 | 测试（native_rust_e2e.rs） | 实测要点 |
|---|---|---|---|
| 1 | allowlist 正例 | `native_rust_allowlist_positive_run_succeeds_and_env_is_whitelist_exact` | 钉值命中（fixture sha256 `282af314…d73aa3`，本会话 sha256sum 复测一致）+ 白名单子命令 → SUCCEEDED；stdout 单行 JSON 帧解析为结构化结果；transcript [Q,P,RUNNING,SUCCEEDED] |
| 2 | allowlist 负例 | `native_rust_rejects_sha_mismatch` / `native_rust_rejects_missing_binary` / `native_rust_rejects_subcommand_not_in_whitelist` / `native_rust_rejects_manifest_with_secrets` / `native_rust_resource_enforcement_unavailable_rejects_dispatch` | sha 不符→POLICY_DENIED/NATIVE_IDENTITY_MISMATCH；缺失→DEPENDENCY/NATIVE_BINARY_MISSING；越白名单→POLICY_DENIED/NATIVE_SUBCOMMAND_NOT_ALLOWED；secrets 三形态矩阵（非空数组/对象/字符串，评审 F3 扩展）→POLICY_DENIED/NATIVE_SECRETS_UNSUPPORTED；transcript 全部 [Q,P,FAILED] 且工作区未创建（零子进程拉起） |
| 3 | 任意路径拒绝 | `native_rust_rejects_relative_binary` / `native_rust_rejects_input_path_escape` / `native_rust_source_has_no_shell_entry` | PATH 名解析派发面拒绝（NATIVE_IDENTITY_UNPINNED）；`../` 穿越与 `/etc/passwd` 绝对注入被 in/ 前缀校验拒绝；adapter 源码剥注释后 grep 断言 `sh -c`/`bash -c` 等 Shell 构造入口零命中 |
| 4 | env 泄漏断言 | 判据 1 测试内（fixture 回显自身环境） | 子进程 env 键 ∈ {LANG, PATH, TMPDIR, RIP_RT1_CUSTOM, PWD, SHLVL, _}（前三 + Manifest env 白名单，PWD/SHLVL/_ 为 bash 自设非继承）；宿主键（HOME/CARGO_*/SSH_AUTH_SOCK 等）零透传（env_clear 实证）；PATH=钉值二进制所在目录、TMPDIR=任务 tmp、LANG=C.UTF-8 逐值断言；保留键覆盖拒绝（评审 F4：`native_rust_rejects_reserved_env_key_conflict`） |
| 5 | 超时击杀 | `native_rust_timeout_terminates_group_and_kills_grandchild` | 预算 1s 到期 → killpg TERM（fixture `trap '' TERM` 忽略）→ 2s 窗口 → killpg KILL（耗时 ≥2.5s 实证窗口）→ TIMED_OUT/-32002；fixture 派生 `sleep 60` 孙进程 → /proc/<pid> 轮询至消失（进程组零残留，magick delegate gs/soffice.bin 类孙进程口径覆盖） |
| 6 | 取消 | `native_rust_cancel_partial_output_not_delivered` | cancel_requested_at 轮询命中（RUNNING 期）→ CANCELLED 终态信封（category=CANCELLED，code=-32004）；fixture 先产出完整结果帧再阻塞——信封 result=None（部分输出不伪装成功），stdout 留档 Artifact |
| 7 | 输出帽 | `native_rust_read_side_cap_terminates_group`（读侧）+ `native_rust_result_side_truncate_registers_artifact`（结果侧） | 读侧：200KB 帧 vs 4KB 帽 → 触顶 killpg + NATIVE_OUTPUT_CAP_EXCEEDED，帽内字节（4096+64KiB）留档且 Artifact sha256 对账；结果侧：100KB 结果 vs 65536 帽 → envelope::truncate 降级 + usage.truncated=true + 原始字节 Artifact 对账 |
| 8 | 统一信封 | 各路径测试内 `assert_unified_envelope`（成功/失败/超时/取消 + capability/wrapper/资源强制多路径） | 每路径 envelope::validate 过；transcript 与 tool_run 行终点一致；`environment.runtime_type="native_rust"` 写入 |

能力面判据（S3/S6 附带，capability_catalog.rs 11 测试本会话 11/11 复跑）：种子 16 条加载（15 available + 1 missing pdf.mutool）、probe(L0) 全量 sha256 实测与指纹公式复算一致、16 位前缀不升格不编造、身份不符标 stale 不报 missing、missing 复核五态、失效/重探/过滤面——11 测试名清单见 E.1 回显。

### A.3 评审与修订（6 applied / 0 rejected）

| # | 严重度 | 问题 | 修订 |
|---|---|---|---|
| 1 | medium | D8.4 资源强制探测与派发环境不对称：probe_exec 继承父进程全量 env、真派发 env_clear 最小集，会话宿主上 systemd-run 探测恒过/派发恒败（批次会话本机 systemd 262 两形态实测复现：继承 env exit=0；清环境 exit=1 "Failed to connect to user scope bus"），Auto 恒选 SystemdRun 后必败且被误归工具 EXECUTION，Auto 全链从未端到端跑通 | probe_exec 改为与真派发同口径探测（env_clear + PATH/TMPDIR/LANG 最小集，native_rust.rs:901-920）；真实 Auto 全链正例补测 `native_rust_auto_enforcement_real_chain_dispatches`（机制无关断言，本机落 prlimit 兜底路径）；ADR D8.4 + 本报告 B.3 裁决 3 同步；G 节遗留 7（wrapper 会话变量放行评估：批次会话实测 --machine Permission denied / --set-env systemd 262 不支持 / caller env 直传泄 XDG_RUNTIME_DIR+INVOCATION_ID 三形态均不可行，留续作） |
| 2 | medium | CapabilityId 段首放宽裁决未同步冻结契约面：v1.json pattern 仍要求段首字母，与 validate.rs/capability.rs 两 gate 矛盾（python3 re 实测 archive.7z → False）；sk3 段首数字形态零覆盖 | v1.json capability_ref pattern 同步为 `^[a-z0-9][a-z0-9_-]*(\.[a-z0-9][a-z0-9_-]*){1}$`（本会话读文件核实 :168 现值）；sk3 补 archive.7z 段首数字正例两处（:181-192 shapes_accept 形态 A + :322-333 negative_matrix） |
| 3 | low | Adapter secrets 深度防御门 fail-open：`as_array().unwrap_or(true)` 把非数组形状当空集放行 | 改显式 fail-closed（仅 None/空数组放行，native_rust.rs:801）；e2e secrets 负例扩展三形态矩阵 |
| 4 | low | Manifest runtime.env 可覆盖保留白名单键（PATH/TMPDIR/LANG）撞穿 D4 结构性保证；validate.rs 对 env 无校验；LD_PRELOAD/LD_LIBRARY_PATH 类键可经登记注入 | parse_allowlist 对保留键冲突 PREPARING 拒绝（NATIVE_ENV_RESERVED_CONFLICT，native_rust.rs:457-473）；validate.rs native_rust 分支补 env 校验（对象形状 + 值为字符串 + 动态链接器注入类键登记拒绝）；sk3 负例补两键；e2e 补 `native_rust_rejects_reserved_env_key_conflict`；ADR D4 补保留键语义 |
| 5 | low | capability_ref stale 处置（早拒）与 F §6:227 原文（refresh 后重判）偏差未登记，且 ADR:66 将自身偏差归属为契约要求；adapter 层 stale 拒绝路径零测试 | B.3 新增裁决 15（偏差登记）；RT-1-ADR D3 论据引文修正；e2e capability_gate 补第三腿 stale 负例（invalidate 后派发 → NATIVE_CAPABILITY_STALE） |
| 6 | low | Provider 契约形状偏差未全数登记（probe 取 `&ProbeBudget`、BinaryIdentity sha256 拆分 + WrapperTarget 独立类型、entry 字段 Option 化） | B.3 新增裁决 16/17（对照 capability.rs 实际形状核录），DI-1 消费以裁决为准 |

### A.4 本批明确不做（按任务书边界，非遗漏）

mcp-server/runtime-gateway 派发链接入（任务书 §1 明示现状零命中，消费属 DI-1）；两份存量 native_rust Manifest 钉值补录与绝对路径迁移（RT-4，任务书 §3.2 不修改面）；Capability Provider 完整探测/TTL/失效事件面（D2 分期，DI-1 前置批）；全仓 fmt 统一（独立卫生批，G 节遗留 4）；DI-1 工具 Manifest 编写与钉值补录。

## B. 架构与复用

### B.1 落位与范式对齐（对齐 python_worker/podman_host 既有范式面）

D1 裁决：adapter 挂 tool-engine 不拆新 crate（`crates/tool-engine/src/adapter/native_rust.rs`，1726 行），复用 AdapterOutcome/envelope::truncate/Engine 接线；否决独立 crate（零 lock 增量红线，Cargo.lock 399 恒定）。范式对齐逐点实测核对（本报告会话逐行读过下列行号）：

| 范式点 | 既有先例（本批前） | RT-1 落点 | 核对证据 |
|---|---|---|---|
| 引擎四态接线（QUEUED→PREPARING→RUNNING→终局统一信封） | `python_worker.rs:504` 起 `execute_via_engine`（同形态至 ~:790 DriveOutcome 返回）；`podman_host.rs:701` 起 `execute_container_via_engine`（至 ~:1023） | `native_rust.rs:1450` `execute_native_rust_via_engine` | 本会话读 :504-520（load_row + NotStartable 守卫 + transcript 初始化同构）与 :780-792（runtime_type 写入 + DriveOutcome 返回同构）、podman_host.rs:701-706/1016-1023 |
| 超时看门狗（try_wait 轮询 deadline） | `podman_host.rs:483-512` | `native_rust.rs` 看门狗（try_wait 轮询，:1182 等） | 本会话读 podman_host.rs:483-512 与 native_rust.rs 对应段 |
| stdout 单行 JSON 帧协议（D-TC-03） | `podman_host.rs:519-530`（首行 `{"status":"result","data":…}` 解析） | native_rust 读侧同款帧协议 | 本会话读 :519-530 |
| 协作取消（cancel_requested_at 轮询，D-TC-04） | `podman_host.rs:595-608`（可空列 Option mapper，NULL 即未请求） | `native_rust.rs:1054-1059` 同款查询 | 本会话读两侧源码 |
| 错误信封词表（九类） | `envelope.rs:14-24` `ERROR_CATEGORIES: [&str; 9]` | D7 五态映射全部落九类内（B.3 裁决 4） | 本会话读 :14-24 |
| 零 tokio 依赖 | tool-engine 全 crate std 面 | `std::process::Command` + 线程 + mpsc；`grep -n tokio crates/tool-engine/Cargo.toml` exit 1（本会话实测） | E 节门禁 |

进程组 kill（D5）为 RT-1 新增面：`process_group(0)`（native_rust.rs:1108）+ killpg 经固定路径 `/usr/bin/kill` 负 pgid 投递（:1023；std 无 killpg 且 libc/nix 为零依赖红线禁止项；kill_bin 是 Adapter 自有基础设施固定值 `NativeRustConfig`，非 Manifest/Agent 来源，与 podman_bin 同性质）。python_worker/podman_host 无此面（podman 隔离在容器口径、python_worker venv 无进程组击杀语义），属 native_rust 受控子进程档位的新增基础设施，非复制面。

### B.2 Capability 面与事实分层（承接 RT-0.5）

- `crates/tool-engine/src/capability.rs`（1546 行）：RT-0.5-F §2 全字段数据结构 + `CapabilityProvider` trait（get/list/fingerprint/probe/invalidate/refresh）+ `StaticCatalogProvider` L0 最小面（种子形状全量校验 + 受信前缀 stat + 全量 sha256 对照 + wrapper 双钉 + 身份不符标 stale 不报 missing + missing 条目受信根复核）+ 指纹纪律（probe 实测全量 sha256 后按 F §5.1 公式计算，16 位前缀不升格不编造）。
- 事实分层不变：Provider 记录环境事实（宿主有什么），Tool Registry 记录工具事实（哪些工具可调用）——`manifest.capability_ref` 单向引用，派发前置二道校验（available + fresh 才放行）；Provider 不注册工具、Registry 不探测环境。
- 种子 `manifests/capabilities/host-catalog-seed.json`（561 行）：RT-0.5-C §3 转录 16 条（本会话 python3 实测：entries=16，15 available + 1 missing；ffprobe 前缀 `a2ae3d0209aec739` 16 位口径；wrapper_target 嵌套于 binary_identity——libreoffice `baa182393db76be0`、archive.7z `d48e622e9009acdb`、7za/7zr 照录 null 不编造）。
- env 白名单（D4）为 RT-1 新结构性面：`env_clear()` 后注入显式最小集（native_rust.rs:1093-1102），保留键冲突拒绝（:457-473，评审 F4），Manifest env 仅可追加非保留键且 validate.rs 登记面校验。python_worker/podman_host 无 env_clear 白名单面（容器/venv 档位依赖运行环境语义），native 档位因直连宿主而新增。

### B.3 实现口径裁决记录（17 条，与 ADR/契约字面的偏差与调和；设计裁决零变更）

1. **killpg 经固定路径 `/usr/bin/kill` 负 pgid 投递**：std 无 killpg、libc/nix 为零依赖红线禁止项；kill 工具为 Adapter 自有基础设施固定值（`NativeRustConfig.kill_bin`），非 Manifest/Agent 来源。
2. **读侧帽 = max_output_bytes + 64 KiB 帧封装容差**：若读侧帽与结果帽相等，超帽帧必死在封装开销处、结果侧 envelope::truncate 不可达死代码；容差使双层语义都可达，真帽仍由 truncate 按 max_output_bytes 强制（判据 7 两测试分别覆盖两侧）。
3. **资源强制探测定案于 PREPARING + 探测与派发同环境口径**（评审 F1 修订）：Auto=systemd-run 探测 → prlimit 探测 → 双不可用 PREPARING 拒绝（INFRA retryable），拒绝发生在派发前置，RUNNING 期不回退裸跑；probe_exec 以与真派发一致的 D4 清环境最小集探测（机制可用性是平台事实而非 Manifest 属性，探测只可能误判保守方向）；`ResourceEnforcement::Off` 仅测试载具显式声明（生产默认 Auto）。
4. **派发面平台错误码超出 D7 五码字面**：category 严格九类内（envelope.rs:14-24 词表零越界），code 为平台侧字符串（podman_host "PREPARING_FAILED"/"CONTAINER_TRANSPORT_FAILED" 同先例）：NATIVE_IDENTITY_MISMATCH / NATIVE_SUBCOMMAND_NOT_ALLOWED / NATIVE_ARGV_TEMPLATE_DENIED / NATIVE_IDENTITY_UNPINNED / NATIVE_SECRETS_UNSUPPORTED / NATIVE_CAPABILITY_MISSING / NATIVE_CAPABILITY_STALE / NATIVE_CAPABILITY_UNAVAILABLE / NATIVE_WRAPPER_TARGET_UNRESOLVED / NATIVE_OUTPUT_CAP_EXCEEDED / NATIVE_ENV_RESERVED_CONFLICT（评审 F4 新增）；D7 五码原样承载五态主路径。
5. **CapabilityId 段首放宽为 `[a-z0-9]`**：F §2 注释「两段小写」与 RT-0.5-C 目录权威数据（archive.7z/7za/7zr 段首数字）冲突，以数据面为准；评审 F2 修订后 **三面同规则**（validate.rs capability_ref_ok / capability.rs CapabilityId::new / v1.json pattern），两段形状约束不变。
6. **种子 ffprobe 前缀按 16 位口径实测修正**：RT-0.5-C §3.2 原文 17 位（越出 16 位取证口径一字）；实测首 16 位 `a2ae3d0209aec739` 落种子（本会话读种子核实），C 目录文档不改写。
7. **7za/7zr 的 wrapper exec 目标前缀照录 null**（C §3.12/3.13 本批未取证）：不编造；probe(L0) 实测后填充全值（本会话读种子核实 wrapper.prefix=None）。
8. **`{{input_path}}` 暂存语义**：input.input_content 在场时由 Adapter 写入 in/<input_path>（「输入文件由引擎落 in/」最小实现）；slot 值经 in/ 前缀校验（canonicalize 消解符号链接 + 词法归一）；artifact mount_as 式富暂存属 DI-1 面。
9. **stdin 请求帧 `{"input", "budget"}`**：run_id 不透传工具；podman_host 的 request.json 含 run_id 属工作区排障文件差异，两协议面并行不冲突。
10. **probe_timestamp 以 ISO 字符串承载**（F §2 为 OffsetDateTime）：tool-engine 依赖面无 time 库（零 Cargo.toml 改动红线）。
11. **capability_ref 全量等值二道校验未挂 trait**：`measured_sha256`/`measured_target_sha256` 为 StaticCatalogProvider 固有方法；派发面以目录前缀 vs 钉值前缀对照承载，完整探测面留 DI-1。
12. **wrapper exec 目标解析 fail-closed**：仅识别 `exec <绝对路径>` 形态、要求唯一目标，无法唯一解析即拒绝派发（NATIVE_WRAPPER_TARGET_UNRESOLVED）。
13. **`cargo fmt --all -- --check` 为 HEAD 既有红**：批次会话实测 stash 后全仓 661 处漂移（rustfmt 1.81/1.99 双工具链同值）；本批 scoped fmt 处置（本会话复测：修订面 7 文件 clean，lib.rs:27 use 导入序为既有漂移——S3 diff 只加 `pub mod capability;` 未触 :27，git show 7146d8f0 佐证）。
14. **登记面 additive 兼容**：validate.rs native_rust 分支在「无任何钉值/白名单字段」时保持旧两字段语义（rp-plugin/rp-knowledge-fetch 名称形态登记不破，sk3 既有断言全绿）；任一钉值/白名单字段在场时 binary 强制绝对路径。
15. **capability_ref stale 早拒而非 refresh-后重判**（评审 F5 登记）：F §6:227 原文为 refresh 后重判，RT-1 实现为 stale → POLICY_DENIED/NATIVE_CAPABILITY_STALE 直接拒绝（fail-closed，F §5.3 stale 不可供决策同旨）；refresh 语义留 DI-1 完整探测面评估；ADR:66 引文已修正。
16. **probe/refresh 预算按引用传递**（评审 F6 登记）：F §3 冻结形状为值传递 ProbeBudget，实现签名取 `&ProbeBudget`（CancelToken Arc 载体，语义等价避免 Clone）；trait 为 RT-1 自有实现面，DI-1 消费以本条为准。
17. **BinaryIdentity 身份字段拆分与 WrapperTarget 独立类型**（评审 F6 登记）：F §2 原文 `sha256_hex` 单字段全量 + 嵌套 `Option<Box<BinaryIdentity>>`；实现为 `sha256_prefix`（16 位取证口径）+ `Option<sha256_full>`（probe 实测后填充）+ 独立 WrapperTarget 类型（无嵌套 wrapper——本机未观测 wrapper 的 wrapper 不建模）；CapabilityEntry executable_path/version/binary_identity Option 化承载 missing 条目。

## C. 文件清单（git status --porcelain 实测逐文件）

### C.1 工作树实测（本报告会话，`git status --porcelain` 原文）

```text
 M crates/tool-engine/src/adapter/native_rust.rs
 M crates/tool-engine/tests/native_rust_e2e.rs
 M crates/tool-registry/src/validate.rs
 M crates/tool-registry/tests/sk3_tool_manifests.rs
 M docs/README.md
 M docs/phase0/RT-1-ADR-NATIVE-ADAPTER.md
 M docs/phase0/RT-1-BATCH-REPORT.md
 M schemas/tool-manifest/v1.json
```

8 条 M = 评审修订 F1-F6 落点（未提交；`git diff --stat` 实测 8 files changed, 337 insertions(+), 49 deletions(-)）：代码/测试 4 文件（F1/F3/F4/F5 修订 + 2 新测试）、schema 1 文件（F2 pattern）、文档 3 文件（ADR 引文修正 F5 + 本报告 A-K 重构 + README 登记行更新）。分支 main 领先 origin/main 4 提交（`git branch -vv` 实测），CI 终态待 push 后 watch（E2 节）。

### C.2 提交面逐文件（`git show --name-status` 实测，4 commits 共 17 文件次）

| 提交 | 文件（name-status） | 说明 |
|---|---|---|
| `9c76f6f9` S2 | M crates/tool-registry/src/validate.rs；M crates/tool-registry/tests/sk3_tool_manifests.rs；M schemas/tool-manifest/v1.json | native_rust 分支校验增强 + 正负例矩阵 3 测试 + additive 五可选字段 |
| `7146d8f0` S3 | A crates/tool-engine/src/capability.rs；M crates/tool-engine/src/lib.rs；M crates/tool-registry/src/validate.rs；A manifests/capabilities/host-catalog-seed.json | Provider 数据结构 + L0 最小面 + 种子 16 条 + 段首放宽 |
| `ff70f6aa` S4+S5+S6 | M crates/tool-engine/src/adapter/mod.rs；A crates/tool-engine/src/adapter/native_rust.rs；A crates/tool-engine/tests/capability_catalog.rs；A crates/tool-engine/tests/fixtures/native_rust/env_echo.sh；A crates/tool-engine/tests/native_rust_e2e.rs | Adapter 进程面 + 引擎接线 + 15+11 测试 + 钉值 fixture |
| `1571a227` S8 | M docs/README.md；A docs/phase0/DI-0-P34-PREMISE-REVISION-ADR.md；A docs/phase0/RT-1-ADR-NATIVE-ADAPTER.md；A docs/phase0/RT-1-BATCH-REPORT.md；A docs/phase0/RT-1-TASK.md | 报告收口 + 登记行 + 三份设计文档入库 |

### C.3 逐文件说明（行数 `wc -l` 本会话实测）

| 文件 | 行数 | 说明 |
|---|---:|---|
| crates/tool-engine/src/adapter/native_rust.rs | 1726 | NativeRustAdapter 进程面 + execute_native_rust_via_engine 引擎接线（D3-D8 全落点） |
| crates/tool-engine/src/capability.rs | 1546 | RT-0.5-F §2 数据结构 + CapabilityProvider trait + StaticCatalogProvider L0 |
| crates/tool-engine/tests/native_rust_e2e.rs | 1154 | 17 测试（S6 15 + 评审 F1/F4 新增 2），判据 1-8 逐项 |
| crates/tool-engine/tests/capability_catalog.rs | 466 | 11 测试（种子/probe/指纹/缓存失效面） |
| crates/tool-registry/src/validate.rs | 1375 | native_rust 分支增强（钉值域/argv 卫生/capability_ref/env 校验） |
| crates/tool-registry/tests/sk3_tool_manifests.rs | 403 | 6 测试（+3 native_rust 正负例 + F2 段首数字正例两处） |
| crates/tool-engine/tests/fixtures/native_rust/env_echo.sh | — | fixture，sha256 `282af314ee118066…d73aa3`（本会话 sha256sum 复测一致，入库自持） |
| manifests/capabilities/host-catalog-seed.json | 561 | 16 条 RT-0.5-C §3 转录（15 available + 1 missing） |
| schemas/tool-manifest/v1.json | — | native_rust 分支 additive 五可选字段 + capability_ref 段首数字 pattern（F2） |
| docs/phase0/RT-1-TASK.md | 137 | 任务书（S1→S8、八项判据、八条红线） |
| docs/phase0/RT-1-ADR-NATIVE-ADAPTER.md | 145 | D1-D8 设计裁决（含 F5 引文修正） |
| docs/phase0/DI-0-P34-PREMISE-REVISION-ADR.md | 123 | DI-0 P3-4 前提修订 ADR |
| docs/phase0/RT-1-BATCH-REPORT.md | — | 本报告 |
| docs/README.md | — | RT-1 报告/任务书/ADR/DI-0 四行登记（:32-35） |

零触碰面（porcelain + `git status --short -- vendor/ Cargo.lock Cargo.toml migrations/ integrations/rtis/` 空输出，批次会话实测；本会话 porcelain 全清单对照）：Cargo.lock/vendor/Cargo.toml/migrations/integrations/rtis/web/生产库。

## D. 迁移与兼容

**本批零迁移。**

1. **数据库零迁移**：migrations/ 在 porcelain 与提交面均为零条目（SQLite 链头 / PG 链头未动，`git status --short -- migrations/` 空输出）。
2. **依赖面零迁移**：Cargo.lock 399 零增量（本会话实测）、vendor 375 零改动、tool-engine 零 tokio（exit 1 实测）——capability.rs/native_rust.rs 全部 std + 既有 workspace 内 crate。
3. **native_rust 语义升级（Manifest 词表 → 真实 Adapter）**：升级前 `runtime.type=native_rust` 仅是两份 Manifest 的词表登记（`manifests/tools/rip.knowledge.document.fetch.json` rp-knowledge-fetch/fetch、`rip.knowledge.source.sync.json` rp-plugin/sync，binary=名称、零钉值字段——本会话 python3 读 Manifest 实测），且 mcp-server/runtime-gateway 派发链零命中（任务书 §1），无任何真实执行面；升级后有真实 Adapter + 派发前置校验 + 统一信封。语义升级**不破坏登记面**：validate.rs additive 兼容（旧两字段形态原语义保留，B.3 裁决 14，sk3 既有断言全绿）；**执行面 fail-closed**：无钉值/名称形态派发面 PREPARING 拒绝（NATIVE_IDENTITY_UNPINNED）——与升级前「词表登记但不可执行」行为等价（都不可执行），升级后为显式拒绝而非静默无路径。
4. **Manifest 批量迁移留 RT-4**：两份存量 native_rust Manifest 的 binary 绝对路径 + 64hex 钉值补录属 RT-4 逐项迁移（任务书 §3.2 不修改面；补录形状模板 = v1.json additive 五可选字段 + sk3 正例）。迁移完成前这两份工具保持「登记在案、执行面显式拒绝」状态，如实登记不入幻觉面。
5. **schema additive**：v1.json native_rust 分支仅增五可选字段 + 旧形状条件必填，词表零新增值；capability_ref pattern 段首数字（F2）为既有裁决的契约面同步，非新词表。

## E. 测试真实结果（定向测试 + clippy 回显；本报告会话 2026-11-19 独立复跑，命令原文 + 回显原文）

### E.1 定向门禁（本会话实测）

```text
$ cargo test --offline -p tool-engine native_rust -- --test-threads=1
     Running tests/native_rust_e2e.rs (target/debug/deps/native_rust_e2e-ee6478fe9d32cb5d)
running 17 tests
test native_rust_allowlist_positive_run_succeeds_and_env_is_whitelist_exact ... ok
test native_rust_auto_enforcement_real_chain_dispatches ... ok          ← 评审 F1 新增
test native_rust_cancel_partial_output_not_delivered ... ok
test native_rust_capability_ref_gate_positive_and_missing ... ok        ← 含 F5 stale 第三腿
test native_rust_read_side_cap_terminates_group ... ok
test native_rust_rejects_input_path_escape ... ok
test native_rust_rejects_manifest_with_secrets ... ok                   ← F3 三形态矩阵
test native_rust_rejects_missing_binary ... ok
test native_rust_rejects_relative_binary ... ok
test native_rust_rejects_reserved_env_key_conflict ... ok               ← F4 新增
test native_rust_rejects_sha_mismatch ... ok
test native_rust_rejects_subcommand_not_in_whitelist ... ok
test native_rust_resource_enforcement_unavailable_rejects_dispatch ... ok
test native_rust_result_side_truncate_registers_artifact ... ok
test native_rust_source_has_no_shell_entry ... ok
test native_rust_timeout_terminates_group_and_kills_grandchild ... ok
test native_rust_wrapper_double_pin_dispatch ... ok
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.61s
（tool-engine 其余 17 套件全部编译通过并运行，native_rust 过滤外 0 命中，全绿）

$ cargo test --offline -p tool-engine --test capability_catalog -- --test-threads=1
running 11 tests
test invalidate_marks_stale_with_counts ... ok
test list_filter_domain_status_feature ... ok
test probe_above_l0_returns_not_probed ... ok
test probe_l0_identity_mismatch_marks_stale_not_missing ... ok
test probe_l0_measures_full_sha256_and_fingerprint ... ok
test probe_l0_missing_entry_rechecked_still_missing ... ok
test probe_l0_missing_file_reports_missing_five_state ... ok
test probe_l0_wrapper_double_pin_measured ... ok
test refresh_after_probe_returns_fresh_entry ... ok
test seed_loads_16_entries_15_available_1_missing ... ok
test seed_rejects_duplicate_ids_and_bad_shapes ... ok
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s

$ cargo test --offline -p tool-registry sk3 -- --test-threads=1
running 6 tests
test sk3_dual_manifest_forms_are_key_equal_and_static_valid ... ok
test sk3_four_tools_register_discover_query ... ok
test sk3_license_unknown_rejected_at_registration ... ok
test sk3_native_rust_allowlist_negative_matrix ... ok                   ← F2 archive.7z 正例两处
test sk3_native_rust_allowlist_registers_end_to_end ... ok
test sk3_native_rust_allowlist_shapes_accept ... ok
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.15s
```

### E.2 clippy（本会话实测，零告警）

```text
$ cargo clippy --offline -p tool-engine --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.13s   （零告警，链式 && 继续）
$ cargo clippy --offline -p tool-registry --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.09s   （零告警）
```

### E.3 workspace 回归门（本会话实测，串行）

```text
$ cargo test --offline --workspace -- --test-threads=1
（日志 /tmp/rt1-reporter-workspace.log；exit 0；grep 统计 146 个 'test result: ok' 套件行）
汇总（awk 实测）：146 套件 890 passed / 0 failed
```

S7 批次会话记录 888 passed（S6 15 测试基线）；评审修订后 890（+2：F1 auto_enforcement、F4 reserved_env）——本会话 890 复跑与修订门禁记录一致。

### E.4 红线与基线门（本会话实测）

```text
$ grep -c '^\[\[package\]\]' Cargo.lock
399                                          ← 零 lock 增量红线
$ grep -n tokio crates/tool-engine/Cargo.toml
（零命中，exit 1）                            ← 零 tokio 红线
$ ls vendor | wc -l
375                                          ← vendor 零改动
$ git status --porcelain
（8 条 M，全为 C.1 所列修订面；vendor/Cargo.lock/Cargo.toml/migrations/integrations/rtis 零条目）
$ rustfmt --edition 2021 --check <本批落点 8 文件>
（native_rust.rs/capability.rs/mod.rs/e2e/capability_catalog/validate.rs/sk3 全部 clean；
  仅 lib.rs:27 use 导入序既有漂移——S3 提交 diff 只加 pub mod capability; 未触 :27，git show 7146d8f0 佐证）
```

S1/S7 的批次会话实测（定向 15/15、workspace 888、scoped fmt 全清单）已由本会话 17/17、890、修订面复测覆盖；S1 时点 native_rust 过滤 0 tests 的基线态为批次会话记录，本会话未复跑（无意义——代码已在位）。

## E2. CI（E2）

**CI 终态回填（2026-11-19 修复批会话实测）。**

- 首次 push（收口提交 `ae21a0a8`，含批次 5 提交）：CI run [44363607817](https://github.com/emoeem/research-intelligence-platform/actions/runs/44363607817) 终态 **failure**（2026-11-18T17:57:21Z）。四 jobs 三绿一红：Web 工作台（构建与 vitest）/协议一致性载具（REST vs MCP 双通道）/Python worker 测试 success；Rust workspace（测试与 clippy 零警告）failure——`-p tool-engine --test capability_catalog` 8 passed 3 failed exit 101：
  - `probe_l0_measures_full_sha256_and_fingerprint`（capability_catalog.rs:169）：`Missing { searched_roots: ["/usr/bin/pdftotext"] }`——CI runner（ubuntu-latest）无 poppler；
  - `refresh_after_probe_returns_fresh_entry`（:463）：`Missing { searched_roots: ["/usr/bin/pdfinfo"] }`——同因；
  - `probe_l0_wrapper_double_pin_measured`（:208）：`ProbeFailed { detail: "sha256 prefix mismatch at /usr/bin/7z: expected f802383f13b8331a, measured 60fc00b4e1ed3766 …" }`——CI runner 的 7z 与 RT-0.5-A 本机实测钉值不同构建。
- 性质判定：三个测试将 RT-0.5-A 本机 Arch 实测钉值（宿主真实二进制路径 + sha256 前缀）硬编码为断言前提，跨环境不可移植；本机同代码复跑 capability_catalog 11 passed 0 failed（0.03s），CI 同代码 3 红——宿主绑定测试缺陷，非 Adapter/Provider 逻辑回归（native_rust_e2e 17/17、sk3 6/6、clippy 在 CI 亦绿，失败仅限 capability_catalog 三测）。
- **CI 修复批**：capability_catalog.rs 三测试前置 `host_pin_guard`（测试内 helper：`std::fs::read` + `sha256_hex` 前缀对照种子钉值——pdftotext f85872f94fb67433 / pdfinfo 3bdf68a79b9a3c22 / 7z 双钉 f802383f13b8331a + target d48e622e9009acdb），守卫不满足（二进制不在位或前缀不符）时 eprintln SKIP 并 return，与 selftest V-9 playwright SKIP 口径一致；不降级断言、不伪造绿面，守卫触发即环境不满足，宿主完整性由 probe 的 stale/missing 语义本身承载。本机守卫全过 11/11 绿 + clippy `-p tool-engine --all-targets -D warnings` 零告警 + rustfmt --edition 2021 --check clean；守卫分支负向验证（不存在路径 → SKIP；前缀不符 → SKIP）。
- 修复批 commit push 后 CI run 终态随本次 watch 回填（占位：**待 watch**）。
- 批次零依赖面变化（lock 399/vendor 375 零增量实测）→ 既有 CI 门禁集无新增需求，无需本批新增 CI 项。

## F. 性能

**本批无吞吐/延迟/内存基线，如实登记。** 开销面构成（定性 + 实测锚点）：

- **PREPARING 校验开销**：per-dispatch 全量 sha256 实测对照（O(bytes)，大二进制线性成本）+ 执行位 stat + capability_ref 二道校验（provider.get + fresh 判定）。种子 probe(L0) = stat + 全量 sha256 + 指纹复算，capability_catalog 11 测试 0.05s（本会话实测）为量级参考，非生产基线。
- **资源强制开销（Auto 档）**：PREPARING 一次 probe_exec 派发探测（清环境 systemd-run → 失败落 prlimit 探测）+ 真派发时 prlimit 包裹（本机实测落 prlimit 兜底路径）。systemd-run `--scope` 单元创建开销（RT-0.5-E 注 A 锚定的 per-dispatch 额外进程开销）**未计量**（未验证，与 RT-0.5 报告口径一致）；且现口径下 SystemdRun 首选机制不可达（G 节遗留 7），scope 开销暂不构成实际路径成本。
- **进程面开销**：`process_group(0)` + try_wait 轮询（cancel_poll 间隔）+ 轮询线程；超时击杀测试 5.61s 完成 17 测试（含 ≥2.5s TERM 窗口实证）为量级参考。
- **读侧帽**：流式读 + 64KiB 帧封装容差，触顶 killpg 即停（不读完全量）；结果侧 envelope::truncate 复用既有截断语义零新成本。
- **零 tokio**：std::process::Command + 线程 + mpsc，无异步运行时引入（exit 1 实测）。

未验证项如实：adapter 启动/派发延迟无基准数字、无吞吐基线、systemd-run scope 创建耗时未计量、Auto 档真实 systemd-run 全链在本机未跑通（落 prlimit）——均留 DI-1 生产派发计量。

## G. 遗留与风险（裁决与待办全录）

| # | 事项 | 归属 |
|---|---|---|
| 1 | L1/L2/L3 探测、TTL/失效事件完整面（F §5）、探测定时预算 | DI-1 前置批（RT-1 ADR D2 分期） |
| 2 | mcp-server/runtime-gateway 派发链接入 native_rust（现状零命中维持，DI-1 经 tool-engine 派发链验证） | DI-1 |
| 3 | 两份既有 native_rust Manifest（rp-plugin/rp-knowledge-fetch）钉值补录与绝对路径迁移 | RT-4 逐项迁移 |
| 4 | 全仓 fmt 漂移（批次会话实测 661 处，HEAD 既有，本会话未复跑计数）统一格式化 | 独立卫生批（不属 RT-1 落点） |
| 5 | capability_ref 全量等值二道校验挂 trait 面 | DI-1 完整探测面一并裁决 |
| 6 | `ResourceEnforcement::Off` 生产禁用的配置审查面（如派发层配置校验） | RT-3 Runtime Policy Resolver |
| 7 | **SystemdRun 档 wrapper 侧会话变量放行评估**（评审 F1 登记）：清环境口径下 `systemd-run --user --scope` 探测不可用 → Auto 确定性落 prlimit 兜底（fail-closed 且一致），D8.4 首选机制在现口径下不可达。批次会话实测三形态均不可行：`--machine=<user>@.host` → "Permission denied"；`--set-env` → systemd 262 不支持；caller env 直传 → XDG_RUNTIME_DIR/INVOCATION_ID 泄入工具进程 env 破 D4 精确集断言。续作需评估 wrapper 专用最小会话变量注入 + 判据 4 env 断言的 systemd-run 路径豁免 | DI-1 完整探测面批或 RT-3 |

风险面：Auto 档在具备用户会话的宿主上确定性落 prlimit 兜底——fail-closed 且探测/派发一致（F1 修正后），但 prlimit 限 **RLIMIT_CPU/RLIMIT_AS 单进程语义**，对 fork 出的孙进程保护弱于 cgroup 口径（systemd-run scope 为 cgroup 级）——孙进程击杀由判据 5 进程组 killpg 承载，资源帽语义差异如实登记，续作裁决见条目 7。

## H. 下一批依赖

**下游批次链（DI-0 ADR）：RT-1（本批）→ RT-2 Python Worker 项目级 venv 治理 → DI-1 Document Router 消费面 → RT-3 Runtime Policy Resolver → RT-4 逐项迁移评估（不得批量改 Manifest）。**

### H.1 RT-2 venv 治理消费面

- RT-1 确立的「runtime 档位逐个真实化」路径：native_rust 已有真实 Adapter + 派发前置校验 + 统一信封（判据 8）；RT-2 对 python_worker 档位做项目级 venv 治理（16 份 python_worker Manifest 面，RT-0.5-D §2 映射），复用同一套 Engine 接线/信封/取消/超时范式（python_worker.rs:504 既有接线为底座）。
- RT-1 的红线模式（零 lock 增量、零 tokio、fail-closed 派发前置、门禁命令原文入报告）直接作为 RT-2 的批次纪律模板。

### H.2 DI-1 Document Router 消费面（前置条件三条就绪 + DI-1 自备面）

- **NativeRustAdapter 就绪**：`execute_native_rust_via_engine(NativeRustCall { engine, run_id, adapter, manifest, capability })` 即 DI-1 PDF 工具端到端派发入口；D3 两形态 allowlist、D4 env 白名单（保留键冲突拒绝）、D5/D6 进程组与取消、D8 工作区三目录与输出帽全部实测（本报告 E/A.2）。
- **RT-0.5-F 契约消费面就绪**：`StaticCatalogProvider::load(种子)` + trait 六方法（L0）+ capability_ref 派发前置校验；DI-1 的 `pdf.pdftotext`/`pdf.pdfinfo` 引用直接命中种子条目（16 条实测口径）。消费时以 B.3 裁决 15/16/17 为实现口径准绳，不按 F 原文签名假设对接。
- **Manifest allowlist 登记形状就绪**：validate.rs native_rust 分支 + v1.json additive 字段 = DI-1 新工具登记模板。
- **DI-1 仍需自备**：工具 Manifest 编写与登记、钉值实测补录（pdftotext 全量 sha256 经 probe(L0) 实测回填）、`resources.cpu/memory` 取值核验；完整探测面（L1/L2/TTL）若 DI-1 需要，属条目 G1 前置批。

## I. 浏览器截图

**本批无 UI 面，无截图。** 批次产物全部为 Rust 代码/测试/Manifest/schema/文档；web/ 零改动（porcelain 全清单无 web/ 条目，本会话实测）；零浏览器操作、零页面交互。运行证据全部为命令行实测（E 节测试套件与门禁回显）；既有 web/qa/ 截图属此前批次产物，本批未触碰。

## J. RTIS 对账

**零接触。** integrations/rtis 在 `git status --porcelain` 全清单与 4 个提交的 name-status 中均零条目（本会话实测）；本批未调用任何 RTIS 工具面、未读写 RTIS 数据。错误码词表零越界：全部平台码 category 严格落 envelope.rs:14-24 九类内（B.3 裁决 4），未向 RTIS 词表或信封九类外发明任何值。

## K. 教训

1. **探测与派发必须同环境口径，"probe 通过"≠"派发可达"。** 评审 medium#1 同源：probe_exec 继承父进程全量 env 而真派发 env_clear——会话宿主上 systemd-run 探测恒过、派发恒败且被误归工具 EXECUTION。机制可用性是平台事实，探测环境应模拟生产派发环境（清环境探测只可能误判保守方向）；Auto 全链正例补测后该类缺陷才有测试面。
2. **改契约语义必须同步到所有冻结承载面。** CapabilityId 段首放宽改了代码两面但漏了 schema 面，两 gate 矛盾（validate.rs 放行 / v1.json 拒绝 archive.7z）到评审才暴露——改一个 id 规则时先 grep 它的全部承载面（代码、JSON Schema、文档注释、测试矩阵）。
3. **纵深防御门的默认值方向要显式想一遍。** `as_array().unwrap_or(true)` 把非数组 secrets 当空集放行（fail-open）——主闸在登记面时现实风险低，但兜底门的 fail-open 分支就是未来数据形状变化时的洞；安全门默认值必须 fail-closed（F3 修订）。
4. **std 无 killpg 的零依赖约束下，用平台稳定工具面替代 libc 绑定。** `/usr/bin/kill` 负 pgid 投递（固定路径 Adapter 自有配置）+ 判据 5 的 /proc 零残留测试，使进程组语义在不引 libc/nix 的前提下可用且有测试覆盖。
5. **两层防护的参数要有可达性推演。** 读侧帽与结果侧帽相等会让结果侧 truncate 变死代码——64KiB 帧封装容差使两层都可达（B.3 裁决 2）；写双层防护时问一句"第二层在什么输入下真的执行"。
6. **workspace 数字是滚动账目，钉值型结论以链头复跑为准。** S7 记录 888 passed，评审修订后 890——引用历史数字时必须带上它对应的代码时点，收口报告以最终链头复跑为准（本报告 E.3 即复跑值）。
7. **评审修订要落到"偏差登记"而不是只改行为。** F5/F6 两类 low issues 的本质是实现与契约原文的偏差无人记账——偏差登记条目（B.3 裁决 15/16/17）让 DI-1 消费者不按原文假设对接，行为修正与记账同步才闭环。

## 坏消息单列

1. **D8.4 首选机制不可达**：清环境口径下 `systemd-run --user --scope` 探测不可用，Auto 确定性落 prlimit 兜底（fail-closed 且一致，但首选机制在现口径下恒不触达；prlimit 单进程资源帽语义弱于 cgroup 口径，孙进程资源面靠 killpg 承载）——三形态放行评估均不可行（G 节遗留 7，留续作）。
2. **两份存量 native_rust Manifest 实际不可执行**：rp-plugin/rp-knowledge-fetch 为名称形态零钉值，执行面 fail-closed 下派发必拒（与升级前等价，但「登记在案」与「可执行」的差距显性化）——钉值补录属 RT-4，迁移前这两份工具不可用（G 节遗留 3）。
3. **全仓 fmt 漂移 661 处为 HEAD 既有红**（批次会话 stash 后实测，本会话未复跑计数）：本批 scoped fmt 处置，整仓统一格式化未做（不属本批落点，独立卫生批遗留）。
4. **性能面无数字**：无吞吐基线、无 adapter 派发延迟基准、systemd-run scope 创建耗时未计量、Auto 全链真实 systemd-run 路径本机未跑通——全部如实标"未验证"（F 节）。
5. **CI 首跑红面已回填并修复**：收口提交 `ae21a0a8` push 后 CI run 44363607817 failure——capability_catalog 三测试宿主绑定钉值在 CI runner（ubuntu-latest）不成立（无 poppler ×2、7z 异构构建 ×1）；修复批以 `host_pin_guard` SKIP 语义前置（E2 节），本机 11/11 绿，修复批 run 终态见 E2（随修复批 push watch）。宿主绑定测试的教训：跨环境测试面不得把单机实测快照当断言前提，钉值前提要么前置守卫要么下沉 fixture。
6. **评审 medium#1 初稿缺陷如实**：D8.4 探测/派发环境不对称使 Auto 全链从未端到端跑通（测试载具全部 Off 档）且缺陷未入初稿遗留清单——评审暴露后 F1 修正 + Auto 正例补测，属初稿质量问题如实登记。
