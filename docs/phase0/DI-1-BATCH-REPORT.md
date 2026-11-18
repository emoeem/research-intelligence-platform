# DI-1 批次验收报告：Document Router + Document Pack 三通道原生端到端（PDF / Office / 图像）

- 日期：2026-11-19
- 状态：批次收口（S0→S8 全步执行 + 评审 1 high/5 medium/3 low 修订 9 条全落地；判据 1-10 逐项有测试、逐项有实测）
- 任务书：[DI-1-TASK.md](DI-1-TASK.md)；设计裁决：[DI-1-ADR-DOC-ROUTER.md](DI-1-ADR-DOC-ROUTER.md)（含 D3/D7 两条调和裁决条目）
- 结构说明：本报告按 A-K 十一节标准模板重排（初批 S8 版 A-G 结构 + H 修订补记的完整内容保留为文末附录与正文对应节，历史数字不改写口径）

## 坏消息（单列）

1. **CI 本批未跑、未 watch**：全部改动尚在工作树（未 commit、未 push），origin/main 顶端仍为 RT-1 收口的 12258e86（`git log origin/main -1` 实测）——CI 从未见过本批任何字节。E2 节为静态证据口径，fresh-checkout 行为**未验证**，待 push 后 watch 回填。
2. **workspace 全量回归（154 套件 946 passed 0 failed）为上一会话收口记录，本报告会话未复跑**；本会话复跑面 = 定向门禁（document-router 47 / dispatch 5 / tool-engine native_rust 20 / tool-registry 42 / capability_catalog 11 / clippy 四 crate / rustfmt 19 文件），均 fresh 全绿。
3. **无吞吐基线**：性能节只有单文件延迟面实测；Router 编排开销未从 CLI 成本中隔离计量（如实，未建即写未建）。
4. **office 内存帽 16Gi 的 RLIMIT_AS 实质是近乎无约束的地址空间帽**（512Mi~8Gi 全档死、16Gi 起 exit 0 实测阶梯推翻纸面 512Mi）——真实内存收紧缝（systemd MemoryMax）留 DI-3，风险在 G 节。
5. **Router 统一输出本批无外层信封**：限界 = 子 run 读帽 1 MiB 的结构承载，`envelope::truncate` 只作用于子 run 终局信封；外层信封化与帽升级随 DI-3 facade run（ADR D7 #10/D8 已加实现口径注）。
6. **归档面归属口径**：批报要求句「归档留 DI-2」与仓内冻结记录不一致——ADR D6 表与 DI-0 §5 明文归档两阶段防护属 **DI-3**（G 节按仓内 ADR 口径登记，差异点如实标注）。
7. **结构重排记账**：初批 S8 版报告（A 落点对照/B 门禁/C 偏差 8 条/D 判据 1-10/E 登记回执/F DI-2 缝/G 遗留 8 条 + H 修订 9 条）重排为 A-K 模板；原 C 节「偏差 8 条」并入 A/G 节，原 H 修订表全文保留为附录。

## A. 完成

**交付面**（S0→S8 + 9 条评审修订全部落地）：

| 交付物 | 内容 | 判据 |
|---|---|---|
| `crates/document-router/`（新成员） | sniff（D6 手写魔数表 + ZIP 全条目扫描 fail-closed）/ channel（D3 表 + D7 帽常量，`channel_steps` 显式标注对账镜像）/ router（前置门 + 三通道编排 + 预写 in/ + 逐子 run 引擎驱动）/ normalize（D3 统一输出 Schema）/ error（D8 映射）+ 四套测试（channel_gate/sniff_boundary/three_channel_e2e/fixture_pins） | D1/D3/D5/D6/D7/D8 |
| adapter `result_mode` 双模（D4） | `native_rust.rs` 唯一 additive 触碰点：frame 缺省零改动；file 模式 exit 0 → 产物收集 data、无帧解析、读帽触顶 `NATIVE_OUTPUT_CAP_EXCEEDED` FAILED；拒绝码专属化 `NATIVE_RESULT_MODE_INVALID`（评审 8） | 红线 8 |
| dispatch `native_rust` 分支（S5） | mcp-server 消费 RT-1 G 遗留 #2：承载范围守卫 → 文档输入根解析 → **64 MiB stat 前置帽**（评审 6）→ Router 编排 → 统一载荷；RunChannel 两新字段装配 | 判据 10 |
| 四份 Manifest + seed 回填（S4） | rip.doc.{pdf.info,pdf.text,office.to_pdf,image.meta}（绝对路径 + 64hex 钉值 + capability_ref + `result_mode:"file"` + resources + network:none）；seed 四条 sha256_full 实测回填 | D3 表 |
| S0 登记面修订 | `manifests/packs/planned/document.json`（trust_ruling.level + exit_conditions 第三条，:13-20 实测在位）+ PHASE-ROADMAP P3-4 + PHASE3-DESIGN 行内标注 | DI-0 §4.2 |
| 测试语料入库（评审 2） | 13 件 fixture 入 `crates/document-router/tests/fixtures/`（152KB）+ `fixture_pins.rs` sha256 逐件钉值 | CI 口径 |

