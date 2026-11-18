# DI-1 任务书：Document Router + Document Pack 三通道原生端到端（PDF / Office / 图像）

- 日期：2026-11-19
- 状态：任务书已定稿（DI-1 设计裁决员产出；实现批次开工即按本任务书执行）
- 依据：[DI-0-P34-PREMISE-REVISION-ADR.md](DI-0-P34-PREMISE-REVISION-ADR.md) §4/§5（DI 链入口与 S0 修订指令）；[RT-0.5-E-RISK-AND-ISOLATION.md](RT-0.5-E-RISK-AND-ISOLATION.md) §1/§2/§3/§4；[RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md](RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md) §6/§7/§8；[RT-1-ADR-NATIVE-ADAPTER.md](RT-1-ADR-NATIVE-ADAPTER.md) D1-D8；[RT-1-BATCH-REPORT.md](RT-1-BATCH-REPORT.md) H.2（DI-1 前置三条就绪 + 自备面四条）
- 设计裁决：本任务书不重复论证，实现口径一律以 [DI-1-ADR-DOC-ROUTER.md](DI-1-ADR-DOC-ROUTER.md) D1 到 D8 为准
- 前置：RT-1 已收口（commit ae21a0a8 + 12258e86 CI 修复，2026-11-19 `git log` 实测）；NativeRustAdapter / StaticCatalogProvider / allowlist 登记形状三条消费面就绪（RT-1-BATCH-REPORT H.2）

## 1. 目标

把 Document Pack 从 planned 存根推进到三通道原生端到端（DI-0 §4 判据 1 保留版），OCR 面全数留 DI-2：

1. **三通道端到端**，每通道至少一条真实 run 经 tool-engine 引擎全链（QUEUED→PREPARING→RUNNING→终态）：
   - **PDF 通道**：pdfinfo（元数据/页数/页尺寸探测）+ pdftotext（文本抽取，`\f` 页锚），capability_ref = pdf.pdfinfo / pdf.pdftotext；
   - **Office 通道**：soffice `--headless` 隔离临时 profile `--convert-to pdf`（实测唯一不挂死面，ADR D3）→ 级联 PDF 通道两子 run；capability_ref = document.libreoffice；
   - **图像通道**：magick `identify -ping` 元数据/尺寸受限面（禁 convert 别名，capability_ref = image.magick）。
2. **统一输出**：文本 + 元数据 + 页锚/尺寸 + 警告（ADR D3 Schema），经引擎信封 output_schema 校验交付；warnings 如实呈现（office→pdf 转换面声明、嗅探/扩展名不一致），零静默降级。
3. **Router 最小实现**（新 crate，ADR D1）：手写魔数嗅探（D6）→ 通道表编排（D3）→ 能力/帽前置门（D7）→ 规范化；RT-0.5-E §3 路由图最小形态，档位判定前置承接（RT-3 接入点 DI-3 登记）。
4. **mcp-server dispatch native_rust 分支补位**（RT-1 遗留 #2 消费）：`"native_rust" =>` 分支 + `execute_native_rust_via_engine` 接线（dispatch.rs:230-281 现两分支 + other 拒绝）。

不目标（本批不做）：OCR/渲染链（pdftoppm + tesseract、chi_tra 告警面，DI-2）；归档解包两阶段防护与 exiftool（DI-3）；Runtime Policy Resolver 判定面（RT-3，DI-1 仅以 Manifest permission_level + 嗅探签名承载）；Capability Provider 完整探测/TTL/失效事件（若实现需要，属 RT-1 遗留 G1 前置批独立裁决）；外层 facade run 审计锚（DI-3，D7 前置门语义已登记升级缝）。

## 2. 现状锚点（2026-11-19 实测；代码为只读实读，CLI 为 /tmp/di1-probe 本机实测，样本 = 仓库既有 rip-data/parse-smoke/in/）

### 2.1 代码锚点

