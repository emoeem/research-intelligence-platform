# PHASE-6A 批任务书 · Report IR + MD/HTML/JSON 渲染器（Phase 6 报告智能第一步；vendor+deny 政策首战双 crate：comrak + docx-rs）

- 批次：PHASE-6A（RIP3 计划 :537 §8.5「统一 Report IR，不同格式用不同 Renderer；禁止每种格式重新调用 AI 生成一遍」+ :515 §8.1 Report Domain 结构化面；Phase 6 行 :670 第一批交付的最小可测切片 = IR schema 单源 + Markdown/HTML/JSON(SARIF) 三渲染器 + fixture e2e；PDF(typst)/DOCX 全量渲染器/模板/图表/质检属 6b）
- 日期：2026-11-23
- 状态：任务书已定稿（2026-11-23 侦察后落稿；实施按本任务书执行；禁 git add/commit/push，工作树留主会话审查收口）
- 依据：[RIP3-ARCHITECTURE-UPGRADE-PLAN.md](../rip3/RIP3-ARCHITECTURE-UPGRADE-PLAN.md) :513-515（§8.1 Report Domain：内容由 ResearchTask + Finding + Evidence + Artifact + Workflow Trace + Knowledge References 组成，事实内容引用 RTIS，3.0 负责结构化/渲染/导出）/:537-541（§8.5 导出格式与 Report IR：第一批 PDF、DOCX、HTML、Markdown、JSON；统一 IR；禁每格式重跑 AI）/:525-529（§8.3 双解释——本批只承载字段位，生成器 6b）/:543-545（§8.6 导出质检——6b）/:670（Phase 6 行：Report Domain/IR、模板、Diagram Engine、五 Renderer；验收 = 多格式内容一致/结论可追溯/图表关联 Evidence/中文正确/导出质量自动检测/双解释）/:321（Rust 新依赖走增量 `cargo vendor` 保 `--offline`——vendor 政策原文）+ [03-DOMAIN-MODEL.md](../rip3/03-DOMAIN-MODEL.md) :124-144（§1.9 Finding/ResearchTask/Report 编排面 + reports（PG）report_ir jsonb 形状 + 实体关系）/:179-181（RTIS 映射：findings rtis_ref 细节不落 3.0 库；reports → 3.0 Report Engine 负责结构化/渲染/导出，RTIS 零行为变更）+ [RT-7-TASK.md](RT-7-TASK.md)（任务书结构先例 + D5 编号登记词形）+ [WSW-8-BATCH-REPORT.md](../rip3/wsw/WSW-8-BATCH-REPORT.md) §7.4（lock 对比以 HEAD 为准 + reqwest 双检门禁词形）+ [PROBE-RESULT.md](../../PROBE-RESULT.md) M2 约定 1（Cargo.lock 与 vendor 一并提交；vendor 更新走 `cargo vendor` 全量重生成）
- 前置：RT-7 已定稿在链（HEAD `1b5836f3`，`git status --porcelain` = 0 行实测 2026-11-23）；workspace 基线 = WSW-8 收口 1337 passed 零红（WSW-8-BATCH-REPORT.md :108）；Cargo.lock 基线 417 / reqwest=1（WSW-8 §7.4 双检词形）；vendor 392 目录 387M（git ls-files 20009 文件——RT-7 A10）
- 设计裁决：本批特有裁决见 §0 D1-D7（**不另立 ADR**——RT-5/6-TASK、RT-7-TASK 先例，裁决内嵌本档）
- 现状锚点：§1 A1-A16 均为 2026-11-23 本会话实测（HEAD `1b5836f3`、工作树 0 行、Cargo.lock 417、migrations 30、migrations_pg 7），实施时若行号漂移按词形重定位并以本档词形为准

---

## 0. 设计裁决（本批特有，ADR 内嵌）