**本会话 fresh 复跑门禁（2026-11-19，全部实跑实贴）**：

```
$ cargo test --manifest-path .../Cargo.toml --offline -p document-router -- --test-threads=1
lib 17 + channel_gate 8 + fixture_pins 2 + sniff_boundary 7 + three_channel_e2e 13 = 47 passed 0 failed（e2e 36.75s）
$ cargo test --manifest-path .../Cargo.toml --offline -p mcp-server --test native_rust_dispatch -- --test-threads=1
5 passed 0 failed（0.96s）
$ cargo test --manifest-path .../Cargo.toml --offline -p tool-engine native_rust -- --test-threads=1
native_rust_e2e 20 passed 0 failed（6.50s）
$ cargo test --manifest-path .../Cargo.toml --offline -p tool-registry
42 passed 0 failed（sk3_tool_manifests 7 + registration_flow 11 等）
$ cargo test --manifest-path .../Cargo.toml --offline -p tool-engine --test capability_catalog
11 passed 0 failed
$ cargo clippy ... -p document-router -p mcp-server -p tool-engine -p tool-registry --all-targets -- -D warnings
Finished 零告警
$ rustfmt --edition 2021 --check（19 个本批 .rs 落点）
exit 0 clean
```

（命令实际形态带 `--config 'source.crates-io.replace-with="vendored-sources"' --config 'source.vendored-sources.directory="…/vendor"'`：本会话执行 cwd 在仓库外，仓库内 `.cargo/config.toml` 的 vendor 替换未加载，须以 `--config` 显式给 vendor 面——教训见 K-8。）

**静态门 fresh 复核**：Cargo.lock `[[package]]` 400 条 / `source = ` 375 条 / `git diff --stat Cargo.lock` = +13 行 -0 行（document-router 成员条目 + mcp-server deps 一行，逐行实贴见 D 节）/ `git status --porcelain -- vendor/` = 0 行 / Router 生产代码零 `std::process::Command`（grep 仅 lib.rs:10、router.rs:6 两处文档注释命中）。

**上一会话记录（本会话未复跑，如实标注）**：S1 基线 146 套件 890 passed；收口 workspace 全量 154 套件 946 passed 0 failed exit 0；/tmp/di1-probe 探针（soffice txt/cat 挂死三复现、RLIMIT_AS 阶梯、xlsx 首条目反例等）与 D3/D7 调和裁决证据链——均见于 ADR 与附录 H 表。

## B. 架构与复用（Router × RT-1 adapter × RT-0.5-F 契约三层）

三层各司其职、单向依赖，无第二事实源（红线 7）：

```
RT-0.5-F 契约层（定义，无实现面新增）
  RT-0.5-E §2 十四项资源限制 → channel.rs 帽常量后端承载（:123-137）
  RT-0.5-F §6/§7 CapabilityProvider trait + 五态 → capability.rs 既有（本批零 diff）
  九类信封词表 envelope.rs:16-24 → error.rs D8 映射唯一出口（category 不新增）
        │ 消费
        ▼
RT-1 adapter 层（tool-engine，本批唯一 additive = result_mode）
  native_rust.rs：prepare_workspace 不清目录（:255-278，预写前提）→ in/ 前缀校验
  （:1322-1330 不放松）→ env 白名单 / prlimit Auto 档（--as 强制）→ timeout killpg
  → 读侧帽 → frame|file 双模成功面 → stdout/stderr 留档 → collect_out_artifacts
  → cleanup_tmp（:1524，lo-profile 用后即焚载体）→ execute_native_rust_via_engine
  （:1432-1452 pub 公开 API）
        │ 公开 API 消费
        ▼
Router 层（document-router，本批新建）
  route()（router.rs:160-232）：前置门 bytes 帽（:171-176）+ 嗅探门（:177-186，
  零 run 行 INPUT 拒绝）→ route_pdf（:235）/ route_office（:301 级联）/ route_image（:426）
  → drive_step_at（:496-557：预写 in/ :508-518 → submit_correlated trace 自锚定
  :533-535 → execute_native_rust_via_engine :537-543）→ read_stdout（:561-577）
  → build_unified（D3 Schema）
```

**复用事实（grep/实读取证）**：