| 锚点 | 事实 | 出处 |
|---|---|---|
| 帧协议强制 | exit 0 + stdout 非 JSON 帧 → FAILED `NATIVE_UNSUPPORTED_OUTPUT`（raw CLI 无帧可发的结构缺口） | crates/tool-engine/src/adapter/native_rust.rs:1224-1261 |
| stdout/stderr 留档 | 全量 stdout 落 out/stdout.log、stderr 落 out/stderr.log（file 模式数据载体） | native_rust.rs:1216-1221 |
| input_content 仅文本 | `Value::as_str`，二进制（PDF/docx）结构性不可经 JSON 串承载 | native_rust.rs:747 |
| argv 槽位语法 | 仅整 token 精确匹配 `{{input_path}}`/`{{out_dir}}`/`{{tmp_dir}}`/`{{slot}}`；混合 token 字面量透传（profile URI 不可组合） | native_rust.rs:736-772 |
| 工作区不清目录 | `prepare_workspace` create_dir_all，Router 预写 in/ 文件在 PREPARING 后仍在位 | native_rust.rs:255-278 |
| in/ 前缀校验 | `{{input_path}}` 归一 + 前缀强制（防任意文件读取），预写面不放松 | native_rust.rs:1322-1330 |
| 公开消费 API | `NativeRustCall{engine, run_id, adapter, manifest, capability}` + `execute_native_rust_via_engine` | native_rust.rs:1432-1452 |
| 资源强制 | Auto 档本机落 prlimit 兜底（systemd-run 清环境探测不可用，fail-closed） | native_rust.rs:77-87；RT-1 报告 G 遗留 1 |
| tmp 回收 | `cleanup_tmp` 四路径统一（lo-profile 用后即焚载体） | native_rust.rs:1524 |
| dispatch 缺分支 | python_worker/podman_host 两分支 + other 拒绝；native_rust 零命中 | crates/mcp-server/src/dispatch.rs:230-281 |
| allowlist 登记面 | validate.rs native_rust 分支（780 行起）+ binary_sha256/subcommands/argv_template/capability_ref 字段（790-794） | crates/tool-registry/src/validate.rs:780-794 |
| argv 令牌卫生 | NUL/换行/`..`/shell 元字符登记面拒绝 | validate.rs:243-263 |
| 信封词表 | 九类 category 唯一词表；truncate + usage.truncated 既有 | crates/tool-engine/src/envelope.rs:16-24、:250、:278 |
| ToolRun 状态机 | 八态（Queued/WaitingForRunner/Preparing/Running/Succeeded/Failed/TimedOut/Cancelled） | crates/domain/src/tool.rs:14-31 |
| 能力种子 | 16 条（15 available + 1 missing pdf.mutool）；pdf.pdftotext/pdfinfo/document.libreoffice/image.magick 条目 sha256_full=null 待回填 | manifests/capabilities/host-catalog-seed.json |
| lock 基线 | `[[package]]` 399 条、source=registry 375 条、成员 24 条（wsw-message:3790 在册）；sha2 0.10.9/0.11.1 双版本在 lock | Cargo.lock（`grep -c` 实测） |
| 钉版复用先例 | WSW-1 新成员零 vendor 增量（sha2/base64 钉既有版本） | Cargo.toml:71-76 |
| rip.doc.parse 冻结 | format 枚举 md/txt/html、零 PDF/Office/图像覆盖、python_worker 不迁不扩 | manifests/tools/rip.doc.parse.json:46-58；DI-0 §3 |

### 2.2 本机 CLI 实测（/tmp/di1-probe，2026-11-19；样本 rip-data/parse-smoke/in/）

