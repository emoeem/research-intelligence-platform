# RT-2 批次报告：Python Worker 原生环境治理（项目级 venv / lock / 启动探测 / 版本哈希记录 / 协议一致性）

- 日期：2026-11-21
- 批次范围：设计（任务书 + ADR）→ S1 基线 → S2 lock 落地 → S3 探测面 → S4 adapter 接线 → S5 登记面 → S6 测试面 → S7 收口门 → S8 报告收口 → 评审（2 medium + 5 low）→ 修订 6 项全 applied → 门禁复测
- 依据：[RT-2-TASK.md](RT-2-TASK.md)（S1→S8、八项判据、八条红线）、[RT-2-ADR-VENV-GOVERNANCE.md](RT-2-ADR-VENV-GOVERNANCE.md)（D1-D8）、[RT-0.5-C-CAPABILITY-CATALOG.md](RT-0.5-C-CAPABILITY-CATALOG.md) §5.1/§4（指纹公式/五态）、[NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md](NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md) §3.1/§5/§9:224
- 复核结论：八项判据 8/8；八条红线逐条实测通过；评审 7 条 issues（2 medium + 5 low）→ 修订 1-6 全 applied（含批次报告/ADR/任务书同步）；定向门禁两轮内全绿，E 节为**本报告会话（批次报告员，2026-11-21）独立复跑**，命令原文 + 回显实摘
- 分支 main，链头 `061cfdef`；本批 24 落点全在工作树（未提交，C 节）；CI 终态待 push 后 watch（E2 节）

## A. 完成情况

### A.1 步骤逐项完成（S1→S8 + 评审修订）

| 步骤 | 内容 | 状态 |
|---|---|---|
| S1 | 门禁基线核验：Cargo.lock 399 / tool-engine 零 tokio / porcelain 仅既知面 / 基线 python_worker_e2e 8/8 + 双 crate clippy 绿 / libtest 过滤器语义实勘 / venv before 快照（A3/A4 三漂移证据 + 解释器 sha256）复录 | 完成 |
| S2 | lock 落地：requirements.in 三钉值 → `uv pip compile --generate-hashes` 产 lock（20 包/239 hash 行）→ test_lock_integrity.py 5/5 绿 + 篡改负例咬合实证 → ci.yml python job additive 挂校验（setup-python 3.12 零安装面保持）→ 供给三步命令文档化（本批不实跑重建） | 完成 |
| S3 | venv.rs 探测面：L0（pyvenv.cfg + symlink 双录 + 解释器全量 sha256 + ELF 分类 + owner_mode）/ L1（`-V` 固定 argv / 5s 超时 / 4096 输出帽 / cfg 对拍 major.minor 硬门 patch 双录）/ L2（PEP376 dist-info → 归一 capset → deps_fingerprint + direct_deps 覆盖 + lock sha256 对账）；C §5.1 指纹公式独立可复算；五态 + D7 映射 + 四面 stale；单测 17 项全绿 | 完成 |
| S4 | adapter 接线：PythonWorkerConfig 两字段 + ensure_ready PREPARING 期 fail-closed 探测（现场重探）+ D4 三档 resolve_interpreter + spawn 解释器替换 command[0]（`-I` 保留）+ 五态拒绝经 AdapterFailure.envelope_override 交付；dispatch/rp-agent 构造点读 runtime.venv 注入；rp-workflow 补 None 字段记缝 5；pack 打包面排除 .venv（paths.rs + checksums.rs 双点） | 完成 |
| S5 | 登记面 additive：v1.json 可选 venv 块（additionalProperties false，词表零新增值，interpreter 相对性 pattern）+ validate.rs venv 块校验（path 禁 `..`、interpreter 禁绝对路径/`..`、direct_deps 非空字符串数组）+ sk3 正例 2 形态 + 负例矩阵 7；tool-registry 全量单线程绿 | 完成 |
| S6 | 测试面：venv_parity_worker.py（stdlib-only fixture，协议行为 100% 由 worker_runtime.py 基座承载）+ python_worker_venv_e2e.rs 八判据 8 测试全绿 + 既有面（python_worker_e2e 8/8、first_batch_b2/b5 各 2/2、run_all.py 237 ok=True） | 完成 |
| S7 | 收口门：§6 全部命令复跑 + workspace 全量（唯一失败为 document-router ffprobe 宿主库损坏 2 项，OS 级与本批零关联）+ fmt（本批 16 Rust 文件全净 / 仓库 691 处既存漂移）+ 零增量复核（Cargo.lock/vendor/migrations/rtis/manifests 零 diff）+ venv.rs 零 Registry 写入口 grep 断言 | 完成 |
| S8 | 批次收口：本报告（A-K 十一节）+ docs/README.md RT-2 三行登记（:35-37）+ RT-3/RT-4 前置输入登记 + 缝 5 承接硬性前置（ADR :207-208） | 完成 |
| 评审 | 7 issues（2 medium + 5 low）→ 修订 1-6 全 applied（BATCH-REPORT/ADR/TASK/validate.rs/venv.rs/v1.json/sk3/fixture/e2e 同步）；定向门两轮内全绿 | 完成 |

### A.2 八项判据对照（任务书 §5；测试在本报告会话 17+8 复跑全绿，见 E 节）

