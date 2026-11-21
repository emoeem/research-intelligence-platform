# RT-2 任务书：Python Worker 原生环境治理（项目级 venv / lock / 启动探测 / 版本哈希记录 / 协议一致性）

- 日期：2026-11-21
- 状态：任务书已定稿（RT-2 设计裁决员产出；实现批次开工即按本任务书执行）
- 依据：[NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md](NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md) §3.1（64-76 行）、§5（132-138 行）、§9:224 RT-2 行、§10:239 验收行；[RT-0.5-C-CAPABILITY-CATALOG.md](RT-0.5-C-CAPABILITY-CATALOG.md) §2/§4/§5（条目形态/五态/指纹公式）；[RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md](RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md) §2（18-93 行）；[RT-0.5-D-TOOL-MAPPING.md](RT-0.5-D-TOOL-MAPPING.md):5、24-36（16 份 python_worker → RT-2 迁移归属表）；[RT-2-ADR-VENV-GOVERNANCE.md](RT-2-ADR-VENV-GOVERNANCE.md)（本批设计裁决，D1 到 D8）
- 前置：RT-0/RT-0.5 已收口；RT-1 已收口（9c76f6f9→1571a227，批次报告 2026-11-19）；DI-1 已收口。RT-2 与 RT-1 平行治理面（RT-1 治 native_rust 钉值，RT-2 治 python_worker venv），互不触碰对方落点
- 设计裁决：本任务书不重复论证，实现口径一律以 [RT-2-ADR-VENV-GOVERNANCE.md](RT-2-ADR-VENV-GOVERNANCE.md) D1 到 D8 为准

## 1. 目标

把 NATIVE-FIRST §9:224 RT-2 行（「项目级 venv、lock、启动探测、版本/哈希记录、协议一致性测试」）从现状「venv 已存在但无 lock、无探测、无任何 Manifest 消费路径」补齐到受治理的可信 Python Worker 原生档：

1. **lock**：`workers/python/requirements.in`（直接依赖钉定）+ `requirements.lock.txt`（uv `--generate-hashes` 全传递闭包哈希钉定）入 git；pip `--require-hashes` 消费面（ADR D2）。
2. **venv 治理**：仓库级 `workers/python/.venv` 单例身份化（VenvIdentity 探测），Pack 分发面排除 .venv 快照（ADR D1）。
3. **启动探测**：L0/L1/L2 探测面（路径/身份/版本/依赖集指纹）+ 五态 + C §5.1 指纹公式实例化；venv 状态 = 环境事实，不进 Tool Registry（ADR D3/D6）。
4. **版本与哈希记录**：解释器 sha256 全量 + 版本双录（实测 vs pyvenv.cfg）+ 依赖 capset + lock sha256 + fingerprint 16 hex，落探测缓存与批次报告（ADR D6）。
5. **协议一致性正反例**：venv 解释器与系统解释器对拍 + 声明 venv 探测失败的 PREPARING 拒绝面（ADR D8/D7）。
6. **接线**：`PythonWorkerConfig` additive 字段 + 解释器解析优先级三档（Manifest 显式绝对路径 > venv 探测 > 系统 python3 现状），生产构造点三处中接线两处（dispatch/rp-agent），rp-workflow 构造点不接线记缝 5（ADR D4/D5/D8）。

不目标（本批不做）：16 份 Manifest 的 `runtime.venv` 声明补录与 command 迁移（RT-4，依据 RT-0.5-D §表逐项）；python_worker env_clear 化（缝 2，后续安全批/RT-3）；Runtime Policy Resolver（RT-3）；venv 条目并入 RT-0.5-C 目录/Provider（缝 3）；未声明 venv Manifest 的裸名 PATH 解析收紧（缝 1）。

## 2. 现状锚点（开工只读核验清单；A1-A12 全部为 2026-11-21 本会话实测，证据原文见 [RT-2-ADR-VENV-GOVERNANCE.md](RT-2-ADR-VENV-GOVERNANCE.md) §A）