- Router 生产代码零 `std::process::Command`、零直连 SQL：grep 全 src 仅两处文档注释（`crates/document-router/src/lib.rs:10`、`router.rs:6`）；每子 run 经 `execute_native_rust_via_engine`（router.rs:26 import，:537 调用）走真实 ToolRun 状态机/信封/Artifact/Trace——E §5「不建第二事实源」逐字兑现。
- 帽值在后端强制而非 Router 前端自断：resources.cpu/memory 由 adapter prepare 读取后 prlimit 包裹、timeout 经 registry→engine submit 真实生效（RT-1 既有面，评审实测复核）；Router 门（页/像素）是链内**第二道**，不是唯一一道。
- dispatch 分支（dispatch.rs:295-418）：承载范围守卫（:306-313 通道表外 native 工具 VALIDATION 拒绝）→ `resolve_document_input` 前缀校验（:793）→ stat 帽前置（:332-346，fs::read **之前**）→ StaticCatalogProvider::load（:363-366）→ Router route（:378-380）→ 统一载荷（Succeeded :407-415 / Failed :392-406 带 runs 摘要）。
- `channel_steps`（channel.rs:80-117）为 D3 表**对账镜像，非编排来源**（模块文档 :8-16 显式标注「两处同改」纪律）——Router 编排消费 router.rs 管线本体，DI-2 扩展按 F 节缝执行，防第二份通道表静默漂移（评审 5 修订）。
- crate 边界即 RT-3 接缝（ADR D1）：Router 只依赖 tool-engine 公开面 + tool-registry + domain + serde/serde_json（Cargo.lock 成员条目 dependencies 六项逐一在册，D 节 diff），RT-3 Policy 判定替换 Router 判定面不牵动 tool-engine。

## C. 文件清单（`git status --porcelain -uall` 实测逐文件，2026-11-19）

```
 M Cargo.lock
 M Cargo.toml
 M crates/context-broker/tests/acceptance_no_stuffing.rs
 M crates/context-broker/tests/catalog_sources.rs
 M crates/mcp-server/Cargo.toml
 M crates/mcp-server/src/bin/rp-mcp.rs
 M crates/mcp-server/src/dispatch.rs
 M crates/tool-engine/src/adapter/native_rust.rs
 M crates/tool-engine/tests/native_rust_e2e.rs
 M crates/tool-registry/src/validate.rs
 M crates/tool-registry/tests/sk3_tool_manifests.rs
 M docs/README.md
 M docs/phase0/PHASE-ROADMAP.md
 M docs/phase3/PHASE3-DESIGN.md
 M manifests/capabilities/host-catalog-seed.json
 M manifests/packs/planned/document.json
 M schemas/tool-manifest/v1.json
?? crates/document-router/Cargo.toml
?? crates/document-router/src/channel.rs
?? crates/document-router/src/error.rs
?? crates/document-router/src/lib.rs
?? crates/document-router/src/normalize.rs
?? crates/document-router/src/router.rs
?? crates/document-router/src/sniff.rs
?? crates/document-router/tests/channel_gate.rs
?? crates/document-router/tests/fixture_pins.rs
?? crates/document-router/tests/fixtures/clean.md
?? crates/document-router/tests/fixtures/evil.zip
?? crates/document-router/tests/fixtures/oversized-pixels.png
?? crates/document-router/tests/fixtures/sample.docx
?? crates/document-router/tests/fixtures/sample.epub
?? crates/document-router/tests/fixtures/sample.html
?? crates/document-router/tests/fixtures/sample.md
?? crates/document-router/tests/fixtures/sample.pdf
?? crates/document-router/tests/fixtures/sample.png
?? crates/document-router/tests/fixtures/sample.pptx
?? crates/document-router/tests/fixtures/sample.txt
?? crates/document-router/tests/fixtures/sample.xlsx
?? crates/document-router/tests/fixtures/sample.zip
?? crates/document-router/tests/sniff_boundary.rs
?? crates/document-router/tests/three_channel_e2e.rs
?? crates/mcp-server/tests/native_rust_dispatch.rs
?? docs/phase0/DI-1-ADR-DOC-ROUTER.md
?? docs/phase0/DI-1-BATCH-REPORT.md
?? docs/phase0/DI-1-TASK.md
?? manifests/tools/rip.doc.image.meta.json
?? manifests/tools/rip.doc.office.to_pdf.json
?? manifests/tools/rip.doc.pdf.info.json
?? manifests/tools/rip.doc.pdf.text.json
```

共 43 项：17 M + 26 ??。与任务书 §3.1 落点对照——全部落点在册；必要伴随面（任务书 §3.1 未列、批报如实记账）= `crates/mcp-server/src/bin/rp-mcp.rs` + `crates/mcp-server/Cargo.toml`（RunChannel 形状变更编译伴随）、`crates/context-broker/tests/` 两文件（登记计数 25→29 + 预算 8200→8600）、`crates/document-router/tests/fixtures/` 13 件（评审 2 语料入库，`git check-ignore` 实测不受根 `.gitignore:47` `rip-data/` 排除影响）、`docs/phase0/DI-1-TASK.md`（任务书随批入库）。Cargo.lock 唯一增量逐行：