| # | 判据 | 测试（python_worker_venv_e2e.rs / venv.rs） | 实测要点 |
|---|---|---|---|
| 1 | lock 完整性正例 | `python_worker_venv_criterion1_lock_integrity_rust_mirror` + `workers/python/tests/test_lock_integrity.py` 5 项 | lock 20 包每行 ≥1 `--hash=sha256:`（239 条实测）；`.in` 三钉值在 lock 版本一致；Python 5/5 + Rust 镜像全绿 |
| 2 | venv 探测正例 | `python_worker_venv_criterion2_real_probe_available` | 真实 `workers/python/.venv` → status=available；五要素非空（解释器 sha256 `a38210a5…49d` 与本会话 `sha256sum /usr/bin/python3.14` 逐字符一致 / 版本双录 3.14.10 vs cfg 3.14.9 / capset 21 项 / lock sha256 `1001c46c…` 与 sha256sum 互证 / fingerprint `bc258b320adba163`）；**指纹在本报告会话用 python3 独立复算逐字符一致**（E.4） |
| 3 | venv 探测负例五态 | `python_worker_venv_criterion3_failure_matrix` + venv.rs 17 单测 | 删 bin/python → missing；版本篡改/损坏 → VENV_IDENTITY_MISMATCH/形态不符；direct_deps 未覆盖 → 拒绝；lock sha 不符 → VENV_IDENTITY_MISMATCH；chmod 000 实造 → permission_denied；1ns 预算实造 → timeout；D7 全表零合并断言；interpreter 绝对路径/`..` 探测面拒绝；连字符 dist-info 解析 |
| 4 | 解释器优先级三档 | `python_worker_venv_criterion4_interpreter_priority` | 显式绝对路径直用不探测（venv 缺失仍执行）；venv 档 spawn = `<venv>/bin/python` 绝对路径（sys.executable 实拍）；裸名档现状不变 |
| 5 | fail-closed 拒绝 | `python_worker_venv_criterion5_fail_closed_rejection` | transcript [Q,P,FAILED]；DEPENDENCY/VENV_NOT_PROVISIONED 与 EXECUTION/VENV_IDENTITY_MISMATCH 两路；stderr 空 = 零 worker 拉起；resolved_interpreter None = 零解释器拼出（零系统解释器回退） |
| 6 | 协议一致性对拍 | `python_worker_venv_criterion6_protocol_parity` | 同一 fixture 在 venv/系统两解释器下：**initialize/get_capabilities 原始响应**（fixture `_send_result` 按到达序捕获前两条 result，echo `handshake_replay` 原样回放）逐字段全键一致；execute echo、worker 内 timeout message「budget timeout after 200 ms」、协作取消 message「task cancelled by cancel request」逐字段钉值一致；isolated=True 双侧 = `-I` 保留 |
| 7 | 既有面零回归 | `python_worker_venv_criterion7_bare_name_unchanged` + 既有套件 | 裸名派发回归绿；python_worker_e2e 8/8；first_batch_b2/b5 各 2/2；run_all.py 237 tests ok=True（skip 11 既有纪律） |
| 8 | 记录面 | `python_worker_venv_criterion8_identity_record_face` | D6 17 字段序列化在位 + deps_fingerprint 复算；首探快照落 §E.4；venv 状态零 Registry/Manifest 写入口（venv.rs 仅 `use tool_registry::hash` 帮手，grep 断言） |

### A.3 评审与修订（7 issues：2 medium + 5 low → 6 项修订全 applied）

| # | 严重度 | 问题 | 修订 |
|---|---|---|---|
| 1 | medium | `runtime.venv.interpreter` 相对性未冻结三道面：v1.json 无 pattern、validate.rs 不查绝对路径、venv.rs probe 穿越防御不含 interpreter；而 `root.join(interpreter)` 在绝对值时按 std 语义整体替换 base——派发目标逃出 venv 根而 capset/lock 对账仍按 venv 面通过（D4 否决「回退系统解释器」要堵的静默降级面的旁路形态） | 三面补齐：validate.rs interpreter 拒绝绝对路径；venv.rs probe 纳入纵深防御（UnsupportedOutput 拒绝）；v1.json allOf 三 pattern（8 例 Python regex 语义验证）；sk3 负例第 7 例 `/usr/bin/python3` 必拒 + venv.rs 新单测（修订 1） |
| 2 | medium | rp-workflow.rs 构造点 hardcode `venv_config: None` 不读 `runtime.venv`：ADR D8 缝清单未收录（缝清单 4 条漏缝 5）、A9「两处构造点」与实勘矛盾；RT-4 补录后经 workflow-engine 派发将静默不触发探测/fail-closed | ADR D8 缝清单扩为五条 + 缝 5 完整条目与验收断言（workflow 派发链对 venv 声明 Manifest 必须同样探测拒绝）；A9 按全仓 grep 实勘改写（生产 3 + 测试 5）；RT-4 落 venv 声明的硬性前置写入「生效与关系」（修订 2） |
| 3 | low | ADR D7 表 timeout 行 ToolRunStatus=TIMED_OUT 与状态机矛盾：`is_legal(Preparing, TimedOut)=false`（state.rs:95 钉死），PREPARING 期探测超时不可能落 TIMED_OUT | ADR D7 表 timeout 行回写为「FAILED（状态机约束，category/code/retryable 三轴兑现，python_worker.rs:681-691 注记）」（修订 3） |
| 4 | low | 批次报告 sk3 计数笔误（「sk3 11 = 既有 9」双错） | 修正为「sk3 9 = 既有 7 + venv 正负例 2」并修订批复跑确认（修订 4；本会话门 3 复跑 9 passed 一致） |
| 5 | low | parse_capset `splitn(3,'-')` 对连字符分发名（python-dateutil-2.9.0.dist-info）整探误拒 | 改 `rsplit_once('-')` 按最后一个 '-' 切分（venv.rs:621）+ 连字符 fixture 单测（修订 5） |
| 6 | low | 判据 6 字面要求 initialize/get_capabilities 响应逐字段一致，e2e 原以常量面代理 | fixture `_send_result` 按到达序捕获前两条原始 result（adapter 握手序恒 initialize→get_capabilities，execute 走 `_send_fitted` 不经 `_send_result`）+ echo `handshake_replay` 回放；e2e 增原始握手两方法逐字段对拍（initialize 4 键 + get_capabilities 6 键 + 全对象相等），捕获不改写、基座零改动（修订 6） |
| 7 | low | 批次报告 §6 sk3 括注等文字漂移（与修订 4 同源）+ docs/README.md 登记行计数过期 | 本报告会话同步：docs/README.md「四缝登记→五缝登记」「偏差记账 5 条→6 条」两处按 ADR/报告实文回写（C.2 记账） |

### A.4 本批明确不做（按任务书 §1 边界，非遗漏）

16 份 Manifest 的 `runtime.venv` 声明补录与 command 迁移（RT-4，缝 4）；python_worker env_clear 化（缝 2）；Runtime Policy Resolver（RT-3）；venv 条目并入 RT-0.5-C 目录/Provider（缝 3）；裸名 PATH 解析收紧（缝 1）；venv 重建 + `pip install --require-hashes` 实跑（供给动作非测试动作，网络面不入测试红线 7）；仓库级 fmt 绿化（独立卫生批）。

## B. 架构与复用

### B.1 venv 探测面（`crates/tool-engine/src/venv.rs`，1257 行，本会话 wc 实测）× RT-0.5 口径