| 锚点 | 事实 | 出处 |
|---|---|---|
| 16 Manifest 全裸 python3 | `command: ["python3","-I",...]`，零 venv 消费路径 | rip.doc.parse.json:13-17 等 16 份（A1） |
| 派发器 | `Command::new(&command[0])` 裸名 PATH 解析；env 追加式继承；握手/能力面校验已齐 | python_worker.rs:253、261-263、314-357（A2） |
| venv 在位但漂移 | 3.14.10 + numpy 2.5.4/onnxruntime 1.29.0/tokenizers 0.23.3；pyvenv.cfg 记 /home 旧路径 + 3.14.9；与系统包版本分裂 | A3/A4 |
| 系统分裂 | 系统 python3 tokenizers 缺失（embedding 恒 hash-fallback）；numpy/onnxruntime 由 pacman 提供非 pip | A5 |
| Pack 快照 | installed/.venv 8273 条 checksums、同款 pyvenv.cfg 漂移；plugin-runtime 源码 .venv 零命中 | A6 |
| lock 缺失 | workers/python 无 requirements/lock 文件；Manifest 依赖仅自由文本 | A7 |
| 校验面 | validate.rs python_worker 分支四字段（755-781）；v1.json 同面（140-155） | A8 |
| 接线点 | 生产构造点三处（dispatch.rs:925 / rp-agent.rs:320 / rp-workflow.rs:1023）+ 测试文件构造点 5 处（4 个测试文件）；本批接线前两处，rp-workflow.rs 不接线（缝 5，ADR D8） | A9 |
| 工具可用性 | pip-tools 缺失；uv 0.13.9（pacman）在位；`uv pip compile --generate-hashes` 实测通过（20 包 239 hash）；无约束解析漂移实证（numpy 2.5.5/ort 1.32.0 ≠ venv 2.5.4/1.29.0）；`python3 -m venv` 裸建成功 | A10 |
| CI 分工 | python job = 裸 3.12 零安装；cargo job 全 --offline | A11 |
| 五态/映射先例 | AdapterOutcome/map_worker_code/九类信封；RT-1 D7 表 | A12 |
| 进程范式先例 | std::process::Command + 线程 + mpsc，零 tokio | python_worker.rs:13-21（RT-1 任务书 §2 同源锚点） |
| 错误信封先例 | PREPARING 失败 INFRA/DEPENDENCY 双码先例 | python_worker.rs:524-558（WORKER_HANDSHAKE_FAILED）；RT-1 判据 2 |

## 3. 范围

### 3.1 允许改动（文件清单草案；新增/修改落点）

