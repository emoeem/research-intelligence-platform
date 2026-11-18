# DI-1 ADR：Document Router 设计裁决（落位、通道表、嗅探、帽值、错误映射、证据接线）

- 日期：2026-11-19
- 状态：已定稿（DI-1 设计裁决；实现口径冻结，对应实现任务书 [DI-1-TASK.md](DI-1-TASK.md)）
- 依据：[RT-0.5-E-RISK-AND-ISOLATION.md](RT-0.5-E-RISK-AND-ISOLATION.md) §3 路由图（52-72 行）、§1 三档路由、§2 十四项资源限制、§4 本机特化裁决；[RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md](RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md) §6 集成点、§7 五态；[RT-1-ADR-NATIVE-ADAPTER.md](RT-1-ADR-NATIVE-ADAPTER.md) D1-D8；[DI-0-P34-PREMISE-REVISION-ADR.md](DI-0-P34-PREMISE-REVISION-ADR.md) §5 DI 链入口；[RT-1-BATCH-REPORT.md](RT-1-BATCH-REPORT.md) H.2 DI-1 消费面登记（293-298 行）
- 代码现状锚点（2026-11-19 只读实读）：crates/tool-engine/src/adapter/native_rust.rs（帧协议 :1224-1261、stdout 留档 :1221、input_content 文本限制 :747、argv 槽位语法 :736-772、工作区 :255-278、公开 API :1432-1452）、crates/mcp-server/src/dispatch.rs:230-281（native_rust 无分支）、crates/tool-registry/src/validate.rs:243-263/780-794、crates/tool-engine/src/envelope.rs:16-24（九类词表）、Cargo.lock（399 条目 / 375 条 source=registry）
- 本机实测锚点（2026-11-19，临时探测目录 /tmp/di1-probe，样本 = 仓库既有 rip-data/parse-smoke/in/，全部只读或写 /tmp，零网络）：本文各裁决逐条引用，原始命令与回显摘录见 [DI-1-TASK.md](DI-1-TASK.md) §2
- 决策记录形态：D1 到 D8，每条含裁决、论据（引用在位证据）、否决项

## D1：Router 落位新 crate `document-router`，不进 tool-engine 内模块

**裁决**：Document Router 落新 workspace 成员 `crates/document-router/`，依赖面 = tool-engine（公开 API）+ tool-registry + domain + serde/serde_json + sha2（workspace 既有钉版），零新外部依赖。

**论据**：

- Router 是**文档域编排层**（RT-0.5-E §3 路由图：签名→档位→能力查找→策略→规范化），不是执行后端。tool-engine 的 adapter/ 面是执行后端层（python_worker/podman_host/native_rust 并列，adapter/mod.rs:13-17），把文档域知识塞入引擎基座会造成层次倒挂：mcp-server 经依赖链间接获得文档域传递面。
- 新成员零 vendor 增量有直接先例：WSW-1 建 `crates/wsw-message` 时「sha2/base64 均已在 Cargo.lock/vendor；作为本 crate 的直接依赖钉选既有版本，不增加 registry package」（Cargo.toml:71-76 原文注释）。sha2 在 lock 内有两个已钉版本（0.10.9 / 0.11.1，Cargo.lock:2594/2605），serde 同在；新成员只新增自身 `[[package]]` 条目（name+version+成员依赖行，无新 source=registry 条目）：lock 总条目 399→400，source=registry 条目恒 375（2026-11-19 `grep -c` 实测），vendor/ 零 diff。成员条目进 lock 是常态事实（wsw-message 条目在 Cargo.lock:3790 在册）。
- RT-1 ADR D1 否决 adapter 新 crate 的论据（需 Engine/RunRow/TerminalParts 等 engine 内部件跨 crate 重导出，native_rust.rs 引擎接线紧耦合）**不适用于 Router**：Router 只消费公开面 `execute_native_rust_via_engine(NativeRustCall{...})`（native_rust.rs:1432-1452，pub）与 `StaticCatalogProvider`（capability.rs:1028，pub），无需 engine 内部件。
- crate 边界是 RT-3 的干净接缝：DI-3 把 Router 最小档位判定前置承接 RT-3 Policy Resolver（DI-0 §5:100-106），RT-3 落地后替换 Router 判定面，不牵动 tool-engine。

**否决项**：