| 通道探针 | 命令（原文） | 实测回显要点 |
|---|---|---|
| PDF 元数据 | `/usr/bin/pdfinfo …/sample.pdf` | exit 0；`Pages: 2`、`Page size: 595 x 842 pts (A4)`、`PDF version: 1.7`；`pdfinfo -h` grep `json` 零命中（无 -json 旗标） |
| PDF 文本 | `/usr/bin/pdftotext …/sample.pdf out.txt` 与 `pdftotext …/sample.pdf -` | exit 0；UTF-8 中文正文；`\f`(0x0C)×2 = 2 页锚；`-` stdout 形态 exit 0 |
| Office 转换 | `env -i PATH=/usr/bin TMPDIR=<ws> LANG=C.UTF-8 /usr/lib/libreoffice/program/soffice --headless --norestore -env:UserInstallation=file:///<ws>/lp7 --convert-to pdf --outdir <ws>/out7 …/sample.docx` | exit 0，`writer_pdf_Export`；同法 sample.xlsx exit 0（calc_pdf_Export，fresh profile 4408ms）——**RT-1 D4 env 最小集下 soffice 无需 HOME 追加** |
| Office 挂死面 | `--convert-to txt:Text` / `--cat`（默认 env、`--norestore`、HOME 覆盖三形态） | 三次复现 exit 124/143 挂死，outdir 遗留 `lu*.tmp` 残迹 → **禁登 txt/cat 过滤器** |
| 图像元数据 | `/usr/bin/magick identify -ping -quiet -format "%m %wx%h %[colorspace] depth=%z" …/sample.png` | exit 0；`PNG 834x250 sRGB depth=8`（9ms）；`identify -json` 不支持（unrecognized option） |
| magick coder 面 | `magick identify -list format` | MSL/MVG/PDF/PS/URL 面开放（policy 空壳佐证）→ 嗅探前置为唯一闸 |
| ZIP 嗅探 | python3 struct 实读六样本首条目与 marker | docx/pptx 首条目 `[Content_Types].xml`；**xlsx 首条目 `docProps/app.xml`（首条目规则反例）**；epub `mimetype`、zip `docs/a.txt`、evil.zip `../escape.txt` 无 [Content_Types].xml；`word/document.xml`/`xl/workbook.xml`/`ppt/presentation.xml` marker 全命中 |
| 钉值实测 | `sha256sum /usr/bin/pdftotext /usr/bin/pdfinfo /usr/bin/magick /usr/lib/libreoffice/program/soffice /usr/lib/libreoffice/program/soffice.bin` | `f85872f94fb67433d6fa411ea3a10c263205eada4622b48ccc700442ad29b13c` / `3bdf68a79b9a3c221959c6278aa6f5db757e5b9a6c99fb39c3182d0ff1651ac3` / `ff050139cbc6dbd93a5a50980c60693f8be0bbe49a0497ebdaf5a56595452a40` / `3d585422d7cdddf9bcc954e7064da24c7e44d43508a5abaeb11de9d918d24f13` / `baa182393db76be02cdf0ee22f3654177b00e3b945036927cfe54d4d2cbee2ad`（与种子 16 位前缀逐一吻合；S1 开工复测，漂移即按 C §5.3 标 stale 上报，不静默换值） |

## 3. 范围

### 3.1 允许改动（新增/修改落点）

| 落点 | 内容 | 步骤 |
|---|---|---|
| `crates/document-router/`（新增成员） | Cargo.toml（依赖 tool-engine/tool-registry/domain/serde/serde_json/sha2 钉版）+ src/{lib,sniff,channel,router,normalize,error}.rs + tests/ | S3/S5/S6 |
| `crates/tool-engine/src/adapter/native_rust.rs` | `result_mode` 双模（frame 缺省零改动 \| file：exit 0 → 产物收集 data，无帧解析）；唯一触碰点 | S2 |
| `crates/tool-engine/tests/native_rust_e2e.rs` | file 模式正例/超帽负例 + frame 模式零回归（15/15 保持） | S2/S6 |
| `crates/tool-registry/src/validate.rs` + `schemas/tool-manifest/v1.json` | native_rust 分支 additive `result_mode` 枚举校验 + schema 同步（词表零新增值） | S2 |
| `crates/tool-registry/tests/sk3_tool_manifests.rs` | result_mode 正负例断言 | S2 |
| `manifests/tools/rip.doc.pdf.info.json`、`rip.doc.pdf.text.json`、`rip.doc.office.to_pdf.json`、`rip.doc.image.meta.json`（新增） | D3 通道表四 Manifest（绝对路径 + 64hex 钉值 + capability_ref + argv_template + result_mode:"file" + resources + network:none） | S4 |
| `manifests/capabilities/host-catalog-seed.json` | 四条所用能力 sha256_full 实测回填（C §5.1「只录实测值」纪律） | S4 |
| `crates/mcp-server/src/dispatch.rs` | `"native_rust" =>` 分支（submit_correlated + execute_native_rust_via_engine + Router 门序） | S5 |
| `manifests/packs/planned/document.json` | DI-0 §4.2 冻结文本修订（exit_conditions/trust_ruling） | S0 |
| `docs/phase0/PHASE-ROADMAP.md` §4.3 P3-4 行、`docs/phase3/PHASE3-DESIGN.md` §2.3:53 | DI-0 §6 指派的回填标注（行内标注引用 DI-0 ADR，不改写历史裁定） | S0 |
| `docs/phase0/DI-1-BATCH-REPORT.md` + `docs/README.md` | 批次报告 + 索引登记行 | S8 |

### 3.2 不修改