| 落点 | 类型 | 内容 | 步骤 |
|---|---|---|---|
| `workers/python/requirements.in` | 新增 | 直接依赖钉定首版：`numpy==2.5.4`、`onnxruntime==1.29.0`、`tokenizers==0.23.3`（当前 venv 实测工作版本，ADR D2；升级属独立供给决策不在本批） | S2 |
| `workers/python/requirements.lock.txt` | 新增 | `uv pip compile requirements.in --generate-hashes -o requirements.lock.txt` 产物，全传递闭包 + 每包 sha256 多值 | S2 |
| `workers/python/tests/test_lock_integrity.py` | 新增 | 纯 stdlib lock 完整性校验（每包行 ≥1 hash / 直接依赖与 .in 一致 / 无 `..`） | S2 |
| `.github/workflows/ci.yml` | additive | python-workers job 增 `test_lock_integrity.py` 调用（零安装保持） | S2 |
| `crates/tool-engine/src/venv.rs` | 新增 | VenvConfig/VenvIdentity/VenvProbe（L0/L1/L2）+ 五态 + C §5.1 指纹公式 + 单测（fixture venv 双分支） | S3 |
| `crates/tool-engine/src/lib.rs` | 修改 | `pub mod venv` | S3 |
| `crates/tool-engine/src/adapter/python_worker.rs` | additive | `PythonWorkerConfig` 增 `venv_config`/`venv_identity` 字段；`ensure_ready()` 前置 venv 探测；spawn 绝对解释器路径替换 `command[0]`；五态拒绝路径 | S4 |
| `crates/tool-engine/src/adapter/mod.rs` | 修改 | venv.rs re-export | S4 |
| `crates/mcp-server/src/dispatch.rs` | 修改 | `build_python_adapter` 读 `runtime.venv` 构造注入 | S4 |
| `crates/agent-engine/src/bin/rp-agent.rs` | 修改 | 同款接线 | S4 |
| `crates/workflow-engine/src/bin/rp-workflow.rs` | additive | 既有构造点补 `venv_config: None, venv_identity: None`（零行为改动）；`runtime.venv` 读取接线属缝 5（ADR D8：RT-3/RT-4 承接，验收断言 = workflow 派发链对 venv 声明 Manifest 必须同样探测拒绝） | S4 |
| `crates/plugin-runtime/src/pack.rs`（或 pack 打包落点，以实勘为准） | additive | 打包/校验和面排除 `.venv/`（Pack 分发不带 venv 快照，ADR D1） | S4 |
| `schemas/tool-manifest/v1.json` | additive | python_worker 分支可选 `venv` 对象（path/interpreter/requirements_lock/direct_deps，ADR D5 形状）；词表零新增值 | S5 |
| `crates/tool-registry/src/validate.rs` | additive | python_worker 分支 venv 块校验（path 禁 `..`、direct_deps 非空字符串数组、requirements_lock 路径形态） | S5 |
| `crates/tool-registry/tests/sk3_tool_manifests.rs` | 修改 | venv 声明正负例断言 | S5 |
| `crates/tool-engine/tests/python_worker_venv_e2e.rs` | 新增 | §5 八项判据测试载具（含协议对拍） | S6 |
| `docs/phase0/` 三份 + `docs/README.md` 登记行 | 文档 | 任务书 + ADR（本批已落）+ RT-2 批次验收报告（S8） | S8 |

### 3.2 不修改

- 现有 16 份 python_worker Manifest 任何语义字段（含 command 形态——venv 声明补录属 RT-4 逐项迁移，ADR D8 缝 4；本批正式 Manifest 零 diff）。
- `schemas/worker-protocol/v1.json`、`workers/python/runtime/worker_runtime.py` 协议行为、六个 worker 脚本执行语义、`workers/python/tests/` 既有测试（只新增文件）。
- `crates/tool-engine/src/adapter/native_rust.rs`、`podman_host.rs`、`egress_proxy.rs`、`structured_output.rs`、`capability.rs`（RT-1 落点零触碰）。
- `migrations/`、`Cargo.toml` 依赖面、`Cargo.lock`（399 条目）、`vendor/`、`manifests/capabilities/host-catalog-seed.json`（16 条为准，ADR D3）。
- 生产 `platform*.db`、`platform-dryrun.db`、`integrations/rtis/`；`workers/python/.venv` 本体内容（探测只读；venv 重建仅在 lock 供给流程作者动作时发生，不属本批测试面）。

## 4. 实施步骤（S1 → S8，串行，每步独立可验证）

