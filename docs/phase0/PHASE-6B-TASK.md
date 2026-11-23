# PHASE-6B 批任务书 · PDF/DOCX 渲染器 + 导出质检 + V0008 reports 落库（Phase 6 报告智能第二步；vendor+deny 政策二战：pdf-writer/subsetter/skrifa 轻量 PDF 链 + 手写 OOXML；typst 库依赖侦察停批）

- 批次：PHASE-6B（RIP3 计划 :537 §8.5「统一 Report IR，不同格式用不同 Renderer」第一批格式 PDF/DOCX 补齐 + :543-545 §8.6 导出质检三面 + 03-DOMAIN-MODEL :132-133 reports PG 表 V0008 落库；6A 观察池八面（PHASE-6A-BATCH-REPORT.md §10）中①PDF 选型②DOCX 全量渲染器⑤导出质检⑦reports 落库四面本批立项，③Diagram Engine/④双解释/⑥IR 装配/⑧rp_server-web 继续留观察池）
- 日期：2026-11-23（侦察同日落稿；HEAD `7ee11904` 起——6A 收口批已在链）
- 状态：任务书已定稿（2026-11-23 侦察后落稿；实施按本任务书执行；禁 git add/commit/push，工作树留主会话审查收口）
- 依据：[RIP3-ARCHITECTURE-UPGRADE-PLAN.md](../rip3/RIP3-ARCHITECTURE-UPGRADE-PLAN.md) :537-541（§8.5 第一批五格式 + 统一 IR + 禁每格式重跑 AI）/:543-545（§8.6 导出质检十项原文）/:531 §8.4 Diagram Engine（本批只落 attack_paths 单类 SVG 最小面）/ :670（Phase 6 行验收：多格式内容一致/结论可追溯/图表关联 Evidence/中文正确/导出质量自动检测）/ :672（Phase 7 知识回流行——本批零触碰）/ :321（vendor 政策原文）+ [03-DOMAIN-MODEL.md](../rip3/03-DOMAIN-MODEL.md) :132-133（reports（PG）冻结形状）/:144（全稳定 ID 关联禁标题关联）/:181（RTIS 零行为变更，报告组装全在 3.0）+ [PHASE-6A-BATCH-REPORT.md](PHASE-6A-BATCH-REPORT.md)（IR 冻结面 §7 sha256=888e9210 + 观察池八面 §10 + 裁决 B 停批链 §1 + 坏消息 9 SARIF 官方 schema 差距登记）+ [PHASE-6A-TASK.md](PHASE-6A-TASK.md)（任务书结构先例 + 红线/回滚词形）+ [WSW-8-BATCH-REPORT.md](../rip3/wsw/WSW-8-BATCH-REPORT.md) §7.4（lock 对比以 HEAD 为准 + reqwest 双检 + vendor `--config` rsproxy 词形 :42）+ [SEC-0-ADVISORIES-DECISION.md](SEC-0-ADVISORIES-DECISION.md)（unmaintained INFO 级 ignore 先例——本批零新 ignore）
- 前置：PHASE-6A 已收口在链（HEAD `7ee11904`，`git status --porcelain` = 0 行实测 2026-11-23）；workspace 基线 1365 passed 零红（6A C11）；Cargo.lock 基线 432 / reqwest=1 / zip=0（6A 裁决 B 终态）；vendor 406 目录 399M；IR 冻结面 sha256 = `888e9210f1ba44a5706439adb7a486ee75f8c4b6cc3c871c6184b5eb577a499e`（6A C19）
- 设计裁决：本批特有裁决见 §0 D1-D8（**不另立 ADR**——6A/RT-5-6/RT-7 先例，裁决内嵌本档）
- 现状锚点：§1 A1-A18 均为 2026-11-23 本会话实测（HEAD `7ee11904`、工作树 0 行、Cargo.lock 432、vendor 406 目录 399M、migrations_pg V0001-V0007、fmt 存量红 943 hunks），实施时若行号漂移按词形重定位并以本档词形为准

---

## 0. 设计裁决（本批特有，ADR 内嵌）