- `manifests/tools/rip.doc.parse.json`、`workers/python/parsing_worker.py`（DI-0 §3 冻结盘点表口径：不迁不扩）。
- `python_worker.rs`/`podman_host.rs`/`egress_proxy.rs`/`structured_output.rs`/`capability.rs` 既有行为（回归由 §6 测试面守护；capability.rs 本批零 diff，消费 `StaticCatalogProvider::load` 公开面）。
- 其余 15 份工具 Manifest、`migrations/`、既有 workspace 成员 Cargo.toml 依赖面、`vendor/`。
- 生产 `platform*.db`、`platform-dryrun.db`、`integrations/rtis/`、`schemas/packs/planned/*` 其余存根。
- adapter 除 result_mode 外的一切（D4 封顶：DI-1 adapter 触碰边界 = 该一处 additive 字段）。

## 4. 实施步骤（S0 → S8，串行，每步独立可验证）

- **S0 登记面修订（DI-0 §4.2 指派）**：`manifests/packs/planned/document.json` 按 DI-0 §4.2 冻结文本修订（exit_conditions 追加 DI-1 解锁语句；trust_ruling.level 改 `distro-package-verifiable + input-risk-routed`，rationale 引 DI-0 §2.1/§2.2）；PHASE-ROADMAP P3-4 行 + PHASE3-DESIGN §2.3:53 行内标注（superseded-by DI-0，不改写原文）。
- **S1 门禁基线核验**：`grep -c '^\[\[package\]\]' Cargo.lock`（399）、`grep -c 'source = ' Cargo.lock`（375）、`git status --short`（干净）；§6 基线命令复跑（tool-engine 定向 + clippy + workspace 回归）；五条钉值 sha256 复测对照 §2.2 表（漂移即 stale 上报，不静默换值）；四 CLI 在位复测（pdfinfo/pdftotext/magick/soffice --version）。
- **S2 adapter result_mode 双模**：native_rust.rs 增量（allowlist 解析 `runtime.result_mode`，缺省 frame；file 模式 exit 0 → Succeeded data = 产物收集 + stdout 留档引用 + stdout_capped；读帽语义不变）；validate.rs/v1.json/sk3 additive；既有 15 测试全绿为 frame 零回归门 + file 模式新正负例。
- **S3 document-router 骨架**：新成员 Cargo.toml + workspace members 登记（Cargo.toml:3-28）；sniff.rs（D6 表 + xlsx 首条目反例测试）；channel.rs（D3 通道表 + 帽值表 D7）；error.rs（D8 映射表）；normalize.rs（统一输出 Schema 结构）；S1 门禁复测（lock 400/375/vendor 零 diff——400 = 399 + document-router 成员自身条目，唯一允许增量）。
- **S4 Manifest 登记**：四份工具 Manifest 按 D3 表 + 钉值回填（含 office wrapper 双钉 binary_target_sha256）+ seed 四条 sha256_full 回填；`cargo test -p tool-registry`（sk3 面）+ Registry 登记通过；每个 Manifest 登记回执（validate 输出）入批次报告。
- **S5 Router 编排接线**：router.rs 三通道管线（D3/D5：预写 in/ → 逐子 run `execute_native_rust_via_engine` → 产物回读 → 规范化）+ dispatch.rs native_rust 分支（嗅探/帽前置门 INPUT 拒绝 → submit_correlated → 通道编排 → 统一信封）。
- **S6 测试面落地**：crates/document-router/tests/ 三套（sniff 边界、通道表/帽门、三通道真实 run e2e）+ native_rust_e2e file 模式补齐；fixture 全部来自 rip-data/parse-smoke/in/ 既有样本 + magick 本地生成 JPEG（零网络）；负例五类逐项（§5 表）。
- **S7 收口门**：§6 全部命令（定向三套 + clippy + fmt scoped + workspace 回归）+ lock/vendor/生产库/RTIS 复核 + `git status` 只含 §3.1 落点。
- **S8 批次收口**：DI-1-BATCH-REPORT.md 落库（命令原文 + 输出摘要 + 判据对照表 + 实测钉值 + 遗留登记）+ docs/README.md 索引行 + DI-2 前置条件登记（DI-2 消费 pdf.pdftoppm/ocr.tesseract 与 Router 通道表扩展缝）。

## 5. 完成标准（判据表，S6 逐项有测试、S8 逐项有实测输出）