- **S1 门禁基线核验**：开工复跑 `grep -c '^\[\[package\]\]' Cargo.lock`（期望 399）、`grep -n tokio crates/tool-engine/Cargo.toml`（期望零命中）、`git status --short`（期望只含本批落点 + 既知工作树改动，不得有本批外新增）、§6 两条测试面命令确认基线绿；复录现状快照（venv 探测五字段 + pyvenv.cfg 漂移三证据，A3/A4 口径）进批次报告作 before 基线。
- **S2 lock 落地**：写 `requirements.in`（钉定三直接依赖）；`uv pip compile workers/python/requirements.in --generate-hashes -o workers/python/requirements.lock.txt` 生成 lock；抽查 lock 内 numpy/onnxruntime/tokenizers 版本与 `.in` 钉值一致；`test_lock_integrity.py` 落地并在本机 `python3 workers/python/tests/test_lock_integrity.py` 实跑全绿；ci.yml additive 挂接（保持 setup-python 3.12 零安装面）。供给命令（venv 重建 + `pip install --require-hashes`）按 ADR D2 三步写进批次报告文档段（本批不实跑重建——现有 venv 在位且探测可用即正例载体；重建属环境供给动作，网络面不入测试）。
- **S3 venv 探测面**：`crates/tool-engine/src/venv.rs` 按 ADR D3/D6 落 VenvConfig/VenvIdentity/VenvProbe：L0（pyvenv.cfg 读面 + bin/python symlink 解析 + 解释器全量 sha256 + owner/mode）、L1（`<venv>/bin/python -V` 受控探测，固定 argv、短超时、输出帽，杀面沿用 adapter 线程范式）、L2（site-packages dist-info 解析 → capset → deps_fingerprint）；指纹公式复算 C §5.1；五态判定与 stale 失效（pyvenv.cfg/解释器 sha256/lock sha256/依赖集任一变化即 stale）；单测用 fixture venv（测试内 `python3 -m venv` 临时目录，离线零安装）+ 真实 venv 存在性双分支。
- **S4 adapter 接线**：python_worker.rs additive（ADR D5）：config 两字段 + `ensure_ready()` PREPARING 期探测 + 探测失败五态拒绝（D7 表）+ spawn 绝对解释器路径替换 + 探测身份随握手记录；dispatch.rs/rp-agent.rs 构造点读 `runtime.venv` 注入；pack 打包面排除 `.venv/`；mod.rs/lib.rs 注册。
- **S5 登记面 additive**：v1.json python_worker 分支可选 `venv` 对象（ADR D5 形状冻结）；validate.rs venv 块校验（path 绝对或 working_dir 相对、禁 `..`、direct_deps 非空字符串数组、requirements_lock 形态）；sk3_tool_manifests.rs 正负例（合法 venv 块 / `..` 穿越 / direct_deps 空数组 / 非字符串元素 / 词表外字段）。
- **S6 测试面落地**：`tests/python_worker_venv_e2e.rs` 按 §5 八项判据逐项落测试；协议对拍用 stdlib-only fixture worker（或复用 tests/fixtures 既有 worker 面形态，RT-1 fixture 先例：测试自持、sha256 一并入库）；既有 `python_worker_e2e.rs`/`first_batch_b2.rs`/`first_batch_b5.rs` 零回归。
- **S7 收口门**：§6 全部命令 + `cargo fmt --all -- --check` + `cargo test --offline --workspace`（跨 crate 回归）+ lock/vendor/migrations/integrations/rtis/manifests/tools 零增量复核 + `git status` 复核（只含 §3.1 落点）。
- **S8 批次收口**：RT-2 批次验收报告落 `docs/phase0/`（命令原文 + 输出摘要 + 八项判据对照表 + VenvIdentity 首探快照全字段实测值 + 供给命令文档段）；docs/README.md 索引补 RT-2 行；RT-3/RT-4 前置输入登记（venv 五态 + VenvIdentity 为 RT-3 输入；RT-4 迁移按 RT-0.5-D §表逐项；缝 5 承接验收断言 = workflow 派发链对 venv 声明 Manifest 必须同样探测拒绝，随 RT-3/RT-4 登记，ADR D8）。

## 5. 完成标准（八项判据，S6 逐项有测试、S8 逐项有实测输出）