| 层 | 实现 | 锚点 | RT-0.5 口径对齐 |
|---|---|---|---|
| L0 身份 | pyvenv.cfg 解析（home/version，形态不符即拒）+ bin/python symlink 双录（read_link 原值）+ 解释器全量 sha256 + ELF 魔数分类（elf-exec/elf-pie/script）+ /etc passwd/group owner_mode 反查 | venv.rs:429-486（PyVenvCfg）、:737（classify_executable）、:754（owner_mode_of） | RT-0.5-C §5.1 条目形态（sha256/file_type/owner_mode/symlink_target——symlink 双录承 D3 要求，为 D6 schema additive 字段） |
| L1 版本 | `<venv>/bin/python -V` 固定 argv、5s 预算 kill、4096 输出帽；实测版本 vs cfg 按 **major.minor 硬门、patch 漂移双录不翻转**（A4② 3.14.9 vs 3.14.10 在案为证据面） | venv.rs:37（PROBE_TIMEOUT=5s）、:40（PROBE_OUTPUT_CAP=4096） | C §5.1 版本双录；口径偏差见 B.3 裁决 2 |
| L2 依赖集 | site-packages dist-info 目录名 `rsplit_once('-')` 解析（PEP376 名称段连字符安全，修订 5）+ PEP 503 归一 + 排序 capset → deps_fingerprint；direct_deps 覆盖检查；lock 文件 sha256 对账（expected 与实测不符即拒） | venv.rs:621、:553+（normalize_dist_name）、:377-402（lock 对账） | C §5.1 指纹公式实例化：`fingerprint = sha256(canonical_json{bin_sha256, capset, version})[:16]`（venv.rs:499-521 canonical_fingerprint_json 键序手工固定 + :548-550 short_fingerprint）——**本会话 python3 独立复算 deps_fingerprint `7a09f0e800127095` 与 fingerprint `bc258b320adba163` 逐字符一致**（E.4） |

五态（`ProbeFailureKind` venv.rs:110-124）：Missing / PermissionDenied / Timeout / UnsupportedOutput / ProbeFailed，**零兜底合并**；`envelope_mapping`（venv.rs:128-149）逐类映射 D7（RT-1 D7 表复用，`VENV_` 前缀专属化，词表全部落信封九类内 envelope.rs:14-23——DEPENDENCY/POLICY_DENIED/TIMEOUT/EXECUTION/INFRA 本会话读词表核实）：Missing→DEPENDENCY/VENV_NOT_PROVISIONED/false；PermissionDenied→POLICY_DENIED/VENV_PERMISSION_DENIED/false；Timeout→TIMEOUT/-32002/false；UnsupportedOutput→EXECUTION/VENV_IDENTITY_MISMATCH/false；ProbeFailed→INFRA/VENV_PROBE_FAILED/true。`status()` 映射 C §4 条目词表五态（Missing/PermissionDenied/ProbeFailed/Incompatible），serde `rename_all="snake_case"` 对齐小写词形（偏差 5）。stale 判定四面（venv.rs:235-242）：pyvenv.cfg / 解释器 sha256 / deps_fingerprint / lock sha256 任一变化即 stale。

### B.2 adapter 接线面（python_worker.rs additive）与事实分层

- **PythonWorkerConfig 两字段**（python_worker.rs:48-52）：`venv_config`（声明块）+ `venv_identity`（预探缓存）；:51 注记「无论是否在场，声明 venv 的 ensure_ready 一律现场重探（fail-closed 口径）」——venv 状态不跨 dispatch 复用，身份缓存仅随握手记录消费（:143-151）。
- **ensure_ready PREPARING 期探测**（python_worker.rs:153-189）：fail-closed，探测失败即 AdapterFailure（envelope_override），无任何继续路径；引擎面拒绝（python_worker.rs:683-694）status 恒落 FAILED（state.rs:95 `Preparing→TimedOut` 非法为纲），D7 的 category/code/retryable 三轴逐字段兑现（偏差 4）。
- **D4 三档 resolve_interpreter**（python_worker.rs:191）：Manifest 显式绝对路径直用不探测 > 声明 venv available 替换 > 裸名零改动（现状 16 份全走此档，行为面无差）。
- **spawn 替换**（python_worker.rs:467）：`<venv>/bin/python` 非 canonical 绝对路径替换 `command[0]`（保 venv 激活面语义），argv 余量与 `-I` 逐字节保留。
- **构造点**（本会话 grep 实测）：生产 3 处——dispatch.rs:891-933（build_python_adapter，`runtime.venv` → `from_value::<VenvConfig>` :920 + venv_identity: None :933）、rp-agent.rs:311-328（同款 :315-316/:328）、rp-workflow.rs:1030-1033（**不接线，缝 5**，hardcode None）；测试文件 5 处（4 个测试文件，additive 补 None 零行为面）。接线面封闭性 = A9 实勘（ADR:21）。
- **信封机制 additive**：`AdapterFailure.envelope_override: Option<Box<EnvelopeOverride>>`（Box 守 clippy result_large_err），RT-1 文件与既有调用点零改动。
- **事实分层不变**：venv 状态 = 环境事实，只进探测缓存 + 批次报告，零 Registry/Manifest 写入口（红线 2，venv.rs 对 tool_registry 仅 hash 帮手依赖）——Provider 记环境、Registry 记工具的单向引用范式与 RT-1 capability 面同构。

### B.3 pack 分发面、登记面与实现口径裁决

- **pack 排除面**：`plugin-registry/src/checksums.rs:26` `pub const VENV_DIRNAME=".venv"` + :148 walk 排除（写面与 unlisted 检查同面）+ `plugin-runtime/src/paths.rs:326` copy_tree 排除（D1：pack 分发不带 venv 快照；A6 实测 pack 快照 = 通用目录拷贝的意外产物，checksums 含 8273 条 .venv 条目、pyvenv.cfg 携带 /home 旧路径）。已安装 pack（rip-data installed/.venv）为部署面文件本批零触碰。
- **登记面 additive**：v1.json :142/:154-161（可选 venv 对象；path 必填 minLength:1 禁 `..` 由 validate 承载；interpreter `^[^\\/]+非盘符+禁..段` allOf 三 pattern :161）；validate.rs :779-830（非对象/词表外字段/path 空/`..` 穿越/optional 键字符串/interpreter 绝对路径与盘符拒绝）；词表零新增值；sk3 正例 2 形态（全量 :478+ / 缺省可选 :489+ / 绝对路径正例）+ 负例矩阵 7（`..` / 空数组 / 非字符串元素 / 词表外字段 / 缺 path / 非对象 / `/usr/bin/python3`，sk3_tool_manifests.rs:504-600 本会话逐条核对）。
- **实现口径裁决 6 条**（偏差记账，ADR/任务书同步完毕，全录见本报告历史版 §7，要点）：
  1. §6 `python_worker` 过滤器基线态 0 命中（libtest 按测试函数名过滤）——S6 测试按 `python_worker_venv_*` 前缀命名使过滤器成为载具；既有 8 项由 `--test python_worker_e2e` 直跑 + workspace 全量承载。
  2. L1 对拍口径 = major.minor 硬门 + patch 双录不翻转（否则判据 2 真实 venv 正例与判据 3 篡改负例互斥无解）；篡改（major.minor 变）与损坏均走 incompatible/unsupported_output → VENV_IDENTITY_MISMATCH。
  3. rp-workflow 构造点不接线记缝 5（ADR D8 五条缝，ADR:194）；RT-4 硬性前置（ADR:208）。
  4. envelope_override additive + Box；PREPARING 拒绝 status 恒 FAILED（状态机为纲），D7 三轴兑现。
  5. VenvStatus serde snake_case 对齐 C §4 词表；`InterpreterIdentity.symlink_target` 为 D6 additive 字段（D3 双录要求）。
  6. workspace 回归 2 项 document-router 失败为宿主 ffprobe 库损坏（OS 级、本批零关联，宿主零安装红线下不修复，见 E.3/坏消息）。