| # | 判据 | 要求（负例为硬性红线） |
|---|---|---|
| 1 | PDF 通道端到端 | sample.pdf 经 Router：pdfinfo 子 run（SUCCEEDED，stdout.log 含 Pages: 2）+ pdftotext 子 run（SUCCEEDED，页锚 2 段）→ 统一输出过 output_schema，channel="pdf"，两条 run 行 + Artifact sha256 对账齐 |
| 2 | Office 通道端到端 | sample.docx 经 Router：soffice 子 run（SUCCEEDED，out/sample.pdf 产物登记，profile_uri 指向本 run tmp/）→ 级联 pdfinfo + pdftotext 子 run → 统一输出 channel="office" + warnings 携带转换面声明；office 通道测试 `--test-threads=1` |
| 3 | 图像通道端到端 | sample.png 与生成的 sample.jpg 经 Router：magick identify 子 run SUCCEEDED → 统一输出 metadata 含 format/width/height/colorspace/depth |
| 4 | 不支持类型拒绝 | sample.epub / sample.zip / evil.zip（zip_other）与伪造 `.pdf` 扩展名纯文本文件 → 嗅探门 INPUT 拒绝（ROUTER_UNSUPPORTED_TYPE），零子进程拉起、零 run 行；text 签名拒绝注明走 rip.doc.parse |
| 5 | 能力 missing 显式错 | harness 注入缺失 capability_ref（pdf.mutool 种子条目形态）→ 子 run PREPARING 拒绝，DEPENDENCY `NATIVE_BINARY_MISSING` 显式信封（不静默换实现、不降级） |
| 6 | 超帽截断 | 子 run stdout 超 max_output_bytes → `NATIVE_OUTPUT_CAP_EXCEEDED` FAILED（不静默丢字节）；Router 统一输出超信封帽 → envelope::truncate + usage.truncated=true + Artifact 对账 |
| 7 | soffice 隔离 profile | 每 run profile_uri 唯一（run_id 派生）+ tmp 用后即焚断言；两 run 不得共享 profile 目录（E §4.2）；office 套件单线程 |
| 8 | env 白名单断言 | 四工具子进程环境 = PATH/TMPDIR/LANG 精确集（本批 Manifest env 零追加）；fixture 复用 env_echo 形状断言零宿主键继承（SSH_AUTH_SOCK/HTTP_PROXY/数据库路径零出现） |
| 9 | 嗅探权威 | `.pdf` 扩展名 + PNG 魔数 → 路由图像通道 + `source.sniff.mismatch=true` + warnings 记录（魔数权威，扩展名仅提示）；xlsx 首条目反例样本正确判 zip_ooxml/xlsx 子型 |
| 10 | dispatch 分支 | mcp-server native_rust 分支经 Gateway 派发一条真实 run（trace_id = run_id 自锚定，REST /api/traces 可反查）；`other` 拒绝面不回归 |

## 6. 测试面（收口命令，与 CI 同门）

```bash
# 定向三套（document-router 全量 + adapter file 模式；office 通道套件在 router 测试内
# 以 #[ignore] 外单独 test 名承载并强制 --test-threads=1）
cargo test --manifest-path /media/emo/code/pensting/research-platform/Cargo.toml \
  --offline -p document-router -- --test-threads=1
cargo test --manifest-path /media/emo/code/pensting/research-platform/Cargo.toml \
  --offline -p tool-engine native_rust -- --test-threads=1
cargo test --manifest-path /media/emo/code/pensting/research-platform/Cargo.toml \
  --offline -p tool-registry

# 静态检查门（新成员 + 触碰面）
cargo clippy -p document-router -p tool-engine -p tool-registry --all-targets -- -D warnings

# 格式门（scoped：本批落点文件；全仓 fmt 漂移 661 处为 HEAD 既有红，RT-1 报告 G 遗留 4，不属本批）
cargo fmt -p document-router -p tool-engine -p tool-registry -- --check

# 跨 crate 回归（S7）
cargo test --offline --workspace
```

基线记录（本任务书写作时点 2026-11-19 实测）：RT-1 收口批门禁终态为「定向 native_rust 15/15、capability_catalog 11/11、workspace 回归 146 套件 888 passed 0 failed、lock 399、vendor 零改动」（RT-1 批次报告 commit 1571a227 原文）；DI-1 开工以 S1 复跑为准，任何基线漂移先上报再动手。