- tool-engine 内模块（`src/document_router.rs`）：tool-engine 依赖面纪律（RT-1 任务书红线 2「现状依赖面不改」）要求 Router 所需 sha2 进 engine 直接依赖，扩大引擎基座依赖面；文档域测试混入 engine 测试套件；否决。
- 挂 mcp-server 内：Router 需被 workflow-engine/agent-engine 侧复用（DI-3 路由整合），挂派发 CLI 使复用方向倒挂；否决。
- 复用 name `doc-router`/`rip-doc-router`：workspace 成员命名惯例为全拼 kebab（context-broker/unified-search/wsw-message），取 `document-router`。

## D2：执行链复用既有引擎，Router 不自建子进程派发；dispatch 层补 native_rust 分支

**裁决**：Router 的每一步工具执行都经 tool-engine 公开 API `execute_native_rust_via_engine`（native_rust.rs:1450）驱动，子 run 全部是真实 ToolRun（状态机/信封/Artifact/Evidence/Trace 既有面）；Router 自身零 `std::process::Command` 调用。mcp-server dispatch.rs 补 `"native_rust" =>` 分支（对应 RT-1 批次报告 G 节遗留第 2 条「DI-1 经 tool-engine 派发链验证」）。

**论据**：

- RT-0.5-E §5:122「全部执行后端复用同一 Tool Registry、ToolRun、Artifact、Evidence、Trace 与预算/取消语义，不建立第二套事实源」；DI-1 红线同款。
- dispatch.rs 现状两分支（python_worker :231-244 / podman_host|podman_vm :245-272）+ `other =>` 拒绝（:273-280），native_rust 分支缺位即 RT-1 收口时「mcp-server/runtime-gateway 零命中」现状（RT-1-TASK §2 实测）——DI-1 是该缺位的登记消费批。
- 受理走 `submit_correlated`（dispatch.rs:227-229 先例，trace_id = run_id 自锚定），使 MCP 通道 run 可经 REST /api/traces 反查。

**否决项**：

- Router 绕过 adapter 直接 spawn CLI（自有超时/帽/环境逻辑）：第二套事实源 + 绕过 allowlist/钉值/资源强制全链，违反 NATIVE-FIRST §4 统一执行架构，否决。
- Router 内嵌第二套状态机（自管 QUEUED/RUNNING）：与 Engine 状态机分叉（E §2 条目 14），否决。

## D3：通道表（输入签名 → capability_id → Manifest/argv 模板 → 输出规范化）；soffice 本机特化裁决

**裁决**：DI-1 通道面 = 四份 native_rust 工具 Manifest，Router 按嗅探签名编排下表管线。argv 模板全部为本机实测通过形态（2026-11-19 /tmp/di1-probe 实测），`{{slot}}` 语法遵守 adapter 既有规则（整 token 精确匹配 `{{input_path}}`/`{{out_dir}}`/`{{tmp_dir}}`/`{{slot}}`，混合 token 字面量透传，native_rust.rs:736-772 实读）：

| 通道（嗅探签名） | capability_id | Manifest（runtime 形态） | argv 模板（实测冻结） | 输出规范化 |
|---|---|---|---|---|
| PDF（`%PDF-`） | pdf.pdfinfo | rip.doc.pdf.info（形态 A：binary + 64hex 钉值 + capability_ref） | `["{{input_path}}"]` | stdout 报告文本解析 `Pages:`、`Page size: W x H pts`（本机无 `-json` 旗标，`pdfinfo -h` grep 实测零命中） |
| PDF（`%PDF-`） | pdf.pdftotext | rip.doc.pdf.text（形态 A） | `["{{input_path}}", "-"]` | stdout 文本（`-` = stdout，本机 exit 0 实测）；页锚 = `\f`（0x0C）分隔切分（sample.pdf 实测 2 页 = 2 个 `\f`） |
| Office（ZIP→OOXML） | document.libreoffice | rip.doc.office.to_pdf（形态 A + wrapper 双钉：soffice + soffice.bin，C §5.1:270） | `["--headless", "--norestore", "{{profile_uri}}", "--convert-to", "{{out_format}}", "--outdir", "{{out_dir}}", "{{input_path}}"]` | out/ 产物 PDF（soffice 命名 `<输入basename>.pdf`，Router 按 staged 名确定性回读）→ **级联 PDF 通道**（pdfinfo + pdftotext 两个子 run） |
| 图像（`\x89PNG` / `\xFF\xD8\xFF`） | image.magick | rip.doc.image.meta（形态 B：subcommands 白名单 `["identify"]`） | `["-ping", "-quiet", "-format", "{{identify_fmt}}", "{{input_path}}"]` | stdout 单行元数据串（本机实测 `PNG 834x250 sRGB depth=8`，9ms）解析 format/width/height/colorspace/depth |