## C. 文件清单（git status --porcelain 实测逐文件）

### C.1 工作树实测（本报告会话，`git status --porcelain` 原文，30 项）

```text
 M .github/workflows/ci.yml
 M crates/agent-engine/src/bin/rp-agent.rs
 M crates/mcp-server/src/dispatch.rs
 M crates/plugin-registry/src/checksums.rs
 M crates/plugin-registry/src/lib.rs
 M crates/plugin-runtime/src/paths.rs
 M crates/tool-engine/src/adapter/mod.rs
 M crates/tool-engine/src/adapter/python_worker.rs
 M crates/tool-engine/src/lib.rs
 M crates/tool-engine/tests/first_batch_b2.rs
 M crates/tool-engine/tests/first_batch_b5.rs
 M crates/tool-engine/tests/python_worker_e2e.rs
 M crates/tool-registry/src/validate.rs
 M crates/tool-registry/tests/sk3_tool_manifests.rs
 M crates/workflow-engine/src/bin/rp-workflow.rs
 M deny.toml
 M docs/README.md
 M docs/phase0/SEC-0-ADVISORIES-DECISION.md
 M docs/phase0/SEC-0-BATCH-REPORT.md
 M docs/phase0/SEC-0-TASK.md
 M schemas/tool-manifest/v1.json
?? crates/tool-engine/src/venv.rs
?? crates/tool-engine/tests/fixtures/venv_parity_worker.py
?? crates/tool-engine/tests/python_worker_venv_e2e.rs
?? docs/phase0/RT-2-ADR-VENV-GOVERNANCE.md
?? docs/phase0/RT-2-BATCH-REPORT.md
?? docs/phase0/RT-2-TASK.md
?? workers/python/requirements.in
?? workers/python/requirements.lock.txt
?? workers/python/tests/test_lock_integrity.py
```

30 项归类：**RT-2 本批 24 落点（17 M + 7 新增）** + RT-2 任务书/ADR 2（本批设计面文档）+ SEC-0 三文档与 deny.toml 4（WSW-3/SEC-0 轨既知工作树改动，任务书 §6 基线记录，非本批落点）。本报告落笔时会话内新增改动仅 docs/README.md 两处计数同步（B.3 裁决 7，仍在 docs/README.md 同一 M 条目内，porcelain 条目数不变）。

### C.2 逐文件说明（行数为 `wc -l` 本会话实测）

| 文件 | 行数 | 说明 |
|---|---:|---|
| crates/tool-engine/src/venv.rs | 1257 | 探测面全部落点：L0/L1/L2 + 五态 + D7 映射 + 指纹公式 + stale 四面 + 17 单测 |
| crates/tool-engine/tests/python_worker_venv_e2e.rs | 1015 | 八判据 8 测试（判据 6 含原始握手捕获回放对拍） |
| crates/tool-engine/tests/fixtures/venv_parity_worker.py | 84 | stdlib-only 协议对拍 fixture（协议行为 100% 基座承载；`_send_result` 捕获 + `handshake_replay` 回放） |
| workers/python/requirements.in | 9 | 三直接依赖钉定（numpy==2.5.4 / onnxruntime==1.29.0 / tokenizers==0.23.3，D2） |
| workers/python/requirements.lock.txt | 291 | uv `--generate-hashes` 产物：20 包 / 239 条 `--hash=sha256:` 行（本会话 grep 计数实测）；sha256 `1001c46c…` |
| workers/python/tests/test_lock_integrity.py | 105 | 纯 stdlib lock 完整性校验 5 项 |
| .github/workflows/ci.yml | — | python job additive 挂 lock 完整性校验（:99-104，setup-python 3.12 零安装面保持） |
| crates/tool-engine/src/adapter/python_worker.rs | — | 两字段 + ensure_ready 探测 + D4 三档 + spawn 替换 + 拒绝面（B.2 全锚点） |
| crates/tool-engine/src/adapter/mod.rs、lib.rs | — | EnvelopeOverride + re-export + `pub mod venv` |
| crates/mcp-server/src/dispatch.rs | — | build_python_adapter 读 runtime.venv 注入（:891-933） |
| crates/agent-engine/src/bin/rp-agent.rs | — | 同款接线（:311-328） |
| crates/workflow-engine/src/bin/rp-workflow.rs | — | 构造点补 None（:1030-1033）；runtime.venv 读取接线属缝 5 |
| crates/plugin-runtime/src/paths.rs、crates/plugin-registry/src/{checksums.rs,lib.rs} | — | pack 打包/checksums walk 双点排除 .venv（VENV_DIRNAME） |
| crates/tool-engine/tests/{python_worker_e2e,first_batch_b2,first_batch_b5}.rs | — | 既有构造点 additive 补 None 字段（零行为面） |
| schemas/tool-manifest/v1.json | — | python_worker 分支可选 venv 块 + interpreter 相对性 pattern（:142/:154-161） |
| crates/tool-registry/src/validate.rs | — | venv 块校验（:779-830） |
| crates/tool-registry/tests/sk3_tool_manifests.rs | — | venv 正例 2 形态 + 负例矩阵 7 |
| docs/README.md | — | RT-2 三行登记（:35-37；本会话两处计数同步：五缝 / 偏差 6 条） |
| docs/phase0/RT-2-TASK.md / RT-2-ADR-VENV-GOVERNANCE.md / RT-2-BATCH-REPORT.md | — | 任务书 + 设计裁决（D1-D8、A 现状锚点、五缝）+ 本报告 |