| # | 裁决 | 理由 |
|---|---|---|
| D1 | **Report IR v1 schema 单源（SARIF 2.1.0 对齐 + 自有维度 property bag 扩展位）**：schema 文件落 `schemas/report-ir/report-ir-v1.schema.json`（单源，include_str! 内嵌实施，intel-db/src/ir.rs:23 先例），draft 2020-12，经既有 `tool_registry::jsonschema_lite::validate_instance`（jsonschema 0.56，tool-registry Cargo.toml:18-21 镜像钉版说明）校验——**零新校验器零新校验依赖**。顶层字段：`schema_version` / `report_id`（`rep-` 前缀词形，03-DOMAIN-MODEL :141）/ `report_kind`（词表 `src_vuln_report`/`pentest_report`/`management_report`——:517 §8.2 A/B/C）/ `title` / `generated_at`（ISO8601）/ `language`（缺省 `zh`——:670 中文正确验收）/ `task_ref{task_id?,workflow_run_id?}`（ID 引用非跨库 FK——:144）/ `executive_summary`(markdown) / `scope{authorization?,assets[]}`（B 类授权范围/资产位）/ `methodology`(markdown) / `timeline[{date,event}]` / `coverage{dimensions[{dimension,total,tested,confirmed}]}`（022 七维投影位）/ `findings[]` / `attack_paths[{path_id,title,steps[],evidence_refs[]}]`（攻击路径只承载引用——:531 图表事实以 RTIS 攻击链为准）/ `appendix` / `properties`（自由扩展位）。`FindingIr`：`finding_id`（`find-` 前缀）+ `rtis_ref`（`rtis://poc/<id>` 词形，细节不落 IR——03-DOMAIN-MODEL :140 rtis_ref 词形 + 020:36 红线）+ `title` + `severity`（词表 `critical`/`high`/`medium`/`low`/`info`，RTIS 权威值镜像只读——:140）+ `result_state`（五态词表 `confirmed`/`false_positive`/`not_reproduced`/`not_tested`/`inconclusive`——020 ResultGuard 词表同源，大小写归一 snake_case）+ `location{artifact_uri?,start_line?,end_line?,snippet?}`（SARIF physicalLocation 对齐位）+ `vuln_type` + `affected_scope[]` + `preconditions`/`principle`/`repro_steps`（markdown 片段——HTML 面经 comrak 渲染）+ `request_response[{direction,content}]` + `evidence[]`（EvidenceRef）+ `impact` + `remediation{priority,suggestions[]}` + `technical_explanation`/`plain_explanation`（8.3 双解释**字段位**——本批只承载不生成）+ `properties`。`EvidenceRef`：`evidence_id`/`artifact_id`/`tool_run_id`/`trace_id`（六 ID 投影——015:3-4 同款列名）+ `uri`（`ripartifact://` 或 `rtis://` 词形，agent-engine/src/evidence.rs:109 先例）+ `content_hash`（sha256，021 面）+ `evidence_type?`。**SARIF 对齐面**（§1 A16 模型要点）：自有维度全部走 property bag（`properties.rip.*`），不侵入标准键 | :515 IR 单源原文 + :541 禁每格式重跑 AI（schema 单源 = 三渲染同输入的物理前提）；SARIF 对齐取其 result/location/rule/artifact 四模型与 property bag 扩展位，标准键零污染（severity/六 ID/RTIS 全部 `properties.rip.*`——外部 SARIF 工具可读标准面，自有维度无损可逆）；jsonschema_lite 复用 = tool-registry 同校验器先例（ir.rs:5-8 词形） |
| D2 | **落位 = 新 crate `crates/report-engine`；IR = 内存模型 + schema 校验，本批零落库（SQLite 035 不立项）**：①PLAN :513-515 未指定 crate 落点；仓库模式 = 域一 crate（research-engine/verification-engine/workflow-engine/document-router 先例），报告域无既有宿主（intel-db = PG 存取面、context-broker = 装配面均不合适）→ 新 crate，依赖面 = `domain`（ID 类型，零依赖纯类型库）+ `serde`/`serde_json` + `tool-registry`（jsonschema_lite 同校验器）+ `comrak` + `docx-rs`（D3）——**不依赖** verification-engine/wsw-runtime/agent-engine（渲染纯函数零重耦合）；②IR 持久化归属按 03-DOMAIN-MODEL :141 = **PG 面**（reports.report_ir jsonb，属 migrations_pg 序列，现 V0001-V0007，落库接线随 6b 装配/导出批按 028 先例立项 V0008 候选），**不属 SQLite NNN 序列**；③编号登记（RT-7 D5 词形延续）：031 = WSW-7/WSW-8 双占 + 032 = RT-5/6 + 033 空缺 + 034 = RT-7 不立项 → **本批 = 035 编号位，035 不立项 = 零迁移结论**（IR 内存模型 + 渲染纯函数，本批零读库零写库；坐实需表 → 停批上报禁临场加迁移）；migrations/001-030 零 diff + dev-b1.db user_version=30 恒定 | 仓库 crate 模式先例 + domain 零依赖可作 ID 词形根；渲染器先于装配面落地 = Phase 6 第一步最小可测切片（fixture 驱动 e2e，A16）；持久化滞后到有真实写入方（装配/导出）= 避免死表；SQLite/PG 两序列职责分离（020-022 研究面 vs V0002 Document 域先例） |
| D3 | **vendor 双 crate 形态（vendor+deny 政策本批首战，数字全部本会话实测）**：①`Cargo.toml [workspace.dependencies]` 两行 exact 钉（rmcp =3.10.0 先例）：`comrak = { version = "=1.1.0", default-features = false }`（crates.io 今日 latest；BSD-2-Clause；MSRV 1.89 < workspace 1.99；default-features=false 剥 cli/syntect-onig/bon——零 clap/零 syntect/零 shell-words）+ `docx-rs = { version = "=0.4.22", default-features = false }`（MIT；edition 2021；default-features=false 剥 image/wasm/ts-rs——零 png/jpeg 解码树）；②**lock 跃迁声明：417 → 442（净增 25）**——scratch 探针实测（/tmp/p6a-probe，双 crate default-features=false generate-lockfile = 53 包，对账既有 417 复用 28 → 净 25；reqwest=0 零 TLS 面）；③**vendor 净增 25 目录 21.2 MB**（最大 fearless_simd 5.4M——zlib-rs 链；comrak 2.4M；docx-rs 3.0M；encoding_rs 3.5M 经 quick-xml encoding feature——docx-rs 硬编码不可剥）；落地 = `cargo vendor vendor` 全量重生成（M2 约定 1），diff 必须纯增量（既有 392 目录零修改零删除，漂移即停批）；④**deny 增量一行**：`deny.toml [licenses] allow` 补 `"Unicode-DFS-2016"`（finl_unicode 许可表达式 `(MIT OR Apache-2.0) AND Unicode-DFS-2016`——现 allow 只有 Unicode-3.0，实测 25 包中 24 个落在既有 allow 集，唯一缺口此一项；行注登记本批出处）；⑤本机离线编译冒烟已过：rustc 1.99.1 + vendored sources，`comrak::markdown_to_html` + `docx_rs::Docx::new().pack(cursor)` 双 API 编译链接零错（/tmp/p6a-probe 实测）；⑥本批只落 vendor 基线 + docx-rs 冒烟（一个单测产出最小 docx 字节证明 zip 链可用），**DOCX 全量渲染器 = 6b**（vendor 跃迁一次进基线，6b 不重复跃迁） | :321 vendor 政策原文 + WSW-8 §7.4 lock 门禁词形（reqwest 双检延续）+ deny fail-closed 口径（deny.toml 头注）；default-features=false 剥离 = vendor 体积最小面（image feature 会引入 png/jpeg 全树）；`=` exact 钉 = WSW-8 chromiumoxide 0.13.5 先例（vendor 基线跃迁批精确钉版） |
| D4 | **渲染器接口形状（Finding 集 → Report IR → 三格式，纯函数零 I/O）**：`crates/report-engine` 公开面 = `pub struct ReportIr`（serde Serialize/Deserialize，D1 字段）+ `ReportIr::from_json(&Value)`（schema 校验 fail-closed）+ 四渲染函数签名：`pub fn render_markdown(ir: &ReportIr) -> String`（IR→Markdown 全文，标题/清单/表格手写序列化）/ `pub fn render_html(ir: &ReportIr) -> String`（IR→完整 HTML 文档 `<!DOCTYPE html>`，markdown 片段位经 comrak `markdown_to_html`，D5）/ `pub fn render_json(ir: &ReportIr) -> serde_json::Value`（native IR JSON，serde Serialize 直出面）/ `pub fn render_sarif(ir: &ReportIr) -> serde_json::Value`（SARIF 2.1.0 导出，D1 映射表：severity→level 三值映射 critical/high→error、medium→warning、low/info→note；result_state→kind confirmed/false_positive→fail、其余→review + `properties.rip.result_state` 原值；六 ID→`properties.rip.six_ids`；rtis_ref→`properties.rip.rtis_refs`）+ `pub fn validate_ir(instance: &Value) -> Result<(), String>`。**禁 AI 调用面**（:541）：四渲染全部确定性纯函数，同 IR 两次渲染字节一致（判据 10） | :537 §8.5「统一 Report IR，不同格式用不同 Renderer」原文 = 函数签名面；纯函数零 I/O = e2e 可 fixture 驱动 + 6b 装配面直接复用；SARIF 导出 = JSON 渲染器的标准交换形态（第一列 JSON 面含 native IR + SARIF 两函数——导出面双形态） |
| D5 | **HTML 模板面 = 零模板引擎手写（不 vendor tera/minijinja/handlebars）**：HTML 由 Rust 字符串组装（`format!` + 单点 `escape_html` 转义函数），报告结构 = 五段固定（报告头/执行摘要/Finding 清单/Finding 详情/附录——:517 §8.2 B 类骨架投影）；markdown 片段（repro_steps/technical_explanation 等字段）经 comrak 转 HTML 片段嵌入。**vendor 候选核实结论**：树内零模板引擎（vendor grep tera/askama/handlebars/minijinja = 0 实测）；不新增第三个 crate——报告节结构固定无需 DSL 化，模板引擎运行时 DSL 反成第二事实源（schema 已是结构单源），escape 单点函数可单测穷举（`<>&"'` 五字符 + 属性上下文） | :541 IR 单源禁第二事实源外推；vendor+deny 每 crate 都是一次基线跃迁成本（本批已双 crate），模板 DSL 对固定五段结构零增益；comrak 已承担 markdown→HTML 面，重复引入模板引擎 = 同批双 HTML 路径违 D1 单源 |
| D6 | **e2e 测试面 = fixture IR 驱动结构对账（非字节级 golden）**：fixture 双形态 = `crates/report-engine/tests/fixtures/` 两份 IR JSON（`main-report.json`：2 Finding——confirmed 带 evidence 六 ID + `rtis://poc/42` 引用 + 双解释字段位 + high severity；`mixed-report.json`：五态各态覆盖含 inconclusive/not_tested）+ Rust builder 辅助（测试内构造）；**对账断言面（判据 7-12）**：(a) 同一 IR 三格式渲染内容一致性：finding 数、每 finding_id/title/severity 逐一在三格式出现、evidence 引用（`rtis://poc/42` 与六 ID 词形）三面 grep 同值；(b) SARIF 结构断言：`version=="2.1.0"`、`runs[0].results` 长度 = findings 数、result 标准键齐（ruleId/level/message.text/properties.rip）、artifacts 与 evidence uri 对账；(c) **空 Finding 集边界**：零 findings IR 三渲染 exit 0 且计数一致（MD/HTML 渲染合法空节、SARIF results=[]）；(d) **负例**：severity 词表外值、五态词表外值、rtis_ref 非 `rtis://` 词形 → `validate_ir` 拒绝（fail-closed）；(e) 确定性：同 IR 两次渲染字节一致。**字节级 golden file 本批不建**（随内容演进恒红——selftest_wsw7 :294 399 绝对钉教训同构；字面快照若 6b 质检批需要再立项带 update 流程） | schema_snapshot 先例（agent-engine migration_idempotent.rs）= 结构断言先例；绝对钉教训（RT-7 A4/A6）= 钉结构不变量非字节；fixture JSON + builder 双形态 = schema 校验面与渲染面都能吃同一输入 |
| D7 | **Phase 6b 边界（本批零触碰零实现）**：①PDF 面全零（typst 引擎 PLAN :537 只列格式未指定引擎——选型属 6b 立项，PLAN 全文 grep typst = 0 实测）；②DOCX 全量渲染器（本批仅 docx-rs vendor + 冒烟单测）；③Diagram Engine（:531 §8.4 全节）；④双解释生成器（:525 §8.3 七问——IR 字段位本批承载，生成 6b）；⑤导出质量检查（:543 §8.6 全节）；⑥IR 装配面（从 research_plan/evidence/artifact 存储读库构造 ReportIr——本批 fixture 驱动，零读库）；⑦reports PG 落库（migrations_pg V0008 候选，D2②）；⑧rp_server 端点 / web 报告页 / PPTX/XLSX 第二批格式（:539）。**登记不进**：以上八面逐条进批次报告观察池，6b 任务书立项时逐条对表 | :670 Phase 6 行验收面按批切分：本批交付「多格式内容一致」的结构前提（IR 单源）+ 三格式渲染器，其余验收点（图表关联 Evidence/双解释/质检）属 6b 面；批间边界显式登记防临场扩批 |