```diff
+[[package]]
+name = "document-router"
+version = "0.2.0"
+dependencies = [
+ "database",
+ "domain",
+ "serde",
+ "serde_json",
+ "tool-engine",
+ "tool-registry",
+]
（另 mcp-server 条目 dependencies +1 行 "document-router"；合计 +13 行 -0 行，git diff 实测）
```

## D. 迁移与兼容

**零迁移，逐项实测**：

- `git status --porcelain -- migrations/` = 0 行：无任何 DB schema/迁移面改动，无平台库写入（红线 4）；测试全走内存库/临时库。
- `git status --porcelain -- manifests/tools/rip.doc.parse.json workers/python/parsing_worker.py` = 0 行：文本解析既有链冻结不迁不扩（DI-0 §3），**rip.doc.parse Manifest 的格式/能力升级留 DI-3**（本批 Router 对 text 签名走 INPUT 拒绝并注明归 rip.doc.parse，router.rs:180-181 `RouterReject::text_not_routed(ext)`；禁造第二套 md/txt/html 解析，D6 表）。
- 兼容面 additive 全清单：`result_mode` 缺省 = frame（既有帧协议工具不迁移；native_rust_e2e 20/20 fresh 全绿 = 帧模式零回归门）；validate.rs/v1.json 仅 additive 枚举（词表零新增值）；既有 15 份工具 Manifest 零改写（porcelain 无其它 manifests/tools 条目）；workspace 仅 members/依赖 additive；lock 唯一增量 = 成员自身条目（C 节 diff）。既有 runtime（python_worker/podman_host）行为零改动，回归由 workspace 全绿门承载（本会话未复跑全量，见坏消息 2）。

## E. 测试真实结果

### E.1 三通道端到端 run 摘要（本会话 fresh 实跑，测试名与断言逐项）

- **PDF 通道**（`pdf_channel_end_to_end_two_runs_unified_output`，e2e fresh ok）：sample.pdf → pdfinfo + pdftotext 两子 run 均 SUCCEEDED；断言统一输出 `channel="pdf"`、`metadata.pages=2`、`page_size.width/height=595/842`（three_channel_e2e.rs:275/283-285）、Artifact sha256 子 run 信封 ↔ file-mode data.artifacts 对账（:296-307）、`engine.trace_id_of(info_run) = run_id` 自锚定（:317）。
- **Office 通道**（`office_channel_end_to_end_cascade_three_runs`，e2e fresh ok）：sample.docx → soffice（out/sample.pdf artifact role=primary）→ 级联 pdfinfo + pdftotext 共三子 run；断言 `channel="office"`、`office_subtype="docx"`、`converted_pdf_bytes>0`（:389-393）、warnings 携带 office→pdf 转换面声明（route_office 入口 push，router.rs:310-312）；套件 `--test-threads=1`（红线 6）。
- **图像通道**（`image_channel_end_to_end_png_and_generated_jpeg`，e2e fresh ok）：sample.png + magick 本地生成 JPEG；断言 `channel="image"`、`dimensions.width=834`（:515-517）、text=""。
- 三通道组合 fresh 单跑 `--test-threads=1`：3 passed 6.91s（soffice 实转在链上）。全 e2e 套件 13/13 passed 36.75s。
- DB 行级逐条 dump 本会话未单独执行；run 行数/终态经上述测试断言承载（fresh 通过）。

### E.2 负例矩阵回显（评审 high/medium 修订后矩阵 10 类，全部在本会话 fresh run 中在册且 ok）