- **soffice 本机特化裁决（实测推翻「Office→txt 直转」直觉）**：`--convert-to txt:Text` 与 `--cat` 在本机 LibreOffice 26.8.2.2（libreoffice-fresh）**挂死**（timeout 杀，exit 124/143，三次复现：默认 env / `--norestore` / HOME 覆盖三形态同败，outdir 遗留 `lu*.tmp` 残迹）；`--convert-to pdf` 全链通过（writer_pdf_Export / calc_pdf_Export exit 0，xlsx fresh-profile 实测 4408ms）。**argv 模板禁登 txt/cat 过滤器，Office 文本抽取一律走 Office→PDF→pdftotext 级联**；warnings 携带「office→pdf 转换面」声明（转换损失如实呈现，不伪装直抽）。
- **soffice env 特化实测**：`env -i PATH=/usr/bin TMPDIR=<ws tmp> LANG=C.UTF-8` + `-env:UserInstallation=file:///<ws>/tmp/lo-profile` 成功（exit 0 + 产物落盘）——RT-1 D4 env 白名单最小集（PATH/TMPDIR/LANG）下 soffice **无需** HOME/XDG 追加。隔离临时 profile 由 `{{profile_uri}}` 槽位承载：值由 Router 结构化构造（`file://` + 本 run `{{tmp_dir}}` 派生绝对 URI，混合 token 无法经 argv 模板组合——native_rust.rs:770-772 字面量透传实读），input_schema 以 pattern 限定 `file:///` URI 形态；profile 位于 tmp/ 由 adapter `cleanup_tmp`（native_rust.rs:1524）四路径用后即焚，E §4.2「禁共享 profile、每任务隔离」由此结构成立。
- **magick 受限面**：只登 `magick` 不登 `convert`（RT-0.5-E §4.6）；元数据路径只 `-ping`（仅读头部不全量解码，像素内存放大器受控）；图像通道输入签名仅 PNG/JPEG，PDF/PS/MSL/MVG/URL 面**永远到不了 magick argv**（嗅探前置：D6），policy.xml 空壳（E §4.1）不构成豁免理由。
- 统一输出规范化 Schema（Router 产出，经引擎信封 output_schema 校验交付）：

```jsonc
{
  "channel": "pdf | office | image",
  "source": {
    "kind": "file", "name": "<staged 名>", "bytes": 0, "sha256": "<64hex>",
    "sniff": {"magic": "pdf|png|jpeg|zip_ooxml|text", "ext": "<输入扩展名小写>", "mismatch": false}
  },
  "text": "",                       // PDF/Office 为抽取文本；image 为 ""
  "metadata": {
    // pdf/office：pages、page_size{width,height,unit:"pts"}、pdf_version、producer（pdfinfo 可得则带）
    // image：format、width、height、colorspace、depth
    // office 另带：office_subtype("docx|xlsx|pptx")、converted_pdf_bytes
  },
  "page_anchors": [                 // pdf/office：按 \f 切分，逐页 char 区间
    {"page": 1, "char_start": 0, "char_end": 0}
  ],
  "warnings": [],                   // 魔数/扩展名不一致、office→pdf 转换面声明、子 run 截断标记等
  "runs": []                        // 本链全部子 run_id（ToolRun 行锚）
}
```

**否决项**：

- Office 直出 txt（`txt:Text`/`--cat`）：本机实测挂死（见上），且任何「再试一次」重试面都属浪费预算的挂死复现，否决。
- `magick identify -json`：本机 IM7 7.1.2-35 不支持（`identify: unrecognized option '-json'` 实测）；`magick <in> json:-` 虽可用但为多行 pretty JSON 且与统一输出规范化重复解析，取 `-format` 单行面（最简、帽内），否决 JSON coder 路径。
- pdfinfo `-json`：本机 poppler 26.11.0 pdfinfo 无该旗标（-h 实测），否决 JSON 假设；Router 以行解析承载（`Pages: \d+` / `Page size: (\d+) x (\d+) pts`）。
- Office 元数据另调 exiftool：meta.exiftool 属 DI-3 面（DI-0 §5），DI-1 不扩；否决。

## D4：结果面双模 `runtime.result_mode`（frame | file）——adapter 最小 additive 增量 + DI-0 前提修订登记