## 1. 现状锚点（2026-11-23 本会话实测，A1-A16）

| 锚点 | 事实 | 出处 |
|---|---|---|
| A1 PLAN §8 原文 | :511 第八部分 Report Intelligence；:513 §8.1 Report Domain（:515 组成句 = ResearchTask + Finding + Evidence + Artifact + Workflow Trace + Knowledge References，事实内容引用 RTIS（3.4），3.0 负责结构化/渲染/导出）；:517 §8.2 三报告类型 A/B/C；:525 §8.3 双解释（通俗七问）；:531 §8.4 Diagram Engine（图表事实以 RTIS 攻击链为准）；:537 §8.5（:539 第一批五格式；:541 统一 IR + 禁每格式重跑 AI）；:543 §8.6 导出质检 | docs/rip3/RIP3-ARCHITECTURE-UPGRADE-PLAN.md:511-545 逐行实测 |
| A2 Phase 6 表行 | :670「6 \| Report Intelligence \| Report Domain/IR、模板、Diagram Engine、PDF/DOCX/HTML/Markdown/JSON Renderer \| 多格式内容一致；结论可追溯；图表关联 Evidence；中文正确；导出质量自动检测；通俗+技术双解释」；:658 前置词形（只有第零部分 2.5 列的四项允许 Phase 0 冻结，其余按阶段 ADR 滚动——IR 字段面 = 本批 D1 滚动裁决非冻结物）；:321 vendor 政策原文（Rust 新依赖走增量 `cargo vendor` 保 `--offline`） | docs/rip3/RIP3-ARCHITECTURE-UPGRADE-PLAN.md:658,670；:321 sed 实测 |
| A3 Report 域模型冻结面 | 03-DOMAIN-MODEL §1.9 :124-144：research_tasks/findings/reports 三表形状全部标注（PG）；findings = finding_id（find- 前缀）/task_id/rtis_ref（RTIS 引用键细节不落 3.0 库）/title/severity（RTIS 权威值镜像只读）/five_state 词表/evidence_ids；reports = report_id（rep- 前缀）/task_id/**report_ir jsonb（04/11 号文档 Schema）**/format_exports jsonb/created_at；:142-144 关系 ResearchTask 1:N Finding/Report，Report N:M Finding/Evidence，全部稳定 ID 关联禁标题关联 | docs/rip3/03-DOMAIN-MODEL.md:124-144 实测 |
| A4 RTIS 映射原文 | :179 research_tasks → rtis.py task/run 面（3.0 ResearchTask 是编排容器）；:181 reports（漏洞类）→ RTIS 报告面（现状 = skill 模板 + 文件约定，无结构化工具），报告事实数据引用 RTIS，3.0 Report Engine 负责结构化/渲染/导出，RTIS 零行为变更（报告组装全在 3.0） | docs/rip3/03-DOMAIN-MODEL.md:179,181 |
| A5 Finding 事实源现状 | **Rust 树内零 `struct Finding`**（grep 全 crates = 0 命中）——Finding 事实在 RTIS（integrations/rtis/rt.py poc 面）；020:8-9 原文「Finding 事实不经本域落库（五态 result_status 是研究结论状态，不是 RTIS Finding 事实）」；020:36 `research_plan_item.rtis_poc_id INTEGER` 只存 RTIS POC 数字 ID 引用（正文/利用细节零复制——RTIS 红线）；五态词表 CONFIRMED/FALSE_POSITIVE/NOT_REPRODUCED/NOT_TESTED/INCONCLUSIVE（verification-engine/src/store.rs:211-260 ResultGuard 守卫 + 020:91 CHECK 词表） | migrations/020_research_verification.sql:8-9,36,91；crates/verification-engine/src/store.rs:211-260；grep 实测 |
| A6 021 Evidence 面 | research_evidence 表（021:15-28）：evidence_id（re- 前缀）/item_id/plan_id/tool_run_id/trace_id（「六 ID 统一关联，015 同款列语义」行注）/evidence_type CHECK（provider_result/normalized_result/log_excerpt）/content_hash（payload_json 规范化序列化 SHA256）/payload_json（append-only 不可变）；四索引（item/plan/hash/trace） | migrations/021_research_evidence.sql:15-28,41-44 |
| A7 六 ID 定义 | 015 头注 :3-4「trace_id/task_id/agent_run_id/tool_run_id/artifact_id/evidence_id 六 ID 统一关联与跨栈查询」；五张既有表 ALTER trace_id（015:53-55）；correlation_id = audit_event 事件链通道（agent-engine/src/trace.rs:246 直查面）；ID 新类型在 domain/src/ids.rs（:179 ArtifactId/:241 TraceId/:305 TaskId/:368 EvidenceId） | migrations/015_trace_correlation.sql:3-4,53-55；crates/agent-engine/src/trace.rs:246；crates/domain/src/ids.rs |
| A8 coverage 矩阵面 | 022：research_plan_item.dimension ALTER 七维 CHECK（assets/api_endpoints/identity_permissions/input_points/business_flows/file_processing/data_flows，缺省 assets）+ research_hypothesis 表（022:24-40）；CoverageCell/CoverageMatrix/coverage_matrix 纯函数（verification-engine/src/coverage.rs:22,55,63）；PlanItemRecord.dimension `#[serde(skip)]`（store.rs:100-103——契约 v1 冻结面不动，存储面与 coverage 推导消费） | migrations/022_coverage_matrix.sql:24-40；crates/verification-engine/src/coverage.rs:22-63；store.rs:100-103 |
| A9 RTIS 引用形状 | `rtis://poc/{poc_id}` 构造词形（context-broker/src/sources/rtis.rs:245,290）；domain/src/context.rs:464 注释「来源 URI（设计稿 §9.1 citations uri，如 rtis://poc/900008）」；rtis_source.rs:84 测试词形 `rtis://poc/42`；verification-engine provider.rs:219 `rtis-poc:{id}` template_ref 词形；`ripartifact://{tool_run_id}/{artifact_id}` evidence uri 先例（agent-engine/src/evidence.rs:109）；rt.py poc get/use 走 RtisPocProvider cli_subprocess（provider.rs:134-159） | crates/context-broker/src/sources/rtis.rs:245,290；crates/domain/src/context.rs:464；crates/context-broker/tests/rtis_source.rs:84；crates/verification-engine/src/provider.rs:134-159,219 |
| A10 Document IR 先例（IR 模式同构） | intel-db/src/ir.rs:1-23：Document IR v1 = schema 文件单源 `schemas/document-ir/ir-v1.schema.json` include_str! 内嵌（:23 IR_V1_SCHEMA 常量）+ jsonschema_lite::validate_instance 校验（:5-8）+ schema 无法表达约束 Rust 侧兜底（parent 存在性/环检测/连通性——:10-11）；research-engine/src/contract.rs:43-98 = 14 份 Schema include_str! 先例 | crates/intel-db/src/ir.rs:1-23；crates/research-engine/src/contract.rs:43-98 |
| A11 渲染面现状 | **Rust 树零 MD→HTML 实现**（grep to_html/`<html`/render_html 于 crates/ = 0 业务命中——markdown 词只出现在 mime/解析词表 intel-db parser.rs:44）；**树内零模板引擎**（vendor grep tera/askama/handlebars/minijinja = 0）；serde 1.0 + serde_json 1.0 全树在位（Cargo.toml:51-52）；rp_server.py 6282 行零 report 端点（grep report = 0 业务命中）；web/package.json 零 marked/react-markdown | grep 实测（命令见 §5）；apps/server/rp_server.py wc -l = 6282 |
| A12 comrak 实测 | crates.io 今日 latest = **1.1.0**（cargo search/info --registry crates-io 实测）；license **BSD-2-Clause**（既有 allow 集内）；rust-version 1.89（workspace 1.99 满足）；features default = [cli, syntect-onig, bon]——**default-features=false 后核心依赖 8 个**：caseless 0.2 / finl_unicode 1.4.0 / memchr-n 0.1.7 / phf 0.14 / rustc-hash 2 / smallvec 1.13 / typed-arena 2.0.2（+传递）；无 pest/regex/syn（1.x 已移除 pest 解析器）；`markdown_to_html(&str, &Options)` 主 API（src/lib.rs:111），Options::default() 免 bon 可用（冒烟实测） | cargo info --registry crates-io comrak 实测 + registry src Cargo.toml 逐依赖读 |
| A13 docx-rs 实测 | crates.io 今日 latest = **0.4.22**（实测）；license **MIT**；edition 2021；features default = [image]——**default-features=false 后核心依赖 10 个**：base64 0.22.1（树内已有）/ crc32fast 1.5 / quick-xml 0.41（**encoding feature 硬编码 → encoding_rs 3.5M 不可剥**）/ serde + serde_json（树内已有）/ smallvec 1.15 / thiserror 2.0 / zip 8.6.0（deflate → flate2/zlib-rs/zopfli/fearless_simd 链）；`Docx::new().add_paragraph(...).build().pack(file)` 主 API（README:37-44） | cargo info --registry crates-io docx-rs 实测 + registry src Cargo.toml 逐依赖读 |
| A14 lock/vendor 基线 | `grep -cE '^\[\[package\]\]' Cargo.lock` = **417**；reqwest = **1**（chromiumoxide 0.13.5 传递，WSW-8 §7.4）；vendor/ = **392 目录 / 387M**（git ls-files vendor/ = 20009 文件——RT-7 A10）；.cargo/config.toml vendored-sources 四行（replace-with vendored-sources + directory = "vendor"） | 本会话 grep/du/ls 实测；docs/rip3/wsw/WSW-8-BATCH-REPORT.md §7.4 |
| A15 deny 面 | cargo-deny 0.20.2 用户级（~/.cargo/bin，deny.toml 头注）；allow 集 `deny.toml:19-33` 十项（MIT/MIT-0/Apache-2.0/Unicode-3.0/Zlib/ISC/BSD-2-Clause/BSD-3-Clause/CDLA-Permissive-2.0——:30-33 为 WSW-0 补录四项）；private.ignore = true（workspace 自有 crate 出审计面）；**双 crate 净增 25 包许可实测：24 个落既有 allow 集，唯一缺口 = finl_unicode `(MIT OR Apache-2.0) AND Unicode-DFS-2016`（Unicode-DFS-2016 不在 allow）**；encoding_rs `(Apache-2.0 OR MIT) AND BSD-3-Clause` 与 zlib-rs Zlib 均已覆盖 | deny.toml:19-33；/tmp/p6a-probe cargo vendor 逐包 license 提取实测 |
| A16 门禁与 SARIF 探针 | HEAD `1b5836f3` 干净树（git status --porcelain = 0）；rustc/cargo 1.99.1；workspace 基线 1337 passed（WSW-8 :108）；migrations = 30 + dev-b1.db user_version = 30；migrations_pg = V0001-V0007（**research_tasks/findings/reports PG 三表零建**——grep migrations_pg = 0 命中）；**scratch 探针**：双 crate default-features=false + vendored sources + rustc 1.99.1 离线编译链接零错 + `comrak::markdown_to_html`/`Docx::new().pack()` 双 API 冒烟过（/tmp/p6a-probe）；SARIF 2.1.0 模型要点（凭记忆组织未联网——D1 对齐面）：顶层 `$schema`+`version:"2.1.0"`+`runs[]`；run = tool.driver{name,version,rules[]}/results[]/artifacts[]/properties；result = ruleId/level(none|note|warning|error)/message.text/kind(默认 fail)/locations[]/properties；location = physicalLocation{artifactLocation{uri},region{startLine,snippet}}；rule = id/shortDescription/help{markdown}/defaultConfiguration{level}；artifact = location.uri/mimeType/hashes{"sha-256"}；property bag = 全节点通用扩展位（自有维度落点） | git/rustc/ls 实测；docs/rip3/wsw/WSW-8-BATCH-REPORT.md:108；/tmp/p6a-probe build+test 实录；SARIF 记忆面明示为设计输入非核对事实 |