| # | 裁决 | 理由 |
|---|---|---|
| D1 | **PDF 引擎裁决：typst 库依赖停批，落 pdf-writer + subsetter + skrifa 轻量组合（手写排版）**。四维权衡实测（本会话 /tmp 探针全录，§1 A5-A7）：①**deny advisories 门（决定性）**：typst 0.15.1（crates.io latest）+ typst-pdf 0.15.1 探针链 6 条 error 级——RUSTSEC-2026-0194/0195（quick-xml 0.38.4 **vulnerability**，修复线 ≥0.41.0，但 citationberg 0.7.0 钉 quick-xml ^0.38.1 ← hayagriva 0.10.1 ← typst-library 0.15.1，semver 堵死不可达——6A docx-rs zip 8.x 同构）+ RUSTSEC-2025-0141（bincode 1.3.3 unmaintained）/RUSTSEC-2026-0206（rustybuzz）/RUSTSEC-2026-0192（ttf-parser）/RUSTSEC-2024-0320（yaml-rust）四条 unmaintained error 级；[advisories] ignore vulnerability（有修复线）违 deny.toml 现行纪律（RUSTSEC-2026-0459 不 ignore 行注词形），unmaintained×4 需 SEC-0 级独立裁决——本批零 ignore 增行（红线 2）；②**vendor 体积**：typst+typst-pdf lock = 299 包、对账基线 432 净增 176 (name,version)、vendor 全树 298 目录 240M / 增量 157 目录 ≈144M（KiB 实测 147048）——是轻量链（+8 包 ≈9.0M）的 16 倍，且 typst-assets 打包字体 10M 入 vendor 树（git 成本）；③**许可**：typst 链另需 deny [licenses] 增 2 行（ar_archive_writer `Apache-2.0 WITH LLVM-exception` + roman-numerals-rs `0BSD OR CC0-1.0`）——轻量链许可全落既有 allow 集**零增行**；④**字体面**：typst 库模式 PDF 导出需 typst-pdf 独立 crate + World trait 实现（typst 库本体零 PDF 导出——vendor 内 typst 12 crate 无 pdf 面），typst-as-lib 0.18.0（lock 290 包）为包装层但同链同 deny 面；typst-assets embed-fonts 打包字体违零安装红线 6。**替代候选全探针**：krilla 0.8.2（71 包）advisories FAILED（rustybuzz+ttf-parser ×2 unmaintained）、printpdf 0.12.14（132 包）FAILED（bincode）——均停批。**终选链（deny 双检全绿零改动）**：`pdf-writer =0.15.0`（PDF 对象图写出；MIT OR Apache-2.0；依赖 bitflags/itoa/memchr/ryu 全在基线 432 同版本零增量）+ `subsetter =0.2.6` default-features=false（字体子集化；MIT OR Apache-2.0；rust-version 1.85；forbid unsafe；**kurbo/skrifa/write-fonts 全是 optional**，剥 variable-fonts 后运行时唯一依赖 rustc-hash——基线在位）+ `skrifa =0.42.1`（Google Fonts fontations 字体解析，RUSTSEC-2026-0192 的维护替代面；rust-version 1.85）+ `crc32fast =1.5.5`（D3 DOCX 容器）。**净增 8 包 ≈9.0M**（pdf-writer/subsetter/skrifa/read-fonts/font-types/bytemuck/bytemuck_derive/crc32fast；KiB 实测合计 9168——四钉 exact 组合权威探针 /tmp/p6b-probe-final 实测，skrifa 直依钉 0.42.1 → read-fonts 0.39.2/font-types 0.11.3，无 arrayvec），lock 432 → **440**。离线编译冒烟已过（rustc 1.99.1 + vendored：skrifa `FontRef::new` 解析系统 LiberationSans + `subsetter::subset` 非空产出 + pdf-writer `%PDF-` 字节——/tmp/p6b-probe-pdf-writer 实测 1 passed）；组合 API 形状实测：pdf-writer = builder（`Pdf::new()` → `catalog`/`pages`/`start_page` → `finish() -> Vec<u8>`，src/lib.rs:307；Type0Font/CidFont/Cmap/FontDescriptor/Widths/GlyphId 全类型面在 lib.rs:130-175 词形）；subsetter = `subset(data: &[u8], index: u32, mapper: &GlyphRemapper) -> Result<Vec<u8>>`（src/lib.rs:108；GlyphRemapper::remap(u16)->u16——README「forbids unsafe code and has only one dependency」原文） | :537 §8.5 第一批含 PDF + :321 vendor 政策 + deny fail-closed 口径（deny.toml 头注）；typst 排版质量（两端对齐/kerning）以「手写排版 tradeoff」登记观察池（§8 观察池 9）；pdf-writer/subsetter = typst 生态自产（Laurenz Mädje），API 与 typst-pdf 0.x 历史实现同构——CID Type0 字体嵌入 + glyph 定位路径有 typst-pdf 0.x 先例背书 |
| D2 | **字体面来源 = 系统字体目录扫描（零安装红线），零打包字体入库**：①`crates/report-engine` 新模块 `src/fonts.rs`：`pub fn discover_system_fonts() -> Vec<FontSource>`（扫描 `/usr/share/fonts`/`/usr/local/share/fonts`/`~/.local/share/fonts`/`~/.fonts` 标准目录，`walkdir`（基线在位）遍历 .ttf/.otf/.ttc，**唯一 I/O 面**——独立函数非渲染器内，判据 4 零 I/O 反证按渲染函数口径 grep）；②`FontSource { data: Vec<u8>, index: u32 }`（.ttc 经 index 选面——Noto CJK = .ttc collection）；③`render_pdf(ir, &[FontSource])` 收字体字节参数——**渲染纯函数零 I/O 红线延续**（6A D4 词形），e2e 测试读系统字体文件传入；④CJK 字体选择 = skrifa 解析 cmap 含 U+4E00-9FFF 探测（选含中文字形的首个字体），缺 CJK 字体面 → 渲染降级 .notdef 不 panic，质检 qc_pdf「中文字体正确」项 fail（:545 词形——**fail 不 abort**，QcReport 留错误信息 = :545「导出失败保留错误信息」原文词形）；⑤**零打包字体**：Noto CJK 全家 ~100M+ 入 git 树违 vendor 政策面；OFL 许可虽允许重分发，打包字体属 CI 字体缺失后备议题 → 观察池（§8 观察池 10）；⑥测试 skip 守卫：系统无 CJK 字体时中文判据 SKIP 分开计数（RP_TEST_PG_DSN 同款词形——document_domain.rs:21-30 先例），本机已实测在位（fc-list 2795 面、CJK 83 面——A9） | 宿主零安装红线（6A 红线 6 词形延续）+ :670「中文正确」验收 = 字体嵌入面在渲染器内闭环（skrifa cmap→glyph→CID Type0 嵌入）；系统字体探测纯 std::fs + walkdir（基线复用零新增）；渲染零 I/O 与字体发现 I/O 分层 = 判据 4 反证词形可守 |
| D3 | **DOCX 裁决：六个候选 crate 全停批（deny advisories probe 全跑，A6 表），落手写 OOXML 序列化 + stored-ZIP 容器**。①停批链实测：`docx` 1.1.2（lock 40 包，zip 0.5.13）→ RUSTSEC-2020-0071（time segfault）+ RUSTSEC-2026-0367/0368（zip 0.5.13 affected）；`rust-docx` 0.3.1（lock 101 包，**零 zip 依赖**）→ RUSTSEC-2026-0194/0195（quick-xml vuln）+ RUSTSEC-2026-0206/0192（rustybuzz/ttf-parser unmaintained）；`zavora-docx` 0.1.4 与 `docx_basic` 0.1.0 → zip 8.6.0（6A docx-rs 同链死路）；`ooxml` 0.2.8（lock 143 包，zip 0.6.6，parser 非 writer）→ RUSTSEC-2024-0388（derivative unmaintained）+ 0194/0195；docx-rs 0.4.22 = crates.io latest 无升级版（6A C8 实测）；②**手写路线**：`src/render/docx.rs` 纯函数 `render_docx(ir: &ReportIr) -> Vec<u8>`——最小 OOXML 包 `[Content_Types].xml` + `_rels/.rels` + `word/document.xml`（标题/段落/表格 XML 手写序列化，escape 单点复用——XML 转义与 HTML 转义同五字符集 `<>&"'`，6A escape.rs:1 词形复用）+ stored（无压缩）ZIP 容器手写（local file header + central directory + EOCD，CRC32 经 crc32fast =1.5.5——lock 3 包、双检全绿零改动）；③**crc32fast 先例价值**：XLSX/PPTX（:539 第二批格式）同为 OOXML+ZIP 容器，本批容器模块（`src/ooxml.rs`：zip 打包 + XML 零件组装）即第二批格式复用底座；④DOCX 零字体零图片面（文本 XML 由 Word 端解析渲染——中文零嵌入负担，:670 中文正确 = XML UTF-8 原样 + qc_docx 断言） | 六 crate probe 全停批坐实（必答项 deny advisories probe 逐个跑——6A 坏消息 1 教训词形落实）；手写序列化先例 = 6A D5（render_markdown 手写标题/清单/表格）同构；stored-ZIP = ZIP 规范最小合法面（Word/WPS 接受 stored 条目），零压缩零 flate2 依赖；crc32fast = 标准实现零手写算法正确性负担 |
| D4 | **V0008 立项裁决：reports 表落 PG（migrations_pg V0008__reports.sql）+ intel-db report_repo 读写面**。①形状（03-DOMAIN-MODEL :132-133 逐列冻结）：`CREATE TABLE reports (report_id TEXT PRIMARY KEY /* rep- 前缀 */, task_id TEXT NOT NULL /* ID 引用非跨库 FK——:144 词形；research_tasks PG 表 V0001-V0007 未建（6A A16），无 FK 目标 */, report_ir JSONB NOT NULL /* 04/11 号文档 Schema = Report IR v1 冻结面 */, format_exports JSONB NOT NULL DEFAULT '{}' /* 导出台账位：{format:{sha256,bytes,exported_at}} additive-only */, created_at TIMESTAMPTZ NOT NULL DEFAULT now())` + `CREATE INDEX idx_reports_task ON reports(task_id)`——零 CHECK 词表（词表由 IR schema 单源承载，禁第二事实源——6A 红线 3 词形）；②refinery 纪律：V0008 头注照 V0007 词形（每文件事务+checksum、幂等已应用跳过、只在开发库副本与测试库执行、生产 PG 实例不接本批写面）；**V0007:12 词形「扩词值须升 V0008+」与 V0008=reports 兼容**（donor_channel 扩词若发生走 V0009+，登记坏消息预案）；③读写接口 = `crates/intel-db/src/report_repo.rs`（document_repo 同构词形）：`store_report(input) -> Result<StoredReport>` / `load_report(report_id)` / `list_reports_by_task(task_id)`——**report-engine 纯函数零 I/O 红线不破**（repo 在 intel-db 侧，report-engine 只出 IR 与渲染字节）；RTIS 零接触（report_ir 内 `rtis://` 引用只读透传零解析——6A 红线 4 词形）；④测试 = `crates/intel-db/tests/reports_pg.rs`（skip_unless_pg 词形——document_domain.rs:21 先例）：V0008 幂等（refinery 二跑零变化）+ rep- 前缀 roundtrip（store→load 对账 report_ir 字节面）+ format_exports additive 写面；⑤编号登记：SQLite 036 **不立项**（reports 属 PG 面——6A D2② 词形延续），migrations/001-030 零 diff | :132-133 reports 冻结形状 + :144 稳定 ID 关联；V0001-V0007 现链（A8）；intel-db = migrations_pg 唯一执行器（lib.rs:133-140 embed_migrations 词形）+ document_repo 先例同构；持久化滞后到真实写入方（装配/导出批）= 6A D2②③ 词形延续——本批即「有真实写入方」批 |
| D5 | **图表裁决：cetz/fletcher 停（typst 包生态非 crates.io，库模式需运行时包缓存播种 + 网络，违 --offline 零安装；且 typst 本体已停批 D1）；attack_paths → SVG 手写纯函数最小面**：`src/render/svg.rs` 纯函数 `render_attack_path_svg(ir: &ReportIr) -> String`——attack_paths 单类（path 步骤 → 方框节点 + 箭头连线 + evidence_refs 文本标注），字符串组装零依赖（6A D5 手写词形同构）；**PDF/DOCX 面图表以攻击路径结构化步骤列表呈现**（文本化——矢量图嵌入需 SVG path→PDF 转换器，本批不立）；Diagram Engine 全模块（Mermaid/Sequence/Architecture/Attack Path/Data Flow/Network Topology/Before-After 七类——:531 §8.4 原文）= 观察池（§8 观察池 3），Phase 6 第三批候选；web 面 echarts 6.1.0 已在（web/package.json:23）——web 报告页属观察池复用面 | :531「图表事实以 RTIS 攻击链为准」+ IR attack_paths 字段位已承载引用面（6A 冻结面 §7）；cetz/fletcher 分发面 = typst universe（GitHub/packages.typst.org），非 cargo vendor 可达面——零安装红线直接排除；SVG = 文本零依赖可判据化（well-formed + evidence 可追溯 grep） |
| D6 | **导出质检三面落法（:545 十项原文分组）+ QcReport 纯函数 + SARIF 官方 schema 收口**。①三面分组（:545 十项逐项落位）：**结构完整性面**（章节完整/目录正确/页码正确）——PDF 页对象 `/Type/Page` 计数 vs 排版预期页数断言、DOCX 章节标题层级结构断言；**资产正确性面**（图片存在且未损坏/中文字体正确/代码块完整/表格不溢出）——PDF `/FontFile2` 字体嵌入在位（CJK 态）、PDF 尾 `%EOF` 完整、DOCX zip central directory + CRC32 全对、6b 产物零图片位（IR 无图片字段——该项对 6b 产物恒 pass 空集，登记口径）；**事实可追溯面**（引用可追溯/Finding 与 Evidence 对应/图表与内容一致）——IR 覆盖对账（finding 计数/finding_id/title/evidence 六 ID/rtis_ref 在产物字节面 grep 同值——6A 判据 7 词形延续）+ SVG 节点数 = attack_paths 步骤数断言；②`src/qc.rs` 纯函数：`pub struct QcCheck { name: &'static str, passed: bool, message: String }` + `pub struct QcReport { format: &'static str, checks: Vec<QcCheck> }`（serde Serialize——「保留错误信息」原文词形）+ `qc_pdf(ir, &[u8]) -> QcReport` / `qc_docx(ir, &[u8]) -> QcReport` / `qc_sarif(ir, &Value) -> QcReport`；③**SARIF 官方 schema 收口（6A 坏消息 9）**：官方 schema 资产落 `schemas/sarif/sarif-schema-2.1.0.json`（oasis-tcs/sarif-spec main 分支实测 HTTP 200、112768 字节、draft-04——A11；资产文件非 crate 不进 vendor/deny 面），`qc_sarif` 经既有 `tool_registry::jsonschema_lite::validate_instance` 校验 `render_sarif` 输出——**零新校验依赖**；若 draft-04 与 jsonschema_lite 不兼容（S3 冒烟发现）→ 停批上报重新裁决（负例面预登记）；④负例面：截断 PDF（尾部字节切除）→ qc_pdf fail；CRC 篡改 DOCX → qc_docx fail | :545 十项原文逐项可查（A2）+ 6A 判据 7/8 词形延续 + 坏消息 9 收口承诺（6A §10 观察池 5）；质检纯函数 = 渲染器同构零 I/O；「允许重新导出」= QcReport 结构化返回非 panic |
| D7 | **新渲染函数签名钉定（6A D4 四签名零改动 + 新增三函数一装配面）**：`crates/report-engine` 公开面新增 `pub struct FontSource { pub data: Vec<u8>, pub index: u32 }` + `pub fn render_pdf(ir: &ReportIr, fonts: &[FontSource]) -> Vec<u8>`（IR→PDF 字节，CID Type0 字体子集嵌入 + 分页排版——纯函数零 I/O）+ `pub fn render_docx(ir: &ReportIr) -> Vec<u8>`（IR→OOXML+stored-ZIP 字节——纯函数零 I/O）+ `pub fn render_attack_path_svg(ir: &ReportIr) -> String`（attack_paths→SVG——纯函数零 I/O）+ `pub fn qc_pdf/qc_docx/qc_sarif`（D6）+ `pub fn discover_system_fonts() -> Vec<FontSource>`（唯一 I/O 面，独立模块）；**既有四渲染函数（render_markdown/render_html/render_json/render_sarif）+ validate_ir/from_json 签名与实现零 diff**（IR 冻结面词形——判据 3） | :537「不同格式用不同 Renderer」原文 + :541 禁 AI 重跑（同 IR 多格式确定性——判据 9）；6A 纯函数零 I/O 词形延续（字体数据经参数注入 = 反证词形可守） |
| D8 | **Phase 7 边界（知识回流不进本批）**：PLAN :672 Phase 7 = Knowledge Feedback & Maintenance（Finding 到候选知识、Evidence 溯源、上游更新监测、失效链接检查、旧 POC 检查、自动质量巡检、维护报告）——**全留 Phase 7，本批零实现零触碰**；连同 6A 观察池延续面（双解释生成器 :525/IR 装配面/rp_server 端点/web 报告页/PPTX/XLSX/Diagram Engine 七类）逐条登记 §8 观察池 | :672 Phase 7 行原文 + 6A D7 八面登记词形延续；批间边界显式登记防临场扩批 |