**裁决**：RT-1 adapter 帧协议强制 stdout 首行 = `{"status":"result",...}`（native_rust.rs:1224-1261，exit 0 + 非 JSON stdout 落 `NATIVE_UNSUPPORTED_OUTPUT` FAILED）。本机四工具全部为 raw CLI（pdftotext/pdfinfo 文本输出、soffice 转换静默、magick -format 单行），**无一可发帧**。DI-1 为 Manifest 新增可选字段 `runtime.result_mode`：`"frame"`（缺省，既有行为零改动）| `"file"`：

- file 模式：exit 0 → `AdapterOutcome::Succeeded`，`data = {"exit_code": 0, "stdout_bytes": N, "stdout_capped": bool, "artifacts": [out/ 产物收集]}`（stdout/stderr 留档 out/stdout.log、out/stderr.log 为既有面，native_rust.rs:1216-1221 实读），**不做帧解析**；stdout 读侧帽语义不变（超帽 → `NATIVE_OUTPUT_CAP_EXCEEDED` FAILED，不静默丢字节——与帧模式 :1248-1255 同语义）。
- 登记面：validate.rs native_rust 分支 additive 校验（枚举两值）+ schemas/tool-manifest/v1.json 同步；词表零新增信封 category（file 模式错误仍落九类内）。
- 四份 DI-1 Manifest 全部 `result_mode: "file"`；既有帧协议工具不迁移不回归（RT-1 15/15 判据测试保持全绿为门）。

**论据**：DI-0 §5:111 原文「DI-1 只做工具 Manifest 登记与端到端 run，不回改 adapter」的前提在其写作时点成立（当时 DI-1 范围 = PDF 单通道，本文档写作时 DI-1 范围按指令扩为三通道端到端）。三通道 raw CLI 端到端在帧协议下**无解**：不用 adapter 即破 D2 统一执行链（否决），wrapper 补帧即破禁 Shell/零依赖纪律（否决）。故修订该前提：**DI-1 允许 adapter 一处最小 additive 增量（result_mode 字段），帧模式行为零回归**，登记于本 ADR 作为 DI-0 §5 的偏差修正记录；「不加第二个增量」为 DI-1 adapter 触碰边界。

**否决项**：

- frame 包装二进制（Rust 重编译小工具包一层帧）：为四个 CLI 维护四个 wrapper 是常驻维护面且 argv 转发层引入新的注入面，否决。
- 修改帧协议本身（宽容多行/尾随输出）：破坏 D-TC-03 帧不变量对全部既有 runtime 的语义，影响面大于 additive 字段，否决。

## D5：二进制输入暂存——Router 预写 `<workspace_root>/<run_id>/in/`，adapter 零改动

**裁决**：子 run 二进制输入（PDF/docx/xlsx/pptx/PNG/JPEG）由 Router 在驱动该 run **之前**预写至 `<config.workspace_root>/<run_id>/in/<staged 名>`（路径确定性：workspace_root 来自同一 `NativeRustConfig`，run_id 由 Router 提交时指定），argv 的 `{{input_path}}` 槽位传相对名；adapter `input_content` 仅承载 UTF-8 文本（`Value::as_str`，native_rust.rs:747 实读），不扩。

**论据**：

- adapter `prepare_workspace` 用 `create_dir_all` 不清目录（native_rust.rs:255-278 实读），预写文件在 PREPARING 后仍在其位；`{{input_path}}` 前缀校验（native_rust.rs:1322-1330，防任意文件读取）对预写面照常生效，边界不放松。
- 二进制经 JSON 字符串通道（input_content）不可行：PDF/Office 字节非 UTF-8 安全，as_str 通道结构性不可承载；base64 扩展属 adapter 第二处增量（D4 已封顶一处），否决。
- 「输入文件由引擎侧落 in/」是 RT-1 ADR D8.3 的原意表述；Router 属引擎侧调用方（与 adapter 同信任位、同配置源），预写即该职责的最小实现；RT-1 批次报告 B.3 #8「artifact mount_as 式富暂存属 DI-1 面」——本裁决为该面的 DI-1 形态（预写），Artifact 挂载式富暂存留 DI-3。

**否决项**：

- input_content 扩 base64 字段：第二处 adapter 增量，破 D4 封顶；大文件 JSON 化双倍内存放大，否决。
- 子 run 直接引用上一 run 的 out/ 绝对路径：绕过 in/ 前缀校验（防任意文件读取的边界失效），否决——级联产物必须重写进下一 run 自己的 in/。

## D6：魔数嗅探手写表（零新依赖）；ZIP/OOXML 判定用全量条目扫描，禁「首条目」规则

