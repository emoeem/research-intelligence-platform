# DI-3/RT-4 任务书：运行时迁移整合批（29 份 Manifest 逐项迁移 + PolicyResolver 接入 dispatch 派发决策点 + document-router ffprobe 钉值收口 + di2_media guard 单文件随挂——RT/DI 轨合并批）

- 日期：2026-11-22
- 状态：任务书已定稿（2026-11-22 侦察后落稿；实现批次按本任务书执行，实施走 Agent 主会话路径）
- 依据：[RT-3-BATCH-REPORT.md](RT-3-BATCH-REPORT.md) §5（29 份 RuntimeDecision 快照 = 本批机器可判输入）+ §6（3 处 inconsistent gap 清单转 RT-4 登记）+ §7（缝 5 解除登记）；[RT-3-TASK.md](RT-3-TASK.md) §1 不目标原文（「29 份 Manifest 的逐项迁移与 `runtime.venv` 声明补录」「Resolver 接入 dispatch 派发决策点」两项即本批范围；「podman_vm adapter 实现」维持设计保留面）+ §3.2 不修改面（当时禁改的本批解禁）；[RT-3-ADR-POLICY-RESOLVER.md](RT-3-ADR-POLICY-RESOLVER.md) D9（决策/派发分离——RT-4 接线点 = dispatch.rs:224-296 直读面）+ D5/D6/D7（派生与硬拒矩阵口径）；[RT-1-ADR-NATIVE-ADAPTER.md](RT-1-ADR-NATIVE-ADAPTER.md) :42（binary 必须 `/` 前缀绝对路径）+:61-62（「钉值补录属 RT-4 逐项迁移」原文 + 执行面强制）；[DI-2-BATCH-REPORT.md](DI-2-BATCH-REPORT.md) :269（DI-3 七缝登记——本批只消费钉值/guard 相关缝）+ [DI-2-ADR-CHANNELS.md](DI-2-ADR-CHANNELS.md) :9（DI-3 剩余面归属）；[WEB-SECURITY-WORKBENCH-PLAN.md](../rip3/WEB-SECURITY-WORKBENCH-PLAN.md) :609（WSW-7 RTIS Adapter 接线 = 本批输出消费面）
- 前置：RT-3 已收口（`b53562a6`，policy.rs 决策面 + 缝 5 承接全绿）；MA 轨收官（HEAD `f1be3f4e`）；**SQLite 030 空位在且本批不立项（D5）；PG V0008 空位在且本批不立项**；工作树干净（`git status --porcelain` = 0 行实测）
- 设计裁决：本批特有裁决见 §0 D1-D5（不另立 ADR——迁移判据全部可溯源至 RT-3 快照与 ADR 已冻结词表；如有分歧以 RT-3-ADR/RT-3-BATCH-REPORT 为准并回改本档）
- 现状锚点：§1 A1-A14 均为 2026-11-22 本会话实测行号（HEAD `f1be3f4e`、`git status --porcelain` 干净实测），实施时若行号漂移按函数名重定位并以本档词形为准

---

## 0. 设计裁决（本批特有，ADR 内嵌）