## 1. 现状锚点（2026-11-23 本会话实测，A1-A18）

| 锚点 | 事实 | 出处 |
|---|---|---|
| A1 基线 | HEAD `7ee11904`（6A 收口批 commit）干净树 `git status --porcelain` = 0；Cargo.lock **432**（`grep -cE '^\[\[package\]\]'`）、reqwest=1、zip=0；vendor **406 目录 / 399M**；migrations 30 / migrations_pg **V0001-V0007**；fmt 存量红 **943 hunks**（`cargo fmt --all -- --check` grep "^Diff in" = 943——6A C13 词形恒定）；workspace 1365 passed（6A C11） | git/grep/du/ls 实测 |
| A2 §8.6 十项原文 | PLAN :543 节题「## 8.6 导出质量检查」+:545 十项逐字：「每次正式导出自动验证：章节完整、图片存在且未损坏、目录正确、页码正确、中文字体正确、代码块完整、表格不溢出、引用可追溯、Finding 与 Evidence 对应、图表与内容一致。导出失败保留错误信息并允许重新导出。」 | docs/rip3/RIP3-ARCHITECTURE-UPGRADE-PLAN.md:543,545 sed 实测 |
| A3 PLAN 引擎词形 | **PLAN 全文 grep typst = 0、grep 字体烘焙 = 0**——PLAN 只列格式未指定引擎（6A D7① 结论延续；「typst PDF 渲染」词形不出自 PLAN，本批 D1 选型 = 滚动裁决非 PLAN 冻结面） | grep 实测 |
| A4 reports 冻结形状 | 03-DOMAIN-MODEL :124 §1.9 起头；:132-133「reports（PG）：report_id（前缀 rep-）、task_id、report_ir jsonb（04/11 号文档 Schema）、format_exports jsonb、created_at」；:144 全稳定 ID 关联禁标题关联；:181 RTIS 零行为变更（报告组装全在 3.0） | docs/rip3/03-DOMAIN-MODEL.md:124,132-133,144,181 sed 实测 |
| A5 typst 探针 | typst 0.15.1 = crates.io latest（`cargo search --registry crates-io typst` 实测）；license Apache-2.0；rust-version 1.92；edition 2024；**typst 库本体零 PDF 导出**（vendor 内 typst 12 crate 无 pdf 词形——PDF 在 typst-pdf 独立 crate）；typst+typst-pdf lock = **299 包**（`grep -cE` Cargo.lock）；对账基线 432 → 净增 **176 (name,version)**（comm -23 实测）；vendor 全树 298 目录 240M / 增量 157 目录 ≈**144M**（KiB 147048）；typst-assets 0.15.1 在锁（embed-fonts 默认 feature，vendor 实测 10M 打包字体） | /tmp/p6b-probe-typst 探针实测（cargo generate-lockfile + cargo vendor + du） |
| A6 typst deny 停批链 | `cargo deny check advisories`（workspace deny.toml 原样）FAILED，**6 error**：RUSTSEC-2026-0194/0195（quick-xml 0.38.4 **vulnerability**×2——quadratic runtime + NsReader 无界分配；修复线 ≥0.41.0；链 = citationberg 0.7.0 钉 quick-xml ^0.38.1（vendor Cargo.toml 实测）← hayagriva 0.10.1（typst-library 钉 ^0.10.1 实测）← typst-library）+ RUSTSEC-2025-0141（bincode 1.3.3 unmaintained ← syntect ← two-face ← typst-library）+ RUSTSEC-2026-0206（rustybuzz 0.20.1 unmaintained ← typst-layout/typst-pdf/krilla）+ RUSTSEC-2026-0192（ttf-parser 0.25.1 unmaintained ← fontdb/typst-library 全链）+ RUSTSEC-2024-0320（yaml-rust 0.4.5 unmaintained ← syntect）；deny licenses 另需 2 行（ar_archive_writer `Apache-2.0 WITH LLVM-exception` + roman-numerals-rs `0BSD OR CC0-1.0`——probe 实测 rejected） | /tmp/p6b-probe-typst `cargo deny check advisories` + `licenses` probe 实测 |
| A7 轻量 PDF 探针 | pdf-writer **0.15.0**（latest；MIT OR Apache-2.0；lock **6 包**；依赖 bitflags 2.13.2/itoa 1.0.19/memchr 2.8.3/ryu 1.0.23 **全在基线 432 同版本在位**）+ subsetter **0.2.6**（latest；MIT OR Apache-2.0；rust-version 1.85；forbid unsafe；**kurbo/skrifa/write-fonts 全 optional（variable-fonts feature），default-features=false 后唯一运行时依赖 rustc-hash 2.1.3（基线在位）**）+ skrifa **0.42.1**（rust-version 1.85）+ crc32fast **1.5.5**（cfg-if 基线在位零增量）——**四钉 exact 组合探针（/tmp/p6b-probe-final）deny 双检全绿零改动**（licenses ok + advisories ok，lock 19 条目）；**净增 8 包清单**：pdf-writer/subsetter/skrifa/read-fonts/font-types/bytemuck/bytemuck_derive/crc32fast（基线 grep 全零命中实测；**无 arrayvec**——skrifa 0.42.1→read-fonts 0.39.2 链零 arrayvec），KiB 实测合计 **9168 ≈ 9.0M**；kurbo 0.13.1/polycool 0.4.0/write-fonts 0.48.1/arrayvec 随 variable-fonts 剥除不进本批 | /tmp/p6b-probe-{pdf-writer,subsetter,crc32fast,final} 探针 + `cargo deny check` 双检实测 |
| A8 refinery 链 | migrations_pg 执行器唯一 = intel-db：`refinery::embed_migrations!("../../migrations_pg")`（lib.rs:135）+ `run_migrations`（:138-143，每文件事务+checksum）；V0007 尾（donor 两列+一索引）+ V0007:12 词形「扩词值须升 V0008+」；V0002 词形先例（TEXT PRIMARY KEY + 前缀行注 + CHECK 词表列内 + 零 FK 跨库引用标注）；refinery = workspace 依赖 0.8 postgres feature（Cargo.toml:77） | crates/intel-db/src/lib.rs:133-143 + migrations_pg/V0007__knowledge_donor.sql + Cargo.toml:77 实测 |
| A9 字体面 | 系统字体 fc-list = **2795 面**，CJK **83 面**（Noto Sans/Serif CJK ttc 在 /usr/share/fonts/noto-cjk/——Light/DemiLight/Medium/ExtraLight 权重族），Liberation Sans/Serif/Mono 在位；**零安装红线 = 字体从系统目录取，零打包入库**（D2） | fc-list 实测 |
| A10 轻量链冒烟 | /tmp/p6b-probe-pdf-writer：pdf-writer+subsetter(default-features=false)+skrifa 三 dep 离线 `cargo test --offline` **1 passed**（skrifa `FontRef::new` 解析 LiberationSans-Regular.ttf + `subsetter::subset` 非空产出 + `Pdf::new().finish()` 字节 `%PDF-` 头断言）——rustc 1.99.1 编译链接零错 | 探针 tests/smoke.rs 实测 |
| A11 SARIF 官方 schema | `https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json` 实测 HTTP **200**、**112768 字节**、`$schema: http://json-schema.org/draft-04/schema#`（draft-04）；资产文件非 crate 不进 vendor/deny 面（schemas/ 目录先例 = document-ir/report-ir） | curl 实测 |
| A12 DOCX 候选探针 | 六 crate probe lock 全录：docx 1.1.2 = 40 包（zip 0.5.13）/rust-docx 0.3.1 = 101 包（零 zip）/zavora-docx 0.1.4 = 133 包（zip 8.6.0）/docx_basic 0.1.0 = 85 包（zip 8.6.0）/ooxml 0.2.8 = 143 包（zip 0.6.6）/docx-rs 0.4.22（6A 已测）——**六者 advisories probe 全 FAILED**（A6 表逐条 RUSTSEC）；crates.io 无 zip 9.x 兼容 DOCX 写 crate（search docx/ooxml/docx-rs 实测） | /tmp/p6b-probe-{docx,rust-docx,zavora-docx,docx_basic,ooxml} 探针实测 |
| A13 krilla/printpdf 探针 | krilla 0.8.2（lock 71 包）advisories FAILED（rustybuzz/ttf-parser ×2 unmaintained）；printpdf 0.12.14（lock 132 包）FAILED（bincode 1.3.3 unmaintained）——均停批（SEC-0 级 ignore 可豁免但违本批零 ignore 增行纪律，D1） | probe 实测 |
| A14 图表面现状 | cetz/fletcher = typst 包生态（typst universe，非 crates.io）——typst 本体停批 + 库模式运行时包播种违零安装 → 停（D5）；web/package.json:23 `"echarts": "6.1.0"` 在位（web 报告页属观察池）；report-engine 树内零 svg 词形（grep 实测） | grep 实测 |
| A15 deny 面 | deny.toml 54 行：[licenses] allow 11 项（MIT/MIT-0/Apache-2.0/Unicode-3.0/Zlib/ISC/BSD-2-Clause/BSD-3-Clause/CDLA-Permissive-2.0/Unicode-DFS-2016）；[advisories] ignore 三条 unmaintained（SEC-0 词形）+ RUSTSEC-2026-0459 不 ignore 行注（vulnerability 有修复线由 bump 消除——本批 D1 引用词形）；**轻量 9 包许可全落既有 allow 集（probe licenses ok 零改动实测）→ 本批 deny.toml 零 diff** | deny.toml 全文实测 + probe 双检 |
| A16 vendor 命令词形 | WSW-8 :42：`cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' vendor vendor --locked`（仓库 .cargo/config.toml vendored-sources 覆盖下的在线路径）；M2 约定 1（Cargo.lock 与 vendor 一并提交） | docs/rip3/wsw/WSW-8-BATCH-REPORT.md:42 实测 |
| A17 report-engine 现状 | crates/report-engine 在 members（6A）：Cargo.toml 19 行（domain/serde/serde_json/tool-registry/comrak 五依赖）；src/lib.rs 四函数 re-export；tests 四面 26 测试（consistency 7/html_escape 5/negative 9/sarif_structure 5）+ fixtures 两份 IR JSON；**IR 冻结面 sha256 = 888e9210f1ba44a5706439adb7a486ee75f8c4b6cc3c871c6184b5eb577a499e**（判据 3 恒定钉） | crates/report-engine/* 实测 + 6A C19 |
| A18 PG 测试 skip 门 | intel-db/tests/document_domain.rs:21-30 `skip_unless_pg()` 词形（RP_TEST_PG_DSN 未设或不可达 → eprintln SKIP + return；判据 6「迁移幂等：refinery 二跑零变化」在既有测试面 :6）；verification-engine 零 migrations_pg 接线（lib.rs:23 行注「情报核零新表」） | crates/intel-db/tests/document_domain.rs:21-30 + crates/verification-engine/src/lib.rs:23 实测 |

## 2. 范围

### 2.1 允许改动（落点清单）

| 落点 | 类型 | 内容 | 步骤 |
|---|---|---|---|
| `Cargo.toml` | 修改 | D1/D3：`[workspace.dependencies]` 增四行 exact 钉（`pdf-writer = { version = "=0.15.0" }` / `subsetter = { version = "=0.2.6", default-features = false }` / `skrifa = { version = "=0.42.1", default-features = false }` / `crc32fast = { version = "=1.5.5" }`，行注选型记录指向本档 D1/D3——skrifa default-features=false 需 S2 冒烟复核（FontRef 可用性），不可剥则登记坏消息去钉）；**其余零触碰**（comrak :105 行、refinery :77 行零 diff）；members 零新增（report-engine 已在） | S2 |
| `Cargo.lock` | 重生成 | 432 → **440**（净增 8 = A7 清单；reqwest=1/zip=0 恒定）；`cargo vendor` 全量重生成（A16 词形） | S2 |
| `vendor/` | 增量 | 净增 8 目录 ≈9.0M（A7 清单对账）；既有 406 目录零修改零删除（多版本命名置换若发生按 6A 坏消息 3 词形登记）；**typst 链零目录零字节**（D1 停批词形） | S2 |
| `crates/report-engine/` | 增量 | D2/D3/D5/D6/D7：Cargo.toml 依赖面 +4 行；src/fonts.rs（FontSource + discover_system_fonts——唯一 I/O 面）；src/render/pdf.rs（render_pdf：skrifa cmap/glyph 解析 + subsetter 子集 + pdf-writer CID Type0 嵌入 + 手写分页排版）；src/render/docx.rs（render_docx：手写 OOXML）；src/ooxml.rs（stored-ZIP 容器 + CRC32——XLSX/PPTX 先例底座）；src/render/svg.rs（render_attack_path_svg）；src/qc.rs（QcCheck/QcReport + qc_pdf/qc_docx/qc_sarif）；tests/ 新增五面（pdf_render/docx_render/svg_diagram/qc_gates/qc_negative）+ tests/fixtures/ 复用 6A 两份 IR JSON + SARIF schema 校验 fixture；**既有 src/ 四文件与 tests/ 四面零 diff**（判据 3） | S3-S5 |
| `schemas/sarif/sarif-schema-2.1.0.json` | 新建 | D6③：官方 SARIF 2.1.0 schema 资产（A11 URL 实拉 + sha256 落批次报告 + 行注出处） | S5 |
| `migrations_pg/V0008__reports.sql` | 新建 | D4①：reports 表 + idx_reports_task（V0007 头注词形纪律） | S6 |
| `crates/intel-db/` | 增量 | D4③：src/report_repo.rs（store_report/load_report/list_reports_by_task——document_repo 词形同构）+ src/lib.rs 模块声明行（**intel-db 既有文件唯一 diff = lib.rs mod 行**）+ tests/reports_pg.rs（skip_unless_pg 词形） | S6 |
| `apps/server/selftest_p6b.sh` | 新建 | ck/sk 词形（selftest_p6a.sh:16-21 先例）：lock 440/reqwest=1/zip=0 三检 + vendor 增量目录计数 + deny 双检 + deny.toml 零 diff 反证 + IR schema sha256 恒定钉 + PDF/DOCX 质检载具（cargo test -p report-engine --test-threads=1）+ 生产库五文件只读指纹（1/1/1/0/0——6A C17 真实指纹值）+ 跨轨零 diff 反证 + SKIP 守卫预挂（cargo-deny/sqlite3 缺位） | S7 |
| `docs/phase0/PHASE-6B-BATCH-REPORT.md` | 新建 | 判据 1-12 实测对照 + 坏消息单列 + 观察池（§8）+ **Phase 7 前置输出专节**（reports PG 形状冻结声明 + 渲染函数签名钉定 + 质检三面 :545 映射表） | S8 |
| `docs/README.md` | additive | RIP3 批次表增一行（6A 先例词形） | S8 |

### 2.2 不修改

- **WSW 面**：crates/wsw-runtime/** 全树零 diff；WSW-7/WSW-8 链尾钉零随更；scanner.rs 零 Finding 写面红线零触碰。
- **MA 面**：SQLite 026/027/029 + rp_server 鉴权四件套与 SSE 面零 diff；caller/预算/协作链零触碰。
- **生产库**：migrations/platform.db 等**五文件全程 mode=ro** 只读指纹（1/1/1/0/0——6A C17 实测真实指纹值）；migrations/001-030 零 diff（**036 不立项**）；**生产 PG 实例零连接**（RP_TEST_PG_DSN 测试库 disposable；V0008 只在开发 PG 副本与测试库执行——V0007 头注词形）。
- **RTIS 面**：integrations/rtis/ 零 diff；RTIS 零直写红线零触碰；report_ir 内 rtis:// 引用只读透传（PDF/DOCX/SVG 产物中 `rtis://` 词形原样出现——零解析零改写）。
- **typst 链零引入**：typst/typst-pdf/typst-as-lib/krilla/printpdf 六 crate 零 lock 零 vendor 零字节（D1/D3 停批词形——`grep -c 'name = "typst"' Cargo.lock` = 0 反证）。
- **IR 冻结面零触碰**：schemas/report-ir/report-ir-v1.schema.json 零 diff（sha256 = 888e9210 恒定）；report-engine 既有 ir.rs/escape.rs/render/{markdown,html,json,sarif}.rs 零 diff；既有 tests/ 四面零 diff。
- **既有 crates 零 diff**：crates/** 除 report-engine 增量与 intel-db lib.rs mod 行 + report_repo.rs/tests 新文件外全树零改动。
- **web/**、**workers/python/**、**apps/server/rp_server.py** 零 diff（D8——web 报告页/rp_server 端点属观察池）；`workers/python/requirements.lock.txt` 零触碰。
- **CI 零改动**：.github/workflows/ci.yml 零 diff（新测试自动入 `cargo test --offline --workspace` 门禁词形）。
- **fmt 存量红**：943 hunks 零触碰零扩大；**新增文件必须 fmt 自净**（cargo fmt -p report-engine -p intel-db -- --check exit 0）。
- **宿主零安装**：cargo-deny 0.20.2 已在零升级零新装；零 sudo；零打包字体入库（D2——系统字体探测）；SARIF schema 属仓库资产文件非宿主安装。

## 3. 实施步骤（S1→S8 串行，每步过门再进）

1. **S1 基线核验**：`git status --porcelain` = 0 + HEAD = `7ee11904`；`grep -cE '^\[\[package\]\]' Cargo.lock` = 432 + reqwest = 1 + zip = 0；`ls vendor/ | wc -l` = 406 + `du -sh vendor/` ≈ 399M；`ls migrations/*.sql | wc -l` = 30 + `ls migrations_pg/` = V0001-V0007；A1-A18 锚点词形复核（deny allow 11 项 / IR schema sha256 / fmt 943 / selftest_p6a PASS=25）；**workspace 全量一次基线**：`CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace --no-fail-fast`（预期 1365 passed 零红；如有红先停批上报）；cargo-deny 双检 exit 0 基线；**门 = 基线数字表落批次报告底稿**。
2. **S2 vendor 跃迁**：Cargo.toml 四行 exact 钉（D1/D3）+ report-engine Cargo.toml 依赖面四行；`cargo update -w` 重解（6A 裁决 B 词形）+ `cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' vendor vendor --locked`（A16 词形）全量重生成；**门 = lock 440（+8 与 A7 清单逐一对应）+ reqwest = 1 + zip = 0 + typst 词形零命中反证 + vendor diff 只增不删（既有 406 目录零删除；置换面登记）+ `cargo build --offline --workspace` exit 0 + `cargo deny check advisories && cargo deny check licenses` 双检 exit 0 且 `git diff deny.toml` = 空（零增行）+ 轻量链冒烟复跑（A10 词形——skrifa FontRef + subsetter::subset + Pdf::finish 三断言）**。
3. **S3 PDF 渲染器**：src/fonts.rs（FontSource + discover_system_fonts + CJK 字体探测——skrifa cmap U+4E00-9FFF 探测面）+ src/render/pdf.rs（render_pdf：A4 页面布局——标题/执行摘要/Finding 清单/Finding 详情/附录五段投影 + 分页断点手写 + skrifa char→glyph→widths + subsetter 子集 + pdf-writer CID Type0 + ToUnicode cmap 嵌入）；每文件 `cargo fmt -p report-engine -- --check` exit 0 + clippy 零 warn；**门 = render_pdf(main fixture, 系统字体) 字节非空 + `%PDF-` 头 + `/Type /Page` 计数 ≥1 + 中文字节面 CJK 字体嵌入（/FontFile2 grep——A9 本机在位态）；中文判据含 SKIP 守卫（D2⑤）**。
4. **S4 DOCX 渲染器**：src/ooxml.rs（stored-ZIP 打包 + CRC32 + [Content_Types].xml/_rels 组装）+ src/render/docx.rs（render_docx：document.xml 手写序列化——标题/段落/表格三 XML 面 + escape 复用）；**门 = render_docx(main fixture) 字节非空 + PK 头 + EOCD 在位 + 逐条目 CRC32 全对（自解析断言）+ 中文 UTF-8 原样（word/document.xml 字节面 grep）**。
5. **S5 SVG + 质检面**：src/render/svg.rs（render_attack_path_svg——D5 单类最小面）+ src/qc.rs（D6 三面 + QcReport）+ `schemas/sarif/sarif-schema-2.1.0.json` 实拉（A11 URL + sha256 落档）+ qc_sarif 经 jsonschema_lite 校验 render_sarif 输出；**门 = 三质检函数对 S3/S4 产物 + render_sarif(main fixture) 全 pass + 负例三面（截断 PDF fail / CRC 篡改 DOCX fail / SARIF schema 校验拒绝面）逐个 fail 且 QcReport.message 非空（:545 保留错误信息词形）**。
6. **S6 V0008 + report_repo**：migrations_pg/V0008__reports.sql（D4① + V0007 头注词形纪律）+ intel-db/src/report_repo.rs + lib.rs mod 行 + tests/reports_pg.rs（skip_unless_pg）；**门 = `cargo build -p intel-db --offline` exit 0 + RP_TEST_PG_DSN 可达时 reports_pg.rs 全绿（幂等 + roundtrip + format_exports additive）+ 不可达时 SKIP 分开计数 + 生产库五文件 mode=ro 指纹不变**。
7. **S7 测试与门禁收口**：tests/ 五面新文件（pdf_render/docx_render/svg_diagram/qc_gates/qc_negative——fixture 复用 6A 两份 IR JSON）+ apps/server/selftest_p6b.sh（ck/sk + SKIP 守卫预挂——selftest_p6a.sh:16-21 词形）；**workspace 全量一次**：`CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace --no-fail-fast`（预期 = 1365 基线 + 本批新增测试数，零红）+ `cargo clippy --offline --workspace --all-targets -- -D warnings` exit 0 + `cargo fmt --all -- --check` = 943 hunks 恒定且 report-engine/intel-db 新文件不在 dirty 清单 + deny 双检 exit 0 + 生产库五文件 ro 指纹 + `git status --porcelain` 只含 §2.1 落点核对；**门 = §4 判据 1-2/3/11/12 全过**。
8. **S8 文档与收口**：PHASE-6B-BATCH-REPORT.md（判据对照 + 坏消息单列 + 观察池 + Phase 7 前置输出：reports PG 形状冻结声明 + 新函数签名钉定 + 质检三面 :545 十项映射表 + vendor 9 包许可台账）+ docs/README.md 索引行；显式 add 本批清单（禁 add -A——本批只落文件不 commit）。

## 4. 完成标准（判据表；验收 = 命令实跑回显）

| # | 判据 | 验收 |
|---|---|---|
| 1 | **vendor 跃迁声明兑现** | `grep -cE '^\[\[package\]\]' Cargo.lock` = 440（净增 8，A7 清单对应）；`grep -c 'name = "reqwest"' Cargo.lock` = 1 + `grep -c 'name = "zip"' Cargo.lock` = 0 + `grep -c 'name = "typst"' Cargo.lock` = 0 三反证；`git status --porcelain vendor/` 只增不删；pdf-writer/subsetter/skrifa/crc32fast 四 exact 钉 = `=0.15.0`/`=0.2.6`/`=0.42.1`/`=1.5.5`（Cargo.toml grep 对账） |
| 2 | **deny 双检零增行** | `cargo deny check advisories && cargo deny check licenses` 双 exit 0；`git diff deny.toml` = **空**（本批零许可缺口零 ignore 增行——A15；9 包许可台账表落批次报告） |
| 3 | **IR 冻结面零触碰** | `sha256sum schemas/report-ir/report-ir-v1.schema.json` = `888e9210f1ba44a5706439adb7a486ee75f8c4b6cc3c871c6184b5eb577a499e` 恒定；`git diff crates/report-engine/src/ir.rs crates/report-engine/src/escape.rs crates/report-engine/src/render/{markdown,html,json,sarif}.rs` = 空；既有 tests/ 四面零 diff |
| 4 | **新渲染函数签名钉定 + 零 I/O 反证** | D7 六签名 grep 逐条对账（render_pdf/render_docx/render_attack_path_svg/qc_pdf/qc_docx/qc_sarif + discover_system_fonts 独立面）；**render 函数零 I/O 反证**：render_{pdf,docx,svg} + qc_* 函数体内无 std::fs/std::process/Command 词形（grep 反证表——discover_system_fonts 为唯一 I/O 面，独立模块不在反证清单） |
| 5 | **IR→PDF 渲染对账（fixture IR→PDF 字节非空 + 页数断言）** | main fixture（2 Finding）+ 系统字体 → render_pdf 字节非空 + `%PDF-` 头 + `%%EOF` 尾 + `/Type /Page` 计数 = 排版预期页数（≥1 断言 + 计数与 /Count 一致）+ CJK 字体嵌入面（/FontFile2 词形 grep 命中——A9 本机在位态；SKIP 守卫分计）+ finding_id/title 中文原样在 PDF 字节流可见（ToUnicode cmap 面——文本可提取性以 qc_pdf 断言面承载） |
| 6 | **IR→DOCX 渲染对账** | main fixture → render_docx 字节非空 + `PK\x03\x04` 头 + central directory EOCD 在位 + 逐条目 stored CRC32 全对（自解析断言）+ `[Content_Types].xml` 与 `word/document.xml` 两零件在位 + 中文/finding_id/rtis_ref 在 document.xml 字节面 grep 同值（6A 判据 7 词形延续） |
| 7 | **质检三面正例（:545 十项映射）** | qc_pdf(main PDF)/qc_docx(main DOCX)/qc_sarif(main SARIF) 三面 QcReport 全 passed；:545 十项逐项 → 三面检查名映射表落批次报告（D6① 分组词形）；「导出失败保留错误信息」= QcCheck.message 结构化承载 |
| 8 | **质检负例** | 截断 PDF（尾部字节切除 fixture）→ qc_pdf 含 fail 项且 message 非空；CRC 篡改 DOCX（单字节翻转）→ qc_docx fail；SARIF 注入词表外 severity → qc_sarif fail（或 validate_ir 层拒收——负例面二选一实测落档） |
| 9 | **SARIF 官方 schema 校验（6A 坏消息 9 收口）** | schemas/sarif/sarif-schema-2.1.0.json 在位（sha256 落档）；render_sarif(main fixture) 经 jsonschema_lite 校验 = Ok；mixed fixture 同过；若 draft-04 兼容性失败 → 停批上报重新裁决 D6（负例面预登记） |
| 10 | **SVG 攻击路径面（图表关联 Evidence）** | render_attack_path_svg(main fixture)：`<svg` 起头 + XML 标签平衡断言 + attack_paths 步骤数 = 节点数断言 + evidence_refs 词形在 svg 字符串 grep 命中（可追溯）+ 中文原样；图表与内容一致 = 判据 7 质检③面承载 |
| 11 | **workspace 全量与静态门零红** | `cargo test --offline --workspace --no-fail-fast` 零红（1365 基线 + 本批新增测试数落档——awk `/^test result:/` 汇总口径 failed=0）+ `cargo clippy --offline --workspace --all-targets -- -D warnings` exit 0 + `cargo fmt --all -- --check` = 943 hunks 恒定且 crates/report-engine/**、crates/intel-db 新文件不在 dirty 清单（新代码 fmt 自净）+ deny 双检 exit 0 |
| 12 | **收口零越界** | 生产库五文件 mode=ro PRAGMA user_version = 1/1/1/0/0 逐文件回显 + `git status --porcelain` 只含 §2.1 落点（Cargo.toml/Cargo.lock/vendor 增量/report-engine 增量/intel-db 增量/migrations_pg V0008/schemas/sarif/selftest_p6b.sh/两份 md + docs/README 一行）+ ci.yml/web/workers/integrations/rtis/migrations/ 全零 diff 反证 |

## 5. 测试面（收口命令）

```bash
# 门禁基线（S1）
git status --porcelain | wc -l          # 0
git log --oneline -1                    # 7ee11904（6A 收口批在链）
grep -cE '^\[\[package\]\]' Cargo.lock  # 432 → S2 后 440
grep -c 'name = "reqwest"' Cargo.lock   # 1（恒定双检——WSW-8 §7.4 词形）
grep -c 'name = "zip"' Cargo.lock       # 0（恒定反证）
grep -c 'name = "typst"' Cargo.lock     # 0（typst 链零引入反证——D1）
ls vendor/ | wc -l                      # 406 → S2 后 414（+8）
du -sh vendor/                          # 399M + ~9.0M ≈ 408M（±目录粒度）
ls migrations/*.sql | wc -l             # 30（036 不立项——零 diff）
ls migrations_pg/*.sql | wc -l          # 7 → S6 后 8（V0008）
# Rust 全量（workspace 一次必须跑；vendor 跃迁后全量重测收口）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace --no-fail-fast
# 静态门
CARGO_TARGET_DIR=/home/emo/rt3-target cargo clippy --offline --workspace --all-targets -- -D warnings
cargo fmt --all -- --check > /tmp/fmt_p6b.txt 2>&1; grep -c "^Diff in" /tmp/fmt_p6b.txt          # = 943（恒定）
grep -c "report-engine\|report_repo" /tmp/fmt_p6b.txt                                            # 0（新代码 fmt 自净）
cargo deny check advisories && cargo deny check licenses                                         # 双检 exit 0
git diff deny.toml | wc -l                                                                       # 0（零增行反证——D1）
# vendor 跃迁对账（S2）
git status --porcelain vendor/ | grep -c "^ D\|^D"                                               # 0（既有目录零删除）
sha256sum schemas/report-ir/report-ir-v1.schema.json                                             # 888e9210…（恒定钉）
# report-engine 判据面（S3-S5，定向串行）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test -p report-engine --offline -- --test-threads=1  # 全绿（测试数落档）
grep -rn "render_pdf\|render_docx\|render_attack_path_svg\|qc_pdf\|qc_docx\|qc_sarif" crates/report-engine/src/lib.rs
grep -rn "std::fs\|std::process\|Command" crates/report-engine/src/render/pdf.rs crates/report-engine/src/render/docx.rs crates/report-engine/src/render/svg.rs crates/report-engine/src/qc.rs | wc -l   # 0（渲染/质检零 I/O 反证）
# SARIF schema 资产（S5）
sha256sum schemas/sarif/sarif-schema-2.1.0.json                                                  # sha256 落批次报告
# intel-db 判据面（S6，PG skip 门）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test -p intel-db --offline reports_pg -- --test-threads=1   # RP_TEST_PG_DSN 可达=全绿 / 不可达=SKIP 分计
# selftest（S7）
bash apps/server/selftest_p6b.sh                                                                 # PASS/SKIP 分开计数 FAIL=0
# 生产库只读指纹（S7）
for f in migrations/platform.db migrations/platform-dryrun.db migrations/platform-fullbatch.db migrations/platform-governance.db migrations/platform-search.db; do sqlite3 "file:$f?mode=ro" "PRAGMA user_version"; done   # 1 1 1 0 0（全程 mode=ro）
# 零 diff 反证（S7）
git status --porcelain -- crates/wsw-runtime/ crates/verification-engine/ crates/agent-engine/ web/ workers/ integrations/rtis/ migrations/ .github/ | wc -l   # 0
git status --porcelain | wc -l   # 仅 §2.1 落点（≤12 行）
```

基线记录（本任务书写作时点 2026-11-23 实测）：

- HEAD `7ee11904` 干净树（6A 收口批在链）；Cargo.lock 432 / reqwest=1 / zip=0；vendor 406 目录 399M。
- migrations = 30（036 不立项延续）；migrations_pg = V0001-V0007（reports 表零建）。
- typst 停批链：lock +176 / vendor +144M / advisories 6 error（2 vulnerability semver 堵死 + 4 unmaintained）+ licenses 2 缺口（A5/A6）。
- 轻量链：+8 包 ≈9.0M / deny 双检全绿零改动 / 离线编译冒烟 1 passed（A7/A10）。
- 系统字体 2795 面、CJK 83 面在位（A9）；SARIF 官方 schema 112768 字节 draft-04 可得（A11）。
- workspace 基线 1365 passed 零红（6A C11）；fmt 存量红 943 hunks；rustc/cargo 1.99.1；cargo-deny 0.20.2 用户级。

## 6. 红线（八条，违反任一即停批上报）

1. **Rust-First + 渲染纯函数零 I/O**：render_pdf/render_docx/render_attack_path_svg/qc_pdf/qc_docx/qc_sarif 全部纯函数零 I/O 零子进程（字体字节经 FontSource 参数注入——D2 分层）；唯一 I/O 面 = discover_system_fonts 独立函数；零 Python/JS/shell 渲染路径；禁 panic（缺字体降级 .notdef + QcReport fail 词形）。
2. **零新依赖政策 = vendor+deny 新基线纪律**：本批依赖增量仅限 pdf-writer/subsetter/skrifa/crc32fast 四 crate（exact 钉；subsetter/skrifa default-features=false——variable-fonts 剥除坐实 A7）；第五 crate 禁临场引入——需第五 crate 即停批重新裁决；vendor diff 只增不删（既有 406 目录零删除；多版本命名置换按 6A 坏消息 3 词形登记）；Cargo.lock 440 与 reqwest=1/zip=0 为本批新基线；**deny.toml 零 diff**（[licenses] 零增行——轻量链全落既有 allow 集 A15；[advisories] 零 ignore 增行——typst 停批链四 unmaintained 与二 vulnerability 不进 ignore，vulnerability ignore 需独立安全裁决违 RUSTSEC-2026-0459 不 ignore 词形）；typst/typst-pdf/typst-as-lib/krilla/printpdf 零引入反证。
3. **IR 冻结面 sha256 不变 + 四渲染零改动**：schema sha256 = `888e9210…` 恒定（判据 3）；后续批禁改字段语义只 additive（6A 冻结面词形）；既有 render_markdown/render_html/render_json/render_sarif + validate_ir/from_json 签名与实现零 diff；禁在渲染器内维护第二份字段清单/第二份 schema；rtis_ref 词形单源 `rtis://poc/<id>` 零解析透传。
4. **迁移纪律 V0008 refinery**：migrations_pg 仅 V0008__reports.sql 一个新文件（refinery 每文件事务+checksum+幂等——V0007 头注词形）；**已应用的迁移文件禁删禁改**（checksum 陷阱）；migrations/001-030 零 diff + 036 不立项（reports 属 PG 面）；坐实需第三张表 → 停批上报按 028 先例重新立项，禁临场加迁移。
5. **生产库零触碰**：五文件 mode=ro 只读指纹逐文件回显（1/1/1/0/0——6A C17 真实指纹值）；禁任何写入路径；**生产 PG 实例零连接**（RP_TEST_PG_DSN 开发副本/测试库面；V0008 只在开发 PG 副本与测试库执行——V0007 头注词形；RTIS 零直写红线零触碰）。
6. **宿主零安装**：cargo-deny 0.20.2 已在零升级；pdf-writer/subsetter/skrifa/crc32fast 经 cargo vendor 入 git 树（rsproxy 拉取一次属实施步）；零新装 CLI 工具、零 sudo、零全局 cargo install；**零打包字体入库**（系统字体探测——D2；OFL 打包字体后备属观察池）。
7. **边界纪律（D8 + 观察池延续面）**：Phase 7 知识回流（:672）、双解释生成器（:525）、IR 装配读库面、rp_server 报告端点、web 报告页（echarts 复用面）、PPTX/XLSX（:539 第二批）、Diagram Engine 七类全模块——本批零实现零触碰，逐条登记观察池；rp_server.py 零 diff。
8. **禁 commit/add -A/push**（本批只落文件，工作树留主会话审查收口——S8 显式 add 仅登记清单）；fmt 存量红 943 hunks 零触碰（新增文件 fmt 自净不入 dirty 清单——判据 11）；既有测试零改动（report-engine/intel-db 测试全为新文件，零触碰既有断言面）。

## 7. 回滚

- **vendor 面 revert（D1/D3）**：`git checkout HEAD -- Cargo.toml Cargo.lock` + `git status vendor/` 比对后 `git clean -fd vendor/`（净增 8 目录属 untracked 增量，既有 406 目录不受影响）；或精确 `rm -rf vendor/{8 目录清单}` + 两文件 checkout。回滚后 `cargo build --offline --workspace` 必须复绿（432 基线自证）。
- **V0008 面 revert（D4）**：**未应用态** = `rm migrations_pg/V0008__reports.sql` + report_repo.rs 删除 + lib.rs mod 行 checkout——零残留；**已应用态**（开发 PG 副本/测试库）= `DROP TABLE reports;` + `DELETE FROM refinery_schema_history WHERE version = 8;` 或整库重建（RP_TEST_PG_DSN 测试库 disposable 词形）——**refinery checksum 纪律：已应用迁移文件禁删禁改**（V0007 头注词形），文件删除必须先清 history 行。
- **渲染器/质检面 revert**：rm report-engine 新增 src/tests 文件 + Cargo.toml 依赖四行 checkout + lib.rs re-export 行 checkout + schemas/sarif/ 删除——工作树态零残留；无 commit 无 push 无 CI 副作用（ci.yml 零 diff）。
- **数据库零回滚面**：生产库五文件全程 mode=ro（红线 5）——无回滚义务；SQLite 面零迁移（036 不立项）。
- **宿主零回滚面**：零安装零升级（红线 6）；SARIF schema 资产文件随 S5 回滚删除即可。
- **实施中止态**：S2 完成后中止 = vendor 基线已跃迁但渲染器未落——按上两条 revert 全撤（vendor 跃迁不单独留存——半跃迁态禁跨批遗留，lock 440 无消费方即回滚——6A 词形同构）。

## 8. 交付物

1. **vendor+deny 新基线（零 deny 增行版）**：Cargo.toml 四 crate exact 钉 + Cargo.lock 432→440（净增 8 清单）+ vendor 净增 8 目录 ≈9.0M 纯增量 + deny.toml 零 diff——判据 1/2 回显逐条入报告；8 包许可台账表（A15 口径——全落既有 allow 集）。
2. **PDF 渲染器 + 字体装配面**：render_pdf（CID Type0 + subsetter 子集 + skrifa 解析 + 手写分页）+ fonts.rs（discover_system_fonts 零安装字体面 + CJK 探测 + skip 守卫）——判据 4/5 回显；IR→PDF 渲染对账表（fixture IR→字节非空 + 页数断言 + 字体嵌入面实测）。
3. **DOCX 渲染器 + OOXML 容器底座**：render_docx（手写 OOXML）+ src/ooxml.rs（stored-ZIP + CRC32——XLSX/PPTX 第二批格式复用底座，D3③）——判据 6 回显；六 crate 停批链 probe 实录（A12 表）落批次报告。
4. **导出质检三面 + SARIF schema 收口**：qc_pdf/qc_docx/qc_sarif + QcReport 结构化错误信息（:545 原文词形）+ schemas/sarif/sarif-schema-2.1.0.json 资产（6A 坏消息 9 收口）——判据 7/8/9 回显；:545 十项 → 三面检查名映射表。
5. **SVG 攻击路径最小面**：render_attack_path_svg（D5 单类）——判据 10 回显（图表关联 Evidence 验收点首次落地）。
6. **V0008 reports PG 落库 + report_repo**：migrations_pg/V0008__reports.sql（D4① 形状）+ intel-db report_repo（store/load/list_by_task）+ reports_pg.rs 测试（幂等 + roundtrip）——判据 3/12 回显；**reports PG 形状冻结声明 = Phase 7 前置输出**（report_id rep- 前缀 + report_ir jsonb = IR v1 冻结面 + format_exports additive 台账形状，后续批禁改列语义）。
7. **selftest_p6b.sh + 门禁收口**：lock 440/reqwest=1/zip=0 三检 + deny 双检零增行反证 + workspace 全量零红 + clippy/fmt 门（943 恒定）+ 生产库五库指纹 + 零 diff 反证——判据 11/12 回显。
8. **docs/phase0/PHASE-6B-BATCH-REPORT.md**：判据 1-12 对照 + 坏消息单列（crate 版号漂移/vendor diff 漂移/skrifa default-features 可剥性复核/draft-04 兼容面）+ 观察池 + Phase 7 前置输出专节 + docs/README.md 索引行。

**观察池（6C/Phase 7 立项对表，本批零实现）**：①双解释生成器（:525 §8.3 七问——IR 双解释字段位已承载）；②IR 装配面（verification-engine store / agent-engine evidence 读库 → ReportIr）；③Diagram Engine 全模块（:531 七类——本批仅 attack_paths SVG 单类）；④rp_server 报告端点 + web 报告页（echarts 6.1.0 复用面）；⑤PPTX/XLSX（:539 第二批——ooxml.rs 底座已备）；⑥**Phase 7 知识回流全面**（:672——Finding→候选知识/Evidence 溯源/上游更新监测/失效链接检查/旧 POC 检查/自动质量巡检/维护报告）；⑦krilla/printpdf 复审时机（RUSTSEC-2026-0206/0192 解除或 SEC-0 级裁决立项时）；⑧typst 复审时机（citationberg 升 quick-xml ≥0.41 后 deny 门可达）；⑨打包字体后备（CI 字体缺失面——OFL 许可面预审）；⑩Latin kerning/两端对齐排版质量（手写排版 tradeoff——pdf-writer 手写面 enhancement）；⑪reports PG 表 research_tasks/findings 建表批（V0009+ 候选——FK 目标补齐）。

---

## 9. 复现（本任务书侦察数字复核命令与预期回显）

```bash
cd /media/emo/code/pensting/research-platform
git log --oneline -1                 # 7ee11904 Phase 6a Report IR + MD/HTML/JSON 渲染器收口…
git status --porcelain | wc -l       # 0
grep -cE '^\[\[package\]\]' Cargo.lock        # 432
grep -c 'name = "reqwest"' Cargo.lock         # 1
grep -c 'name = "zip"' Cargo.lock             # 0
ls vendor/ | wc -l                   # 406
du -sh vendor/                       # 399M
ls migrations/*.sql | wc -l          # 30
ls migrations_pg/*.sql | wc -l       # 7
sed -n '543,545p' docs/rip3/RIP3-ARCHITECTURE-UPGRADE-PLAN.md   # §8.6 十项原文
sed -n '132,133p' docs/rip3/03-DOMAIN-MODEL.md                  # reports 冻结形状
grep -c typst docs/rip3/RIP3-ARCHITECTURE-UPGRADE-PLAN.md       # 0（PLAN 零引擎词形）
sha256sum schemas/report-ir/report-ir-v1.schema.json            # 888e9210f1ba44a5706439adb7a486ee75f8c4b6cc3c871c6184b5eb577a499e
cargo fmt --all -- --check 2>&1 | grep -c "^Diff in"            # 943（存量红恒定）
fc-list | wc -l                     # 2795（系统字体面）
fc-list | grep -ci "CJK"            # 83（中文字体在位）
curl -sL -o /tmp/sarif-check.json https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json && wc -c /tmp/sarif-check.json   # 112768
# typst 停批链复现（探针）
mkdir -p /tmp/p6b-probe-repro/src && cd /tmp/p6b-probe-repro
printf '[package]\nname = "p6b-probe-repro"\nversion = "0.0.0"\nedition = "2021"\nlicense = "Apache-2.0"\n\n[dependencies]\ntypst = { version = "=0.15.1" }\ntypst-pdf = { version = "=0.15.1" }\n' > Cargo.toml
echo "pub fn x(){}" > src/lib.rs && cargo generate-lockfile
grep -cE '^\[\[package\]\]' Cargo.lock    # 299（typst+typst-pdf 全树）
# 轻量链复现（探针——任务书四钉 exact 组合）
mkdir -p /tmp/p6b-pdf-repro/src && cd /tmp/p6b-pdf-repro
printf '[package]\nname = "p6b-pdf-repro"\nversion = "0.0.0"\nedition = "2021"\nlicense = "Apache-2.0"\n\n[dependencies]\npdf-writer = { version = "=0.15.0" }\nsubsetter = { version = "=0.2.6", default-features = false }\nskrifa = { version = "=0.42.1" }\ncrc32fast = { version = "=1.5.5" }\n' > Cargo.toml
echo "pub fn x(){}" > src/lib.rs && cargo generate-lockfile
grep -cE '^\[\[package\]\]' Cargo.lock    # 19（轻量链全树：8 净增 + 10 基线复用 + 自身）
```