| 负例 | 预期 | 测试名（套件） | fresh |
|---|---|---|---|
| epub / zip / evil.zip（zip_other） | INPUT `ROUTER_UNSUPPORTED_TYPE`，零子进程零 run 行 | `unsupported_types_rejected_at_gate_with_zero_rows` + `native_rust_dispatch_gate_rejects_zip_other_without_rows`（e2e/dispatch） | ok |
| 伪 `.pdf` 纯文本（text 签名） | INPUT 拒绝并注明归 rip.doc.parse | `fake_pdf_extension_yields_text_signature`（sniff_boundary）+ e2e 判据 4 面 | ok |
| capability missing（pdf.mutool 形态） | DEPENDENCY PREPARING 拒绝显式信封，run 行 FAILED 留证 | `missing_capability_explicit_dependency_rejection`（e2e） | ok |
| 输入 64 MiB+1（Router 门） | `ROUTER_INPUT_TOO_LARGE`，无任何 run 行 | `input_too_large_rejected_before_any_run`（channel_gate） | ok |
| 输入 64 MiB+1（dispatch stat 前置） | 超帽不读入内存（稀疏文件 set_len 证明读面未触达）+ 零 run 行 | `native_rust_dispatch_input_too_large_rejected_before_read`（dispatch，评审 6 新增） | ok |
| stdout 读帽触顶（真实 soffice 造 ~2MiB 文本 PDF） | `NATIVE_OUTPUT_CAP_EXCEEDED` FAILED，两子 run 行真实终态，不静默丢字节 | `oversized_stdout_cap_exceeded_chain_fails_honestly`（e2e）+ `native_rust_file_mode_cap_hit_fails_without_silent_loss`（tool-engine） | ok |
| 像素帽 121MP（11000x11000 完整合法 PNG 入库 fixture） | `ROUTER_PIXEL_BUDGET_EXCEEDED` 链内 FAILED + identify 子 run 行 SUCCEEDED 留证（评审 1①） | `pixel_budget_exceeded_chain_fails_with_identify_run_evidence`（e2e） | ok |
| 页数帽 2001 页（测试内纯代码构造真实页树 PDF） | `ROUTER_PAGE_BUDGET_EXCEEDED` + pdfinfo 行 SUCCEEDED + pdftotext 未拉起（评审 1②） | `page_budget_exceeded_chain_fails_with_pdfinfo_run_evidence`（e2e） | ok |
| pdfinfo Pages 行未解析 | honest warning（零静默旁路，与图像通道口径拉齐，评审 4） | `pdfinfo_pages_unparsed_carries_honest_warning`（e2e） | ok |
| result_mode 非法字面量 | PREPARING 拒绝专属码 `NATIVE_RESULT_MODE_INVALID`（category POLICY_DENIED，评审 8） | `native_rust_file_mode_invalid_literal_rejected_in_preparing`（tool-engine） | ok |
| 魔数权威 vs 扩展名 | .pdf 名 + PNG 体路由图像通道 + mismatch + warnings | `sniff_authority_routes_png_body_with_pdf_extension`（e2e）+ `mismatch_matrix_ext_hint_vs_sniff`（sniff_boundary） | ok |

实测定形记录：仅 IHDR 头的截断 PNG 被 `identify -ping` exit 1 拒（走 EXECUTION 到不了像素帽门）→ 像素帽负例改用完整小体积大尺寸 PNG（11000x11000，121MP），该实测否决写进 `fixture_pins.rs` 形态前提单测与附录 H-1。

### E.3 Manifest 登记面（S4，上一会话实测记录；本会话抽验字段在位）

四份 Manifest validate + register 回执（初批记录）：rip.doc.pdf.info `45185f19…` / pdf.text `b5ecf53b…` / office.to_pdf `cfeb559e…` / image.meta `fc06d1b1…` 全 ok。本会话抽验：office Manifest `binary=/usr/bin/soffice`（:11）、`result_mode:"file"`（:24）、`memory:"16Gi"`（:104）、`timeout_seconds:120`（:105）、`binary_target_sha256` 字段零命中（grep 实测，D3 调和裁决 B）；pdf.info `binary_sha256=3bdf68a7…`（:12）；seed 四条 sha256_full 与上会话实测钉值逐一吻合（python3 抽取实测）。

## E2. CI（首次 push watch 回填 + CI 修复批账目）