测试离线与能力缺失纪律：全部命令带 `--offline`；fixture 零下载（仓库既有样本 + magick 本地生成 JPEG）；任一通道 CLI 在目标环境缺失（capability 目录 status != available）时，该通道测试按 StaticCatalogProvider status 判定 SKIP 并打印显式标注行（`SKIP <test>: capability <id> status=<status>`，RT-1 host_pin_guard SKIP 先例 commit 12258e86 同款），不得伪造失败或空跑。

## 7. 红线（十条，违反任一即停批上报）

1. **零 vendor 增量**：`vendor/` 零 diff；Cargo.lock source=registry 条目数恒 375（S1/S3/S7 三点复核）；lock 总条目唯一允许增量 = +1（document-router 成员自身条目，399→400，无新外部包条目）；MIME/魔数嗅探手写，禁新增任何依赖（含 dev-dependencies，mime_guess/file-type/infer 等显式禁止）。
2. **禁网络**：运行与测试全程离线（`--offline`）；fixture 零下载；三通道 CLI 面禁任何网络资源获取（magick URL coder / soffice 外链 / poppler 远程对象在结构上被嗅探前置 + network:none 双闸阻断）；证据归档不含任何外联记录即合规，出现外联即停批。
3. **能力缺失按目录 status 跳过并标注**：测试对 status != available 能力 SKIP + 显式标注输出；缺失能力只报告不安装（宿主软件零安装，RT-1 红线 8 沿用）。
4. **生产库零触碰**：`platform*.db`/`platform-dryrun.db` 全程不写；测试内存库/临时库。
5. **RTIS 零接触**：`integrations/rtis/` 零 diff。
6. **soffice 测试单线程**：office 通道套件 `--test-threads=1`；每 run 隔离临时 profile（run_id 派生），禁共享 profile、禁常驻 soffice 实例复用（E §4.2）。
7. **不建第二事实源**：ToolRun/Artifact/Evidence/Trace/预算/取消全部复用引擎既有面；Router 零 `std::process::Command`（D2）；错误信封只用九类词表 + RT-1 NATIVE_* 码表 + 本批 D8 表新增 code（category 不新增）。
8. **adapter 触碰封顶**：native_rust.rs 仅允许 result_mode 一处 additive 增量（D4）；帧模式行为零回归（15/15 既有测试全绿为门）；capability.rs 零 diff。
9. **既有 runtime 零回归**：python_worker/podman_host 行为不变；`cargo test --offline --workspace` 全绿为回归门；rip.doc.parse 及全部既有 Manifest 零改写。
10. **档位不越权**：DI-1 只承载「可信低风险档受限原生」面（Manifest permission_level + 嗅探签名承载，RT-1 ADR 生效条款）；不明来源/高风险档判定不做、不实现、不绕过——Router 嗅探结果不构成信任判定，输入来源声明面留 RT-3/DI-3。

## 8. 回滚

- 每步独立 commit（S0/S2/S3/S4/S5/S6/S8 各一）；`git revert <批次 commit 集>` 整体撤销；result_mode 为 additive（回滚后四份 DI-1 Manifest 一并随批回滚，不影响既有登记）。
- document-router 成员回滚 = 删目录 + members 行移除 + lock 回 399（成员条目唯一增量，无外部状态）。
- 测试载具/fixture 随代码回滚，无外部状态；seed 回填值为只读数据面；生产库/RTIS/宿主软件零操作，无生产回滚面。

## 9. 交付物

1. 代码：`crates/document-router/`（sniff/channel/router/normalize/error + tests）+ adapter result_mode + validate/schema additive + dispatch 分支（S2-S6）。
2. 数据：四份 DI-1 工具 Manifest + seed 四条 sha256_full 回填（S4）。
3. 文档：本任务书、[DI-1-ADR-DOC-ROUTER.md](DI-1-ADR-DOC-ROUTER.md)、S0 三处登记修订、DI-1 批次验收报告（S8）+ docs/README.md 登记。
4. 证据：三通道真实 run 的 ToolRun 行 + 统一输出信封实例 + Artifact sha256 对账 + 负例五类实测输出（S8 对照表）。
5. 前置输出：DI-2 可消费的通道表扩展缝（pdftoppm/tesseract 行位）+ Router 前置门/编排复用面 + DI-3 登记（facade run 审计锚、RT-3 判定移交点）。