零触碰面（本会话实测）：`git diff --stat -- Cargo.lock Cargo.toml vendor/ migrations/ integrations/rtis/ manifests/ schemas/worker-protocol/ workers/python/runtime/` 空输出；`git diff -- manifests/tools/ | wc -l` = 0（28 份 Manifest，python_worker 16 份）；vendor 目录 374 项；`workers/python/.venv` 本体不在 porcelain（.gitignore 生效，`git check-ignore workers/python/.venv/` exit 0）；lock 两文件 check-ignore exit 1（未误 ignore）。

## D. 迁移与兼容

**本批零迁移。**

1. **数据库零迁移**：migrations/ 零 diff（C.1）；生产 `migrations/platform*.db` mtime 停在 2026-11-06/11-07（今日 11-21，stat 实测 5 文件）——零写面。
2. **依赖面零迁移**：Cargo.lock 399 恒定（本会话 grep 实测）、vendor 374 零改动、tool-engine 零 tokio（grep exit 1）、无 Cargo.toml diff——venv.rs 纯 std::fs + 既有进程范式。
3. **worker 派发语义兼容断言**（执行面）：
   - **未声明 venv 的 16 份 Manifest（现状全部）行为面逐字节不变**：裸名 `python3` PATH 解析、env 追加式继承、握手/能力校验全部既有路径零改动（D4 第三档现状档；criterion7 绿 + python_worker_e2e 8/8 + first_batch_b2/b5 各 2/2 + run_all.py 237 ok=True 四面实测）。
   - **协议零分叉**：`schemas/worker-protocol/` 与 `workers/python/runtime/` 零 diff（C.1 实测）；对拍 fixture 协议行为 100% 由 worker_runtime.py 基座承载（venv_parity_worker.py 84 行仅任务面 + 捕获回放），判据 6 两解释器逐字段一致为守护门。
   - **执行面强制只对新声明生效**：`runtime.venv` 在场 → PREPARING 期 fail-closed 探测（不过即拒，零系统解释器回退）；未声明 → 探测零调用。当前 16 份正式 Manifest 零 venv 声明 → 生产行为面无差。
4. **schema additive 兼容**：v1.json venv 块为可选对象、词表零新增值、`additionalProperties: false` 局部封闭；未声明 venv 的现状形状仍接受（sk3 登记面兼容正例）；validate.rs 对旧四字段语义零改动。旧形状登记面不破（tool-registry 19+2+3+11+9 全绿）。
5. **pack 兼容（行为变化显性记账）**：新打包面排除 `.venv`（D1）——此前 pack 意外携带过期 venv 快照（A6），此后 pack 不带 venv、目标机按 lock 重建（NATIVE-FIRST §5.1/§5.2）；已安装 pack 部署面零触碰。checksums 语义面：排除项不再进入清单，unlisted 检查同面排除（校验不误报）。
6. **信封面 additive**：envelope_override 为 Option 字段，既有 AdapterFailure 构造与全部调用点零改动；九类词表零越界（B.1）。状态机零改动（state.rs 未在本批落点）。
7. **Manifest 批量迁移留 RT-4**：venv 声明补录按 RT-0.5-D:24-36 逐项表执行；**硬性前置 = 缝 5 承接**（rp-workflow `resolve_python_worker_value` 接上 runtime.venv 读取 + workflow 派发链探测拒绝验收断言；ADR:208——未接线前 RT-4 不得给经 workflow-engine 派发的工具补 venv 声明）。试点声明只存在于测试 fixture，正式 Manifest 零 diff。

## E. 测试真实结果（定向门禁 + 协议一致性回显；本报告会话 2026-11-21 独立复跑，命令原文 + 回显实摘）

### E.1 定向门禁（门 1：venv 过滤器）

```text
$ cargo test --manifest-path /media/emo/code/pensting/research-platform/Cargo.toml --offline -p tool-engine venv -- --test-threads=1

venv::tests（lib）17 passed; 37 filtered out:
  absolute_and_traversal_interpreter_rejected_at_probe / canonical_json_key_order_is_fixed /
  cfg_version_corrupt_maps_to_unsupported_output / cfg_version_tamper_maps_to_identity_mismatch /
  d7_mapping_table_fully_asserted_no_merge / fixture_venv_probes_available_with_full_identity /
  hyphenated_dist_info_name_parses_by_last_hyphen / l1_timeout_maps_to_timeout_envelope /
  lock_sha256_mismatch_maps_to_identity_mismatch / missing_interpreter_maps_to_missing_state /
  missing_lock_maps_to_missing_state / missing_venv_root_maps_to_missing_state /
  no_read_permission_maps_to_permission_denied / real_venv_dual_branch_available_or_missing /
  stale_detection_covers_all_four_faces / traversal_and_empty_direct_deps_rejected /
  uncovered_direct_dep_maps_to_identity_mismatch   … 全部 ok

python_worker_venv_e2e 8 passed（criterion1-criterion8，14.15s）
test result: ok. 17 passed; 0 failed（lib）+ test result: ok. 8 passed; 0 failed（e2e）
```

### E.2 定向门禁（门 2/3 + worker 契约面 + lock 完整性）

```text
$ cargo test … --offline -p tool-engine python_worker -- --test-threads=1
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 14.09s
（criterion1-8 全命中——过滤器载具化，B.3 裁决 1）

$ cargo test … --offline -p tool-registry -- --test-threads=1
19 passed + 2 passed + 3 passed + 11 passed + sk3: 9 passed
（sk3 9 = 既有 7 + sk3_python_worker_venv_shapes_accept / sk3_python_worker_venv_negative_matrix）

$ python3 workers/python/tests/run_all.py
Ran 237 tests in 11.100s
OK (skipped=11)
SUMMARY tests=237 failed=0 errors=0 failures=0 skipped=11 ok=True

$ python3 -I workers/python/tests/test_lock_integrity.py -v
Ran 5 tests … OK
```

### E.3 clippy 与 fmt 门（本会话实测）

```text
$ cargo clippy --offline -p tool-engine --all-targets -- -D warnings
    Finished `dev` profile … in 0.11s        （零告警）
$ cargo clippy --offline -p tool-registry --all-targets -- -D warnings
    Finished `dev` profile … in 0.08s        （零告警）
$ cargo clippy --offline -p workflow-engine --all-targets -- -D warnings
    Finished `dev` profile … in 0.08s        （零告警）

$ rustfmt --edition 2021 --check <本批 16 个触碰 Rust 文件>   → 16 files fmt CLEAN
$ cargo fmt --all -- --check | grep -c '^Diff in'
691                                           （全部为本批外既存提交态漂移；本批 16 文件零漂移）
```