| # | 裁决 | 理由 |
|---|---|---|
| D1 | **29 份逐项迁移表（六组动作）+ `schemas/tool-manifest/v1.json` 零扩面 + yaml 孪生同步随迁**：**A 组 venv 补录 17 份**（16 份 python_worker JSON：rip.doc.parse / rip.evidence.rtk.auth / ledger / lint / mcp / scaffold / rip.kb.eri / evaluate / inventory / keyword_search / rtis / rip.knowledge.catalog.extract / rip.ops.etl_status / rip.ops.worker-runtime / rip.security.extension.audit / rip.text.embed + YAML-only `rip.http.tls_inspect.yaml`）——`runtime.venv` = `{path: "workers/python/.venv", direct_deps: <逐份实测>}`（RT-2 ADR D5 冻结形状：path 必填 working_dir 相对、direct_deps 必填非空字符串数组、interpreter 缺省 bin/python；v1.json:140 python_worker 分支 :154 venv 块 schema 已在位，**零 schema 改动**）；direct_deps = S2 逐份 AST 扫描 worker 脚本三方 import 与 `workers/python/requirements.lock.txt`（20 包实测在位）交集，非空要求如实承载；**B 组准入钉值 2 份**（rip.knowledge.document.fetch / rip.knowledge.source.sync）：`runtime.binary` 裸名 → 构建产物绝对路径（`target/release/rp-knowledge-fetch` / `rp-plugin`，`/` 前缀，RT-1-ADR :42 语义；不落 /usr/bin——红线 5 宿主零安装）+ `runtime.binary_sha256` 补 64hex 实测钉值（cargo build --release 后 sha256sum 实测，禁猜测）+ provenance 构建命令登记；**C 组 network 修正 1 份**（rip.evidence.rtk.mcp，详见 D2）；**D 组 ffprobe 重钉 1 份**（rip.media.probe，详见 D4）；**E 组零动作核对 9 份**（8 份 DI-1/DI-2 native_rust 已带 binary_sha256——archive.extract / archive.list / doc.image.meta / doc.image.ocr / doc.office.to_pdf / doc.pdf.info / doc.pdf.text / media.frames，probe 属 D 组重钉不入 E；+ nuclei podman_host image digest 已钉——S2 逐份复核钉值 64hex 在位即零动作）；**F 组不动 1 份**（rip.ops.worker-runtime not_applicable 元数据条目，登记面照旧——venv 补录仍做：A 组 16 JSON 含它，decide() R7 元数据短路在最前不消费 venv 子事实，登记面一致性补齐）。**v1.json 零 diff**：venv 块（RT-2 S5 已落）、native_rust 分支 binary_sha256/argv_template、runtime.type 枚举含 podman_vm（v1.json:64）全部已在词表——29 份迁移动作全部是**既有字段值变更**，零结构扩展。**yaml 孪生同步随迁**：20 份 YAML 中 19 份为 JSON 完整同构孪生（本会话 python sort_keys 全等实测 YAML_TWIN_IDENTICAL；dispatch manifest_for 对 json+yaml 双扩展名扫描、排序后 json 先命中——孪生漂移即 rp-tool validate/register 双形态分裂面）；A/B/C 组涉及的 18 份孪生机械同步（nuclei.yaml 随 E 组零动作不动） | RT-3-TASK §3.2 当时禁改 manifests/ 与 v1.json 是 RT-3 批内禁令，RT-4 即解禁批（RT-3-ADR D9(b) 原文「Manifest 零改动——本批」的批界）；RT-3-ADR D4 论据原文「16 份 python_worker 的 venv 声明缺位（RT-2 缝 4 留给 RT-4 补录）」；RT-1-ADR :61 原文「钉值补录属 RT-4 逐项迁移」；venv 补录不产生第二事实源——path/direct_deps 是部署缝补齐（RT-2 D8 缝 4），决策仍由 D4 派生函数产出 |
| D2 | **rip.evidence.rtk.mcp 收敛动作 = `network.mode: "vm_local" → "none"`（stdio-only 事实修正），runtime.type 保持 python_worker、permission_level T2 保持**：rtk.mcp 的 MCP server 交互是 **spawn 子进程 stdio 管道**（../toolkit/src/rtk/mcp.py:1-3 文档串原文「最小 MCP stdio 客户端（JSON-RPC 2.0，按行分隔）……任意实现了 MCP stdio 传输的本地 server」）——零 TCP 监听零网络面，`vm_local`（TOOL-CONTRACT-DRAFT.md:106 登记语义「仅 VM 内部互通」）与实际承载形态结构性不符，登记过度保守。修正后 `derive_isolation_requirement(none, low, false) = none` → floor=0 → audited+locked → 目标 python_worker → R7 声明一致 → **assign**，落「可派发」语义且现状派发路径（dispatch.rs build_python_adapter :949 venv 接线后）零漂移。**否决三替代**：①`runtime.type → podman_vm`——podman_vm adapter 不存在（RT-3 设计保留面）+ v1.json podman_vm 分支必填 image（digest 钉定，`validate.rs` 第 4 条拒绝占位 `<digest>`）——无 VM 镜像面可钉 + dispatch.rs `"podman_host" \| "podman_vm"` 同分支走 container_gateway = **容器 runner 承载 kernel 隔离 = N5 不安全降级**；②T2→T1 降权——T 档与隔离派生正交（RT-3-ADR D2 否决项原文「混维即授权判级与运行时选择互相污染」），降权不消 `vm_local→kernel` 派生；③保持 inconsistent 不迁移——违反本批「可派发」语义（迁移完成态 Resolver 接线后 inconsistent 即 POLICY_DENIED 硬拒，rtk.mcp 现役 list 面被阻断）。known_limits 同步更新（vm_local 表述改为 stdio 零网络面事实） | RT-3-BATCH-REPORT §6 条目 3 原文「迁移建议：runtime.type → podman_vm（VM 就绪前不可派发）」是建议非裁决——本批裁决按 D2 落 none 修正：迁移的目标是「声明=决策+实际承载一致」而非把不可承载的登记硬塞给不存在的 adapter；stdio-only 是可实测事实（mcp.py 零 socket/http import，S2 AST 复核入报告） |
| D3 | **Resolver 接线 = dispatch.rs run_tool 挂点单点接线，decide() 在 :253（runtime_type 提取）与 :279（match）之间调用，Reject/Inconsistent/NotApplicable 三态硬拒，assign 按决策值派发**：①**输入构造**（dispatch.rs 新增私有 `fn resolver_decision_input(manifest) -> DecisionInput`，与 build_python_adapter 同层）：declared_runtime/trust（`derive_trust(origin, license, 钉值在位)`）/complexity（`derive_dependency_complexity`）/isolation（`derive_isolation_requirement`）全部从 manifest 直读派生（tests/policy_resolver_manifests.rs:87 extract_facts 形态搬运）；capability_ref 在场 → `StaticCatalogProvider::load(manifests/capabilities/host-catalog-seed.json)`（rp-mcp.rs:192 同路径，进程级 OnceLock 装配一次）+ `CapabilityProvider::get`（freshness 非 fresh 即 N7 拒——C §5.3:290 原文）；**input_risk = trusted_low_risk 固定基线档**（RT-3 §5 快照 29 行同口径；E §3:62 路由图 input-risk classification 归 Router 侧，DI-1/DI-2 通道内已在 Router 承载，run_tool 通用面分类器接入登记后续批）；venv_declared=true → `venv::probe` 真探测（五态非 available → RT3_VENV_UNAVAILABLE 拒，fail-closed 先于 adapter ensure_ready 双层纵深，省 worker 进程拉起）；egress_wired=true（RT-3-ADR D5 论据「现状形态三工具承载」同口径）/ vm_ready=false（podman_vm 设计保留面）。②**派发语义**：verdict=reject → 受理拒绝 err_payload（code/category 沿 DecisionInput.reject 九类词表，submit 前拒绝不落 run 行——BudgetDenied 受理拒绝同型）；verdict=inconsistent → POLICY_DENIED 拒 + gap 清单随 payload 回显（迁移完成态 28 assign 下 unreachable，负例 fixture 承载——未来新登记声明↔决策不一致即拒，防绕过）；verdict=not_applicable → VALIDATION 拒（元数据条目不派发面——现状 worker-runtime 经 run_tool 可派发，接线后拒绝即 known_limits「登记即置 DISABLED」语义兑现，行为变化显性记账）；verdict=assign → 按 `decision.runtime` 分支派发（迁移完成态决策值=声明值 → 三分支行为零漂移，S4 断言）。③**对 RT-3-ADR D9(d) 的修订**：D9(d) 原文「落地后才允许派发链消费 Resolver（决策覆盖声明 = RT-4 之后的批）」——本批合并执行（迁移+接线同批），修订前提 = 迁移后 29 份 decide 全 assign 实证在先（S2 门）+ 接线后分支决策值=声明值零漂移（S4 逐份断言）——决策覆盖声明与迁移同批落地即消除「迁移后窗口期声明与决策可分叉」的旁路缝；接线面**单点 = mcp-server dispatch run_tool**，agent-engine tool_link（submit_for_caller caller 通道 :137-153）与 rp-workflow resolve_python_worker（:1066，缝 5 已接）组装点零再动（后续批登记） | RT-3-TASK §1 原文「Resolver 接入 dispatch 派发决策点（RT-4 落 Manifest 后的批）」——本批 = 该批；单点接线 = RT-3-ADR D9 否决项「落 mcp-server dispatch 内联（决策要被 workflow-engine 与 RT-4 消费，dispatch 私有函数不可复用）」的本批修正版：决策函数本体已在 tool-engine policy.rs（可复用面成立），dispatch 挂点只做装配与闸门不做决策；拒绝前置省 worker 进程拉起 = RT-2 e2e 判据 8 同款语义 |
| D4 | **ffprobe 钉值漂移收口 = 重钉三处一体 + 媒体四测 guard 随挂，否决 guard-only**：事实 = 本机 ffprobe 已从 9.0.2-3.46（RT-0.5-A 2026-11-18 实测钉值 `a2ae3d0209aec739…`）滚动升级至 **9.1**（本会话 `ffprobe -version` + `sha256sum /usr/bin/ffprobe` 实测 `f4e2aff7625377acff9642f614ff17888e598b57060d5e72ac5720631ae18776`）——宿主已不是钉值宿主，manifests/tools/rip.media.probe.json runtime.binary_sha256 + manifests/capabilities/host-catalog-seed.json media.ffprobe binary_identity（sha256_prefix/sha256_full）双处钉值均失真。收口 = ①重钉三处一体（manifest binary_sha256 + manifest provenance.dependencies 版本行「ffmpeg-full 9.x sha256_prefix …」+ seed binary_identity.sha256_prefix/sha256_full/version.normalized/version.raw/package_provenance.pacman.version/probe_timestamp——RT-0.5-A 重探流程同款，L0 sha256sum+version 实测，L1/L2 关键面按需）②crates/document-router/tests/three_channel_e2e.rs 媒体四测（:1645 tone_wav / :1714 video_frames / :1829 duration cap / :2046 malformed——di2_media×4 基线红全清单，:1898 pixel_budget 是 stub harness 零真依赖不挂）各前置 `host_binary_pin_guard(test, "/usr/bin/ffprobe", "<新钉值前缀 16hex>")`——helper :130 **零新增**（2f25787f 先例形状：先例两归档测各 +7 行，本批四测同款各 +7 行调用），异机 CI runner ffprobe ≠ 本机钉值 → SKIP（fail-closed 拒绝行为本身即身份校验语义正确性实证）。**否决**：①guard-only——本机自身也漂移，SKIP 语义使媒体实跑面在登记宿主上永久失效（2f25787f 的 SKIP 前提是「本机=钉值宿主真跑、异机 SKIP」，本机漂移后前提不成立）；②钉值放宽/删除——RT-1 ADR D3 fail-closed（untrusted binary 拒绝）语义不动，钉值=准入身份锚，环境更新→重新实测→重新钉值（RT-0.5-A 流程）而非放松校验；③回退 ffmpeg 版本——红线 5 宿主零安装 | MA-3-BATCH-REPORT:77/:141/:157 与 MA-4-BATCH-REPORT:5/:48/:59/:164 双批将 di2_media×4 登记为「唯一预期红=ffprobe 环境漂移」放行——收口义务顺延至本批（环境漂移已坐实为宿主升级非测试缺陷）；catalog seed stale 判定面（capability.rs:1316/:1346 sha256 prefix mismatch → stale）使重钉缺一不可：只改 manifest 不改 seed，capability 闸与 adapter 闸两处事实源分叉 |
| D5 | **030 不立项 + 迁移后判据口径 = 28 assign + 1 not_applicable（inconsistent 词表语义保留、快照零成员）**：①**030/V0008 不立项**——venv 补录/钉值更新/绝对路径/network 修正全部是 Manifest 内容改动（`git ls-files manifests/` = 48 文件全 tracked 实测，普通 git diff 承载）；029:121 kind 词表冻结声明（「扩词值须升 030+」）本批不扩 kind 词表；MA-4-TASK A15 十六硬钉 29 钉位**零随更**（S1/S7 双点复核）。②**判据 2 口径**：迁移完成后 Resolver decide 29 份 = **28 assign + 1 not_applicable**（3 处 inconsistent 全部收敛）；verdict 四态词表不减——144 格矩阵与 N4/N6/N9「声明冲突语境 Inconsistent」机制由负例矩阵继续承载，inconsistent 在 29 份快照中零成员 = 迁移完成态实证而非词表裁撤；`rt3_snapshot_dump_for_batch_report`（tests/policy_resolver_manifests.rs:550）重录快照 29 行入本批报告（= 下一批与 WSW-7 的机器可判输入）。③**依赖面零动**：Cargo.lock 399 恒定（实测）+ vendor/ 零 diff（.cargo/config.toml vendored-sources 替换在位，本批纯迁移接线零新依赖 = 零 vendor 候选）+ reqwest=0 + tool-engine 零 tokio | 030 立项判据 = 库 schema 结构变更；本批零 schema 变更；A15 十六硬钉是迁移链水位非 Manifest 计数（MA-4-TASK A15 两假阳性排除声明原文「policy_resolver_manifests.rs 的 29 份派发面 = manifest 计数巧合非迁移钉」——本批 Manifest 改 38 文件不动迁移钉）；inconsistent 语义保留 = RT-3-ADR D6/D7 矩阵 totality 的结构要求（「Assign/Inconsistent/Reject 合并成单一布尔」为 D6 否决项） |