**裁决**：Router 自带手写嗅探器（`crates/document-router/src/sniff.rs`），读取输入头 ≤16 字节 + （ZIP 时）条目名扫描，零第三方 MIME/魔数依赖（禁 mime_guess/file-type/infer 类，红线 1）：

| 签名 | 判据（手写，实测样本验证） | 通道 |
|---|---|---|
| `pdf` | 头 4 字节 `%PDF-` | PDF 通道 |
| `png` | 头 8 字节 `89 50 4E 47 0D 0A 1A 0A`（sample.png 实测） | 图像通道 |
| `jpeg` | 头 3 字节 `FF D8 FF`（fixture 由 magick 本地生成，零网络） | 图像通道 |
| `zip_ooxml` | 头 4 字节 `PK\x03\x04` 且条目名扫描命中 `[Content_Types].xml`；子型按 marker 条目：`word/document.xml`→docx、`xl/workbook.xml`→xlsx、`ppt/presentation.xml`→pptx（本机三样本 marker 全部实测命中） | Office 通道 |
| `zip_other` | `PK\x03\x04` 且无 `[Content_Types].xml`（sample.epub 首条目 `mimetype`、sample.zip `docs/a.txt`、evil.zip `../escape.txt` 实测均无） | **DI-1 显式不支持**（INPUT 拒绝；归档两阶段面属 DI-3，RT-0.5-E §4.3） |
| `text` | 全量 UTF-8 有效（`str::from_utf8` 校验）且无上述魔数（sample.md/sample.txt/sample.html 实测） | **DI-1 显式不路由**（INPUT 拒绝并注明走既有 rip.doc.parse；rip.doc.parse 冻结不迁不扩，DI-0 §3，禁造第二套 md/txt/html 解析） |
| 其它 | 未知魔数/非 UTF-8 二进制 | INPUT 拒绝（ROUTER_UNSUPPORTED_TYPE） |

- **禁「首条目 = [Content_Types].xml」判定**：本机实测 sample.xlsx 首条目为 `docProps/app.xml`（非 [Content_Types].xml），首条目规则将把合法 xlsx 误判 zip_other；条目扫描用顺序 local-file-header 名遍历（上限 256 条目 + 4 MiB 读前缓冲，超限按 zip_other 处理 fail-closed），手写零依赖（无需 EOCD 全解析）。
- 魔数权威、扩展名仅提示：嗅探与扩展名可路由类型不一致 → 以嗅探为准路由 + `source.sniff.mismatch=true` + warnings 记录（不拒绝）；嗅探结果本身不受支持 → 拒绝。
- 嗅探发生于任何子 run 提交之前（URL/MSL/MVG 等 coder 输入在结构上到不了 magick argv，E §4.1 policy 空壳的补偿面）。

**否决项**：

- 引入 `mime_guess`/`file-type`/`infer` 等 crate：新外部依赖破零 vendor 增量，且扩展名库嗅探不满足「魔数权威」要求，否决。
- 信任扩展名优先：.pdf 名 + PNG 体将误派发到 poppler（解析器漏洞面），魔数必须权威，否决。
- 只扫首条目：xlsx 反例（本机实测）直接推翻，否决。

## D7：帽值表（对齐 RT-0.5-E §2 十四项）与 Router 前置门拒绝语义

**裁决**：十四项逐条承载如下；数值为本任务书定稿值（RT-0.5-E 未定数值的项由 Router/Manifest 承载定稿）：

