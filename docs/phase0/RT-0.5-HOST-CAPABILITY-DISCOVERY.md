# RT-0.5：Host Capability Discovery 与宿主软件能力目录

- 日期：2026-11-18
- 状态：设计冻结候选；待 WSW-1 稳定收口后执行只读盘点
- 前置：RT-0 基线审计；遵守 NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md
- 目的：优先复用宿主机已经安装、成熟、受维护的软件能力；仅在依赖冲突、运行环境复杂或隔离需求成立时使用 venv / Podman / VM
- 本批边界：只读发现、能力目录与策略设计；不安装/升级/卸载系统软件，不迁移 Manifest，不改 Adapter 业务代码

## 1. 核心裁决：Host Software First

RIP 不应为了“统一运行环境”重复打包宿主机已经具备的成熟软件。宿主机已安装且身份可信、版本满足要求的软件，应优先作为受控外部能力调用。

典型候选（仅作为探测清单，不代表当前机器已安装）：

| 能力域 | 优先探测的宿主软件 | 常见用途 |
|---|---|---|
| 音视频 | FFmpeg / FFprobe | 媒体探测、转码、抽帧、音轨/字幕处理 |
| Office 文档 | LibreOffice / soffice | DOC/DOCX/XLS/XLSX/PPT/PPTX 转换与无头处理 |
| OCR | Tesseract | 图片/PDF 页面 OCR、多语言识别 |
| PDF | Poppler（pdftotext/pdfinfo/pdftoppm）或 MuPDF | 文本抽取、元数据、页面渲染 |
| 文档转换 | Pandoc | Markdown、HTML、Office 中间格式转换 |
| 图像 | ImageMagick | 格式转换、尺寸/页面预处理 |
| 归档 | 7-Zip / bsdtar / unzip | 归档识别与受控解包 |
| 元数据 | ExifTool | 图片、音视频和文档元数据读取 |

清单可以扩展，但每项能力必须基于真实探测结果登记，不得将候选列表误写成机器现状。

## 2. Runtime 选择输入

策略至少综合以下因素，不再只看 Manifest 中的 Runtime 类型：

1. Host Capability：宿主机是否存在满足要求的可执行文件、版本、子能力和语言/插件数据。
2. Trust Level：工具本身及其来源是否可信、是否经过审核。
3. Input Risk：输入文件是否可能恶意构造；解析器漏洞暴露面和输入来源。
4. Dependency Complexity：是否有冲突的系统库、运行时、GPU/CUDA、浏览器或大型模型依赖。
5. Isolation Requirement：文件系统、网络、资源、内核和设备边界要求。
6. Operational Cost：启动时延、磁盘占用、内存、维护与更新成本。

选择原则：

- 能力已存在、版本满足、调用安全边界可实现：优先宿主受控原生子进程。
- 可信自研 Python Worker 且依赖可锁定：项目级 venv，不使用全局 pip。
- 软件依赖冲突或大型独立运行时：专用容器/环境。
- 不可信代码或需要更强边界：Podman / VM，不能因宿主已有同名软件而绕过隔离。
- 宿主缺失能力：报告缺失与可选替代，不静默降级、不擅自安装。
- 宿主软件存在但版本/能力不满足：记录原因，再评估项目级环境或容器。

## 3. Host Capability Discovery 设计

### 3.1 能力目录数据模型

每个能力项至少记录：

- capability_id：稳定能力标识，如 media.ffmpeg、document.libreoffice、ocr.tesseract。
- provider：软件名称及发行方/项目。
- executable_path：经验证的绝对路径。
- version：规范化版本与原始版本输出。
- binary_identity：可执行文件 SHA-256、文件类型、所有者/权限等身份信息。
- package_provenance：包管理器、包名、包版本；无法确认时标记 unknown。
- capabilities：经过探测的功能集合，例如 FFmpeg 编码器/解码器、Tesseract 语言包。
- status：available / missing / incompatible / probe_failed / disabled。
- probe_timestamp 与 fingerprint：探测时间和能力指纹。
- constraints：最低版本、必需组件、已知兼容性限制。
- security_profile：允许的调用方式、输入风险等级和隔离要求。