## 2. 范围

### 2.1 允许改动（落点清单）

| 落点 | 类型 | 内容 | 步骤 |
|---|---|---|---|
| `Cargo.toml` | 修改 | D3①：`[workspace.dependencies]` 增两行 exact 钉（comrak =1.1.0 / docx-rs =0.4.22，均 default-features = false，行注选型记录指向本档 D3）+ `[workspace.members]` 增 `crates/report-engine` 一行；**其余零触碰**（[profile.release] :117-119、既有依赖行零 diff） | S2 |
| `deny.toml` | 修改 | D3④：`[licenses] allow` 补 `"Unicode-DFS-2016"` 一行 + 行注（finl_unicode 表达式缺口，本批出处）；**其余段零触碰**（[advisories] 三 ignore 行、[graph] 零改动） | S2 |
| `Cargo.lock` | 重生成 | D3②③：417 → 442（净增 25）；reqwest = 1 恒定；`cargo vendor` 全量重生成（M2 约定 1） | S2 |
| `vendor/` | 增量 | D3③：净增 25 目录 21.2 MB（comrak/caseless/core_detect/crc32fast/docx-rs/encoding_rs/entities/fastrand/fearless_simd/fearless_simd_macros/finl_unicode/flate2/memchr-n/multiversion_no_op/phf_codegen/phf_generator/quick-xml/rustc-hash/simd-adler32/simdutf8/typed-arena/typed-path/zip/zlib-rs/zopfli）；**既有 392 目录零修改零删除**（diff 纯增量反证） | S2 |
| `schemas/report-ir/report-ir-v1.schema.json` | 新建 | D1：Report IR v1 schema 单源（draft 2020-12，$id 词形对齐 document-ir 先例） | S3 |
| `crates/report-engine/` | 新建 | D2/D4：Cargo.toml（publish = false + domain/serde/serde_json/tool-registry/comrak/docx-rs 依赖面 + description 行注）+ src/lib.rs + src/ir.rs（ReportIr/FindingIr/EvidenceRef + from_json + validate_ir）+ src/render/{markdown,html,json,sarif}.rs + src/escape.rs（单点 escape_html）+ tests/（fixture 对账 + 负例 + 空集 + 确定性 + docx 冒烟）+ tests/fixtures/ 两份 IR JSON | S3-S5 |
| `apps/server/selftest_p6a.sh` | 新建 | ck/sk 词形（selftest_wsw8.sh:14-20 先例）：lock 442 / reqwest=1 双检 + vendor 净增目录计数 + deny 双检 + 三格式对账命令面（cargo test -p report-engine 载具）+ 生产库五文件只读指纹 + 零 diff 反证 + SKIP 守卫预挂（cargo-deny 缺位/二进制缺位） | S6 |
| `docs/phase0/PHASE-6A-BATCH-REPORT.md` | 新建 | 判据 1-14 实测对照 + 坏消息单列（crate 版号实施时点漂移面——若 comrak/docx-rs 出新 patch 按词形重钉并登记）+ 观察池八面（D7）+ **6b 前置输出：IR 冻结面声明**（schema 文件 hash + 字段清单快照 = §8⑥） | S7 |
| `docs/README.md` | additive | RIP3 批次表增一行（MA-4 行先例词形） | S7 |