| E §2 # | 限制项 | DI-1 帽值 | 承载层 |
|---|---|---|---|
| 1 | 只读输入 | 输入经 Router 预写进子 run in/（D5），`{{input_path}}` 前缀校验 | adapter 既有（native_rust.rs:1322-1330） |
| 2 | 独立输出 | 每 run 独立 `<root>/<run_id>/{in,out,tmp}` | adapter 既有（:255-278） |
| 3 | 非特权身份 | `env_clear` 最小集（PATH/TMPDIR/LANG），零继承 | adapter 既有（D4） |
| 4 | CPU | Manifest `resources.cpu: "1"` → prlimit `RLIMIT_CPU`（本机 Auto 档落 prlimit 兜底，RT-1 报告 G 遗留 1） | manifests + adapter 既有 |
| 5 | 内存 | pdf/image `256Mi`；office **16Gi**（实测定值——初定 512Mi 被 RLIMIT_AS 实测推翻，见文末「D7 调和裁决」；16Gi 的 RLIMIT_AS 实质为近乎无约束地址空间帽，收紧缝 = systemd MemoryMax，RT-1 报告 G 遗留 1） | manifests resources.memory |
| 6 | 时长 | pdf 60s / image 60s / office 120s（office 首跑 profile 初始化放大系数按 4.4s 实测 ×20 余量） | manifests resources.timeout_seconds |
| 7 | 页数 | PDF **2000 页**：pdfinfo 子 run 后解析 `Pages:`，超帽中止链（office 级联同门） | Router 门（链内 FAILED） |
| 8 | 像素 | 图像 **100 MP**（元数据路径 `-ping` 仅头部读；DI-2 渲染/OCR 批在派生前按同表收紧） | Router 门 |
| 9 | 解压大小 | DI-1 无解包面（OOXML 不解包，仅条目名扫描）；两阶段列目/解包防护属 DI-3（E §4.3） | — |
| 10 | 输出上限 | 子 run `max_output_bytes: 1048576`（stdout 读帽）；Router 终局信封经 `envelope::truncate`（envelope.rs:278）+ `usage.truncated`（:250）——**实现口径注（评审修订 2026-11-19）**：`envelope::truncate` 只作用于子 run 终局信封（engine 既有面）；Router 统一输出本批无外层信封（不做 facade run），其限界 = 子 run 读帽的结构承载（pdftotext 文本 ≤1 MiB、pdfinfo/identify 报告微小），外层统一输出的信封化与帽升级随 DI-3 facade run | manifests + engine 既有 |
| 11 | 网络 | 四 Manifest `network.mode: "none"`；嗅探前置使 URL coder 面不可达 | manifests + Router |
| 12 | 环境白名单 | adapter D4 既有；四工具 `runtime.env` 零追加（env -i 最小集本机实测通过，D3） | adapter 既有 |
| 13 | 文件系统边界 | 工作区三目录 + in/ 前缀校验；家目录/凭据/生产库零暴露 | adapter 既有 |
| 14 | 状态统一 | 全部子 run 走 ToolRun 状态机 + 九类信封；Router 拒绝语义见下表 | engine |

- 输入文件大小帽：**64 MiB**（对齐 worker-runtime `max_output_bytes` 上限 67108864 同量级口径），Router 嗅探前 stat 检查，超帽拒绝（实现承载双面：dispatch native_rust 分支在 `fs::read` 全量读入**之前** `metadata` stat 判帽——评审修订 2026-11-19，超帽不读入内存，封闭受信输入根内的内存放大缝；+ `route()` 门对 bytes_len 复核。两面同源 `RouterReject::input_too_large` / `channel::MAX_INPUT_FILE_BYTES`，码 = `ROUTER_INPUT_TOO_LARGE`）。
- **Router 前置门拒绝语义（先于任何子 run 提交）**：无通道签名（zip_other/未知魔数/text）、文件超 64 MiB → dispatch 层 INPUT 类错误载荷返回（dispatch.rs:273-280 `other =>` 拒绝为先例，无 run 行）。链级失败（页数/像素超帽、soffice 产物缺失、规范化失败）发生于子 run 行已在库之后，以 dispatch 错误载荷 + 子 run 行留证；**外层 facade run 审计锚升级属 DI-3**（与 RT-3 Policy 接入一并），本批不做 facade run 以避免发明第二个状态机路径。

**否决项**：

- Router 前置门伪造一个 FAILED run 行落库（无 adapter 参与）：需绕过 Engine 状态机合法性（preparing_failed 是 adapter 私有路径），且错误归因（Router vs 工具）被污染，否决；DI-3 facade run 是正解缝。
- 像素帽套用 DI-2 渲染预算（如 50 MP/页 ×页数）：DI-1 无渲染面，照抄会过度约束元数据抽取；100 MP + `-ping` 头读为元数据面定稿值，否决混用。

## D8：Artifact/Evidence/Trace 接线（复用既有链）与错误映射总表

**裁决**：证据面零新建——每子 run 是真实 ToolRun（`submit_correlated` trace 自锚定，dispatch.rs:227-229 先例）；stdout/stderr 留档（out/stdout.log、out/stderr.log）与 out/ 产物（office 的转换 PDF）由 adapter 既有 `collect_out_artifacts` 登记 Artifact（sha256/size 对账，RT-1 D8 既有面）；Router 统一输出作为外层信封 result 交付（file 模式子 run 的 data 仅含产物引用，规范化由 Router 组装后走引擎信封）。**实现口径注（评审修订 2026-11-19）**：「走引擎信封」在本批的承载形态 = dispatch native_rust 分支 MCP payload 直付统一输出（无外层 run/信封——D7 明文本批不做 facade run，方向自洽）；统一输出的实际限界来自子 run 读帽 1 MiB 的结构承载，`envelope::truncate` 只作用于子 run result（engine 既有终局路径），外层帽升级随 DI-3 facade run（D7 #10 行内注同款）。不建第二套 Artifact/Evidence/Trace（E §5:122）。

