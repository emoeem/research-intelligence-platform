# RT-3 任务书：Tool Runtime Policy Resolver（决策矩阵纯函数 / 不安全降级硬拒绝 / 决策可解释性输出）

- 日期：2026-11-21
- 状态：任务书已定稿（RT-3 设计裁决员产出；实现批次开工即按本任务书执行）
- 依据：[NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md](NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md) §3:51-62、§3.1:64-76、§3.2:78-88、§3.3:90-97、§9:225 RT-3 行、§10:238、§11:250；[RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md](RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md) §6:229-236、§7:248-257；[RT-0.5-E-RISK-AND-ISOLATION.md](RT-0.5-E-RISK-AND-ISOLATION.md) §1-§3；[RT-0.5-C-CAPABILITY-CATALOG.md](RT-0.5-C-CAPABILITY-CATALOG.md) §5.3:290、§6；[RT-0.5-D-TOOL-MAPPING.md](RT-0.5-D-TOOL-MAPPING.md) §2-§5；[RT-2-ADR-VENV-GOVERNANCE.md](RT-2-ADR-VENV-GOVERNANCE.md) D8 缝 5:194
- 设计裁决：本任务书不重复论证，实现口径一律以 [RT-3-ADR-POLICY-RESOLVER.md](RT-3-ADR-POLICY-RESOLVER.md) D1-D10 为准
- 前置：RT-1 已收口（ae21a0a8）；RT-2 已收口（6899feb8）；DI-1/DI-2 已收口（659ed1e8/22c7adb0）。RT-3 消费 RT-1 CapabilityProvider 面与 RT-2 venv 五态面，互不触碰两者落点

## 1. 目标

把 NATIVE-FIRST §9:225 RT-3 行（「按信任等级、依赖复杂度、隔离要求选择 native/python/container/VM；拒绝不安全降级」）从现状「runtime 选择 = dispatch 直读 Manifest `runtime.type`（dispatch.rs:224-296），无任何策略层」补齐到可测试的决策面 **PolicyResolver**：

1. **输入**：工具 Manifest（声明面）+ 四维决策输入——Trust Level / Input Risk / Dependency Complexity / Isolation Requirement（词表冻结见 ADR D2）+ Host Capability（RT-0.5 目录 CapabilityEntry 五态 + Freshness / RT-2 venv 五态 / EgressProxy 接线子事实）。
2. **输出**：runtime 决策（native_rust / python_worker / podman_host / podman_vm）+ 有序规则命中链 reason（每条带文档/代码锚）+ constraints + 交叉校验差距清单 gap + 硬拒绝（verdict=reject，RT3_ 前缀码 + 九类信封 category）。输出 Schema 见 ADR D8。
3. **决策矩阵**：R0-R7 有序瀑布纯函数，3×3×4×4=144 格全覆盖（totality），策略 §3.1/§3.2/§3.3 三表逐行映射（ADR D6）；不安全降级硬拒绝矩阵 N1-N10（ADR D7）。
4. **决策层落位**：`crates/tool-engine/src/policy.rs`（ADR D1 裁决：不建独立 crate）。
5. **决策/派发分离**（ADR D9）：本批派发链零改动、Manifest 零 diff；唯一派发构造点改动 = 缝 5 承接（rp-workflow venv 接线，additive）。

不目标（本批不做）：29 份 Manifest 的逐项迁移与 `runtime.venv` 声明补录（RT-4，依据 RT-0.5-D §表 + 本批 gap 清单逐项）；Resolver 接入 dispatch 派发决策点（RT-4 落 Manifest 后的批）；podman_vm adapter 实现（设计保留面，本批只裁决其「未就绪即拒绝」语义）；python_worker env_clear 化（RT-2 缝 2，后续安全批）；venv 条目并入 CapabilityProvider（RT-2 缝 3）；permission_level 授权面改动（T0/T1/T2 语义归既有 Tool Policy，TOOL-CONTRACT-DRAFT.md:132-134）。

## 2. 现状锚点（开工只读核验清单；A1-A13 全部为 2026-11-21 本会话实测）