## 1. 现状锚点（2026-11-22 本会话实测，A1-A14）

| 锚点 | 事实 | 出处 |
|---|---|---|
| A1 RT-3 决策面 | policy.rs：`TrustLevel` :56 / `Target` :170 / `DecisionInput` :358（venv_declared/venv_status/egress_wired :390/vm_ready/binary_path/binary_sha256 全字段）/ `derive_target` :1278 / `decide` :1310（R7 元数据短路在 R0-R5 之前）；`DecisionInput::base` 缺省 egress_wired=true、vm_ready=false（:424 起）；29 份快照 = 25 assign + 3 inconsistent（document.fetch/source.sync 准入缺口 + rtk.mcp vm_local→kernel）+ 1 not_applicable | crates/tool-engine/src/policy.rs:56,170,358,390,424,1278,1310；RT-3-BATCH-REPORT §5 |
| A2 dispatch 直读面 | run_tool :208；runtime.type 提取 :253-258（`.unwrap_or("")`）；match :279 起：python_worker 分支（build_python_adapter）→ podman_host\|podman_vm 同分支 container_gateway（WaitingForRunner 非终态返回）→ native_rust 分支（RouterManifests::from_dirs + **DI-1/DI-2 通道表 contains 校验**——表外 native_rust 工具 VALIDATION 拒绝，dispatch.rs 注释原文「dedicated dispatch comes with its own batch」）；build_python_adapter venv 接线 :949（`match runtime.get("venv")`）；run_subtask :1194/:1199（spawn rp-agent，不经 runtime.type 派发） | crates/mcp-server/src/dispatch.rs:208,253-258,279-296,297-341,949,1194,1199 |
| A3 Resolver 输入在测试面的装配先例 | tests/policy_resolver_manifests.rs：extract_facts :87（venv_declared 提取 :161 `runtime.get("venv")`）；to_input :127（`d.venv_declared = facts.venv_declared` 但 **venv_status 从未设置**——base() 缺省 None；当前 16 份零 venv 声明 → R1 闸不触发 → assign）；expectation table :250（29 份逐份）；三档路由 :359；快照 dump :550（RT3_SNAPSHOT_DUMP=1 门）——**venv 补录后不改造此测试即 25 assign 全红**（venv_declared=true × venv_status=None → RT3_VENV_UNAVAILABLE） | crates/tool-engine/tests/policy_resolver_manifests.rs:87,127,161,250,359,550 |
| A4 v1.json 词表现状 | runtime.type 枚举八值含 podman_vm（:64）；allOf 分支：podman_vm/podman_host 必填 image+entrypoint；python_worker 必填 protocol_version/worker_name/command + **venv 可选（:154，RT-2 S5 落地：path 必填禁 ..、direct_deps 必填非空、interpreter 相对性 pattern）**；native_rust 必填 binary + binary_sha256/binary_target_sha256/subcommands/argv_template/result_mode/capability_ref 可选 | schemas/tool-manifest/v1.json:64,71,105,140,154 |
| A5 Manifest 文件面 | manifests/tools/ = 48 文件 = 28 JSON（16 python_worker + 11 native_rust + 1 podman_host）+ 20 YAML（19 份 JSON 同构孪生 + 1 YAML-only rip.http.tls_inspect.yaml）；`git ls-files manifests/` 48 全 tracked；`git status --porcelain manifests/ schemas/ migrations/` 空；YAML 孪生与 JSON sort_keys 全等（本会话 python 实测）；ri.knowledge.document.fetch runtime = `{type: native_rust, binary: "rp-knowledge-fetch", subcommand: fetch}`（无 binary_sha256）；ri.knowledge.source.sync runtime = `{binary: "rp-plugin", subcommand: sync}`；ri.evidence.rtk.mcp network.mode=vm_local + T2 + worker_ping probe | ls + git 实测；manifests/tools/rip.knowledge.{document.fetch,source.sync}.json runtime 块；rip.evidence.rtk.mcp.json |
| A6 ffprobe 漂移 | 本机 ffprobe 9.1（`ffprobe -version` 实测）；`sha256sum /usr/bin/ffprobe` = `f4e2aff7625377acff9642f614ff17888e598b57060d5e72ac5720631ae18776`；钉值 `a2ae3d0209aec73992d0c9f8baadce19f4929c276bb58a7305e5beb3ec38eff1`（= 9.0.2-3.46，RT-0.5-A 2026-11-18）双处登记：rip.media.probe.json runtime.binary_sha256 + host-catalog-seed.json media.ffprobe binary_identity.sha256_full（version.normalized=9.0.2、pacman ffmpeg-full 9.0.2-3.46、probe_timestamp 2026-11-18）；capability.rs stale 判定 :1316/:1346（measured 前缀不符 → entry marked stale） | sha256sum + ffprobe -version 实测；manifests/tools/rip.media.probe.json；manifests/capabilities/host-catalog-seed.json；crates/tool-engine/src/capability.rs:1316,1346 |
| A7 di2_media×4 基线红 | three_channel_e2e.rs 四测 :1645 tone_wav / :1714 video_frames / :1829 duration cap / :2046 malformed 全部实跑真 ffprobe（adapter 身份校验实测 sha 对照钉值 → NATIVE_IDENTITY_MISMATCH）；:1898 pixel_budget 是 stub harness（「真 ffprobe/ffmpeg 零依赖——异机无 ffprobe 亦应跑」注释原文）；归档两测 guard 调用点 :2130/:2311（2f25787f 先例：helper :130 host_binary_pin_guard 20 行 + 两测各 +7 行）；基线红双批登记：MA-3-BATCH-REPORT:77/:141/:157 + MA-4-BATCH-REPORT:5/:48/:59/:164（「22 passed; 4 failed」S1 干净 HEAD 同败实证） | crates/document-router/tests/three_channel_e2e.rs:130,1645,1714,1829,1898,2046,2130,2311；git show 2f25787f --stat（34 insertions 单文件）；MA-3/MA-4 批次报告 |
| A8 CI 纪律面 | ci.yml:101 原文「**CI 不装依赖、不跑 uv、不碰 venv**（RT-2-ADR D8.2）」——CI runner 无 workers/python/.venv（setup-python 3.12 + lock 完整性 unittest）；CI 上 venv 声明 manifest 派发 = RT3_VENV_UNAVAILABLE 拒属 fail-closed 正确行为；e2e 正例必须 fixture 临时 venv 承载（seam5_venv_wiring.rs 先例：fixture_dir + 临时 manifest `rip.t.seam5.venv` :116/:134）；**run_tool_e2e.rs:335/:414 消费正式 manifests/tools/rip.doc.parse.json**——venv 补录后 CI 无 .venv 即红，S4 必须 fixture 化 | .github/workflows/ci.yml:85-108；crates/workflow-engine/tests/seam5_venv_wiring.rs:9,116,130,134；crates/mcp-server/tests/run_tool_e2e.rs:66,180,335,414 |
| A9 venv 与 lock 面 | workers/python/.venv 在位（本会话 ls 实测）；requirements.lock.txt 20 包（anyio/click/numpy/onnxruntime/tokenizers/huggingface-hub/pyyaml 等）；RT-2 缝 5 已接：rp-workflow.rs:1066 resolve_python_worker_value + :1109 venv_config 读取（dispatch.rs:949 同款）；RT-2 fingerprint 复算先例 `bc258b320adba163`（venv.rs canonical 公式） | ls workers/python/ 实测；grep lock 20 包；crates/workflow-engine/src/bin/rp-workflow.rs:1066,1109 |
| A10 自研 bin 载体 | rp-knowledge-fetch = crates/tool-engine/Cargo.toml:29 `[[bin]] name`；rp-plugin = crates/plugin-runtime（bin 载体，target/debug/rp-plugin 实测在位）——2 份 B 组 Manifest 的绝对路径 = target/release 构建产物路径（`/` 前缀 + sha256 实测钉值 + provenance 构建命令登记；换机重钉 = RT-1 12258e86 host_pin_guard 先例同款宿主绑定语义） | crates/tool-engine/Cargo.toml:29；crates/plugin-runtime/Cargo.toml；ls target/debug/ |
| A11 链尾硬钉 29 | MA-4-TASK.md:43 A15 十六文件 29 钉位全清单（migration_idempotent.rs:87 / migrations_agent_data_plane.rs:83,100,109 / backup_restore.rs:51 / rp-wsw persist.rs:895-899 等十六处）；029 = MA-3（86ea8530）入链，030 空位；029:121 kind 词表冻结声明「MA-4 扩词值须升 030+」；本批零迁移零库结构改动 → 全清单零随更 | docs/rip3/MA-4-TASK.md:43（A15）；migrations/029_collab_protocol.sql:113-122；ls migrations/030* 反证 |
| A12 DI-3 缝全景 | DI-2-BATCH-REPORT:269 七缝 = ①Rar/tar 嗅探签名位 ②pdftoppm 渲染 OCR 管线位 ③facade run 审计锚位（dispatch.rs:331/:481 注释「审计锚升级属 DI-3」）④统一输出外层信封化位 ⑤媒体域扩展缝 ⑥内存域缝（systemd-run MemoryMax，DI-2-BATCH-REPORT:12/:253 RLIMIT_AS 库映射面）⑦诚实分支正例位；DI-2-ADR:9「DI-3 剩余面 = facade run 审计锚 / exiftool / rip.doc.parse 升级 / 统一输出信封化」——**本批只消费钉值/guard 相关面（D4），七缝主体登记后续批** | docs/phase0/DI-2-BATCH-REPORT.md:12,253,260,269,273；docs/phase0/DI-2-ADR-CHANNELS.md:9；crates/mcp-server/src/dispatch.rs:331,481 |
| A13 caller 通道与 sender 面 | agent loop 工具派发 = tool_link.rs ToolLink::call :137-153（caller_id Some → submit_for_caller :331 tool-engine lib.rs；None → submit_correlated）——adapter 由 cli 侧组装（PythonWorkerDriver 注入，tool_link.rs:59-62「Adapter 配置由调用方按 Manifest runtime 块组装」）；agent_loop.rs:2129 caller 归属注释；**sender 面 = crates/wsw-runtime/src/sender.rs（WSW 轨 HTTP 发送面）——本批零触碰**（跨轨禁改，红线 7） | crates/agent-engine/src/tool_link.rs:59-62,137-153；crates/tool-engine/src/lib.rs:331；crates/agent-engine/src/agent_loop.rs:2129；crates/wsw-runtime/src/sender.rs |
| A14 依赖与 vendor | Cargo.lock 399（grep 实测）+ reqwest=0 + tool-engine 零 tokio；vendor/ 在位 + .cargo/config.toml `replace-with = "vendored-sources"`（--offline 全靠 vendor）——本批纯迁移接线零新依赖 = 零 vendor 候选，vendor 面零 diff | grep -c Cargo.lock；.cargo/config.toml；crates/tool-engine/Cargo.toml deps |