错误映射总表（九类词表唯一，envelope.rs:16-24；五态继承 RT-1 ADR D7 表，不发明第九类外值）：

| 事件 | category | code | ToolRun 状态 |
|---|---|---|---|
| 无通道签名 / 文件超 64 MiB（嗅探门） | INPUT | `ROUTER_UNSUPPORTED_TYPE` / `ROUTER_INPUT_TOO_LARGE` | dispatch 层拒绝（无 run 行，D7 前置门语义） |
| 页数超 2000 / 像素超 100 MP | INPUT | `ROUTER_PAGE_BUDGET_EXCEEDED` / `ROUTER_PIXEL_BUDGET_EXCEEDED` | 链内：已存子 run 行保持其真实终态，链结果 dispatch 载荷 FAILED |
| capability missing（目录五态） | DEPENDENCY | `NATIVE_BINARY_MISSING` | 子 run PREPARING 拒绝 → FAILED（RT-1 D7 既有） |
| capability stale | POLICY_DENIED | `NATIVE_CAPABILITY_STALE` | 子 run PREPARING 拒绝 → FAILED（RT-1 B.3 #15 fail-closed 既有） |
| 钉值 sha256 实测不符 / 子命令越白名单 / argv 注入 / 保留键冲突 | POLICY_DENIED | RT-1 既有 NATIVE_* 码表 | 子 run PREPARING 拒绝 → FAILED（零 spawn） |
| 子 run 非零退出 | EXECUTION | 退出码（RT-1 既有 :1264-1273） | FAILED |
| soffice 被 TERM/KILL 后 out/ 无转换产物 | EXECUTION | `OFFICE_CONVERT_NO_OUTPUT` | 链内 FAILED（挂死残迹 lu*.tmp 如实进 stderr/日志面） |
| stdout 读帽触顶（file 模式） | EXECUTION | `NATIVE_OUTPUT_CAP_EXCEEDED` | FAILED（D4 语义，不静默丢字节） |
| 探测/派发面瞬时故障 | INFRA | `NATIVE_PROBE_FAILED` | FAILED retryable=true（RT-1 D7 既有） |
| 预算到期 / 协作取消 | TIMEOUT / CANCELLED | `-32002` / `-32004` | TIMED_OUT / CANCELLED（既有四路径统一） |
| 结果超信封帽 | （非错误） | — | `envelope::truncate` 降级 + Artifact 对账 + `usage.truncated=true`（既有） |

**论据**：RT-0.5-F §7 五态区分是硬要求（「禁止把 timeout/probe_failed 报成 missing」）；Router 新增事件全部映射进既有九类与 RT-1 码表，`OFFICE_CONVERT_NO_OUTPUT` 与 `ROUTER_*` 为信封 error.code 自由面（九类约束的是 category 非 code），envelope::validate 不受影响。

**否决项**：Router 自定义 category（`ROUTER_*` category）：超出九类词表，envelope::validate 直接拒绝（envelope.rs:160-165），否决。

## 生效与关系

- 本 ADR 为 DI-1 实现批次的设计依据；实现任务书 = [DI-1-TASK.md](DI-1-TASK.md)（S0→S8 步骤、判据表、测试面、红线）。

## D3 调和裁决（实现批追加，2026-11-19；office wrapper 双钉的分层承载）

**触发事实（实现批 /tmp 探针实测）**：RT-1 `verify_wrapper_target` 只识别 `exec <绝对路径>` 静态形态；本机 `/usr/lib/libreoffice/program/soffice`（sh wrapper）的 exec 目标为变量形态（`exec $GDBTRACECHECK "$sd_prog/soffice.bin"` / `exec $RRCHECK $VALGRINDCHECK $STRACECHECK "$sd_prog/oosplash"`，静态目标 0 个）→ `binary_target_sha256` 在场即 PREPARING fail-closed `NATIVE_WRAPPER_TARGET_UNRESOLVED`，office 通道每 run 必死；且 adapter PATH = 钉值二进制所在目录（D4 结构面，Manifest env 禁覆盖 PATH）下 `/usr/lib/...` 形态 wrapper 脚本 `dirname: command not found` exit 1。两者均为本 ADR 写作时点未实测到的实现事实。