能力目录是环境事实与健康状态，不是第二套 Tool Registry。具体工具仍由现有 Manifest、Tool Registry 和 Tool Policy 管理。

### 3.2 安全探测规则

- 不盲目信任 Agent 或请求参数提供的可执行文件路径。
- 使用受控 PATH/search roots；解析为绝对路径后验证文件类型、权限、所有者及包来源。
- 不允许当前工作目录、临时目录或可被低权限用户写入的目录优先于受信系统路径。
- 探测命令固定、参数固定、无 Shell 拼接；设置短超时、输出上限和取消/回收。
- 版本探测与能力探测必须只读，不处理用户文件，不访问网络，不触发插件下载或自动更新。
- 对能力探测结果做缓存；启动时可做轻量指纹校验，定期或文件变化时重探。
- 二进制路径、hash、包版本或能力集合发生变化时，将旧结果标记 stale 并重新验证。
- 探测失败不等于软件不存在；区分 missing、permission_denied、timeout、unsupported_output 和 probe_failed。
- 不把用户可写目录中的同名程序当作系统可信能力；防止 PATH 劫持。

### 3.3 分层探测，避免启动开销

- L0 静态发现：受控路径查找、文件元信息、包数据库归属。
- L1 轻量探测：固定版本参数，如 --version。
- L2 能力探测：只读列出编码器、解码器、语言包、格式支持等；可按需或后台执行。
- L3 功能验证：仅在安装/升级验收或工具首次启用时，使用内置小型 fixture 做无副作用 smoke test；不得在每次服务启动时运行。

### 3.4 关键软件能力断言

- FFmpeg：版本、build configuration、可用 demuxer/muxer、encoder/decoder；不能只检测命令存在。
- Tesseract：版本、已安装语言包、所需语言是否可用；语言数据缺失要明确报告。
- LibreOffice：版本、headless 启动能力、可用模块；禁止并发共享同一用户 profile，运行时使用隔离的临时 profile。
- Poppler/MuPDF：具体命令、版本和 PDF 页面渲染/文本抽取能力。
- Pandoc：版本及目标 writer/reader 支持。
- ImageMagick：版本、delegate/codec 能力与安全策略限制。
- 归档工具：列目录与解包能力分开探测；对路径穿越、符号链接和压缩炸弹设独立防护。

## 4. 文档智能处理的推荐架构

```text
Input / Artifact
  -> Rust Document Router
       -> file signature + MIME + size/page limits
       -> trust / input-risk classification
       -> capability lookup
       -> Runtime Policy decision
            -> Host restricted subprocess
            -> project venv worker
            -> Podman tool-family runtime
            -> VM-backed isolation
       -> parser / OCR / converter
  -> normalized extraction result
       -> text + structure + metadata + page anchors
       -> OCR confidence + language + warnings
  -> Artifact / Evidence / Trace (existing unified path)
```

Rust 负责路由、策略、预算、任务生命周期和结果规范化；FFmpeg、LibreOffice、Tesseract、Poppler 等成熟工具负责专业处理。不要为了“Rust 原生”重写成熟解析器，也不要把所有解析工具打进同一个大镜像。

### 4.1 恶意输入边界

宿主软件已安装不代表所有输入都可以在宿主直接解析。PDF、Office、图片、压缩包和媒体文件都可能利用解析器漏洞或触发资源耗尽。

- 可信来源、低风险文件：可使用受限宿主子进程，但仍需限制文件范围、网络、CPU/内存、时长、页数、像素数、解压大小和输出量。
- 来源未知或高风险文件：优先隔离执行；按风险使用 Podman 或 VM。
- 不得把宿主家目录、凭据、浏览器 Profile、数据库文件或 Podman Socket 暴露给解析器。
- 解析器必须以非特权身份运行；输入只读、输出写入独立临时工作区。
- 禁止解析工具自行联网获取外部资源；网络访问必须走统一 Egress Policy。
- 失败、超时、崩溃和部分输出均须保留统一 ToolRun 状态与审计信息。