## 2. 范围

### 2.1 允许改动（落点清单）

| 落点 | 类型 | 内容 | 步骤 |
|---|---|---|---|
| `manifests/tools/` 16 份 python_worker JSON | 修改 | D1 A 组：runtime.venv 块补录（path=workers/python/.venv + direct_deps 逐份 AST 实测）；含 rip.ops.worker-runtime（登记面一致性；决策面 not_applicable 短路不消费）与 rip.evidence.rtk.mcp（叠加 C 组） | S2a |
| `manifests/tools/rip.http.tls_inspect.yaml` | 修改 | D1 A 组：YAML-only 派发面 Manifest venv 补录（YAML 形态直改） | S2a |
| `manifests/tools/rip.knowledge.document.fetch.json` / `rip.knowledge.source.sync.json` | 修改 | D1 B 组：binary 绝对路径 + binary_sha256 64hex 实测钉值 + provenance 构建命令/版本登记 | S2b |
| `manifests/tools/rip.evidence.rtk.mcp.json` | 修改 | D2 C 组：network.mode vm_local→none + known_limits stdio 事实更新 | S2c |
| `manifests/tools/rip.media.probe.json` | 修改 | D4：binary_sha256 重钉 + provenance.dependencies 版本行更新（9.1 + 新 sha256_prefix） | S2d |
| `manifests/capabilities/host-catalog-seed.json` | 修改 | D4：media.ffprobe binary_identity（sha256_prefix/sha256_full/version.normalized/version.raw/package_provenance/probe_timestamp）+ media.ffmpeg 同包联动面复核（version_coupled constraint 不变，二进制 sha 若同包升级一并实测重钉） | S2d |
| 19 份 YAML 孪生中 18 份 | 修改 | D1：A/B/C 组 JSON 改动的孪生机械同步（yaml.safe_load→改→dump 保键序；nuclei.yaml 零动作不动） | S2e |
| `crates/mcp-server/src/dispatch.rs` | 修改 | D3：run_tool 挂点 `resolver_decision_input` 装配 + decide() 调用 + reject/inconsistent/not_applicable 三态拒绝面（:253-:279 之间；StaticCatalogProvider 进程级 OnceLock 装配；build_python_adapter/build_native_adapter 分支本体零改动） | S4 |
| `crates/tool-engine/tests/policy_resolver_manifests.rs` | 修改 | A3：venv 子事实构造（venv_declared=true → venv_status 静态合成 Available——与 egress_wired=true 注入同口径，真探测面由 RT-2 python_worker_venv_e2e 既有承载）+ 期望表 28 assign + 1 not_applicable 改造 + ADMISSION_GAP_TOOLS/VM_LOCAL_TOOL 常量撤除 + 三档路由断言随 28 份新基线更新 + 快照 dump 重录 | S3 |
| `crates/document-router/tests/three_channel_e2e.rs` | 修改 | D4：媒体四测各 +7 行 host_binary_pin_guard 调用（新 ffprobe 钉值前缀；helper :130 零新增）——**src 零 diff 单文件随挂** | S3 |
| `crates/mcp-server/tests/run_tool_e2e.rs` | 修改 | A8：:335/:414 正式 rip.doc.parse.json 消费点 fixture 化（seam5_venv_wiring 先例同款：临时 manifest + 临时 venv 构造正例 / 缺省 venv 拒绝负例；正式 manifest 零漂移断言保留） | S4 |
| `crates/mcp-server/tests/dispatch_policy_gate.rs` | 新建 | D3 判据 3 负例矩阵 e2e：词表外 runtime.type → RT3_RUNTIME_VOCAB / venv 声明缺省根 → RT3_VENV_UNAVAILABLE（零 worker 拉起）/ not_applicable（worker-runtime 正式 manifest）→ 拒 / 合成 inconsistent fixture（声明↔目标冲突）→ POLICY_DENIED + gap 回显 / podman_vm 声明 → RT3_VM_NOT_READY；V-9 守卫预挂（bin/seed 缺 → SKIP） | S4 |
| `apps/server/selftest_rt4.sh` | 新建 | V-9 守卫预挂收口脚本：rp-tool validate 29 份全过 / Resolver 快照重录比对 / dispatch 拒绝面 CLI 实跑 / 29 份派发面计数与 venv 声明计数回显 / 生产库指纹反证（V-9：bin/seed/载具缺 → SKIP 计数 + exit 0，0f2922cb 教训原文口径） | S5 |
| `docs/phase0/DI-3-RT-4-BATCH-REPORT.md` + `docs/README.md` | 新增/additive | 批次报告（迁移逐项表 29 行 + 命令原文 + 判据对照 + 快照 29 行重录 + 坏消息单列 + WSW-7 前置输出登记节）+ 索引登记 | S7 |