### 2.2 不修改

- **WSW 面**：`crates/wsw-runtime/src/sender.rs`、`scope.rs`、`fuzz/`、`browser.rs` 全树零 diff；WSW-7/WSW-8 链尾钉（migration_idempotent.rs / caller_auth.rs / exchange_persist.rs / fuzz_job.rs / wsw-runtime persist.rs）零随更；scanner.rs 零 Finding 写面红线零触碰（scanner.rs:23 词形）。
- **MA 面**：SQLite 026 caller/api_key、027 预算四表、029 task_claim/ma_message、rp_server 鉴权四件套与 SSE 面（MA-4）零 diff；caller/预算/协作链零触碰。
- **生产库**：migrations/platform.db、platform-dryrun.db、platform-fullbatch.db、platform-governance.db、platform-search.db 五文件全程 mode=ro 只读指纹（user_version=30 逐文件回显）；migrations/001-030 零 diff；migrations_pg/ V0001-V0007 零 diff（035 不立项——D2②③；reports PG 表属 6b）。
- **RTIS 面**：`integrations/rtis/` 零 diff；RTIS 零直写红线零触碰（禁 `poc add`/`poc update` 词形——WSW-7 判据 3 先例；IR 的 rtis_ref 只承载 `rtis://poc/<id>` 引用词形，POC 正文零复制——020:36 词形延续）。
- **既有 crates 零 diff**：crates/** 除 workspace members 声明外全树零改动（document-router/verification-engine/wsw-runtime 等零触碰——渲染纯函数零回写既有域）；intel-db/document-ir schema 零 diff（Report IR 是新 schema 文件不是 document-ir 改版）。
- **web/**、**workers/python/**、**apps/server/rp_server.py** 零 diff（本批无端点无 UI——D7⑥⑧）；`workers/python/requirements.lock.txt` 零触碰。
- **CI 零改动**：.github/workflows/ci.yml 零 diff（新 crate 自动入 `cargo test --offline --workspace` :52 门禁词形——Rust-First：CI 无需感知新 crate）。
- **fmt 存量红**：120 文件 / 936 hunks 零触碰零扩大；**新增文件必须 fmt 自净**（cargo fmt -p report-engine -- --check exit 0——新代码不入存量红清单）。
- **宿主零安装**：cargo-deny 0.20.2 已在（~/.cargo/bin）零升级零新装；零 sudo；comrak/docx-rs 源码经 cargo vendor 进 git 树零宿主下载（rsproxy 拉取一次属实施步，不入宿主全局）。

## 3. 实施步骤（S1→S7 串行，每步过门再进）

1. **S1 基线核验**：`git status --porcelain` = 0 + HEAD = `1b5836f3`；`grep -cE '^\[\[package\]\]' Cargo.lock` = 417 + reqwest = 1；`ls migrations/*.sql | wc -l` = 30 + dev-b1.db user_version = 30 + `ls migrations_pg/` = V0001-V0007；A1-A16 锚点词形复核（PLAN 行号 / deny allow 集十项 / vendor 392 目录）；**workspace 全量一次基线**：`CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace --no-fail-fast`（预期 1337 passed 零红延续；如有红先停批上报）；cargo-deny 0.20.2 双检 exit 0 基线；**门 = 基线数字表落批次报告底稿**。
2. **S2 vendor 跃迁**：Cargo.toml 两行 exact 钉 + members 一行（D3①）；`cargo update -p comrak -p docx-rs --dry-run` 复核解析后 `cargo vendor vendor` 全量重生成（M2 约定 1）；**门 = lock 442（净增 25 与 A12/A13 清单逐一对应）+ reqwest = 1 恒定 + vendor diff 纯增量（既有 392 目录零修改零删除）+ `cargo build --offline --workspace` exit 0 + `cargo deny check advisories && cargo deny check licenses` 双检 exit 0（含新行 Unicode-DFS-2016）+ `git diff Cargo.toml` 只含声明面**。
3. **S3 report-engine 骨架**：crate 目录 + Cargo.toml（依赖面 D2①）+ `schemas/report-ir/report-ir-v1.schema.json`（D1 全字段清单——schema 词表五处枚举：report_kind/severity/result_state/evidence_type/remediation.priority 全 CHECK 面）+ src/ir.rs 类型面（serde Serialize/Deserialize + from_json 经 jsonschema_lite fail-closed + validate_ir）；**门 = `cargo build -p report-engine --offline` exit 0 + schema 校验冒烟（合法 fixture 过 + 词表外拒）**。
4. **S4 渲染器实现**：render/markdown.rs + render/html.rs（escape.rs 单点转义 + comrak 片段面）+ render/json.rs + render/sarif.rs（D1 映射表逐条落）+ docx 冒烟单测（`Docx::new().pack(cursor)` 最小字节产出——D3⑥）；每文件 `cargo fmt -p report-engine -- --check` exit 0 + clippy 零 warn；**门 = 四函数可调 + 冒烟过 + 新文件 fmt 自净**。
5. **S5 e2e 测试面**：tests/fixtures/ 两份 IR JSON（D6 双形态）+ 对账五面测试（三格式一致性 / SARIF 结构 / 空集边界 / 负例拒收 / 确定性）；**门 = `cargo test -p report-engine --offline` 全绿（测试数落档）+ 判据 7-12 逐条命令回显**。
6. **S6 selftest 与门禁收口**：apps/server/selftest_p6a.sh（ck/sk 词形 + SKIP 守卫预挂——selftest_wsw8.sh:13-20 先例）；**workspace 全量一次**：`CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace --no-fail-fast`（预期 = 基线 + report-engine 新增测试数，零红）+ `cargo clippy --offline --workspace --all-targets -- -D warnings` exit 0 + `cargo fmt --all -- --check` 计数 ≤120 文件/≤936 hunks 且新 crate 文件不在 dirty 清单 + deny 双检 + 生产库五文件 ro 指纹 = 30 + `git status --porcelain` 只含 §2.1 落点核对；**门 = §4 判据 1-6/13/14 全过**。
7. **S7 文档与收口**：PHASE-6A-BATCH-REPORT.md（判据对照 + 坏消息单列 + 观察池八面 D7 + 6b 前置输出 IR 冻结面声明 = schema 文件 sha256 + 字段清单快照 + 四函数签名钉定）+ docs/README.md 索引行；显式 add 本批清单（禁 add -A——本批只落文件不 commit）。

## 4. 完成标准（判据表；验收 = 命令实跑回显）

| # | 判据 | 验收 |
|---|---|---|
| 1 | **vendor 跃迁声明兑现** | `grep -cE '^\[\[package\]\]' Cargo.lock` = 442（净增 25，A12/A13 清单对应）；`grep -c 'name = "reqwest"' Cargo.lock` = 1 恒定；`git diff vendor/ --stat` 纯增量（既有目录零修改零删除——`git status --porcelain vendor/` 只增不删）；comrak/docx-rs 版本串 = `=1.1.0`/`=0.4.22` 双 exact（Cargo.toml grep） |
| 2 | **deny 双检含新行** | `cargo deny check advisories && cargo deny check licenses` 双 exit 0；`git diff deny.toml` = Unicode-DFS-2016 一行 + 行注；双 crate 25 包逐包许可落批次报告台账表（A15 口径） |
| 3 | **零迁移 + 编号登记** | migrations/001-030 零 diff + dev-b1.db user_version = 30 + migrations_pg V0001-V0007 零 diff；批次报告登记「035 不立项」结论 + 033/034 空缺词形（RT-7 D5 延续）；reports PG 表登记为 6b V0008 候选 |
| 4 | **IR schema 单源** | schemas/report-ir/report-ir-v1.schema.json 在位且 include_str! 内嵌（`grep -c "include_str" crates/report-engine/src/ir.rs` ≥1）；`validate_ir(合法 fixture)` = Ok、三负例（severity 词表外 / 五态词表外 / rtis_ref 非 rtis:// 词形）逐个 Err 且错误信息含字段名（fail-closed 回显） |
| 5 | **IR 字段面冻结** | D1 清单逐字段在 schema 中存在（顶层 12 字段 + FindingIr 19 字段 + EvidenceRef 7 字段——grep 对账表落批次报告）；schema $id 词形 = `research-platform/schemas/report-ir/report-ir-v1.schema.json`（document-ir 先例同构） |
| 6 | **四渲染函数签名钉定** | D4 四签名 grep 逐条对账（render_markdown/render_html/render_json/render_sarif + validate_ir）；全零 I/O 反证（签名面无 &mut/无 std::fs/无 Command 词形——grep 反证表） |
| 7 | **三格式内容一致性（正例）** | main-report fixture（2 Finding）渲染：finding 数 = 2 三面一致；每 finding_id/title/severity 在 MD/HTML/JSON 逐一出现；`rtis://poc/42` 与六 ID 词形（trace_id/tool_run_id/artifact_id/evidence_id）三面 grep 同值；对账表落批次报告 |
| 8 | **SARIF 结构面** | render_sarif 输出：version = "2.1.0" + runs[0].results 长度 = findings 数 + result 标准键齐（ruleId/level/message.text）+ severity→level 映射逐 Finding 对账（high→error / medium→warning / low→note 实测三行）+ properties.rip.{result_state,six_ids,rtis_refs} 在位 + artifacts 与 evidence uri 对账 |
| 9 | **空 Finding 集边界（负例面）** | 零 findings IR：三渲染 exit 0；MD/HTML 合法空节渲染；SARIF results = []；三面 finding 计数一致 = 0 |
| 10 | **确定性** | 同一 IR 两次 render_markdown / render_html / render_json / render_sarif 输出字节一致（四断言逐函数实测）——:541 禁 AI 重跑的确定性前提 |
| 11 | **HTML 转义与中文面** | escape_html 五字符穷举单测（`<>&"'`）；含中文 title/解释字段渲染 HTML 后 UTF-8 原样（:670 中文正确——中文字面在产物 grep 命中）；comrak 片段面 `<h2>`/`<strong>` 标签产出实测 |
| 12 | **docx 冒烟（6b 前置）** | docx-rs 单测产出最小 docx 字节（pack 路径 zip 链可用——D3⑥）；零新增 reqwest/零 TLS 词形（Cargo.lock reqwest = 1 反证） |
| 13 | **workspace 全量与静态门零红** | `cargo test --offline --workspace --no-fail-fast` 零红（1337 基线 + 本批新增测试数落档）+ `cargo clippy --offline --workspace --all-targets -- -D warnings` exit 0 + `cargo fmt --all -- --check` ≤120 文件/≤936 hunks 且 crates/report-engine/** 不在 dirty 清单（新代码 fmt 自净）+ deny 双检 exit 0 |
| 14 | **收口零越界** | 生产库五文件 mode=ro PRAGMA user_version = 30 逐文件回显 + `git status --porcelain` 只含 §2.1 落点（Cargo.toml/deny.toml/Cargo.lock/vendor 增量/新 crate/schemas 新目录/selftest_p6a.sh/两份 md + docs/README 一行）+ ci.yml/web/workers/integrations/rtis/migrations/ 全零 diff 反证 |

## 5. 测试面（收口命令）

```bash
# 门禁基线（S1）
git status --porcelain | wc -l          # 0
git log --oneline -1                    # 1b5836f3（RT-7 后 CI 修复批在链）
grep -cE '^\[\[package\]\]' Cargo.lock  # 417 → S2 后 442
grep -c 'name = "reqwest"' Cargo.lock   # 1（恒定双检——WSW-8 §7.4 词形）
ls migrations/*.sql | wc -l             # 30
ls migrations_pg/*.sql | wc -l          # 7（V0001-V0007）
sqlite3 "file:migrations/dev-b1.db?mode=ro" "PRAGMA user_version"   # 30
# Rust 全量（2026-11-23 纪律：workspace 一次必须跑；vendor 跃迁后全量重测收口）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace --no-fail-fast
# 静态门
CARGO_TARGET_DIR=/home/emo/rt3-target cargo clippy --offline --workspace --all-targets -- -D warnings
cargo fmt --all -- --check > /tmp/fmt_p6a.txt 2>&1; grep -c "^Diff in" /tmp/fmt_p6a.txt          # ≤936 hunks
grep -c "crates/report-engine" /tmp/fmt_p6a.txt                                                  # 0（新代码 fmt 自净）
cargo deny check advisories && cargo deny check licenses                                         # 双检 exit 0（含 Unicode-DFS-2016 新行）
# vendor 跃迁对账（S2）
git status --porcelain vendor/ | grep -c "^ D\|^D"                                               # 0（既有目录零删除）
du -sh vendor/                                                                                   # 387M + ~21M ≈ 408M（±目录粒度）
grep -n 'comrak\|docx-rs' Cargo.toml                                                             # 两行 exact 钉对账
# report-engine 判据面（S3-S5）
cargo test -p report-engine --offline                                                            # 全绿（测试数落档）
grep -rn "render_markdown\|render_html\|render_json\|render_sarif\|validate_ir" crates/report-engine/src/lib.rs
# selftest（S6）
bash apps/server/selftest_p6a.sh                                                                 # FAIL=0 / SKIP 分开计数
# 生产库只读指纹（S6）
for f in migrations/platform.db migrations/platform-dryrun.db migrations/platform-fullbatch.db migrations/platform-governance.db migrations/platform-search.db; do sqlite3 "file:$f?mode=ro" "PRAGMA user_version"; done   # 逐文件 30
# 零 diff 反证（S6）
git status --porcelain -- crates/wsw-runtime/ crates/verification-engine/ crates/agent-engine/ web/ workers/ integrations/rtis/ migrations/ migrations_pg/ .github/ | wc -l   # 0
git status --porcelain | wc -l   # 仅 §2.1 落点（≤10 行）
```

基线记录（本任务书写作时点 2026-11-23 实测）：

- HEAD `1b5836f3` 干净树（RT-7 后 CI 修复批在链）；Cargo.lock 417 / reqwest=1；vendor 392 目录 387M（git ls-files 20009 文件）。
- migrations = 30（dev-b1.db user_version=30）；migrations_pg = V0001-V0007（research_tasks/findings/reports PG 三表零建）。
- comrak latest = 1.1.0（BSD-2-Clause，MSRV 1.89）；docx-rs latest = 0.4.22（MIT）；双 crate default-features=false lock 净增 25（scratch 探针实测）/ vendor 净增 21.2MB / deny 唯一缺口 Unicode-DFS-2016。
- 树内零 MD→HTML 实现、零模板引擎、零 report 端点；jsonschema 0.56 校验器在位（tool-registry jsonschema_lite）。
- workspace 基线 1337 passed 零红（WSW-8 :108）；rustc/cargo 1.99.1；cargo-deny 0.20.2 用户级。
- scratch 探针（/tmp/p6a-probe）：双 crate 离线编译 + markdown_to_html/Docx::pack 双 API 冒烟零错。

## 6. 红线（八条，违反任一即停批上报）

1. **Rust-First**：IR 与四渲染器全部 Rust 实现（crates/report-engine），零 Python 渲染路径、零 JS 渲染路径、零 shell 渲染路径；report-engine 公开面 = 纯函数零 I/O（D4），禁在渲染函数内读库/读盘/起子进程（装配面属 6b）。
2. **零新依赖政策 = vendor+deny 新基线纪律**：本批依赖增量仅限 comrak/docx-rs 双 crate（exact 钉 + default-features=false）；第三 crate（含模板引擎）禁临场引入——需第三 crate 即停批重新裁决 D5；vendor diff 必须纯增量（既有 392 目录零修改零删除）；Cargo.lock 442 与 reqwest=1 为本批新基线（后续批按 WSW-8 §7.4 词形对 HEAD 对比）；deny 行增量仅限 Unicode-DFS-2016 一行。
3. **不建第二事实源（IR 单源）**：Report IR 是唯一结构源，三格式渲染同一 IR 实例（:541 禁每格式重跑 AI 原文）；禁在渲染器内维护第二份字段清单/第二份 schema；SARIF 自有维度只走 properties.rip.* property bag，标准键零污染（D1）；rtis_ref 词形单源 = `rtis://poc/<id>`（A9 先例），禁 `rtis-poc:` 旧词形进 IR（provider.rs:219 词形只属 verification 面）。
4. **迁移禁改 + 035 不立项**：migrations/001-030 零 diff、migrations_pg/ V0001-V0007 零 diff、生产库五文件全程 mode=ro；IR = 内存模型本批零落库；reports PG 表 = 6b 立项候选（V0008）——坐实需表 → 停批上报按 028 先例重新立项，禁临场加迁移。
5. **生产库零触碰**：五文件只读指纹逐文件回显（判据 14）；禁任何写入路径（含 development 副本误指 production DSN）；selftest_p6a.sh 全部目标 = 临时目录 fixture + 只读 PRAGMA。
6. **宿主零安装**：cargo-deny 0.20.2 已在零升级；comrak/docx-rs 经 cargo vendor 入 git 树（rsproxy 拉取一次属实施步）；零新装 CLI 工具、零 sudo、零全局 cargo install。
7. **边界纪律（D7 八面全留 6b）**：typst/PDF、DOCX 全量渲染器、模板/Diagram Engine、双解释生成、导出质检（8.6）、IR 装配读库面、reports 落库、rp_server/web 面——本批零实现零触碰，逐条登记观察池；rp_server.py 6282 行零 diff。
8. **禁 commit/add -A/push**（本批只落文件，工作树留主会话审查收口——S7 显式 add 仅登记清单）；fmt 存量红 120 文件零触碰（新增文件 fmt 自净不入 dirty 清单——判据 13）；既有测试零改动（report-engine 测试全为新文件，零触碰既有断言面）。

## 7. 回滚

- **vendor 面 revert（D3）**：`git checkout HEAD -- Cargo.toml Cargo.lock deny.toml` + `git clean -fd vendor/` 前先 `git status vendor/` 比对——净增 25 目录属 untracked 增量（git clean 只清新增，既有 392 目录不受影响）；或精确 `rm -rf vendor/{25 目录清单}` + 三文件 checkout。回滚后 `cargo build --offline --workspace` 必须复绿（417 基线自证）。
- **crate/schema 面 revert**：`rm -rf crates/report-engine schemas/report-ir` + Cargo.toml members 行 checkout（S2 已含）+ docs/README 索引行 + selftest_p6a.sh 删除——工作树态零残留；无 commit 无 push 无 CI 副作用（ci.yml 零 diff）。
- **数据库零回滚面**：零迁移零写库（D2/D4）——生产库与 dev 副本全程只读无回滚义务。
- **宿主零回滚面**：零安装零升级（红线 6）。
- **实施中止态**：S2 完成后中止 = vendor 基线已跃迁但渲染器未落——按上两条 revert 全撤（vendor 跃迁不单独留存——半跃迁态禁跨批遗留，lock 442 无消费方即回滚）。

## 8. 交付物

1. **vendor+deny 新基线**：Cargo.toml 双 crate exact 钉 + Cargo.lock 417→442（净增 25 清单）+ vendor 净增 25 目录 21.2MB 纯增量 + deny.toml Unicode-DFS-2016 一行——判据 1/2 回显逐条入报告；25 包许可台账表（A15 口径）。
2. **Report IR v1 冻结面（6b 前置输出——§8⑥ 核心）**：schemas/report-ir/report-ir-v1.schema.json（D1 全字段：顶层 12 + FindingIr 19 + EvidenceRef 7）+ schema 文件 sha256 落批次报告 + validate_ir fail-closed 判据 4 回显；**IR 字段清单快照 = 6b 装配面与 DOCX/PDF 渲染器的输入契约**（后续批禁改字段语义，只 additive——04 号冻结物先例词形）。
3. **三格式渲染器 + SARIF 导出**：crates/report-engine 四函数（render_markdown/render_html/render_json/render_sarif）+ escape 单点 + comrak 片段面——判据 6/7/8/9/10/11 回显；三格式对账表（同 IR 三面 finding/title/severity/六 ID/rtis_ref 逐项一致实测）。
4. **e2e 测试面**：crates/report-engine/tests/ fixture 双形态 + 五面测试（一致性/SARIF 结构/空集/负例/确定性）+ docx 冒烟单测——判据 12 回显；测试数落档（workspace 基线跃迁数字 = 1337 + N）。
5. **selftest_p6a.sh + 门禁收口**：lock 442/reqwest=1 双检 + deny 双检 + workspace 全量零红 + clippy/fmt 门 + 生产库五库指纹 30 + 零 diff 反证——判据 13/14 回显。
6. **docs/phase0/PHASE-6A-BATCH-REPORT.md**：判据 1-14 对照 + 坏消息单列（crate 版号漂移/vendor diff 漂移/SARIF 记忆面与 6b 官方 schema 校验的差距登记）+ 观察池八面（D7）+ **6b 前置输出专节**（IR 冻结面声明：schema sha256 + 字段清单 + 四函数签名 + 观察池对表）+ docs/README.md 索引行。
7. **下一批输入（6b 立项对表，本批零实现）**：①DOCX 全量渲染器（docx-rs 已 vendor——D3⑥）；②PDF 引擎选型（typst/其它——PLAN 未指定，A16）；③IR 装配面（verification-engine store / agent-engine evidence 读库 → ReportIr）；④reports PG 落库（V0008 立项候选 + format_exports 台账）；⑤双解释生成器（8.3 七问）；⑥Diagram Engine（8.4）；⑦导出质检（8.6——含官方 SARIF schema vendor 候选与 6a 结构断言面的差距收口）；⑧rp_server 报告端点 + web 报告页。

---

## 9. 复现（本任务书侦察数字复核命令与预期回显）

```bash
cd /media/emo/code/pensting/research-platform
git log --oneline -1                 # 1b5836f3 WSW-8 CI 修复批二补…
git status --porcelain | wc -l       # 0
grep -cE '^\[\[package\]\]' Cargo.lock        # 417
grep -c 'name = "reqwest"' Cargo.lock         # 1
ls migrations/*.sql | wc -l          # 30
ls migrations_pg/*.sql | wc -l       # 7
sed -n '537,541p' docs/rip3/RIP3-ARCHITECTURE-UPGRADE-PLAN.md   # §8.5 两段原文（统一 IR + 禁每格式重跑 AI）
sed -n '670p' docs/rip3/RIP3-ARCHITECTURE-UPGRADE-PLAN.md       # Phase 6 表行
grep -rn "struct Finding" crates/ | wc -l                       # 0（Finding 事实在 RTIS）
grep -n "rtis_poc_id" migrations/020_research_verification.sql  # :36 INTEGER 只存数字 ID 引用
grep -n "CONFIRMED\|NOT_REPRODUCED\|NOT_TESTED\|INCONCLUSIVE\|FALSE_POSITIVE" migrations/020_research_verification.sql | head -1
# comrak/docx-rs 版号与许可（rsproxy 在线路径；离线环境以 §1 A12/A13 registry src 实测为准）
cargo search --registry crates-io comrak --limit 1     # comrak = "1.1.0"
cargo search --registry crates-io docx-rs --limit 1    # docx-rs = "0.4.22"
# vendor 跃迁预演（scratch，不触仓库）——预期 lock 53 包 / 对账既有 28 复用 / 净增 25
mkdir -p /tmp/p6a-probe-repro/src && cd /tmp/p6a-probe-repro
printf '[package]\nname = "p6a-probe-repro"\nversion = "0.0.0"\nedition = "2021"\n\n[dependencies]\ncomrak = { version = "1.1.0", default-features = false }\ndocx-rs = { version = "0.4.22", default-features = false }\n' > Cargo.toml
echo "pub fn x(){}" > src/lib.rs
cargo generate-lockfile && grep -cE '^\[\[package\]\]' Cargo.lock    # 53
```