workspace 全量回归——**本报告会话复跑实测**：`cargo test --offline --workspace --no-fail-fast`（日志 /tmp/rt2-reporter-workspace.log）→ **1108 passed / 2 failed / 160 个 `test result: ok` target**（awk 汇总 + grep 计数实测；cargo exit 101）。唯一 FAILED target = document-router 三通道 e2e（24 passed / **2 failed**：`di2_media_tone_wav_metadata_only_no_frames_step`、`di2_media_duration_exceeded_chain_fails_with_probe_evidence`），失败原文：`native tool exited 127: /usr/bin/ffprobe: error while loading shared libraries: libsndio.so.7: cannot open shared object file`（three_channel_e2e.rs:1666 断言面）——宿主 OS 级环境损坏（libsndio.so.7 缺失，pacman 无属主；批次会话 `ldd /usr/bin/ffprobe` not found + `pacman -Qo` 无属主双证据），与本批零关联（document-router 工作树零 diff），宿主零安装红线下不修复。1108 = 批次会话 1106 + venv.rs 单测 15→17（修订 1 增 2），与修订后代码态一致。

### E.4 协议一致性回显 + VenvIdentity 首探快照 + 指纹独立复算（本报告会话实测）

判据 2 nocapture 实拍（本会话复现，与批次会话首探快照除 probe_timestamp 外逐字段一致）：

```json
{"venv_root":"…/research-platform/workers/python/.venv",
 "interpreter_path":"…/workers/python/.venv/bin/python",
 "interpreter_identity":{"sha256_hex":"a38210a53e07ac5eb912ee35c2d92ddb7eaa349301b60c188a5f496c7c87449d",
   "file_type":"elf-pie","owner_mode":"uid0:root 755","symlink_target":"python3"},
 "base_interpreter":"/usr/sbin","interpreter_version":"3.14.10","python_version_cfg":"3.14.9",
 "deps_capset":[["anyio","4.16.0"],["click","8.5.0"],["filelock","4.2.9"],["flatbuffers","25.12.19"],
   ["fsspec","2026.9.0"],["h11","0.16.0"],["hf-xet","1.8.0"],["httpcore2","2.16.0"],["httpx2","2.16.0"],
   ["huggingface-hub","2.4.0"],["idna","3.20"],["numpy","2.5.4"],["onnxruntime","1.29.0"],
   ["packaging","26.3"],["pip","26.2.2"],["protobuf","7.36.2"],["pyyaml","6.0.3"],
   ["tokenizers","0.23.3"],["tqdm","4.70.1"],["truststore","0.10.5"],["typing-extensions","4.16.0"]],
 "deps_fingerprint":"7a09f0e800127095",
 "requirements_lock_sha256":"1001c46c032fcab67da8003ab8f981bb693f73fd64e670f51d9f958049daade3",
 "direct_deps_satisfied":true,"status":"available","fingerprint":"bc258b320adba163"}
```

**独立复算（python3 -I，不依赖 Rust 实现）**：以 `sha256sum /usr/bin/python3.14`（= `a38210a5…49d`，与快照一致）+ 自行扫描 site-packages 21 个 dist-info 目录（rsplit 最后一个 '-' + PEP 503 归一 + 排序）重建 capset（与快照 21 项逐一一致）→ canonical json 手工拼键序 → sha256 前 16 位：deps_fingerprint `7a09f0e800127095` ✓、fingerprint `bc258b320adba163` ✓——**双指纹逐字符复现，C §5.1 公式独立可复算成立**。

协议一致性（判据 6，本会话 criterion6 绿）：同一 venv_parity_worker.py 在 venv 解释器与系统解释器下——initialize/get_capabilities **原始响应**（fixture 按到达序捕获前两条 result 经 echo 回放）逐字段全键一致（initialize 4 键 + get_capabilities 6 键 + 全对象相等）；execute echo / timeout message「budget timeout after 200 ms」/ cancel message「task cancelled by cancel request」逐字段钉值一致；isolated=True 双侧 = `-I` 保留；sys.executable 为唯一声明差异面（仅存在于 execute echo，握手响应无差异面）。

### E.5 红线复核（本会话实测）

```text
$ grep -c '^\[\[package\]\]' Cargo.lock        → 399（红线 5：零 lock 增量）
$ grep -n tokio crates/tool-engine/Cargo.toml  → exit 1（零 tokio）
$ git diff --stat -- Cargo.lock Cargo.toml vendor/ migrations/ integrations/rtis/ manifests/
  schemas/worker-protocol/ workers/python/runtime/   → 空输出（红线 3/4/5/6 零 diff 面）
$ git diff -- manifests/tools/ | wc -l         → 0（16 份 python_worker Manifest 零语义改动）
$ git check-ignore workers/python/.venv/       → exit 0（.venv 不入库）
$ git check-ignore workers/python/requirements.{in,lock.txt} → exit 1（lock 面入库不被 ignore）
$ sha256sum workers/python/requirements.lock.txt → 1001c46c032fcab67da8003ab8f981bb693f73fd64e670f51d9f958049daade3
$ grep -c -- '--hash=sha256:' requirements.lock.txt → 239；包行 20
$ stat migrations/platform*.db                 → mtime 2026-11-06/11-07（红线 6：生产库零触碰）
$ git status --porcelain -- integrations/rtis/ → 0 条目（红线 6：RTIS 零接触）
$ grep venv.rs 对 tool_registry 的依赖         → 仅 hash 帮手（红线 2：零 Registry 写入口）
```

红线 1（禁全局 pip/宿主零安装）：实现面无 pip install/--user 调用，venv 操作仅 `workers/python/.venv` 只读探测 + 测试临时目录 fixture venv；uv 仅供给命令文档段；CI 保持 setup-python 3.12 零安装（ci.yml:99-104 实读）。红线 7（测试离线 + 单线程）：E.1-E.3 全部命令带 `--offline` + `--test-threads=1` 实跑。红线 8（fail-closed）：判据 5 实测 [Q,P,FAILED] + 零 worker 拉起 + 零回退。

## E2. CI（E2）

**待 watch 回填。** 本批 24 落点全在工作树未提交（C.1），未 push，无 CI run 可 watch；本报告会话无法提供 run 编号与终态（未验证）。已就绪面：