### 2.2 不修改

- `schemas/tool-manifest/v1.json` 与 `crates/tool-registry/` 零 diff（D1：venv/binary_sha256/argv_template/podman_vm 词表全部已在；九条校验面零变更）。
- `migrations/` 全部 001-029 零 diff；**030/V0008 不立项**（D5）；A15 十六硬钉 29 钉值零随更。
- `crates/tool-engine/src/policy.rs` 纯函数面零 diff（D3：决策逻辑零改动，接线只做装配——lib.rs/派生函数/decide/144 格矩阵全不动）；`adapter/{native_rust,python_worker,podman_host,egress_proxy,structured_output}.rs`、`capability.rs`、`venv.rs`、`envelope.rs` 零触碰（RT-1/RT-2 落点）。
- `crates/document-router/src/` 零 diff（DI-1/DI-2 落点；测试单文件 guard 随挂除外）；`crates/wsw-runtime/`（含 sender.rs）零 diff（WSW 轨跨轨禁改）；`crates/workflow-engine/` 零 diff（缝 5 面零再动）；`crates/agent-engine/` 零 diff（tool_link caller 通道接线登记后续批）。
- podman_vm adapter 零实现（RT-3-TASK §1 设计保留面原文；D2 否决项 ①）；run_tool native_rust 通道表零扩展（A2：「dedicated dispatch comes with its own batch」——B 组两工具迁移动作止于「Resolver assign + adapter 准入面齐备」，通道表扩展登记后续批）。
- DI-3 七缝主体（facade run 审计锚 / 统一输出信封化 / UI 接入 / 内存域 systemd-run / pdftoppm 渲染 OCR / Rar 嗅探 / exiftool / rip.doc.parse 升级）零实现——做了即越界（A12）。
- `Cargo.lock`、workspace 根 `Cargo.toml`、`vendor/` 零 diff（D5：399 恒定 + 零新依赖）；`workers/python/`（requirements.in/lock.txt/.venv 本体）零 diff（只读探测）。
- `migrations/platform*.db` 五库与 `platform-dryrun.db` 零触碰（只读指纹反证）；`integrations/rtis/**` 零 diff（红线 2）；`web/**` 零触碰。
- RT-3 已收口落点（policy.rs 测试矩阵 / seam5_venv_wiring）除本批明列改造（policy_resolver_manifests.rs 期望表）外零改动。

