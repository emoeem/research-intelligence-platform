# RT-0.5-D：现有工具族 → 宿主能力/Runtime 候选映射

- 日期：2026-11-18
- 状态：已定稿（RT-0.5-D）。**本文件全部"候选路径"是映射设计，不是机器现状裁决，更不是迁移执行**；逐项迁移在 RT-4 按实测分批裁决（策略 §9），本批不改 Manifest
- 输入口径：RT-0 审计（[RT-0-RUNTIME-BASELINE-AUDIT.md](RT-0-RUNTIME-BASELINE-AUDIT.md) §3）：`manifests/tools/*.json` 共 19 个 = 16 python_worker + 2 native_rust + 1 podman_host；另有 YAML-only 的 `manifests/tools/rip.http.tls_inspect.yaml`（不计入 19 JSON 计数）与 `containerfiles/rip-tool-webresearch/tool-manifest.json`（`rip.http.inspect` 示例，非正式 Manifest）
- 证据来源：rt05-probes/ 五份 2026-11-18 只读实测（引用格式：实测见 rt05-probes/probe-*.md #N）
- 总表见 [RT-0.5-A-HOST-SOFTWARE-INVENTORY.md](RT-0.5-A-HOST-SOFTWARE-INVENTORY.md)；能力目录见 [RT-0.5-C-CAPABILITY-CATALOG.md](RT-0.5-C-CAPABILITY-CATALOG.md)

## 1. 映射原则（策略 §3 选择矩阵 + 设计 §2）

1. 宿主能力存在、版本满足、安全边界可实现 → 受控原生（RT-1 Native CLI Adapter）。
2. 可信自研 Python Worker、依赖可锁定 → 项目级 venv（RT-2），维持 Worker Protocol v1，不容器化。
3. 依赖冲突/重型运行时/需要文件网络资源隔离 → rootless Podman（按工具族镜像）。
4. 不受信代码或高风险输入 → VM 内容器（podman_vm 设计保留，按任务风险启用）。
5. 宿主缺失能力 → 报告缺失 + 成熟替代，不静默降级、不擅自安装（设计 §5）。
6. "系统已安装"不等于"可无约束处理任意输入"（策略 §12）——映射到原生候选的每一条仍受 [RT-0.5-E-RISK-AND-ISOLATION.md](RT-0.5-E-RISK-AND-ISOLATION.md) 边界表约束。

## 2. 现有 Manifest 逐行映射表（20 行 + 1 示例）

"候选路径"列语义：现工具的去向裁决候选（保持/迁移方向），非本批动作。