## 5. 缺失能力与安装策略

- Discovery 只报告，不自动安装、升级、卸载或修改系统配置。
- 缺失能力时，先列出成熟替代软件和功能差异；不允许静默换成质量更差的实现。
- 用户明确授权后，才可由用户或受控部署流程安装系统包。
- 若系统包不可用或版本冲突，再评估项目级 venv、专用容器或 VM。
- 禁止通过全局 pip、随意下载二进制或不受信任的安装脚本补能力。
- 对系统 FFmpeg 等现有软件，必须先检查实际版本/编译能力，不能重复打包一份只为追求环境统一。

## 6. 本批执行步骤

| 步骤 | 内容 | 输出 |
|---|---|---|
| 0.5-A | 只读盘点宿主软件与包来源 | 可执行文件路径、版本、包归属、权限、hash |
| 0.5-B | 盘点关键可选能力 | FFmpeg codec、Tesseract languages、LibreOffice headless、PDF 工具能力等 |
| 0.5-C | 形成 Host Capability Catalog | 机器实况清单、缺失/不兼容/未知项 |
| 0.5-D | 建立候选工具映射 | 现有 Manifest/工具族 → 宿主能力/venv/Podman/VM 候选路径 |
| 0.5-E | 安全与性能风险评估 | 哪些输入可受限原生处理，哪些必须隔离 |
| 0.5-F | 设计 Capability Provider 契约 | 只定义接口、数据结构、缓存/失效策略；本批不实现生产 Adapter |
| 0.5-G | 独立复核与文档验收 | 报告、决策表、后续实现任务书 |

## 7. 本批验收标准

- [ ] 所有宿主软件结论来自本机只读实测；候选软件不得冒充已安装。
- [ ] 每项能力记录绝对路径、版本、包来源/未知状态及能力探测结果。
- [ ] 发现路径不信任、PATH 劫持、权限异常和探测失败时有明确状态。
- [ ] FFmpeg、LibreOffice、Tesseract、Poppler/MuPDF、Pandoc、ImageMagick、归档工具均被检查；不存在时如实标记。
- [ ] 能力目录与 Tool Registry 的职责边界清楚，不建立第二套工具事实源。
- [ ] Runtime Policy 同时考虑宿主能力、信任等级、输入风险、依赖复杂度和隔离要求。
- [ ] 对恶意文档/媒体输入有明确隔离和资源限制策略。
- [ ] 未安装/升级/卸载任何软件，未改系统配置，未改 Manifest/Adapter/Cargo/数据库。
- [ ] WSW-1 工作树及 WSW-1 → WSW-2 顺序完全不受影响。

## 8. 明确不做

- 不把 OCR、Office、PDF、媒体解析一律容器化。
- 不把所有系统命令一律视为可信并直接执行。
- 不重写 FFmpeg、LibreOffice、Tesseract、Poppler 等成熟项目。
- 不创建第二套工具注册、运行记录、Artifact 或 Evidence 数据库。
- 不在本批实现 Native Rust Adapter、Python venv 管理器或 Runtime Policy Resolver。
- 不进行系统级安装、Podman 清理、Cargo 清理或依赖升级。
- 不提交或推送 Git。

## 9. 后续批次关系

RT-0.5 是 RT-1 的前置设计输入。它先确定宿主机已经具备什么能力、这些能力是否可信、哪些场景需要隔离，避免 Native Rust Adapter 只围绕自带二进制设计，之后再返工支持系统软件。

完成 RT-0.5 后，RT-1 实现通用受控 Native Rust/CLI Adapter，并把 Host Capability Catalog 作为能力解析输入；RT-2 建立 Python Worker 项目级环境与锁定机制；RT-3 将 Host Capability、Trust Level、Input Risk、Dependency Complexity、Isolation Requirement 纳入统一 Runtime Policy Resolver；RT-4 再逐项评估工具迁移，不得批量把现有 Manifest 改成 native。

RT-0.5 本身不改变 WSW-1 当前批次与 WSW-2 顺序；实际执行仍须等 WSW-1 稳定收口。