## 3. 实施步骤（S1→S7 串行，每步过门再进）

1. **S1 门禁基线核验**：`grep -cE '^\[\[package\]\]' Cargo.lock` = 399 + `git status --porcelain` 干净 + HEAD = `f1be3f4e`；`CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace --no-fail-fast` **现状全量一次基线**（预期唯一红 = di2_media×4 环境面，MA-4 判据 11 同水位；红项逐一反证登记）；A1-A14 锚点函数名重定位复核；workers/python/.venv probe 现状实测（venv 五态 available 确认）；run_tool_e2e :335/:414 与 mcp_protocol.rs 的 manifest 消费面逐点实勘；ffprobe 重钉原料实测（sha256sum + version + pacman -Qi ffmpeg-full 三命令原文入报告）。
2. **S2 Manifest 迁移面**：S2a 17 份 venv 补录（逐份 `python3 -I -c` AST/import 扫描 worker 脚本三方 import 与 lock 交集定 direct_deps；venv 块按 v1.json:154 形状；YAML-only 直改）；S2b 2 份准入钉值（`CARGO_TARGET_DIR=/home/emo/rt3-target cargo build --offline --release -p tool-engine -p plugin-runtime --bins` + sha256sum 实测 + 绝对路径落 runtime.binary + provenance 登记；禁猜测钉值）；S2c rtk.mcp network none（rtk/mcp.py AST 复核零 socket/http import 原文入报告 + known_limits 更新）；S2d ffprobe 重钉三处一体（D4 清单）+ media.ffmpeg 同包联动复核；S2e 18 份孪生同步；**门 = `rp-tool validate --manifest` 29 份逐份 exit 0 + `git diff --stat manifests/ schemas/` 落点核对（预期 38 文件 + seed，v1.json 零 diff）**。
3. **S3 测试面改造**：policy_resolver_manifests.rs venv 子事实 + 期望表 28 assign + 1 not_applicable + 快照重录（RT3_SNAPSHOT_DUMP=1 实跑 29 行入报告）；three_channel_e2e.rs 媒体四测 guard 随挂；**门 = `cargo test --offline -p tool-engine policy -- --test-threads=1` 全绿 + `-p document-router --test three_channel_e2e` 26/26 全绿（di2_media×4 基线红消除实测）**。
4. **S4 Resolver 接线**：dispatch.rs 挂点装配 + 三态拒绝面（D3 ①②）；run_tool_e2e fixture 化（A8）；dispatch_policy_gate.rs 负例矩阵新建（D3 判据 3 五负例）；**门 = mcp-server 全套 `--test-threads=1` 绿（run_tool_e2e/native_rust_dispatch/mcp_protocol/subtask_dispatch/新 gate）+ 25→28 份原 assign 工具派发行为面与迁移前快照逐字段一致断言（fixture venv available 路径）**。
5. **S5 selftest_rt4.sh**：V-9 守卫预挂全节（D1 落点清单）；门 = 全节 PASS/SKIP 分开计数 exit 0 + 既有 selftest 七套零改动回归（auth/ma2/ma3/phase6/phase7_agents/phase8_workflow/phase9_mcp）。
6. **S6 全量回归收口门**：§5 全部命令实跑（含 workspace 全量一次——**预期零基线红**：di2_media×4 已收口）；`git status --porcelain` 只含 §2.1 落点核对（crates/ 落点 = dispatch.rs + 三测试文件；Cargo.lock/vendor/v1.json/migrations 零 diff 实证；A15 十六硬钉 =29 复核）。
7. **S7 批次收口**：DI-3-RT-4-BATCH-REPORT.md 落盘（迁移逐项表 29 行 + 命令原文 + 判据对照 + 快照重录 + 坏消息单列 + WSW-7 前置输出登记节）+ docs/README.md 登记（显式 add 本批清单，禁 add -A）。

## 4. 完成标准（判据表；验收 = 命令实跑回显）