| 工具 (Manifest id) | 现 runtime（worker/command） | 候选路径 | 理由 | 盘点证据 |
|---|---|---|---|---|
| rip.doc.parse | python_worker（parsing-worker；`python3 -I workers/python/parsing_worker.py`） | **保持** python_worker → RT-2 项目级 venv；PDF/Office/媒体解析不并入本工具，由 RT-1 新增 native CLI 工具（见 §3 能力映射）承接 | 自研可信、Worker Protocol v1；input schema 仅 md/txt/html（text ≤10 MiB），引入 PDF/Office 解析属工具面扩展而非 runtime 迁移 | RT-0 §3/§4；实测见 rt05-probes/probe-pdf-image.md #4（poppler 在位，为未来扩展提供能力） |
| rip.knowledge.document.fetch | native_rust（binary `rp-knowledge-fetch`，subcommand `fetch`） | **保持**原生（受控子进程） | 自研可信 Rust CLI；RT-1 通用 Native Rust Adapter 落地后按 allowlist/参数 Schema 执行；network=egress_allowlist 维持统一 Egress | RT-0 §4（native_rust 有具体二进制、无通用 Adapter） |
| rip.knowledge.source.sync | native_rust（binary `rp-plugin`，subcommand `sync`） | **保持**原生（受控子进程） | 同上 | RT-0 §4 |
| rip.text.embed | python_worker（embedding-worker） | **保持** python_worker → RT-2 venv | ONNX/模型目录（bge-small-zh-v1.5）依赖属重型 Python 依赖；若依赖冲突升级再按策略 §3.2 评估专用容器，不因依赖多直接容器化 | RT-0 §3 |
| rip.kb.evaluate | python_worker（evaluation-worker） | **保持** python_worker → RT-2 venv | 自研可信、依赖可锁定 | RT-0 §3 |
| rip.kb.eri | python_worker（rip-cli-worker；eri_adapter） | **保持** python_worker → RT-2 venv | 同上 | RT-0 §3 |
| rip.kb.inventory | python_worker（rip-cli-worker；rip_inventory） | **保持** python_worker → RT-2 venv | 同上 | RT-0 §3 |
| rip.kb.keyword_search | python_worker（rip-cli-worker；rip_kb_search） | **保持** python_worker → RT-2 venv | 同上 | RT-0 §3 |
| rip.kb.rtis | python_worker（rtis-kb-worker） | **保持** python_worker → RT-2 venv | 同上；memory 512Mi 已按需配置 | RT-0 §3 |
| rip.knowledge.catalog.extract | python_worker（catalog-extract-worker） | **保持** python_worker → RT-2 venv | 同上 | RT-0 §3 |
| rip.evidence.rtk.auth | python_worker（rip-cli-worker；rtk 模块） | **保持** python_worker → RT-2 venv | 同上 | RT-0 §3 |
| rip.evidence.rtk.ledger | python_worker（rip-cli-worker；rtk 模块，T1） | **保持** python_worker → RT-2 venv | 同上 | RT-0 §3 |
| rip.evidence.rtk.lint | python_worker（rip-cli-worker；rtk 模块） | **保持** python_worker → RT-2 venv | 同上 | RT-0 §3 |
| rip.evidence.rtk.mcp | python_worker（rip-cli-worker；rtk 模块，T2，network vm_local） | **保持** python_worker；network 语义维持现有登记，RT-3 Policy Resolver 复核 vm_local 边界 | 同上 | RT-0 §3 |
| rip.evidence.rtk.scaffold | python_worker（rip-cli-worker；rtk 模块，T1） | **保持** python_worker → RT-2 venv | 同上 | RT-0 §3 |
| rip.ops.etl_status | python_worker（rip-cli-worker；rip_etl_status） | **保持** python_worker → RT-2 venv | 同上 | RT-0 §3 |
| rip.ops.worker-runtime | python_worker（abstract-worker；**登记即 DISABLED 的协议底座元数据条目**） | **不迁移、不派发**（probe=none 元数据条目） | 非 callable 工具，仅承载 Worker Protocol v1 元数据 | RT-0 §3；manifest rip.ops.worker-runtime.json known_limits |
| rip.security.extension.audit | python_worker（extension-audit-worker） | **保持** python_worker → RT-2 venv | 自研可信 | RT-0 §3 |
| rip.security.nuclei | podman_host（`localhost/rip-nuclei@sha256:9ff41ce9…`，只读挂载 /task/out） | **保持** Podman | 网络扫描工具，隔离必要；不因宿主能力绕过隔离（策略 §3）；镜像 digest 固定维持 | RT-0 §4 |
| rip.http.tls_inspect（YAML-only，不计入 19 JSON） | python_worker（tls_inspect-worker；egress_allowlist 443） | **保持** python_worker → RT-2 venv；egress 白名单语义维持 | TLS 证书检查专用出站，非重型依赖 | manifests/tools/rip.http.tls_inspect.yaml |
| rip.http.inspect（containerfiles 示例，非正式 Manifest） | podman_host（rip-tool-webresearch@sha256:085b4ef3…） | 保持示例形态；正式化属后续登记事项 | 工具族镜像 + /task/in(ro) + /task/out(rw) 挂载形态是 Podman 候选模板 | RT-0 §3 补充 |

结论：现有 19+1 个 Manifest **无一因宿主能力目录而改变 runtime**；宿主能力目录的映射价值在 §3——为文档/媒体处理提供 RT-1 新工具的能力解析输入。

## 3. 宿主能力 → 未来工具面映射（RT-1 候选，不在现有 Manifest 中）