| 锚点 | 事实 | 出处 |
|---|---|---|
| Manifest 分布现状 | **28 份 JSON = 16 python_worker + 11 native_rust + 1 podman_host**，另有 YAML-only `rip.http.tls_inspect.yaml`（合计 29 份派发面 Manifest）。注意：RT-0.5-D:5 的「19 = 16+2+1」快照已被 DI-1/DI-2 批次推进——9 份新 native_rust（archive.list/extract、doc.pdf.text/info、doc.image.meta/ocr、doc.office.to_pdf、media.probe/frames）由 659ed1e8/22c7adb0 落地，本任务书以 28+1 现状为准 | 本会话 `grep -o '"type": *"[a-z_]*"' manifests/tools/*.json` 分类计数实测 |
| runtime.type 直读派发 | dispatch.rs:224 读 `runtime` 块取 type；:250-296 按 `python_worker` / `podman_host|podman_vm` / `native_rust` 三分支直派；无策略层 | crates/mcp-server/src/dispatch.rs:224,250-296（A1） |
| RT-1 拒绝面（决策对齐锚） | allowlist 解析：binary 非绝对路径拒绝（:348-355）、binary_sha256 缺钉值拒绝（:356-370）、子命令白名单（:407-437）；secrets 非空拒绝（:829-831）；capability_ref 二道校验 fresh+available（:661-711） | crates/tool-engine/src/adapter/native_rust.rs:330-437,829-831,661-711（A2） |
| RT-2 解释器三档 | 显式绝对路径 > venv 探测（fail-closed 五态拒绝）> 现状裸名；探测失败零系统解释器回退 | crates/tool-engine/src/adapter/python_worker.rs:190-217（A3） |
| venv 五态 | VenvStatus = available/missing/incompatible/probe_failed/permission_denied | crates/tool-engine/src/venv.rs:83-104（A4） |
| 既有 InputRisk/CallMode 词表 | `InputRisk` = trusted_low_risk/untrusted/high_risk；`CallMode` = restricted_native_subprocess/isolated_container/vm_isolated；`SecurityProfile`（allowed_call_modes/max_input_risk/notes）；`CapabilityEntry` 十二字段 + `Freshness`/`StaleReason` | crates/tool-engine/src/capability.rs:478-613（A5） |
| CapabilityProvider 面 | StaticCatalogProvider::load（种子 `manifests/capabilities/host-catalog-seed.json` 16 条）+ measured_sha256 + freshness 判定 | crates/tool-engine/src/capability.rs:1028-1118,1216-1225,1409-1460（A6） |
| 缝 5 | rp-workflow.rs:1030-1033 `venv_config: None` hardcode，不读 `runtime.venv`（RT-2 D8 缝 5 原文位）；dispatch.rs build_python_adapter 已读 `runtime.venv`（接线样板） | crates/workflow-engine/src/bin/rp-workflow.rs:991-1035；dispatch.rs:920-933（A7） |
| Manifest 四维派生原料 | permission_level T0×22/T1×5/T2×1；network.mode none×24/egress_allowlist×3/vm_local×1；provenance.origin rip-native×11/opensource×10/internal-kit×7（29 份口径）；native_rust 9 份带 binary_sha256、2 份 rip-native 裸名 binary 无钉值（rip.knowledge.document.fetch/source.sync：`"binary": "rp-knowledge-fetch"`）；nuclei image ref 内 digest 钉定 | 本会话 python 只读模拟全 29 份实测（A8） |
| 已知不一致预判 | ①rip.evidence.rtk.mcp：python_worker + network=vm_local + T2（vm_local→kernel→仅 VM，ADR D5/D7 N6）；②rip.knowledge.document.fetch / source.sync：声明 native_rust 但 binary 非绝对路径 + 无 binary_sha256（RT-1 D3 准入缺口，ADR D6 R6）；③其余 25 份按默认风险档 assign；④rip.ops.worker-runtime：probe=none 元数据条目 → not_applicable | A8 派生 + RT-0.5-D:25,37,40 预判（A9） |
| lock 仓库事实 | `workers/python/requirements.in` + `requirements.lock.txt` 在位（RT-2 S2 落地）→ python_worker complexity=locked 派生判据 | 本会话 `ls` 实测（A10） |
| 信封九类词表 | VALIDATION/INPUT/TIMEOUT/CANCELLED/EXECUTION/POLICY_DENIED/DEPENDENCY/INFRA/RUNNER_UNAVAILABLE | crates/tool-engine/src/envelope.rs:14-23（A11） |
| 依赖面 | tool-engine 零 tokio（deps=database/domain/tool-registry/serde/serde_json/serde_yaml_ng）；Cargo.lock 399 包 | crates/tool-engine/Cargo.toml；`grep -c '^\[\[package\]\]' Cargo.lock`（A12） |
| 测试面现状 | tool-engine tests 16 套件（capability_catalog.rs / python_worker_venv_e2e.rs / native_rust_e2e.rs 等）基线绿；workflow-engine tests 4 套件 | ls crates/*/tests/ 实测（A13） |

## 3. 范围

### 3.1 允许改动（文件清单；新增/修改落点）

| 落点 | 类型 | 内容 | 步骤 |
|---|---|---|---|
| `crates/tool-engine/src/policy.rs` | 新增 | 四维词表 enum（TrustLevel/DependencyComplexity/IsolationRequirement 新增；InputRisk 复用 capability.rs 既有 enum + 别名映射）+ `as_str`/`parse` 双向 + 派生函数（ADR D3-D5）+ `derive_target`/`decide` 纯函数（ADR D6 R0-R7）+ `RuntimeDecision` 序列化结构（ADR D8）+ RT3_ 拒绝码表（ADR D7） | S3 |
| `crates/tool-engine/src/lib.rs` | 修改 | `pub mod policy`（一行） | S3 |
| `crates/workflow-engine/src/bin/rp-workflow.rs` | additive | 缝 5 承接：`resolve_python_worker_value` 读 `runtime.venv` 构造 `VenvConfig` 注入（dispatch.rs:920-933 同款）；未声明 venv 的 Manifest 行为零变化；:1030-1031 注释更新 | S5 |
| `crates/tool-engine/tests/policy_resolver_matrix.rs` | 新增 | 144 格 totality 枚举 + 代表性格点断言 + 负例矩阵 N1-N10 逐条 + reason 可解释性断言 + D2 别名映射双向断言 | S4 |
| `crates/tool-engine/tests/policy_resolver_manifests.rs` | 新增 | 29 份（28 JSON + 1 YAML）集成期望表（A9 预判：25 assign + 2 准入缺口 inconsistent + 1 vm_local inconsistent + 1 not_applicable；每份断言 verdict/runtime/gap 逐字段）+ 按三档风险逐工具评估断言 | S4 |
| `crates/workflow-engine/tests/`（新增测试文件或既有文件 additive） | 新增 | 缝 5 验收断言：workflow 派发链对 venv 声明 Manifest 必须同样探测拒绝（PREPARING 拒绝落 FAILED + D7 五态信封，零 worker 进程拉起、零系统解释器回退——RT-2-ADR 缝 5 原文口径） | S5 |
| `docs/phase0/RT-3-BATCH-REPORT.md` | 新增 | 批次验收报告（S8） | S8 |
| `docs/README.md` | additive | 索引补 RT-3 行（任务书/ADR/批次报告） | S8 |

### 3.2 不修改

- `manifests/tools/` 全部 29 份（**本批零 diff**；逐项迁移属 RT-4，红线 2）。
- `crates/tool-engine/src/adapter/native_rust.rs`、`python_worker.rs`、`podman_host.rs`、`egress_proxy.rs`、`structured_output.rs`、`capability.rs`、`venv.rs`（RT-1/RT-2 落点零触碰；Resolver 只读消费其公开面）。
- `crates/mcp-server/src/dispatch.rs`（派发链零改动，ADR D9a）。
- `schemas/tool-manifest/v1.json`、`crates/tool-registry/`（本批无登记面变更——四维输入是决策面参数不是 Manifest 字段，ADR D3/D4 否决项）。
- `migrations/`、`Cargo.toml` 依赖面、`Cargo.lock`（399 恒定）、`vendor/`、`manifests/capabilities/host-catalog-seed.json`（16 条为准）。
- 生产 `platform*.db`、`platform-dryrun.db`、`integrations/rtis/`；`workers/python/.venv` 本体（如需探测只读）。
- `crates/document-router/`（DI-1 落点零触碰；风险档位分类不属本批）。

## 4. 实施步骤（S1 → S8，串行，每步独立可验证）

- **S1 门禁基线核验**：复跑 A12 两条（`grep -c '^\[\[package\]\]' Cargo.lock` 期望 399；tool-engine Cargo.toml 零 tokio）、`git status --short`（期望无本批外新增）、A13 两条基线命令确认绿（与 §6 同命令）；复录 A8 派生模拟输出（29 份四维派生表）进批次报告作 before 基线；实勘 A7 缝 5 行号漂移（若 rp-workflow.rs 行号已变，以函数名 `resolve_python_worker_value` 重新定位并记录）。
- **S2 测试载具设计**：fixture 设计稿落测试文件头注释——合成 CapabilityEntry（五态×Freshness 各值）、合成 VenvStatus、四维枚举全值、29 份 Manifest 真实读取路径；fixture 零网络零安装（纯内存构造）。
- **S3 policy.rs 决策面**：按 ADR D2-D8 落：词表 enum + parse（词表外拒绝）→ D3/D4/D5 派生函数（输入=serde_json Manifest 值 + 能力子事实，输出=四维枚举）→ `derive_target(inputs) -> Target|Reject`（R0-R6，floor 合成）→ `decide(manifest, inputs) -> RuntimeDecision`（R7 交叉校验 + reason 链 + gap + constraints）；RT3_ 码表（D7）；全函数零 IO 零时钟零随机；`cargo doc` 面注释引用 ADR D 行号。
- **S4 矩阵测试面**：`policy_resolver_matrix.rs`（144 格 totality 枚举断言 + D6 代表性表逐格 + N1-N10 逐条 fixture 断言 + reason 非空/含锚/四维回显断言 + InputRisk 别名映射双向断言）；`policy_resolver_manifests.rs`（29 份集成期望表逐份断言 + A9 四类预判 + 每工具三档风险评估断言：native 文档工具 low→assign / unknown→inconsistent(target=container) / high→reject(RT3_VM_NOT_READY)）。
- **S5 缝 5 承接**：rp-workflow.rs `resolve_python_worker_value` additive 接线 `runtime.venv`；workflow-engine 测试面落「venv 声明 Manifest 经 workflow 链派发必须同样探测拒绝」断言（RT-2-ADR 缝 5 验收断言原文）；未声明 venv 的既有链路回归断言（行为零变化）。
- **S6 全量回归**：§6 全部命令实跑；tool-engine 既有 16 套件 + workflow-engine 4 套件零回归；clippy/fmt 门。
- **S7 收口门**：§6 全部命令 + `cargo fmt --all -- --check` + `cargo test --offline --workspace` + `git status` 复核（只含 §3.1 落点）+ `manifests/tools/` 零 diff 复核（`git diff --stat manifests/tools` 期望空）。
- **S8 批次收口**：RT-3 批次验收报告落 `docs/phase0/`（命令原文 + 输出摘要 + 判据对照表 + 29 份 RuntimeDecision 全量快照 + 3 处 inconsistent 的 gap 清单转 RT-4 输入登记 + 缝 5 解除登记）；docs/README.md 索引补 RT-3 行；RT-4 前置输入登记（逐项迁移依据 = RT-0.5-D §表 + 本批 gap/reason 清单）。

## 5. 完成标准（八项判据，S4/S5 逐项有测试、S8 逐项有实测输出）

| # | 判据 | 测试要求（负例为硬性红线） |
|---|---|---|
| 1 | 决策表全维覆盖正例 | 3×3×4×4=144 格 totality 枚举测试：每格产出 Target 或 Reject，无未处理格（panic/unwrap 即失败）；D6 代表性表逐格断言（audited 行 12 格 + N1-N10 涉及格） |
| 2 | 三表映射可追溯 | 决策 reason 链每条含策略 §3.1/§3.2/§3.3 或 E §1 行锚；§3 总表 8 行中 native_rust/python_worker/podman_host 三行在 29 份集成期望表内有 assign 成员，podman_vm 行由 144 格矩阵承载（现状零声明成员，VM 未就绪一律 Reject，判据 3 守护），mcp_client/http_api/cli_adapter 三行词表外声明 → Reject(RT3_RUNTIME_VOCAB) 断言 |
| 3 | 不安全降级硬拒绝矩阵 | N1-N10 逐条独立 fixture 断言（ADR D7 表列组合 → 期望 verdict + code + category）；N3 断言 RT3_VM_NOT_READY 且 **无任何降级目标**（podman_host/native 均非输出值）；N7 断言 stale/missing 五态全 Reject 零猜测 |
| 4 | 决策可解释性 | RuntimeDecision.reason 非空、有序、每条含 rule id + hit 描述 + 锚；inputs 四维取值回显；同 inputs 两次 decide 输出逐字段相等（纯函数确定性断言）；D2 InputRisk 别名映射双向 parse 断言（low↔trusted_low_risk / unknown↔untrusted / high↔high_risk） |
| 5 | 29 份集成期望表（28 JSON + 1 YAML）| 逐份 Manifest 断言 verdict/runtime/cross_check/gap：25 assign + rip.knowledge.document.fetch/source.sync inconsistent（gap=绝对路径+钉值缺口）+ rip.evidence.rtk.mcp inconsistent（gap=vm_local→kernel→vm）+ rip.ops.worker-runtime not_applicable；每份 reason 引用真实派生来源（origin/钉值/lock/network 值） |
| 6 | 风险档位路由 | 每份可派发 Manifest 按三档 InputRisk 评估：native 文档/媒体/归档工具 low→assign、unknown→inconsistent(target=container)、high→reject(RT3_VM_NOT_READY)；python_worker 工具 low→assign、unknown→inconsistent(target=container)、high→reject；podman_host（nuclei）low/unknown→assign、high→reject；零「unknown 仍 assign native」反例 |
| 7 | 缝 5 承接 | workflow 派发链对 venv 声明 Manifest 同样探测拒绝（PREPARING 拒绝落 FAILED + D7 五态信封、零 worker 进程拉起、零系统解释器回退）；未声明 venv 的 16 份现状链路信封/状态机与基线逐字段一致（回归断言）；正式 Manifest 零 diff 前提下该断言用测试 fixture Manifest 承载（RT-2 缝 4 同口径） |
| 8 | 决策只读面 | policy.rs 无 `&mut` 数据库/Registry/Manifest 写入入口（grep 断言模块无 registry 写入符号）；S7 `git diff --stat manifests/tools` 输出为空；决策结果仅落测试断言与批次报告快照 |

## 6. 测试面（收口命令，与 CI 同门）

```bash
# 定向测试（policy 决策面 + 既有 capability/venv 面，单线程防 fixture 竞争）
cargo test --manifest-path /media/emo/code/pensting/research-platform/Cargo.toml --offline -p tool-engine policy -- --test-threads=1
cargo test --manifest-path /media/emo/code/pensting/research-platform/Cargo.toml --offline -p tool-engine capability -- --test-threads=1

# 缝 5（workflow 链探测拒绝 + 回归）
cargo test --manifest-path /media/emo/code/pensting/research-platform/Cargo.toml --offline -p workflow-engine -- --test-threads=1

# 既有面回归（RT-1/RT-2 落点零回归证明）
cargo test --manifest-path /media/emo/code/pensting/research-platform/Cargo.toml --offline -p tool-engine python_worker -- --test-threads=1
cargo test --manifest-path /media/emo/code/pensting/research-platform/Cargo.toml --offline -p tool-engine native_rust -- --test-threads=1

# 静态检查门
cargo clippy -p tool-engine --all-targets -- -D warnings
cargo clippy -p workflow-engine --all-targets -- -D warnings

# 格式门
cargo fmt --all -- --check

# 跨 crate 回归（S7）
cargo test --offline --workspace
```

基线记录（本任务书写作时点 2026-11-21 实测）：

- `cargo test … -p tool-engine policy -- --test-threads=1`：**未跑**（policy.rs 尚不存在；S3 落地后由此命令承载）。S1 步骤须先复跑 capability/python_worker/native_rust 三定向命令确认现状基线绿（RT-1 收口 8/8、RT-2 收口 8/8 判据全绿历史记录在案，本批未复跑全量，如实标注）。
- 144 格枚举为纯内存测试，`--test-threads=1` 是全批统一门（fixture 共享 + 既有容器/池竞争面延续 RT-1/RT-2 纪律），非性能需要。
- Manifest 派生模拟（A8）为 `/tmp` scratch 只读脚本，已清理零残留；集成测试在仓库内直读 `manifests/tools/`，不复制 Manifest 内容入测试（防漂移，以文件系统为准）。
- `git status --short`（research-platform 内）：任务书写作时点干净（外层 pensting 仓的 `M research-platform` 指针为父仓视图，非本仓工作树改动）。

## 7. 红线（八条，违反任一即停批上报）

1. **决策只读不写 Registry/Manifest**：policy.rs 零写入入口（无 DB connection、无 registry 写符号、无文件写路径——唯一文件读取 = 测试面读 Manifest/种子）；决策结果只落调用方内存与批次报告（ADR D10b；NATIVE-FIRST §4.2:126 不建第二事实源）。
2. **RT-4 才做 Manifest 逐项迁移，本批禁改 manifests/**：`manifests/tools/` 零 diff（S1/S7 双点 `git diff --stat manifests/tools` 复核；策略 §9:226「不批量改写」+ §11:250 本轮禁改原文）；`schemas/tool-manifest/v1.json` 与 `crates/tool-registry/` 同禁。
3. **Host Capability 缺失=决策拒绝而非猜测**：capability 五态非 available / stale / venv 探测失败 → Reject（RT3_CAPABILITY_UNAVAILABLE / RT3_VENV_UNAVAILABLE），不沿用旧探测值、不静默换实现、不降级（C §5.3:290 原文；E §3:75）。
4. **宿主零安装**：零系统级安装（无 pacman/pip/cargo install 动作）；探测逻辑零复制（消费 RT-1 Provider 与 RT-2 venv::probe 既有面）；NATIVE-FIRST §12:262「缺失能力只报告不安装」口径。
5. **生产库零触碰 + RTIS 零接触**：`platform*.db`/`platform-dryrun.db` 全程不写不读；`integrations/rtis/` 零 diff。
6. **测试离线 + --test-threads=1**：Rust 测试面一律 `--test-threads=1` + `--offline`；144 格/负例矩阵全内存 fixture；集成测试只读仓内 manifests/ 种子文件，零网络依赖（RT-2 红线 7 同门延续）。
7. **派发链零改动（缝 5 除外）**：dispatch.rs 零 diff；rp-workflow.rs 仅限 :1030-1033 additive venv 接线；未声明 venv 的 Manifest 派发行为逐字节不变（判据 7 回归断言守护）；native_rust.rs/python_worker.rs/podman_host.rs/capability.rs/venv.rs/document-router 零触碰。
8. **fail-closed 禁静默降级**：N1-N10 全部硬拒绝无警告放行路径；VM 未就绪一律 Reject 不降 podman_host/native（E §1:17 原文）；决策词表外输入一律 Reject 不猜默认值；`Cargo.lock` 399 恒定 / tool-engine 零 tokio / 零新依赖（S1/S7 双点复核）。

## 8. 回滚

- 每步独立 commit（S3 决策面 / S4 矩阵测试 / S5 缝 5 / S8 报告各一）；`git revert <批次 commit>` 整体撤销。
- policy.rs 为纯新增模块（lib.rs 一行注册），revert 后零残留；缝 5 接线 revert 后 rp-workflow 回到 `venv_config: None` 现状（RT-2 缝 5 重开，需在 revert 说明登记）。
- 测试 fixture 全内存/临时构造，用后即焚；生产库/RTIS/Podman 对象/宿主软件零操作，无生产回滚面。

## 9. 交付物

1. 代码：`crates/tool-engine/src/policy.rs` 决策面（词表/派生/瀑布/输出 Schema/拒绝码表）+ lib.rs 注册（S3）。
2. 测试：144 格 totality + N1-N10 负例矩阵 + 可解释性断言 + 29 份集成期望表 + 三档风险路由 + 缝 5 探测拒绝（S4/S5），§6 收口命令输出全绿（S6/S7）。
3. 缝 5 承接：rp-workflow venv 接线 + workflow 链 fail-closed 断言（S5；RT-2-ADR 缝 5 解除登记）。
4. 文档：本任务书、[RT-3-ADR-POLICY-RESOLVER.md](RT-3-ADR-POLICY-RESOLVER.md)（本批已落）+ RT-3 批次验收报告（S8）+ docs/README.md 登记。
5. RT-4 前置输出：29 份 Manifest 的 RuntimeDecision 全量快照（reason 链 + gap 清单 + 三档风险评估）= 逐项迁移的机器可判输入。