| # | 判据 | 验收 |
|---|---|---|
| 1 | **29 份迁移后 Resolver decide = 28 assign + 1 not_applicable** | policy_resolver_manifests 期望表逐份 verdict/runtime/cross_check/gap 逐字段断言全绿；ADMISSION_GAP_TOOLS/VM_LOCAL_TOOL 撤除后 inconsistent 零成员；RT3_SNAPSHOT_DUMP=1 快照 29 行重录入报告（每行 verdict=assign/not_applicable） |
| 2 | **迁移后 Manifest 经 v1.json 校验 + 负例面** | `rp-tool validate --manifest` 29 份逐份 exit 0；负例：临时 fixture venv 块 path 缺/direct_deps 空/interpreter 绝对路径/`..` → validate 拒（RT-2 S5 既有负例矩阵词形复证，零新校验代码） |
| 3 | **dispatch 派发 e2e 拒绝面（负例为硬性红线）** | dispatch_policy_gate.rs 五负例全绿：①词表外 runtime.type → RT3_RUNTIME_VOCAB（VALIDATION）；②fixture venv 声明指缺省根 → RT3_VENV_UNAVAILABLE（DEPENDENCY）且零 worker 进程拉起（探测拒绝先于 spawn）；③rip.ops.worker-runtime 正式 manifest 经 run_tool → VALIDATION 拒（not_applicable 不派发面）；④合成 inconsistent fixture（声明↔目标冲突）→ POLICY_DENIED + gap 清单随 payload 回显；⑤podman_vm 声明 fixture → RT3_VM_NOT_READY（RUNNER_UNAVAILABLE，零 gateway 派发零降级） |
| 4 | **接线后原 assign 工具派发行为面零漂移** | 28 份 assign 工具在 venv available 环境（fixture/本机）派发信封与迁移前快照逐字段一致（run_tool_e2e fixture 化后全绿；native 九工具通道表内派发零变化；nuclei podman_host 分支零变化）；决策值=声明值 → 三分支路由与迁移前逐字节同（S4 断言原文） |
| 5 | **venv 补录面** | 17 份 venv 声明 valid（v1.json 形状）+ direct_deps 逐份与 worker 脚本 import 面一致（AST 扫描原文入报告）+ probe available（本机）+ adapter spawn 走 venv 解释器（`-I` 保留，RT-2 D4 三档语义）；CI 环境（无 .venv）下 venv 拒属 fail-closed 正确行为（ci.yml:101 纪律原文不动） |
| 6 | **ffprobe 钉值收口** | 本机媒体四测真实执行全绿（26/26，SKIP=0）；新钉值与实测 sha256sum 一字不差（命令原文入报告）；seed 与 manifest 双处一致；异机 SKIP 语义保留（guard 前缀不符 → println SKIP 理由） |
| 7 | **document-router 零回归** | three_channel_e2e 26/26 + 归档两测既有 guard 语义不变（:2130/:2311 调用零改动）+ OCR 通道既有面零 diff；`cargo test --offline -p document-router -- --test-threads=1` 全绿 |
| 8 | **零迁移零新依赖** | Cargo.lock 399 恒定 + vendor 零 diff + reqwest=0 + `ls migrations/030*` 反证 No such file + A15 十六硬钉 =29 双点复核零随更 + `git diff --stat migrations/ schemas/` 空 |
| 9 | **全量回归** | `cargo test --offline --workspace --no-fail-fast` 全绿（**零基线红**——di2_media×4 收口后 MA-3 以来首次全绿）；既有 selftest 七套零改动通过 |
| 10 | **WSW-7 前置输出面** | 批次报告专节登记：29 份迁移后 RuntimeDecision 快照（rip.kb.rtis / rip.evidence.rtk.* 的 assign 决策 + constraints 词形 + T2 授权引用面）+ RT3_ 拒绝码表 + Resolver 派发拒绝语义（D3 ②）——WSW-7 RTIS Adapter 接线（WEB-SECURITY-WORKBENCH-PLAN:609）的决策面输入 |

## 5. 测试面（收口命令）

```bash
# Rust 全量（2026-11-22 纪律：workspace 一次必须跑；本批收口后预期零基线红）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace --no-fail-fast
# 定向：Resolver 决策面（期望表 28+1 + 快照重录）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline -p tool-engine policy -- --test-threads=1
# 定向：dispatch 接线面（run_tool_e2e fixture 化 + 新 gate 负例矩阵）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline -p mcp-server -- --test-threads=1
# 定向：document-router（媒体四测 guard 随挂 + 收口后 26/26 全绿）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline -p document-router -- --test-threads=1
# 定向：RT-2 既有 venv 面回归（零 diff 复证）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline -p tool-engine venv -- --test-threads=1
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline -p workflow-engine -- --test-threads=1
# 静态门
CARGO_TARGET_DIR=/home/emo/rt3-target cargo clippy --offline -p mcp-server -p tool-engine -p document-router --all-targets -- -D warnings
cargo fmt --all -- --check
# Manifest 校验门（29 份逐份；S2/S6 双点）
for m in manifests/tools/*.json manifests/tools/rip.http.tls_inspect.yaml; do
  CARGO_TARGET_DIR=/home/emo/rt3-target cargo run --offline -p tool-registry --bin rp-tool -- validate --manifest "$m" || echo "FAIL $m"
done   # 期望零 FAIL 行
# 钉值与孪生一致性门（S2/S6 双点）
grep -rn '"binary_sha256"' manifests/tools/rip.media.probe.json | grep -c f4e2aff7625377ac   # 1（重钉在位）
python3 -c "
import json,yaml,glob
bad=[]
for p in sorted(glob.glob('manifests/tools/*.json')):
    y=p[:-5]+'.yaml'
    import os
    if os.path.exists(y):
        a=json.dumps(json.load(open(p)),sort_keys=True,ensure_ascii=False)
        b=json.dumps(yaml.safe_load(open(y)),sort_keys=True,ensure_ascii=False)
        if a!=b: bad.append(p)
print('YAML_TWIN_DRIFT',bad)"   # 期望空列表
# 迁移水位反证（030 不立项 + A15 十六硬钉 29）
ls migrations/030* 2>&1 | grep -c "No such file"   # 1
grep -c "Some(29)" crates/database/tests/backup_restore.rs   # 1（与 S1 基线一致）
grep -cE '^\[\[package\]\]' Cargo.lock   # 399
# 既有 selftest 回归（七套零改动）
bash apps/server/selftest_auth.sh && bash apps/server/selftest_ma2.sh && bash apps/server/selftest_ma3.sh
bash apps/server/selftest_phase6.sh    # SQL 写静态断言既有红口径不变
bash apps/server/selftest_phase7_agents.sh && bash apps/server/selftest_phase8_workflow.sh && bash apps/server/selftest_phase9_mcp.sh
# 本批新增（V-9 守卫预挂；PASS/SKIP 分开计数）
bash apps/server/selftest_rt4.sh
```

基线记录（本任务书写作时点 2026-11-22 实测）：