| # | 判据 | 测试要求（负例为硬性红线） |
|---|---|---|
| 1 | lock 完整性正例 | `requirements.lock.txt` 存在且每个非注释包行 ≥1 `--hash=sha256:`；`.in` 三直接依赖在 lock 中版本一致；`test_lock_integrity.py` 全绿（CI python job 同款零依赖） |
| 2 | venv 探测正例 | 对真实 `workers/python/.venv` 探测 → status=available；VenvIdentity 五要素非空（解释器 64hex sha256 / 版本双录 / capset / lock sha256 / fingerprint 16 hex）且指纹按 C §5.1 公式复算一致 |
| 3 | venv 探测负例五态 | fixture venv 各破坏面：删 `bin/python` → missing；pyvenv.cfg version 篡改/损坏 → incompatible 或 unsupported_output（形态不符路径）；capset 注入 direct_deps 未覆盖包 → direct_deps_satisfied=false 拒绝；lock 文件 sha256 不符 → VENV_IDENTITY_MISMATCH；无读权限面 → permission_denied；逐态断言映射 D7 表 category/code/retryable，零兜底合并 |
| 4 | 解释器优先级三档 | Manifest 显式绝对路径直用不探测；声明 venv 且 available → command[0] 替换为 `<venv>/bin/python` 绝对路径（spawn argv 断言）；未声明 venv → 现状行为逐字节不变（裸名派发回归断言） |
| 5 | fail-closed 拒绝 | 声明 venv + 探测 missing/incompatible/lock 不符 → PREPARING 拒绝落 FAILED，错误信封按 D7（DEPENDENCY/POLICY_DENIED/EXECUTION），**零系统解释器回退**（argv 断言从未拼出非 venv 解释器）；transcript [Q,P,FAILED] 且零 worker 进程拉起 |
| 6 | 协议一致性对拍 | 同一 stdlib fixture worker 在 venv 解释器与系统解释器下 initialize/get_capabilities/execute/timeout/cancel 响应逐字段一致（`sys.executable` 除外）；`-I` 保留断言（venv 依赖可达 + 协议行为不变同证） |
| 7 | 既有面零回归 | `python_worker_e2e.rs`（8 项）/`first_batch_b2.rs`/`first_batch_b5.rs` 全绿；`workers/python/tests/run_all.py` 离线全绿（SUMMARY ok=True）；未声明 venv 的派发链信封/状态机逐字段与基线一致 |
| 8 | 记录面 | VenvIdentity 序列化 schema 逐字段非空断言（判据 2 载体）；批次报告落首探快照（含 before 基线 A3/A4 漂移证据与 after 探测值对照）；venv 状态零写 Tool Registry/Manifest（grep 断言探测面无 registry 写入入口） |

## 6. 测试面（收口命令，与 CI 同门）

```bash
# 定向测试（venv + python_worker 全量，单线程防工作区/池竞争）
cargo test --manifest-path /media/emo/code/pensting/research-platform/Cargo.toml --offline -p tool-engine venv -- --test-threads=1
cargo test --manifest-path /media/emo/code/pensting/research-platform/Cargo.toml --offline -p tool-engine python_worker -- --test-threads=1

# 登记面（validate/sk3）
cargo test --manifest-path /media/emo/code/pensting/research-platform/Cargo.toml --offline -p tool-registry -- --test-threads=1

# worker 契约面（离线零依赖；unittest 串行为默认）
python3 workers/python/tests/run_all.py

# 静态检查门
cargo clippy -p tool-engine --all-targets -- -D warnings
cargo clippy -p tool-registry --all-targets -- -D warnings

# 格式门
cargo fmt --all -- --check

# 跨 crate 回归（S7）
cargo test --offline --workspace
```

基线记录（本任务书写作时点 2026-11-21 实测）：

- `cargo test … -p tool-engine venv -- --test-threads=1`：**未跑**（venv.rs 尚不存在；S3 落地后由此命令承载）。S1 步骤须先复跑 `cargo test --offline -p tool-engine python_worker -- --test-threads=1` 与 clippy 确认现状基线绿（RT-1 收口记录 8/8 判据全绿，本批未复跑全量，如实标注）。
- lock 工具实测（A10，/tmp scratch，已清理零安装残留）：`uv pip compile requirements.in --generate-hashes -o requirements.lock.txt` 退出码 0、解析 20 包、239 条 hash 行；`pip-compile` 缺失；`uv` 0.13.9（pacman uv 0.13.9-1.1）。
- venv 实测（A3/A4）：`workers/python/.venv/bin/python -m pip list` → numpy 2.5.4/onnxruntime 1.29.0/tokenizers 0.23.3；`python3 -I -c "import tokenizers"` → ModuleNotFoundError（系统面）；`cat pyvenv.cfg` → /home 旧路径 + version 3.14.9；`python3 -m venv /tmp/…` 裸建成功。
- `git status --short`（任务书写作时点）：deny.toml + SEC-0 三文档为既知工作树改动（WSW-3/SEC-0 轨），本批落点不得与其混淆。