- **首次 push（收口提交 `659ed1e8`）**：CI run [44393220433](https://github.com/emoeem/research-intelligence-platform/actions/runs/44393220433) 终态 **failure**（Rust workspace job，四 jobs 三绿）。红面单点：`three_channel_e2e` 12 passed 1 failed exit 101——`missing_capability_explicit_dependency_rejection`（three_channel_e2e.rs:648）断言 `error.message` 含 "missing" 失败，实际 message = `pinned binary /usr/bin/pdfinfo not present on host`。性质：harness 沿用 Manifest 的 `/usr/bin/pdfinfo` 单机钉值作 identity 前提，CI runner（ubuntu-latest）无 poppler → identity 钉值校验先拒 NATIVE_BINARY_MISSING（DEPENDENCY 类，fail-closed 行为本身正确），capability missing 二道路径在异构宿主永远测不到——宿主绑定 harness 缺陷，非 Router/adapter 逻辑回归（本机同代码 13/13 绿；其余 12 e2e + channel_gate/sniff_boundary/fixture_pins/dispatch/tool-engine/registry/context-broker 在 CI 全绿，异机守卫面按设计生效）。
- **CI 修复批**：harness 改钉值自持 identity stub——测试内造最小可执行脚本（`#!/bin/sh\nexit 0`，/bin/sh 解释器跨宿主稳定在位）+ chmod 0o755 + 实时 `sha256_hex` 写入 harness Manifest 的 `runtime.binary`/`runtime.binary_sha256`，identity 校验对照实测钉值恒过 → 二道校验按测试意图触发 capability missing 显式 DEPENDENCY。钉值前提不依赖宿主单机快照（RT-1 12258e86 修复批同款教训落实：要么前置守卫要么实时自持），覆盖点不丢（capability missing 路径任意宿主可测）。本机复跑：`three_channel_e2e` --test-threads=1 13 passed 0 failed（36.78s）+ rustfmt --edition 2021 --check clean + clippy `-p document-router --all-targets -D warnings` 零告警。
- **CI 修复批 commit push 后 run 终态随本次 watch 回填（占位：待 watch）**。

## F. 性能（单文件延迟面；无吞吐基线，如实）

本会话 fresh 实测（bash `time`，样本 = 入库 fixtures，2026-11-19）：

| 面 | 实测 | 备注 |
|---|---|---|
| pdfinfo sample.pdf | 0.039 / 0.012 / 0.012 s（3 跑，首跑冷） | `Pages: 2`、`Page size: 595 x 842 pts (A4)` 回显核对 |
| pdftotext sample.pdf | 0.019 / 0.019 / 0.017 s | `-` stdout 形态，192 字节 |
| magick identify -ping sample.png | 0.036 / 0.008 / 0.008 s | `PNG 834x250 sRGB depth=8` |
| soffice fresh profile → PDF | 6.919 / 4.841 s（2 跑） | `env -i PATH TMPDIR LANG` 最小集 + 每 run 独立 `-env:UserInstallation`，`writer_pdf_Export` 产出 22592 字节 sample.pdf；fontconfig cache 警告如实出现、转换 exit 0 |
| e2e 套件墙钟 | 全 13 用例 36.75s；三通道组合 6.91s | 含真实 soffice 链 + 测试内构造 2001 页 PDF |

- 余量口径：soffice 4.8~6.9s vs Manifest 时长帽 120s = **17~25×** 头寸（D7 #6「4.4s ×20 余量」与本测一致；上会话 fresh-profile 4408ms 同域）。
- **未建吞吐基线（如实）**：本批无并发/批量吞吐测量；Router 编排自身开销（嗅探/预写/规范化）未从 CLI 成本中隔离计量——上表为载具外 CLI 直测口径，Router 内单文件端到端墙钟未单独出数。DI-2 渲染批引入页级放大前应先建该基线。

## G. 遗留与风险

1. **OCR/媒体渲染面 → DI-2**：pdftoppm + tesseract、chi_tra 显式告警缝（落 Router warnings 字段）、渲染批派生前页数/像素帽按 D7 #7/#8 收紧；通道行位扩展缝见 H 节。
2. **归档两阶段防护 + exiftool → DI-3**（口径注：批报要求句「归档留 DI-2」与 ADR D6 表 / DI-0 §5 冻结记录的 DI-3 不一致，以仓内 ADR 为准登记；zip_other 现行为 = 嗅探门 INPUT 拒绝 fail-closed，无解包面故无解压放大风险）。
3. **隔离面 → DI-3**：外层 facade run 审计锚（链级失败现以 dispatch 错误载荷 + 子 run 行留证）、systemd MemoryMax 内存真收紧（现 RLIMIT_AS 16Gi 近乎无约束，风险项）、Artifact mount_as 式富暂存、RT-3 Policy 判定移交。
4. **wrapper 变量形态 exec 目标解析 → RT-4 前处理台账**：RT-1 `verify_wrapper_target` 只识别静态形态；office 双钉维持裁决 B 分层承载（Manifest 钉 wrapper hash + Catalog 目录钉 resolved target）。
5. **capability 完整探测/TTL/失效事件 → RT-1 G1 独立批**：本批消费 StaticCatalogProvider L0 最小面（get 为纯缓存读，capability.rs:783，不做存活检测）；测试守卫层已用二道 stat 补偿（评审 2）。
6. **全仓 fmt 漂移 661 处**：HEAD 既有红，本批 scoped fmt（19 落点文件 rustfmt --check exit 0）兑现，全仓收口不属本批（RT-1 G 遗留 4 沿用）。
7. **pack-plan v1.json trust_ruling.level 词表缝**：`distro-package-verifiable + input-risk-routed` 不在 schema 现行 enum；存根按 DI-0 §4.2 冻结文本落盘（document.json:14 实测），仓内无 planned 存根校验测试面（全绿不受影响），schema enum 扩展登记后续批。
8. **基线漂移记账**：S1 基线 888→890（HEAD 线内追加测试，git diff 溯源，非回归）。

## H. 下一批依赖

**RT-2（Python Worker 原生环境）**：roadmap 定义 =「项目级 venv、依赖锁、启动探测、版本/哈希记录、协议一致性测试」（PHASE-ROADMAP.md:786 实测）。与本批产物零耦合：DI-1 对 python_worker 零触碰（porcelain 无该文件）、rip.doc.parse 冻结，RT-2 开工无需本批任何前置；本批给 RT-2 的只有「文本面继续由 rip.doc.parse 承载、禁造第二套」的边界事实（D6）。

**DI-2（能力面扩全，本批已备好的缝）**：

- 行位扩展缝 = **`router.rs` 通道管线本体**（`route_pdf` 在 pdftotext 之后/替代位插 pdftoppm 渲染 + tesseract 步），**并同步 `channel_steps` 对账镜像行**（channel.rs Step 表）+ 两套锁定用例（`channel_table_rows_match_d3` / `d3_channel_table_rows_locked`）——只扩 channel.rs 不动 router.rs 是静默漂移（模块文档 :8-16 两处同改纪律写死）。
- 复用面（DI-2 零新增执行机制）：D5 预写/回读（drive_step + read_stdout）、D6 嗅探表、D7 前置门/链内门、D8 reject 映射、submit_correlated 自锚定；新 Manifest `result_mode:"file"` 同形态登记。
- 能力目录：pdf.pdftoppm / ocr.tesseract 需按 S4 同纪律 probe + sha256_full 实测回填（现 seed 16 条无此二条）。
- chi_tra 缺失告警落 Router warnings（office→pdf 转换面声明同位，build_unified warnings 字段）；OCR 文本走 stdout.log 回读、页图产物走 out/ 收集（out_listing/collect_out_artifacts 既有面）。

**DI-3 清单（承接面已登记）**：facade run 审计锚、归档两阶段、exiftool、rip.doc.parse Manifest 升级（文本通道整合）、Router 统一输出外层信封化与帽升级。

## I. 浏览器截图

**本批无 UI 面，零截图（如实）**：交付物 = Rust 库 crate + 引擎/派发后端通道 + Manifest 数据面，无任何 web/UI/前端改动（porcelain 无 web/ 下条目）；无浏览器 e2e 用例故无截图可附。仓库先例（SK-3 skverify 浏览器四截图 docs/phase6/browser-e2e/）属工具 UI 批次，本批不适用。

## J. RTIS 对账

**零接触，实测**：

- `git status --porcelain -- integrations/rtis/` = 0 行；全仓 porcelain 无任何 `rtis` 命中（红线 5 达成）。
- `git status --porcelain -- migrations/` = 0 行且无 `platform*.db` 命中（红线 4 达成，生产库零写）。
- 本批不触知识摄取/检索链路，RTIS 旧链 15,155 条对账**未复跑**（无触碰面故无需；如需复跑留独立批次，本报告不以其为门）。

## K. 教训

1. **纸面帽值必须实测定值，强制面与帽值分开验证**：RLIMIT_AS 强制机制本身有效，被推翻的是 512Mi 这个数（512Mi~8Gi 全档死）——帽值表每行落地前过一遍本机阶梯，调和裁决（D7 条目）是与 D3 同形态的必要记录位。
2. **脚本/wrapper 类二进制的钉值面要探针先行**：soffice wrapper exec 目标为变量形态，静态双钉解析 fail-closed 会让整条通道每 run 必死；分层承载（钉 wrapper hash + 目录钉 resolved target）是保安全意图的活路。
3. **静态能力种子没有存活检测**：`get` 纯缓存读导致异机守卫照样放行、随后以 adapter 身份校验失败炸测试——测试守卫需要二道 stat + 显式 SKIP，且 SKIP 行要带测试名（否则多能力守卫时日志无法归属）。
4. **fixture 不入库 + 缺失即 panic = CI 必红组合**：评审 high/medium 连发源于同一根因（语料依赖被 .gitignore 整体排除的运行时数据）；入库 + sha256 钉值（env_echo 先例同款）一次性消除环境耦合。
5. **登记的 code ≠ 链上行为**：页数/像素帽在构造器形状断言里「存在」但零真实穿链负例——帽负例必须让超帽输入真实走 `route()` 并断言子 run 行留证；截断 PNG 被 identify exit 1 拒这类「负例造不出预期门」的实测定形要写进测试注释与批报。
6. **双承载必漂移**：程序化通道表与硬编码管线并存时，表行增而行为零变是静默的——要么单一来源，要么显式标注「对账镜像非编排来源」+ 两处同改纪律写进模块文档。
7. **ADR 行文与实现口径要同步回写**：office 内存帽、统一输出信封限界两例都是「实现偏离文字」被评审抓出后补注——调和裁决与行内实现口径注应当随实现一起落，不留到评审。
8. **cargo 配置上下文是隐形门**：cwd 不在仓库根时仓库内 `.cargo/config.toml`（vendor 替换）不加载，`--offline` 撞 registry 报「candidate versions didn't match」（本报告会话实测复现）；收口命令必须写明 cwd 前提，或带 `--config` 显式 vendor 面；env 形态 `CARGO_SOURCE_*` 实测未生效，`--config KEY=VALUE` 实测生效。
9. **scoped fmt 门用文件级 rustfmt --check**：`cargo fmt -p <成员>` 会波及非落点的 HEAD 既有漂移（初批 13 文件误触恢复）；文件级 check 是不触碰历史的 scoped 形态。

---

## 附录：初批结构保留（历史不改写）

### 附-1 评审修订补记（初批 H 节 9 条，全表保留）

| # | 评审意见（severity） | 修订落点 | 复测 |
|---|---|---|---|
| H-1 | 页数帽/像素帽零行为级负例（high） | e2e 新增 `pixel_budget_exceeded_chain_fails_with_identify_run_evidence`（11000x11000=121MP 完整合法 PNG 入库 fixture；实测定形：截断 PNG 被 identify exit 1 拒，手写 IHDR 形态被否决）+ `page_budget_exceeded_chain_fails_with_pdfinfo_run_evidence`（纯代码构造 2001 页最小合法 PDF） | e2e 13/13 |
| H-2 | fixture 未入库/无钉值/缺失 panic、守卫只读静态种子（medium） | 13 件语料入库 + `fixture_pins.rs` 钉值 + 四处读样本面改 fixtures + capability 守卫二道 stat（available-but-executable-missing → SKIP） | fixture_pins 2/2 |
| H-3 | SKIP 行缺测试名（low） | `require_capability`/`capability_available` 加 test 参数，对齐 `SKIP <test>: capability <id> status=<status>` | 全套复跑 |
| H-4 | office 内存帽未回修 ADR + known_limits 互斥（medium） | ADR 追加「D7 调和裁决」+ D7 表 #5 行内标注；Manifest 删「内存帽 512Mi」残留行（16Gi 单源） | 文档面 |
| H-5 | 页数帽静默旁路（medium） | route_pdf/route_office 对 pages=None push honest warnings（router.rs:261-266/:382-387 实测在位）+ 新增 `pdfinfo_pages_unparsed_carries_honest_warning` | e2e 13/13 |
| H-6 | D3 通道表双承载漂移（medium） | channel.rs 标注「对账镜像，非编排来源」+ 批报 F.1 扩展缝改指 router.rs 管线本体 | 文档面 |
| H-7 | 64 MiB 帽检查时点与 ADR 不符（medium） | dispatch stat 前置（dispatch.rs:332-346，fs::read 之前）+ `native_rust_dispatch_input_too_large_rejected_before_read` 稀疏文件负例 | dispatch 5/5 |
| H-8 | result_mode 拒绝借 NATIVE_ARGV_TEMPLATE_DENIED（low） | 改专属码 `NATIVE_RESULT_MODE_INVALID`（native_rust.rs:97，ResultMode::parse 拒绝面；e2e 钉值断言 1 处同步） | native_rust 20/20 |
| H-9 | 统一输出限界与 ADR 文字不符（low） | ADR D7 #10 + D8 加实现口径注（envelope::truncate 只作用子 run；限界=子 run 读帽 1MiB；外层随 DI-3 facade run） | 文档面 |

### 附-2 初批门禁终态（S7 时点记录，本报告会话未复跑全量）

初批 S7：document-router 42 passed（e2e 10）/ dispatch 4 / tool-engine native_rust 20 / sk3 7 / clippy 零告警 / workspace 150 套件 938 passed 0 failed / lock 400/375/vendor 0。评审修订后（上会话复测）：document-router 47（+3 e2e 负例/告警）/ fixture_pins 新套件 2 / dispatch 5（+stat 负例）/ workspace 154 套件 946 passed 0 failed exit 0 / clippy 四 crate 零告警 / fmt 落点 clean / lock 400、registry 375、vendor 零 diff。S1 基线：146 套件 890 passed（888+2 HEAD 线内追加，溯源记账）。初批 C 节偏差记账 8 条（D3 调和裁决 B / 16Gi 实测定值 / SOFFICE_LOCK 互斥 / context-broker 计数校准 / mcp-server 伴随落点 / 判据 5 code 命名 / pack-plan enum 缝 / S1 +2）全文内容已并入本报告 A/G 节对应条目。

## 复现

```bash
cd /media/emo/code/pensting/research-platform
# 定向门禁（预期：47 / 5 / 20 passed，0 failed；clippy 零告警；fmt exit 0）
cargo test --offline -p document-router -- --test-threads=1        # 预期 47 passed（e2e 13 项含三通道真实链 + 10 负例/告警面）
cargo test --offline -p mcp-server --test native_rust_dispatch -- --test-threads=1   # 预期 5 passed
cargo test --offline -p tool-engine native_rust -- --test-threads=1                  # 预期 20 passed
cargo clippy --offline -p document-router -p mcp-server -p tool-engine -p tool-registry --all-targets -- -D warnings  # 预期 Finished 零告警
# 静态门（预期：400 / 375 / 0 / 0）
grep -c '^\[\[package\]\]' Cargo.lock
grep -c '^source = ' Cargo.lock
git status --porcelain -- vendor/ | wc -l
git status --porcelain -- integrations/rtis/ migrations/ | wc -l
```

（若执行 cwd 不在仓库根，cargo 命令须追加 `--config 'source.crates-io.replace-with="vendored-sources"' --config 'source.vendored-sources.directory="<repo>/vendor"'`，教训 K-8。）