- workspace 全量基线：唯一红 = di2_media×4（MA-3/MA-4 双批登记环境面；本批 S3 收口后判据 9 预期零红——收口效果即本批验收的一部分）。
- `policy_resolver_manifests.rs` 现状 25 assign 断言绿（venv_declared 全 false）；S2a 落地后立即红（A3）——S2a 与 S3 同 commit 纪律（迁移与测试面改造不拆批，防中间态红窗口）。
- Manifest 派生模拟零 scratch 脚本残留（S2a direct_deps 扫描用 `python3 -I` 一次性命令原文入报告，不落文件）。

## 6. 红线（八条，违反任一即停批上报）

1. **生产库零触碰 + RTIS 零接触**：`migrations/platform*.db` 五库与 `platform-dryrun.db` 全程不写不读（只读指纹反证 1/1/1/0/0 进报告）；`integrations/rtis/**` 零 diff；web/** 零触碰。
2. **决策只读不写**：Resolver 接线面零写入入口（不写 Registry/Manifest/数据库；decide() 输出只进派发闸门内存态）——policy.rs 纯函数面零 diff 结构性成立（D3；RT-3-ADR D10b 延续）；迁移写面唯一 = manifests/ 声明文件（git diff 承载，非运行时写入）。
3. **fail-closed 禁静默降级**：Reject 不降级（RT3_VM_NOT_READY 无任何 podman_host/native 降级目标）；inconsistent/not_applicable 派发面一律硬拒不警告放行；venv 探测非 available 硬拒零系统解释器回退（RT-2 D4 三档语义）；capability 非 fresh 拒（C §5.3:290）；钉值不匹配 = untrusted binary 拒（RT-1 D3 原文，D4 收口不放松校验）。
4. **钉值如实 + 宿主零安装**：B/D 组钉值全部来自本机实测 sha256sum 原文（禁猜测、禁沿用旧值、禁从 lock/镜像 digest 推算）；绝对路径 = 构建产物 `/` 前缀路径（不落 /usr/bin、零 pacman/pip/cargo install 动作——RT-3-TASK 红线 4 同门）；9 份 E 组既有钉值零随更（非本机漂移证据不重钉）。
5. **历史迁移禁改 + 030 不立项**：migrations/001-029 零 diff；030/V0008 空位保持（D5：venv 补录/钉值更新 = Manifest 内容改动非库结构迁移；029:121 kind 词表冻结不扩）；A15 十六硬钉 29 零随更（S1/S6 双点复核）。
6. **零新依赖**：Cargo.lock 399 恒定 + vendor/ 零 diff（vendored-sources 在位，本批零 vendor 候选）+ tool-engine 零 tokio + reqwest=0（S1/S6 双点）。
7. **落点与跨轨零 diff**：sender 面（crates/wsw-runtime/，含 sender.rs）零 diff；sender/podman_vm adapter/document-router src/workflow-engine/agent-engine/v1.json/tool-registry 零触碰（§2.2 全清单）；run_tool native_rust 通道表零扩展；DI-3 七缝主体做了即越界（A12）；rp-tool validate 只读消费不落库。
8. **测试离线 + --test-threads=1 + 全量一次**：Rust 测试一律 `--offline --test-threads=1` + `CARGO_TARGET_DIR=/home/emo/rt3-target`；workspace 全量一次收口门（判据 9）；e2e 正例 fixture 临时 venv 承载（CI 不碰 venv 纪律 ci.yml:101 不动）；V-9 守卫预挂（bin/seed/载具缺 → SKIP 计数 + exit 0，0f2922cb 教训口径）；SKIP 与 PASS 分开计数不混报。

## 7. 回滚

- 单 commit 交付（S2 迁移 + S3 测试改造 + S4 接线同批 commit 纪律见 §5 基线注；selftest/报告可分 commit）；`git revert` 整批撤销。
- Manifest 29 份 + 18 孪生 + seed 随 revert 回到 RT-3 快照态（3 处 inconsistent 重开——revert 说明须登记 RT-3-BATCH-REPORT §6 gap 清单复效）。
- dispatch.rs 接线随 revert 消失，回 runtime.type 直读现状（RT-3-ADR D9 现状复原，决策/派发分离重启——需在 revert 说明登记）。
- 测试面改造（期望表/guard/e2e fixture 化/dispatch_policy_gate.rs/selftest_rt4.sh）随 revert 零残留；di2_media×4 基线红随 revert 复效（如实登记）。
- 零迁移零库残留（revert 后无 DROP/清理步骤）；生产库/RTIS/宿主软件零操作无生产回滚面。

## 8. 交付物

1. **迁移面**：29 份派发面 Manifest 逐项迁移（17 venv + 2 准入钉值 + 1 network 修正 + 1 ffprobe 重钉 + 9 零动作核对；rtk.mcp 与 worker-runtime 为 A 组成员——前者的 C 组动作叠加、后者的 F 组不动仅指决策面）+ 18 份孪生同步 + capability seed 重钉——`git diff --stat` 38 Manifest 文件 + 1 seed 实证；v1.json 零 diff。
2. **接线面**：dispatch.rs run_tool 挂点 Resolver 接线（D3：装配 + decide + 三态拒绝面）——决策函数本体零改动（policy.rs 零 diff），派发链从直读升级为决策驱动。
3. **测试面**：policy_resolver_manifests 期望表 28 assign + 1 not_applicable + 快照重录 + three_channel_e2e 媒体四测 guard 随挂 + run_tool_e2e fixture 化 + dispatch_policy_gate.rs 五负例矩阵 + selftest_rt4.sh（V-9 预挂）。
4. **文档面**：docs/phase0/DI-3-RT-4-BATCH-REPORT.md（迁移逐项表 29 行 + 命令原文 + 判据对照 + 快照 29 行重录 + 坏消息单列 + WSW-7 前置输出登记节）+ docs/README.md 登记。
5. **WSW-7 前置输出登记**（批次报告专节）：①29 份迁移后 RuntimeDecision 快照（WSW-7 RTIS Adapter 接线 = WEB-SECURITY-WORKBENCH-PLAN:609「Tool / Plugin Registry 接入、Nuclei / ZAP / RTIS Adapter」的决策面输入——rtis/rtk 系工具 assign 决策 + constraints 词形 + T2 authorization_ref 授权引用面）；②RT3_ 拒绝码表与派发拒绝语义（D3 ②）——WSW-7 建 Scanner Adapter 派发时按本批决策面词表消费，禁第二决策实现；③nuclei podman_host assign 形态（e2:egress-allowlist constraints）为 WSW-7 Scanner 容器面直接复用。
6. **下一批输入**：DI-3 七缝主体 + run_tool 通道表扩展（B 组两工具专用派发面）+ Router 侧 input_risk 分类器接入 + agent-engine tool_link Resolver 组装点——登记观察池，各按独立批立项（本批零实现）。