## 7. 红线（八条，违反任一即停批上报）

1. **禁全局 pip / 宿主零安装**：零系统级安装（pip install 全局/--user 面禁用；uv 为 pacman 既装面，禁升级/禁 pip 装 pip-tools）；venv 操作只限 `workers/python/.venv`（探测只读）与测试临时目录（S3 fixture venv）；NATIVE-FIRST §5.1「禁止全局 pip 安装」+ §12:262「缺失能力只报告不安装」原文口径。
2. **venv 状态 = 环境事实不进 Tool Registry**：探测面零 Registry/Manifest 写入入口（不建第二事实源，NATIVE-FIRST §4.2:126；C §6 职责边界表原文）；记录只落探测缓存 + 批次报告（ADR D6）。
3. **Worker Protocol v1 语义零分叉**：`schemas/worker-protocol/v1.json` 零 diff；`worker_runtime.py` 协议行为零改动；§4.2 映射表（timeout×1000→timeout_ms、input→task、submit_key 直通）零改动；对拍判据 6 为守护门。
4. **16 份 Manifest 零语义改动**：本批正式 `manifests/tools/*.json` 零 diff（venv 声明试点只存在于测试 fixture，ADR D8 缝 4）；schema/validate 变更全部 additive，词表零新增值（旧形状登记不破）。
5. **Cargo.lock 399 恒定 / vendor 零增量 / tool-engine 零 tokio**：venv.rs 纯 std::fs + 既有进程范式；禁新依赖（含 dev-dependencies）；S1/S7 双点复核。
6. **生产库零触碰 + RTIS 零接触**：`platform*.db`/`platform-dryrun.db` 全程不写；`integrations/rtis/` 零 diff。
7. **测试离线 + --test-threads=1**：Rust 测试面一律 `--test-threads=1` + `--offline`；lock 生成（uv）与 pip install（网络）是供给动作非测试动作，测试面用 fixture/真实 venv 读面，零网络依赖；CI 保持 setup-python 3.12 零安装分工（A11）。
8. **fail-closed 禁静默降级**：声明 venv 的 Manifest 探测不过一律 PREPARING 拒绝（ADR D4 否决项 1），禁回退系统解释器、禁把探测失败伪装成执行失败或 success；Pack 分发面排除 .venv 后不得出现「pack 自带过期快照」回归面（A6 形态）。

## 8. 回滚

- 每步独立 commit（S2 lock 面 / S3 探测面 / S4 接线 / S5 登记面 / S6 测试面 / S8 报告各一）；`git revert <批次 commit>` 整体撤销；additive 字段/模块回滚后旧 16 份 Manifest 语义不受影响（零 diff 面本就无回滚残留）。
- `requirements.in`/`requirements.lock.txt` 为数据面文件，随 commit 回滚；`workers/python/.venv` 本体 gitignore（.gitignore:16）不入库，回滚零残留。
- 测试临时 venv（fixture）用后即焚；生产库/RTIS/Podman 对象/宿主软件零操作，无生产回滚面。

## 9. 交付物

1. 数据：`workers/python/requirements.in` + `requirements.lock.txt`（S2，哈希钉定 lock 首版）。
2. 代码：`venv.rs` 探测面 + `python_worker.rs` additive 接线 + dispatch/rp-agent 构造点 + validate.rs/v1.json additive + pack 打包排除面（S3-S5）。
3. 测试：八项判据测试 + lock 完整性测试 + fixture venv 载具（S2/S6），§6 收口命令输出全绿（S7）。
4. 文档：本任务书、[RT-2-ADR-VENV-GOVERNANCE.md](RT-2-ADR-VENV-GOVERNANCE.md)（本批已落）+ RT-2 批次验收报告（S8）+ docs/README.md 登记。
5. 前置输出：RT-3 消费的 venv 五态 + VenvIdentity 探测面；RT-4 消费的 `runtime.venv` 声明面与逐项迁移依据（RT-0.5-D §表 + 本批承载面）。
