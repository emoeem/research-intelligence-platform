# PHASE-6B 批次报告 · PDF/DOCX 渲染器 + 导出质检 + V0008 reports 落库（收口态）

- 批次：PHASE-6B（任务书 [PHASE-6B-TASK.md](PHASE-6B-TASK.md)；RIP3 §8.5 第一批格式 PDF/DOCX 补齐 + §8.6 导出质检三面 + 03-DOMAIN-MODEL :132-133 reports PG 表 V0008）
- 实施日：2026-11-23（侦察同日落稿；S1-S5 实施于同日 WIP 批 `deba165b`——配额耗尽中断保全；S6-S8 收口于同日接手续批，HEAD 基点 `18a657c7`）
- 状态：**收口态**（判据 1-12 全过；workspace 1389 passed 零红；vendor+deny 二战兑现零 deny 增行）
- 实施裁决：任务书 §0 D1-D8 内嵌裁决照办，无临场重裁（skrifa `features=["std"]` 一处实施面修正见坏消息 1）

---

## 1. 判据 1-12 实测对照（验收 = 命令实跑回显）

| # | 判据 | 实测 |
|---|---|---|
| 1 | vendor 跃迁声明兑现 | `grep -cE '^\[\[package\]\]' Cargo.lock` = **440**（净增 8 与 A7 清单逐一对应）；reqwest=1 / zip=0 / typst=0 三反证实测；四 exact 钉 Cargo.toml grep 各 =1（`pdf-writer "=0.15.0"` / `subsetter "=0.2.6"` default-features=false / `skrifa "=0.42.1"` default-features=false features=["std"] / `crc32fast "=1.5.5"`）；`git status vendor/` 只增不删（8 目录 deba165b 入库，`cargo vendor --locked` 复跑 tracked diff=0） |
| 2 | deny 双检零增行 | `cargo deny check advisories` = ok + `cargo deny check licenses` = ok（selftest_p6b ==3 实测）；`git diff deny.toml` = **空**（零许可缺口零 ignore 增行——A15 兑现；八包许可台账 §4） |
| 3 | IR 冻结面零触碰 | `sha256sum schemas/report-ir/report-ir-v1.schema.json` = `888e9210f1ba44a5706439adb7a486ee75f8c4b6cc3c871c6184b5eb577a499e` 恒定；`git diff` ir.rs/escape.rs/render/{markdown,html,json,sarif}.rs + 既有 tests/ 四面 = **零 diff**（fmt 定点自净后复核） |
| 4 | 新渲染函数签名钉定 + 零 I/O 反证 | 七签名 re-export grep 各 =1（selftest ==5）；渲染/质检零 I/O 反证：render/{pdf,docx,svg}.rs + qc.rs 内 `std::fs\|std::process\|Command` grep = **0**（selftest ==5）；discover_system_fonts 唯一 I/O 面独立 fonts.rs（grep =1） |
| 5 | IR→PDF 渲染对账 | pdf_render.rs 4 测试：字节非空 + `%PDF-` 头 + `%%EOF` 尾；`/Type /Page` 计数 = Pages `/Count`；CJK 嵌入 `/FontFile2`（本机 A9 在位态，SKIP 守卫分计）；finding_id 经 `/Subject` 字面 grep；中文 title 经 ToUnicode CMap UTF-16BE hex 词形可见；同 IR 同 fonts 两次渲染字节一致（确定性） |
| 6 | IR→DOCX 渲染对账 | docx_render.rs 5 测试：`PK\x03\x04` 头 + EOCD 在位 + 逐条目 stored CRC32 全对（自解析断言）+ `[Content_Types].xml` 与 `word/document.xml` 两零件在位 + 中文/finding_id/rtis_ref 在 document.xml 字节面 grep 同值 + 确定性 + mixed fixture |
| 7 | 质检三面正例 | qc_gates.rs 6 测试：qc_pdf/qc_docx/qc_sarif 对 main fixture 全 pass + mixed fixture 三面全 pass + SARIF schema 资产与编译期内嵌字节一致 + SVG 对账；QcCheck.message 结构化承载（pass 时空串）；**qc_pdf 正例对 CJK 面分环境计**：系统无 CJK 字体时 pdf_sections_present/pdf_attack_paths/pdf_code_blocks 三项按 SKIP 分计（D2⑤），其余八项在任何环境都是硬判据（坏消息 7——CI runner 实测面） |
| 8 | 质检负例 | qc_negative.rs 5 测试：截断 PDF fail + CRC 篡改 DOCX fail + SARIF 词表外 severity fail + SARIF version 错 fail + 渲染降级无 CJK 字体面 qc_pdf fail（D2④ fail 不 abort 承载实测）——全部 message 非空 |
| 9 | SARIF 官方 schema 校验 | `schemas/sarif/sarif-schema-2.1.0.json` 在位（sha256 `c3b4bb2d6093897483348925aaa73af03b3e3f4bd4ca38cef26dcb4212a2682e`；oasis-tcs/sarif-spec main 实拉 112768 字节 draft-04）；qc_sarif 经既有 `tool_registry::jsonschema_lite::validate_instance` 校验 render_sarif 输出 = Ok（main + mixed 两 fixture 同过；draft-04 兼容性成立，停批预案未触发） |
| 10 | SVG 攻击路径面 | render_attack_path_svg：`<svg` 起头 + XML 标签平衡 + attack_paths 步骤数 = 节点数断言 + evidence_refs 词形 grep 命中 + 中文原样（qc_gates.rs:123 承载） |
| 11 | workspace 全量与静态门零红 | `cargo test --offline --workspace --no-fail-fast` = **1389 passed / 0 failed**（1365 基线 + 6B report-engine 20 + reports_pg 4；195 个 test result 行 awk 汇总）；clippy `--workspace --all-targets -- -D warnings` exit 0；fmt `--all -- --check` = **943 hunks 恒定**且 report-engine/intel-db 新文件零命中（坏消息 5——WIP 态 61 hunks 拦下后定点自净）；deny 双检 exit 0 |
| 12 | 收口零越界 | 生产库五文件 mode=ro PRAGMA user_version = **1/1/1/0/0** 逐文件回显（selftest ==8）；`git status --porcelain` 只含 §2.1 落点；ci.yml/web/workers/integrations/rtis/migrations/ 全零 diff 反证（selftest ==9 跨轨 0 行 + migrations/*.sql=30 恒定） |

## 2. 落点清单（§2.1 兑现面）

| 落点 | 类型 | 内容 |
|---|---|---|
| `Cargo.toml` | 修改 | 四行 exact 钉 + 选型行注（D1/D3；:105 comrak 行零 diff） |
| `Cargo.lock` | 重生成 | 432→440（净增 8 = A7 清单）；reqwest=1/zip=0/typst=0 恒定 |
| `vendor/` | 增量 | 406→414 目录（八新目录全在位；既有 406 零修改零删除） |
| `crates/report-engine/` | 增量 | Cargo.toml +4 依赖行；src/fonts.rs（FontSource + discover_system_fonts 唯一 I/O 面 + skrifa cmap U+4E00-9FFF CJK 探测）；src/render/{pdf,docx,svg}.rs；src/ooxml.rs（stored-ZIP + crc32fast——XLSX/PPTX 第二批底座）；src/qc.rs（QcCheck/QcReport + qc_pdf/qc_docx/qc_sarif）；src/lib.rs re-export 七签名；tests/ 四新文件 20 测试（pdf_render 4/docx_render 5/qc_gates 6/qc_negative 5）+ fixtures 复用 6A 两份 IR JSON |
| `schemas/sarif/sarif-schema-2.1.0.json` | 新建 | 官方 SARIF 2.1.0 schema 资产（112768 字节；sha256 `c3b4bb2d…2682e`；qc.rs 编译期 include_str! 单源内嵌） |
| `migrations_pg/V0008__reports.sql` | 新建 | reports 表（report_id TEXT PK rep- 前缀 / task_id NOT NULL 稳定 ID 引用非跨库 FK / report_ir JSONB NOT NULL / format_exports JSONB NOT NULL DEFAULT '{}' / created_at TIMESTAMPTZ）+ idx_reports_task；零 CHECK 词表（IR schema 单源承载）；V0007 头注词形纪律 |
| `crates/intel-db/` | 增量 | src/report_repo.rs（store_report/load_report/list_reports_by_task/upsert_format_export——document_repo 同构；只收 serde_json::Value 不引 report-engine 类型）；src/lib.rs 一行 `pub mod report_repo;`（既有文件唯一 diff）；tests/reports_pg.rs 四测试（skip_unless_pg 词形 + 迁移就绪内挂） |
| `apps/server/selftest_p6b.sh` | 新建 | 十段 36 ck：lock 三检四钉对账 / vendor 414 八目录 / deny 双检零 diff / 双 sha256 钉 / 七签名 + 零 I/O 反证 / report-engine 46 载具 / reports_pg PG 实跑 4（rip-pg17 钉 sha256 镜像一次性容器）/ 生产库五文件 ro 指纹 / 跨轨零 diff / 落点核对；SKIP 守卫预挂（p6a:16-21 词形） |
| `apps/server/selftest_p6a.sh` | 连带收口 | 五处陈旧钉更新（坏消息 4——RT-7→wsw8 先例 8b79bd26 词形）：lock 440 / vendor 414 / 测试数 46 / migrations_pg 8 / 跨轨清单 migrations_pg 移交 p6b selftest；其余 6A 面零改动，复跑 **25/0** |
| `docs/phase0/PHASE-6B-BATCH-REPORT.md` | 新建 | 本档 |
| `docs/README.md` | additive | RIP3 批次表增一行（6A 行后插入） |

## 3. 坏消息单列

1. **skrifa `features=["std"]` 实施面修正**：任务书 D1 钉面写 default-features=false；S2 冒烟实测裸剥不可编译（read-fonts 0.39.2 varc.rs no_std 缺 libm 三错 E0599）→ 补 `features=["std"]` 透传 read-fonts/std。零新依赖、净增 8 包清单不变、deny 双检全绿不变；Cargo.toml :121-126 行注在位（任务书 §2.1 S2 预登记复核点按预案兑现）。
2. **report_repo SQL 两处 PG 实测修正**：①`jsonb_build_object($2,…)` 参数类型 PG 不可推（E42P18）→ `$2::text` 显式铸型；②entry 传参 String→Jsonb WrongType → 改 `serde_json::to_value` 直传 Value。均为 reports_pg.rs 集成测试逼出的实现面修正，SQL 语义（`format_exports || jsonb_build_object` additive upsert）零变化。
3. **reports_pg.rs 测试序修正**：四测试字母序下迁移测试（v0008_…）排尾，先跑三测试遇 `relation "reports" does not exist`——document_domain.rs 先例靠 `m < s` 字母序侥幸成立；修正 = run_migrations 挪进 skip_unless_pg（refinery 幂等，每测试独立就绪，顺序无关）。
4. **selftest_p6a.sh 连带收口五处陈旧钉**：6b vendor 跃迁与测试增量使 6A 自检钉值过时（lock 432≠440 / vendor 406≠414 / 测试数 26≠46 / migrations_pg 7≠8 / 跨轨清单含 migrations_pg 误报 V0008 untracked）。按 RT-7 批连带收口 selftest_wsw8 先例（commit 8b79bd26）更新五处，其余 6A 面零改动，复跑 25/0。
5. **WIP 批 S3-S5 文件 fmt 未自净**：`deba165b` 态 fmt --all --check = 1004 hunks（943 存量 + 6b 新文件 61 hunks，S7 fmt 门拦下）——`cargo fmt -p report-engine` 定点自净（脏面恰为 6b 十文件，6A 面与 IR 钉名单零命中）复跑 943 恒定 + report-engine 零命中 + 46 测试复绿。任务书红线 8「新增文件必须 fmt 自净」在 S7 门处补兑。
6. **SARIF schema 资产 sha256 落档**：`c3b4bb2d6093897483348925aaa73af03b3e3f4bd4ca38cef26dcb4212a2682e`（任务书 A11 只落字节数 112768 未落 hash——本批补钉；selftest ==4 在位断言）。
7. **CI runner 无 CJK 字体面（D2⑤ 守卫首实战）**：`18a657c7` 的 CI run 45094990554 红在 `qc_pdf_main_fixture_all_pass`——runner（ubuntu-latest）只有 Liberation/DejaVu 面，2000+ 面 CJK 字体是**本机环境特权**；render_pdf 按 D2④ 正确降级 .notdef，qc_pdf 的 pdf_sections_present/pdf_attack_paths 如实 fail（中文词形不在 ToUnicode 面），而正例测试却硬断言 all_passed——**是测试面缺陷不是渲染器缺陷**。修法：qc_gates.rs 加 `has_cjk_face`（skrifa cmap 三码点探测，pdf_render.rs:37 同款）+ `CJK_DEPENDENT_CHECKS` 三分组 + `assert_all_pass_cjk_aware`（无 CJK 面时三项 SKIP 分计，其余八项硬判）。本机 mount namespace 模拟（仅 LiberationSans 可见）实测：`qc_pdf (11 checks, cjk_face=false)` 两 SKIP + 九 pass，整 crate 46 全绿；CI 环境即该分支的真实执行面。归入 CI 环境敏感三型之外的第④型：**字体面环境差**（后续渲染批一律在 CJK 依赖断言前做 face 探测）。

## 4. vendor 八包许可台账（deny 零增行兑现）

| crate | version | license | deny 面 |
|---|---|---|---|
| pdf-writer | 0.15.0 | MIT OR Apache-2.0 | 既有 allow 集零增行 |
| subsetter | 0.2.6 | MIT OR Apache-2.0 | 同上（default-features=false 剥 variable-fonts） |
| skrifa | 0.42.1 | MIT OR Apache-2.0 | 同上（features=["std"]——坏消息 1） |
| read-fonts | 0.39.2 | MIT OR Apache-2.0 | 同上（skrifa 传递） |
| font-types | 0.11.3 | MIT OR Apache-2.0 | 同上（read-fonts 传递） |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT | 同上（Zlib/Apache-2.0/MIT 全在 allow） |
| bytemuck_derive | 1.12.1 | Zlib OR Apache-2.0 OR MIT | 同上 |
| crc32fast | 1.5.5 | MIT OR Apache-2.0 | 同上 |

`cargo deny check licenses` = ok（零 rejected）；`cargo deny check advisories` = ok（零 RUSTSEC 命中；typst 停批链 6 error 与六 DOCX crate 停批链零引入反证 lock typst=0/zip=0）。

## 5. 质检三面映射表（:545 十项原文 → 检查名，D6① 词形）

| :545 原文项 | qc_pdf | qc_docx | qc_sarif |
|---|---|---|---|
| 章节完整 | pdf_sections_present | docx_sections_present | sarif_runs（runs 结构面） |
| 图片存在且未损坏 | pdf_images | docx_images | —（6b 零图片位恒 pass 空集） |
| 目录正确 | pdf_page_count（页序面） | docx_heading_structure（标题层级目录面） | sarif_tool_driver（driver 结构面） |
| 页码正确 | pdf_page_count + pdf_eof | docx_parts（包完整性面） | sarif_artifacts |
| 中文字体正确 | pdf_cjk_font_embedded + pdf_to_unicode（fail 不 abort——D2④） | —（DOCX 文本 XML 由 Word 端渲染——D3④ 零字体面） | — |
| 代码块完整 | pdf_code_blocks | docx_parts | — |
| 表格不溢出 | —（PDF 表格以段落化排版承载） | docx_table_closed（OOXML 表格闭合面） | — |
| 引用可追溯 | pdf_evidence_refs | docx_fact_traceability | sarif_six_ids |
| Finding 与 Evidence 对应 | pdf_finding_refs | docx_fact_traceability | sarif_finding_mapping + sarif_six_ids |
| 图表与内容一致 | pdf_attack_paths（SVG 节点数=步骤数断言——qc_gates:123 承载） | — | — |

「导出失败保留错误信息并允许重新导出」= QcCheck.message 结构化承载（fail 非 panic 非 abort，重导出即重渲染重质检）；SARIF official schema 校验项 = sarif_official_schema（judge 面 9）。

## 6. Phase 7 前置输出专节（本批交付的下游契约）

1. **reports PG 形状冻结声明**：`reports` 表五列语义冻结（report_id TEXT PK rep- 前缀 / task_id TEXT NOT NULL 稳定 ID 引用非跨库 FK / report_ir JSONB = Report IR v1 冻结面 sha256 888e9210 / format_exports JSONB additive-only 台账 `{format:{sha256,bytes,exported_at}}` / created_at）+ idx_reports_task。后续批禁改列语义只 additive（V0009+ 候选面：research_tasks/findings 建表补 FK 目标——观察池⑪）；写入口唯一 = report_repo 四函数（store/load/list/upsert_format_export）；refinery checksum 纪律：已应用迁移文件禁删禁改。
2. **渲染函数签名钉定**（D7）：`FontSource{data,index}` / `render_pdf(&ReportIr, &[FontSource]) -> Vec<u8>` / `render_docx(&ReportIr) -> Vec<u8>` / `render_attack_path_svg(&ReportIr) -> String` / `qc_pdf/qc_docx/qc_sarif` / `discover_system_fonts() -> Vec<FontSource>`（唯一 I/O 面）——既有四渲染 + validate_ir/from_json 零改动；后续批（rp_server 端点/web 报告页）按此签名消费，禁临场改形。
3. **质检三面 :545 映射表**（§5）= Phase 7 维护报告/质量巡检的质检复用契约（巡检批直接调 qc_* 纯函数 + QcReport 序列化面）。

## 7. 观察池（6C/Phase 7 立项对表，本批零实现）

①双解释生成器（:525 §8.3 七问）；②IR 装配面（verification-engine store / agent-engine evidence 读库 → ReportIr）；③Diagram Engine 全模块（:531 七类——本批仅 attack_paths SVG 单类）；④rp_server 报告端点 + web 报告页（echarts 6.1.0 复用面）；⑤PPTX/XLSX（:539 第二批——ooxml.rs 底座已备）；⑥Phase 7 知识回流全面（:672）；⑦krilla/printpdf 复审时机（RUSTSEC-2026-0206/0192 解除或 SEC-0 级裁决立项时）；⑧typst 复审时机（citationberg 升 quick-xml ≥0.41 后 deny 门可达）；⑨打包字体后备（CI 字体缺失面——OFL 许可面预审）；⑩Latin kerning/两端对齐排版质量（手写排版 tradeoff）；⑪reports PG 表 research_tasks/findings 建表批（V0009+——FK 目标补齐）。

## 8. 收口命令实录摘要

```bash
grep -cE '^\[\[package\]\]' Cargo.lock        # 440
grep -c 'name = "reqwest"' Cargo.lock         # 1
grep -c 'name = "zip"' Cargo.lock             # 0
grep -c 'name = "typst"' Cargo.lock           # 0
ls vendor/ | wc -l                            # 414
CARGO_TARGET_DIR=/home/emo/rt3-target cargo test --offline --workspace --no-fail-fast
#   → 1389 passed / 0 failed（195 个 test result 行汇总）
CARGO_TARGET_DIR=/home/emo/rt3-target cargo clippy --offline --workspace --all-targets -- -D warnings   # exit 0
cargo fmt --all -- --check 2>&1 | grep -c "^Diff in"    # 943（存量恒定；report-engine 零命中）
cargo deny check advisories && cargo deny check licenses # 双 ok；git diff deny.toml = 空
cargo --config 'source.crates-io.replace-with="rsproxy-sparse"' vendor vendor --locked  # exit 0；vendor/ tracked diff=0
sha256sum schemas/report-ir/report-ir-v1.schema.json    # 888e9210…（恒定钉）
sha256sum schemas/sarif/sarif-schema-2.1.0.json         # c3b4bb2d…2682e
bash apps/server/selftest_p6b.sh              # PASS=36 SKIP=0 FAIL=0（含一次性 PG reports_pg 4 绿）
bash apps/server/selftest_p6a.sh              # PASS=25 SKIP=0 FAIL=0（连带收口后）
for f in migrations/platform.db migrations/platform-dryrun.db migrations/platform-fullbatch.db migrations/platform-governance.db migrations/platform-search.db; do sqlite3 "file:$f?mode=ro" "PRAGMA user_version"; done   # 1 1 1 0 0
```