**裁决（B）**：office Manifest `runtime.binary = /usr/bin/soffice`（发行版符号链接 → 同一物理文件，sha256 `3d585422…` 不变，PATH=/usr/bin 使 wrapper 脚本核心工具链可用——实测 exit 0 `writer_pdf_Export` 产出 PDF）；`binary_target_sha256` 不入 Manifest。D3 双钉的安全意图分层承载不变：**Manifest 钉 wrapper hash（sha256 钉死脚本即传递性钉死其全部 exec 行为）+ Capability Catalog 目录钉 resolved target**（seed document.libreoffice `wrapper_target` 双前缀在位 + 本批 sha256_full 回填）；seed `executable_path` 同步更新为 `/usr/bin/soffice`（同一物理文件，hash 不变）。变量形态 exec 目标解析升级属 RT-1 解析器扩展 = adapter 触碰面（本批封顶禁改），登记 RT-4 前处理台账 follow-up。

**否决项**：自建 wrapper 脚本（需 capability_ref 缺席 + PATH 内嵌，偏差面大于 B 且新增未登记能力项）；保留 `binary_target_sha256` 字面遵 D3（office 通道全灭，判据 2 不可能满足）。
- 消费链兑现：RT-1 批次报告 H.2「前置条件三条就绪」（NativeRustAdapter / RT-0.5-F 消费面 / allowlist 登记形状）+「DI-1 自备面四条」（Manifest 编写、钉值补录、resources 取值、完整探测面按需）在本 ADR/任务书逐条承接。
- DI-0 §5 的两处修订随本 ADR 生效并登记：①DI-1 范围由「PDF 单通道」扩为「PDF/Office/图像三通道 + Router 最小实现」（OCR/chi_tra 留 DI-2 不变）；②「不回改 adapter」修订为「允许 result_mode 一处 additive 增量，帧模式零回归」（D4）。
- 与 DI-2/DI-3 的边界：OCR（pdftoppm + tesseract、chi_tra 显式告警）全数留 DI-2；归档两阶段防护、exiftool、Router 外层 facade run 审计锚与 RT-3 判定移交属 DI-3（DI-0 §5 入口指针不变）。

## D7 调和裁决（实现批追加，2026-11-19；office 内存帽 512Mi→16Gi 的实测阶梯与收紧缝）

**触发事实（实现批 /tmp 探针实测）**：D7 表 #5 初定 office 内存帽 512Mi 在 adapter Auto 档 prlimit 兜底（RLIMIT_AS 强制）下不可用——本机 LibreOffice 26.8.2.2 分配器与 RLIMIT_AS 结构性敏感，全阶梯实测：512Mi bad_alloc / 1Gi-4Gi segv(139) / 8Gi abort(134) / **16Gi exit 0**（`prlimit --cpu=120 --as=16Gi -- soffice --headless … --convert-to pdf` 实测产出 sample.pdf）。后端强制机制本身已核实有效（prlimit `--as` 路径，native_rust.rs:1036-1044 实读：Prlimit 臂 `cmd.arg(format!("--as={bytes}:{bytes}"))`），被推翻的是 512Mi 这个帽值而非强制面。D3 调和裁决同形态触发：均为 ADR 写作时点未实测到的实现事实，本条为其 D7 侧对应裁决记录（初批只在批报 C-2 记账，ADR 权威面缺对应条目）。

**裁决**：Manifest `resources.memory` 定值 **16Gi**（实测最小可用档；E §2 #5 内存帽结构保留，adapter Auto 档 prlimit 兜底即按此强制 `--as=16Gi`）。16Gi 的 RLIMIT_AS 实质是近乎无约束的地址空间帽——真实内存收紧缝 = systemd 用户会话可用后 `--property=MemoryMax=` 强制（RT-1 报告 G 遗留 1 沿用），恢复更紧强制面时按届时实测重新定值。阶梯记录在 Manifest known_limits（16Gi 实测口径单源；互斥的「内存帽 512Mi」残留声明已清除，评审修订 2026-11-19），批报记账见 DI-1-BATCH-REPORT.md C-2。

**否决项**：保留 512Mi 帽（office 通道全灭，判据 2 不可满足，D3 调和裁决 A 形态同款否决逻辑）；删 `resources.memory` 字段（E §2 #5 内存帽属十四项统一强制清单条目，结构性不可缺席，否决）。