- ci.yml python job additive 步骤在位（本会话实读 ci.yml:99-104）：`lock 完整性校验（RT-2）` → `working-directory: workers/python` + `python -m unittest tests.test_lock_integrity -v`，纯 stdlib、零安装面保持（setup-python 3.12 不装依赖、不跑 uv、不碰 venv——ADR D8.2 分工纪律）；本机同款测试 5/5 绿（E.2）。
- 批次零依赖面变化（Cargo.lock 399 / vendor 374 实测）→ 既有 CI 门禁集无新增需求。
- 回填路径：批次 commit + push 后 watch run 终态（python job / cargo job），终态与失败面（若有）回填本节。

## F. 性能（venv 探测开销面；本报告会话实测，非生产派发基准）

**探测开销构成（真实 venv 实测）：**

| 分量 | 实测 | 口径 |
|---|---|---|
| L0 身份 | 解释器全量 sha256：14,424 字节（Arch python3.14 主二进制经 libpython 分离面），`time sha256sum` 2ms（含进程启动，实际亚毫秒）；pyvenv.cfg 读 + symlink read_link + ELF 头读 = 纯 fs 亚毫秒 | `ls -l /usr/bin/python3.14` + `time sha256sum` |
| L1 版本 | `.venv/bin/python -V` spawn ×10 = 14ms → **~1.4ms/次**（含子进程冷启动） | `time (for i in $(seq 1 10); do workers/python/.venv/bin/python -V; done)` |
| L2 依赖集 | 21 个 dist-info 扫描 + PEP 503 归一 + 排序 = **0.1ms**（python3 独立复算脚本内计时）；lock sha256 = 291 行文件哈希亚毫秒 | python3 -I 计时段 |
| 合计 | 真 venv 全探面（判据 2 含指纹复算）单测试 0.01s（`test result: ok … finished in 0.01s` 实测）——**探测整体毫秒级** | E.4 nocapture run |

**per-dispatch 语义与成本面：**

- 声明 venv 的 Manifest：`ensure_ready` **每次 dispatch 现场重探**（python_worker.rs:51 注记 fail-closed 口径，不跨 dispatch 复用身份）→ 每次派发 +毫秒级探测成本（上表合计）；探测预算 5s/4096B 帽（venv.rs:37/40）为异常面防护，正常路径不触达。
- 未声明的 16 份 Manifest（现状全部）：探测**零调用**，裸名路径零改动 → 本批对现有生产派发链**零新增开销**。
- venv.rs 探测为纯 std::fs + 单次子进程，零 tokio、零异步面——无运行时引入成本。

**未验证项如实**：生产派发链端到端延迟无基准数字（16 份 Manifest 现状零 venv 声明，生产路径未触达探测面，无真实场景可计量）；lock 重建 + `pip install --require-hashes` 全供给链耗时未计量（本批不实跑，任务书 S2 边界）；venv 探测在 CI runner 的开销未计量——均留 RT-4 迁移后生产派发计量。

## G. 遗留与风险（裁决与待办全录）

| # | 事项 | 归属 |
|---|---|---|
| 1 | **缝 5**：rp-workflow.rs:1030-1033 hardcode `venv_config: None` 不读 `runtime.venv`——RT-4 补录后经 workflow-engine 派发的工具将静默不触发探测/fail-closed（现状 16 份零声明故行为面无差）。承接 = 接上同款 runtime.venv 读取 + **验收断言：workflow 派发链对 venv 声明 Manifest 必须同样探测拒绝**（PREPARING 落 FAILED + D7 五态信封、零 worker 拉起、零回退）；ADR:208 硬性前置——未接线前 RT-4 不得给经 workflow-engine 派发的工具补 venv 声明 | RT-3/RT-4 |
| 2 | **缝 1**：未声明 venv 的裸名 `python3` PATH 解析收敛（现状 16 份全走此档） | RT-4 逐项迁移 |
| 3 | **缝 2**：python_worker env 全量继承 + extra_env 追加语义；env_clear 化属安全面扩展 | RT-3 / 后续安全批 |
| 4 | **缝 3**：venv 条目并入 RT-0.5-C 目录/Provider（现仅批次报告 + 探测缓存承载） | Provider 演进批 |
| 5 | **缝 4**：16 份 Manifest runtime.venv 声明补录 + command 迁移（RT-0.5-D:24-36 逐项） | RT-4 |
| 6 | document-router 2 项测试宿主 ffprobe 库损坏（libsndio.so.7 缺失，pacman 无属主）——OS 级既存环境失败，宿主零安装红线下不修复 | 宿主环境面（RT-0.5-C 条目在位状态事后漂移） |
| 7 | 仓库级 fmt 漂移 691 处（HEAD 既存，本批 16 文件全净）；无 rustfmt.toml | 独立 fmt 卫生批 |
| 8 | venv 重建 + lock 安装供给链（uv compile / venv / pip --require-hashes）未实跑（现有 venv 即正例载体） | 部署供给动作（作者/目标机） |

**RT-3 Runtime Policy Resolver 输入面（本批交付）**：venv 五态词表（VenvStatus，C §4 口径）+ `VenvIdentity` 全字段 pub 面 + 探测入口 `venv::probe(config, working_dir, expected_lock_sha256)`；`AdapterFailure.envelope_override` 五态映射面（D7 表，timeout 行 status 轴已回写 FAILED——RT-3 不再按 TIMED_OUT 状态轴实现）；D4 三档解析与 fail-closed 拒绝先例（信任等级分档的拒绝面范式）；缝 2/5 承接。风险面：venv 治理与 RT-3 信任分档的交叉裁决（如「venv available 但 POLICY_DENIED」组合态的语义）RT-3 需显式建模，本批未触碰。

## H. 下一批依赖

**下游链：RT-2（本批）→ RT-3 Runtime Policy Resolver 或 WSW-4（并行轨，零耦合）→ RT-4 逐项迁移。**

### H.1 RT-3 消费面（venv 五态 + 信封范式）

- venv 五态词表 + VenvIdentity（E.4 快照字段面）+ probe 入口签名 = RT-3 策略输入的**环境事实源**；RT-3 按信任等级选择 native/python/container 时，python 档的 venv 健康判定直接消费五态与 stale 四面，不再发明第二套探测。
- AdapterFailure.envelope_override + D7 五态映射 = RT-3 拒绝面先例（POLICY_DENIED/DEPENDENCY/INFRA 分档先例齐）；timeout 行 status 轴回写（评审修订 3）已消除状态机矛盾。
- 缝 2（env_clear 化）与缝 5（workflow 链接线）为 RT-3 直接承接项；缝 5 承接须在 RT-4 补录前完成（ADR:208 硬性前置）。
- RT-3 纪律模板沿用：零 lock 增量、零 tokio、fail-closed 前置、门禁命令原文入报告、`--test-threads=1`。