| 处理需求 | 候选宿主能力（capabilityId） | 候选路径 | 理由与边界 | 盘点证据 |
|---|---|---|---|---|
| PDF 文本抽取 | pdf.pdftotext | /usr/bin/pdftotext | poppler 26.11.0；页数/输出上限约束 | 实测见 rt05-probes/probe-pdf-image.md #4 |
| PDF 元数据/页数 | pdf.pdfinfo | /usr/bin/pdfinfo | poppler 26.11.0；路由前置探测 | 实测见 rt05-probes/probe-pdf-image.md #4 |
| PDF 页面渲染（OCR 前置） | pdf.pdftoppm | /usr/bin/pdftoppm | poppler 26.11.0；像素/页数硬限额 | 实测见 rt05-probes/probe-pdf-image.md #4 |
| PDF 渲染（gs 链路） | image.magick（delegate gs） | /usr/bin/gs（10.08.0，经 magick delegates.xml 调用，-dSAFER） | policy 空壳 → 不可信 PDF 禁宿主直连，见 E 文档 §4.1 | 实测见 rt05-probes/probe-pdf-image.md #8 #12 |
| Office → PDF / 文本 | document.libreoffice | /usr/lib/libreoffice/program/soffice（headless，隔离临时 profile） | headless 面经 --help 实测；L3 smoke 待 RT-1 | 实测见 rt05-probes/probe-office-convert.md #5 #6 |
| Markdown/HTML/Office 中间格式转换 | convert.pandoc | /usr/bin/pandoc | 51 in / 76 out；文本类为主 | 实测见 rt05-probes/probe-office-convert.md #9 #10 |
| OCR（图像/PDF 渲染页） | ocr.tesseract | /usr/bin/tesseract | chi_sim/eng/osd 在位；**chi_tra 缺失**（见 §4） | 实测见 rt05-probes/probe-av-ocr.md #8 #9 |
| 图像格式转换/预处理 | image.magick | /usr/bin/magick（禁用 convert 别名入口） | IM7 7.1.2-35；policy 空壳 → 不可信图像走隔离 | 实测见 rt05-probes/probe-pdf-image.md #5-#7 #9 #10 |
| 归档识别/受控解包 | archive.7z（Rar/全格式）、archive.7za（无 Rar）、archive.7zr（仅 7z 系）、archive.bsdtar、archive.unzip | /usr/bin/7z、/usr/bin/7za、/usr/bin/7zr、/usr/bin/bsdtar、/usr/bin/unzip | Rar 仅 7z 具备；解包防护见 E 文档 §4.3 | 实测见 rt05-probes/probe-archive-meta.md #6 #7 #8 #10 #11 |
| 文件元数据读取 | meta.exiftool | /usr/bin/vendor_perl/exiftool（固定绝对路径，禁裸 PATH） | 非受信前缀 + Perl 脚本形态 → 调用面见 E 文档 §4.4 | 实测见 rt05-probes/probe-archive-meta.md #12 #14、rt05-probes/probe-path-audit.md #6 |
| 媒体探测 | media.ffprobe | /usr/bin/ffprobe | 只读探测面 | 实测见 rt05-probes/probe-av-ocr.md #7 |
| 媒体转码/抽帧/音轨处理 | media.ffmpeg | /usr/bin/ffmpeg | 248 encoders；生产编码用 libopus/libvorbis（原生 opus/vorbis experimental） | 实测见 rt05-probes/probe-av-ocr.md #3 #4 #5 #6 |

## 4. 缺失能力的替代裁决

### 4.1 pdf.mutool（missing）

- 实况：mutool 不在 PATH，mupdf/mupdf-tools 均未安装（实测见 rt05-probes/probe-pdf-image.md #1 #11）。
- **裁决：不补装、不重复打包。** Poppler 已覆盖本目录的 MuPDF 核心用途：
  - 文本抽取 → pdf.pdftotext（同为 poppler 26.11.0，实测见 rt05-probes/probe-pdf-image.md #4）；
  - 元数据/页数 → pdf.pdfinfo（同上）；
  - 页面渲染 → pdf.pdftoppm（同上）与 gs 10.08.0（magick PDF/PS delegate 依赖在位，实测见 rt05-probes/probe-pdf-image.md #8 #12）。
- 功能差异说明：MuPDF 特有能力（如 mutool clean 结构化重写、JS 嵌入面）不在本目录需求面内；若未来出现该需求，按设计 §5 走用户授权安装流程，不做静默替代。

### 4.2 ocr.tesseract chi_tra 语言包（子能力 missing）

- 实况：语言包仅 chi_sim/eng/osd 3 个，tesseract-data-chi_tra 未安装（实测见 rt05-probes/probe-av-ocr.md #9 #11）。
- **裁决：补装属用户决策，Discovery 只报告不安装**（设计 §5：用户明确授权后才可由用户或受控部署流程安装）。在补装前，工具面必须把 chi_tra 请求显式报错（语言不可用），禁止静默回退 chi_sim（繁简字形差异大，回退即静默降级，违反设计 §5"不允许静默换成质量更差的实现"）。
- 可粘贴命令（由用户决定执行与否）：`sudo pacman -S tesseract-data-chi_tra`。

## 5. 候选路径四象限汇总

| 候选 Runtime | 现有工具（保持） | 未来能力工具（RT-1 新增候选） |
|---|---|---|
| 受控原生子进程（native CLI） | rip.knowledge.document.fetch、rip.knowledge.source.sync | pdf.pdftotext/pdfinfo/pdftoppm、ocr.tesseract、convert.pandoc、media.ffmpeg/ffprobe、archive.*、meta.exiftool、image.magick（仅可信低风险输入） |
| 项目级 venv（python_worker，RT-2） | 其余 14 个 python_worker + rip.http.tls_inspect | — |
| Podman（保持/按需） | rip.security.nuclei；rip.http.inspect（示例） | 不可信来源的文档/图像/归档解析（按 E 文档边界表升级） |
| VM（podman_vm 设计保留） | — | 高风险/对抗性输入（宏文档、PS/EPS、复杂归档、未知 0-day 面），按任务风险启用 |