### H.2 WSW-4（web 工作台轨，与本批零耦合）

- 本批 porcelain 30 项无 web/ 条目（C.1 实测）——WSW-4 开工面与本批落点零交集，无交接前置；WSW 轨既有约定（send-status 分治等）不受影响。

### H.3 RT-4 前置条件（承接 RT-3 之后或并行裁决后）

- 声明块双面已冻结（v1.json + validate.rs + sk3 正负例守护，含 interpreter 相对性三面）；
- 承载面就绪：D4 三档解析、fail-closed 拒绝、`-I` 保留语义对拍实证、pack 排除面；
- 硬性前置两条：缝 5 承接验收断言（G.1）；RT-3 若改派发链构造点须保持接线封闭性（A9 实勘面）。

## I. 浏览器截图

**本批无 UI 面，无截图。** 批次产物全部为 Rust 代码/测试/lock 数据/CI 配置/schema/文档；porcelain 30 项无 web/ 条目（C.1 实测）；零浏览器操作、零页面交互。运行证据全部为命令行实测（E 节门禁回显 + E.4 快照/复算）；既有 web/qa/ 截图属此前批次产物，本批未触碰。

## J. RTIS 对账

**零接触。** `git status --porcelain -- integrations/rtis/` = 0 条目（本会话实测）；本批未调用任何 RTIS 工具面、未读写 RTIS 数据、未触碰 RTIS schema。错误码词表纪律：全部 venv 平台码（VENV_NOT_PROVISIONED / VENV_PERMISSION_DENIED / VENV_IDENTITY_MISMATCH / VENV_PROBE_FAILED + TIMEOUT/-32002）category 严格落信封九类（envelope.rs:14-23，DEPENDENCY/POLICY_DENIED/TIMEOUT/EXECUTION/INFRA 五类在案核实），未向 RTIS 词表或九类外发明任何值。

## K. 教训

1. **契约形状改动必须同批冻结所有承载面。** 评审 medium#1（interpreter 相对性旁路）本质是同一规则落了 validate 一面而 schema/probe 两面未同步——绝对路径经 `Path::join` 整体替换 base 的 std 语义让「声明 venv = 锁定依赖集」可被旁路。改任何「必须相对/必须在内」规则时，先枚举它的全部承载面（schema pattern / 校验器 / 运行时纵深）一起收口。
2. **设计表的每个轴都要对拍状态机。** ADR D7 表 timeout 行写 TIMED_OUT，而 `is_legal(Preparing, TimedOut)=false`（state.rs:95 测试钉死）——冻结设计面与实现面在收口评审才对齐。写状态/信封映射表时先 grep 状态机合法迁移集，把不可达轴直接标死。
3. **「构造点」是 grep 事实不是文档转述。** 任务书写两处、初稿报 6 处、评审实勘 8 处（生产 3 + 测试 5）——构造点封闭性论证（缝外零隐藏构造点）必须以全仓 grep 输出为准，且每次接线后重跑该 grep。
4. **两个互斥判据冲突时，调和口径要落偏差记账而不是硬选边。** 「真实 venv available 正例（cfg 3.14.9 vs 实测 3.14.10）」与「篡改即拒」按任意漂移即 incompatible 的字面口径互斥——major.minor 硬门 + patch 双录的调和既保正例又保负例，且作为 D3 落地裁量显式记账（偏差 2），后来者不用再推一遍。
5. **libtest 过滤器按测试函数名过滤，不按 target 名。** 任务书预设的 `cargo test -p tool-engine python_worker` 基线态 0 命中——判据把命令原文当载具时，测试命名要按过滤器语义规划（S6 以 `python_worker_venv_*` 前缀使过滤器命中 8），否则门禁命令形同虚设。
6. **目录拷贝是快照不是供给。** pack 的 .venv = 通用目录拷贝的意外快照（8273 条 checksums、/home 旧路径、旧版本三漂移俱全）——任何「打包目录」面都要显式排除运行时状态（venv/缓存/锁文件），供给面（lock + 重建命令）与分发面分离（D1）。
7. **lock 格式与消费工具解耦，CI 零安装面才能保住。** uv.lock 否决的核心理由：CI 裸 3.12 消费它需要 uv 二进制；`uv pip compile --generate-hashes` 产 pip 消费型 lock，安装面 = 裸 pip `--require-hashes`，校验面 = 纯 stdlib 5 测试——生成工具、消费工具、校验面三层互不绑定。
8. **指纹公式要「第三方可复算」而不是「实现自证」。** canonical json 键序手工固定（不依赖 serde feature 形态）+ 本报告会话 python3 独立重建 capset 复算双指纹逐字符一致——验证公式的正确方式是换语言重写一遍，不是读实现注释。

## 坏消息单列

1. **缝 5 未接线（评审 medium#2）**：rp-workflow 派发链对 `runtime.venv` 静默不读——RT-4 给任一经 workflow-engine 派发的工具补 venv 声明前必须先承接（验收断言 = workflow 链同样探测拒绝），否则 fail-closed 语义在该链旁路。
2. **初稿两处 medium 如实**：interpreter 相对性三面缺口（可旁路 D4 静默降级面）与 rp-workflow 缝漏登记均为初稿缺口，评审暴露后修订收口——非测试/门禁先发现。
3. **workspace 全量回归 2 项既存环境失败**：document-router ffprobe 宿主库损坏（libsndio.so.7 缺失，OS 级，pacman 无属主）；本报告会话复跑实测 1108 passed / 2 failed / 160 ok targets（E.3，失败原文 ffprobe exit 127）——与本批零关联但使「workspace 全绿」字面不成立，宿主零安装红线下不修复。
4. **仓库级 fmt 红为既存**：`cargo fmt --all -- --check` 实测 691 处漂移全部本批外（HEAD 既存提交态），本批 16 触碰文件全净；全仓绿化未做（独立卫生批遗留）。
5. **CI 终态未验证**：本批未提交未 push，E2 节为占位（待 watch 回填）——门禁绿面均为本机实测，CI 面绿证缺失。
6. **性能无生产基线**：探测开销为组件级实测（毫秒级），无生产派发端到端延迟/吞吐基准；lock 供给链（venv 重建 + require-hashes 安装）未实跑——均如实标未验证（F 节/G.8）。
