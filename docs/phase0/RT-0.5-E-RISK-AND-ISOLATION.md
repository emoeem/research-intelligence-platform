# RT-0.5-E：恶意输入边界、资源限制与隔离策略

- 日期：2026-11-18
- 状态：已定稿（RT-0.5-E）。作为 RT-3 Runtime Policy Resolver 的设计输入；本批不实现 Policy Resolver、不改 Adapter（设计 §8）
- 依据：[RT-0.5-HOST-CAPABILITY-DISCOVERY.md](RT-0.5-HOST-CAPABILITY-DISCOVERY.md) §4/§4.1；[NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md](NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md) §3/§12
- 证据来源：rt05-probes/ 五份 2026-11-18 只读实测（引用格式：实测见 rt05-probes/probe-*.md #N）
- 能力底册：[RT-0.5-C-CAPABILITY-CATALOG.md](RT-0.5-C-CAPABILITY-CATALOG.md)；工具映射：[RT-0.5-D-TOOL-MAPPING.md](RT-0.5-D-TOOL-MAPPING.md)

## 1. 恶意输入边界表（设计 §4.1 三档路由）

判定输入：来源信任（Trust Level）× 格式解析器风险。**"宿主软件已安装"不等于"所有输入都可以在宿主直接解析"**（设计 §4.1）。

| 输入档位 | 判定条件 | 执行方式 | 附加约束 |
|---|---|---|---|
| 可信、低风险 | 来源可信（平台自有 Workspace/已审核上传）且格式为文本类（md/txt/html）、受控媒体 | 受限宿主原生子进程（RT-1 Native CLI Adapter：固定绝对路径、参数 Schema、环境白名单） | 仍强制 §2 全部资源限制；无网络 |
| 来源不明 | 外部抓取/用户上传未审核/沙箱外来源 | 隔离执行：rootless Podman 工具族容器（podman_host）优先；无对应镜像时拒绝或降级到 VM，不静默原生化 | 输入只读挂载、独立输出目录、非特权身份、网络默认关闭 |
| 高风险/对抗性 | 已知攻击面格式（PS/EPS、宏 Office、复杂归档、EXPLOIT 历史密集解析器）、疑似对抗样本、明确威胁模型要求内核级边界 | VM 内 rootless 容器（podman_vm 设计保留，按任务风险启用；NATIVE-FIRST §2.2/§8.1：VM Runtime 属设计保留、未落地）。**podman_vm 未启用/未实现时一律拒绝派发并报告（错误信封注明 VM 未就绪，五态语义见 [RT-0.5-F](RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md) §7），禁止降级 podman_host 或受限宿主原生**，与"不明来源"行拒绝语义对齐 | 与 podman_host 相同的挂载/限额纪律；禁止因宿主已有同名软件绕过隔离（策略 §3） |

按格式域的默认档位（可被 Policy Resolver 按来源证据升级，不得降级）：

| 格式域 | 默认档位 | 域特有理由 | 证据 |
|---|---|---|---|
| 纯文本（md/txt/plain） | 可信低风险 → 受限原生 | 无复杂解析器状态机 | — |
| PDF | 不明来源 → 隔离（Podman）；可信低风险 → 受限原生（poppler） | PDF 解析历史漏洞密集；本机 magick PDF 链路另有 policy 空壳问题（§4.1） | 设计 §4.1 |
| Office（doc/docx/xls/xlsx/ppt/pptx/odt…） | 不明来源 → 隔离；可信低风险 → 受限原生（libreoffice headless + 隔离 profile） | 宏/嵌入对象/解析器漏洞面 | 设计 §4.1 |
| 图像（含 heic/jp2/jxl/webp 等 Magick delegate 覆盖面） | 不明来源 → 隔离；可信低风险 → 受限原生（magick，禁 convert 别名） | 解码器漏洞面 + policy 空壳（§4.1） | 实测见 rt05-probes/probe-pdf-image.md #5 |
| 归档（zip/7z/tar/rar…） | 不明来源 → 隔离；可信低风险 → 受限原生（7z 系/bsdtar/unzip） | 路径穿越/符号链接/压缩炸弹（§4.3） | 设计 §3.4 |
| 媒体（音视频容器/编解码） | 不明来源 → 隔离；可信低风险 → 受限原生（ffmpeg/ffprobe） | demux/decode 漏洞面；ffprobe 只读面风险较低 | 设计 §4.1 |
| PS/EPS | 高风险 → VM（且经 gs -dSAFER 也仅是缓解） | PostScript 是图灵完备文档语言 | 实测见 rt05-probes/probe-pdf-image.md #8 |

## 2. 资源限制清单（所有档位统一强制；原生执行不可省略）

| # | 限制项 | 内容 | 备注（现有 Manifest 实践佐证） |
|---|---|---|---|
| 1 | 只读输入 | 解析器对输入文件仅读权限；输入经 Artifact 暂存路径传入，不直接读用户原始位置 | 现有 Podman 挂载形态 `/task/in (ro)`（rip.http.inspect 示例）；RT-0 §4 |
| 2 | 独立输出目录 | 输出写入任务独立工作区（/task/out 或任务目录），不落宿主家目录/共享路径 | 同上形态 `/task/out (rw)` |
| 3 | 非特权身份 | 解析进程以非特权用户运行；不继承 root；不暴露 SSH Agent、浏览器 Profile、数据库路径、Podman Socket、宿主凭据（策略 §3.1） | 设计 §4.1 |
| 4 | CPU | 按工具登记 cpu 预算（现有实践：cpu=1） | manifests/tools/*.json resources.cpu（该口径属 Podman 容器与 python_worker 派发语境）；宿主原生子进程强制锚：systemd-run --user --property=CPUQuota=（cgroup v2；等价物 prlimit RLIMIT 粗粒度兜底）——机制不可用即拒绝派发，不裸跑（详见注 A） |
| 5 | 内存 | 按工具登记 memory 预算（现有实践 128Mi–512Mi） | manifests/tools/*.json resources.memory（该口径属 Podman 容器与 python_worker 派发语境）；宿主原生子进程强制锚：systemd-run --user --property=MemoryMax=（cgroup v2；等价物 prlimit RLIMIT_AS 粗粒度兜底）——机制不可用即拒绝派发，不裸跑（详见注 A） |
| 6 | 时长 | timeout 硬上限 + 协作式取消 + 进程组回收（现有 budget 硬上限 timeout_ms [1, 600000]） | rip.ops.worker-runtime known_limits |
| 7 | 页数 | PDF/Office 页数上限由 Router 预算（pdfinfo 先探页数再放行） | 能力：pdf.pdfinfo（实测见 rt05-probes/probe-pdf-image.md #4） |
| 8 | 像素数 | 渲染/OCR 像素上限（分辨率×页数即内存放大器：pdftoppm/tesseract/magick 全部适用） | 实测见 rt05-probes/probe-pdf-image.md #4；rt05-probes/probe-av-ocr.md #8 |
| 9 | 解压大小 | 归档解包字节预算 + 条目数预算；超限即中止（压缩炸弹防护） | 设计 §3.4 |
| 10 | 输出上限 | stdout/stderr/Artifact 输出上限 + 截断信封（现有 max_output_bytes [256, 67108864] 实践） | rip.ops.worker-runtime known_limits；manifests tools resources.max_output_bytes |
| 11 | 网络 | 解析器默认禁网；一切出网走统一 Egress Policy/代理（egress_proxy 白名单，P3-2），禁止解析工具自行联网取外部资源（policy 空壳下 URL coder 开放，见 §4.1） | 设计 §4.1；策略 §3.1 |
| 12 | 环境白名单 | 受控环境变量白名单，不继承完整宿主环境；工作目录固定（策略 §3.1 原生准入条件） | 策略 §3.1 |
| 13 | 文件系统边界 | 禁止暴露宿主家目录、凭据、浏览器 Profile、生产 `platform*.db`、RTIS 私有数据、Docker/Podman Socket（策略 §4.2） | 策略 §4.2 |
| 14 | 状态统一 | 失败、超时、崩溃、部分输出均走统一 ToolRun 状态与审计信息，不分叉 | 设计 §4.1；策略 §4.1 |

注 A（原生档 CPU/内存强制机制，条目 4/5 的 RT-1 Native CLI Adapter 实现锚）：受限宿主原生子进程的 cpu/memory 预算不由 manifests/tools/*.json 承载（该口径属 Podman 容器与 python_worker 派发语境），必须由宿主机制强制——优先 `systemd-run --user --scope --property=CPUQuota=… --property=MemoryMax=…` 包裹派发（cgroup v2；2026-11-18 本机核验：systemd-run/prlimit 在位 /usr/sbin/，`systemd-run --user --scope -p CPUQuota=50% -p MemoryMax=64M true` 用户会话内启动成功）；systemd 用户会话不可用时以 `prlimit` 粗粒度等价物兜底（RLIMIT_CPU = 累计 CPU 时间上限、RLIMIT_AS = 地址空间上限，语义粗于 CPUQuota/MemoryMax，预算登记时按此折算）；**两种机制均不可用时的行为 = 拒绝派发并输出错误信封，不裸跑**。

## 3. 文档处理路由图（设计 §4 架构 + 本机能力查找注记）

```text
Input / Artifact
  -> Rust Document Router
       -> file signature + MIME + size/page limits
            （页面数前置探测：pdf.pdfinfo；媒体探测：media.ffprobe）
       -> trust / input-risk classification        # §1 三档判定
       -> capability lookup                         # 查 Host Capability Catalog（RT-0.5-F Provider）
            （条目 status != available 或 stale 时：拒绝并报告，不静默换实现）
       -> Runtime Policy decision                   # RT-3 Resolver：§1 档位 + §2 预算
            -> Host restricted subprocess           # 可信低风险：RT-1 Native CLI Adapter
            -> project venv worker                  # 可信自研 Python 工具（RT-2）
            -> Podman tool-family runtime           # 不明来源：podman_host
            -> VM-backed isolation                  # 高风险：podman_vm（按风险启用）
       -> parser / OCR / converter                  # poppler / libreoffice / tesseract / ffmpeg / magick / 7z …
  -> normalized extraction result
       -> text + structure + metadata + page anchors
       -> OCR confidence + language + warnings      # 语言缺失显式告警（如 chi_tra）
  -> Artifact / Evidence / Trace (existing unified path)
```

- Rust 负责路由、策略、预算、任务生命周期和结果规范化；成熟工具负责专业处理；不为"Rust 原生"重写解析器（设计 §4）。
- 能力查找失败（missing/stale）时路由终止并输出明确错误信封（五态见 [RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md](RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md) §7），不降级到低质量实现（设计 §5）。

## 4. 本机特化风险条目（由 RT-0.5 实测直接导出）

### 4.1 ImageMagick policy.xml 空壳 → 不可信图像/PDF 禁走宿主 magick 直连

- 实况：/etc/ImageMagick-7/policy.xml 唯一生效行为 `Undefined: none`；PDF/PS/SVG/MVG/URL/XPS 等高危 coder 限制、resource 限额、path 限制全部处于注释态；PDF/PS coder 与 gs delegate 均未禁用；URL/HTTP/MSL/MVG 历史高危面默认开放（实测见 rt05-probes/probe-pdf-image.md #9 #10、异常 #2）。
- **裁决：不可信图像/PDF 一律不走宿主 magick 直连，走 Podman/VM 隔离**；可信低风险图像走受限原生时由调用方硬限额（policy 无兜底）；URL/MSL/MVG 类 coder 输入在宿主原生路径一律拒绝；PDF/PS 若必须走 magick 链路，其隔离档位按 §1 PDF/PS 行执行（gs -dSAFER 仅为缓解，非边界，实测见 rt05-probes/probe-pdf-image.md #8）。
- delegate 注入面：delegates.xml 含 libreoffice --convert-to pdf、curl https:decode、c2patool、darktable-cli 等命令面；Adapter 层固定参数模板，禁止把用户输入拼进 delegate 命令（实测见 rt05-probes/probe-pdf-image.md #5 #8、异常 #3）。

### 4.2 LibreOffice → 禁共享 profile，用隔离临时 profile

- 实况：soffice 为 sh wrapper → soffice.bin；headless 面经 --help 实测可用（实测见 rt05-probes/probe-office-convert.md #2 #4 #6）。
- **裁决（设计 §3.4 原文要求）：禁止并发共享同一用户 profile；每次任务使用隔离临时 profile（如 `-env:UserInstallation=file:///<任务临时目录>/lo-profile`），任务结束回收**；并发任务不得指向同一 profile 目录。
- 配套：headless 运行、非特权身份、输入只读、输出独立目录、禁网；不明来源 Office 文档按 §1 隔离档位走 Podman。

### 4.3 7z/归档解包 → 路径穿越 / 符号链接 / 压缩炸弹独立防护

- 实况：archive.7z/7za/7zr 均为 sh wrapper（真实 ELF 在 /usr/lib/7zip/）；Rar/Rar5 仅 archive.7z 支持；bsdtar 无 --formats 清单（实测见 rt05-probes/probe-archive-meta.md #4 #5 #7 #8 #10、异常 #1 #3 #4）。
- **裁决（设计 §3.4：列目录与解包分开探测/授权，路径穿越、符号链接和压缩炸弹设独立防护）**：
  1. 两阶段执行：先列目录（`7z l` / 列表模式）解析条目清单，经 Router 校验（路径穿越 `../`、绝对路径条目、符号链接条目、条目数、累计解压大小）后再放行解包；两阶段分别授权、分别预算。
  2. 解包目标为独立空目录（非任务共享区），解包后拒绝保留符号链接形态进入下游处理。
  3. 解压大小/条目数超限即中止并保留 ToolRun 审计（§2 第 9/10 项）。
  4. 格式路由按能力面：Rar/Rar5 → 仅 archive.7z；常规 7z/zip/tar → archive.7za 或 7z；7z 原生精简 → 7zr；选 7za/7zr 遇 Rar 输入必须显式报错。
  5. wrapper 形态：Adapter 以绝对路径调用 /usr/bin/7z 等入口，指纹校验覆盖 wrapper 与真实 ELF 双方（实测见 rt05-probes/probe-archive-meta.md 异常 #1）。

### 4.4 meta.exiftool → 固定绝对路径 + 包归属校验

- 实况：/usr/bin/vendor_perl/exiftool 不在受信前缀集合（candidateTrust=NO），Perl 脚本形态，权限 555（实测见 rt05-probes/probe-path-audit.md #6、rt05-probes/probe-archive-meta.md #12 #14）。
- **裁决：调用面固定绝对路径 /usr/bin/vendor_perl/exiftool，禁止裸 PATH 解析**；Provider 指纹覆盖脚本本体 + perl 解释器包归属变化；不明来源文件按 §1 隔离档位（元数据解析器同样有漏洞暴露面）。

### 4.5 ffmpeg-full 第三方仓库 → 指纹监控 + stale 重探

- 实况：包名 ffmpeg-full 9.0.2-3.46，非官方 core/extra 构建，nonfree + NVENC/CUDA/VAAPI/Vulkan 全开（实测见 rt05-probes/probe-av-ocr.md #10、异常 #2 #4）。
- **裁决：纳入缓存失效监控（包版本/hash 变化即 stale，见 [RT-0.5-C-CAPABILITY-CATALOG.md](RT-0.5-C-CAPABILITY-CATALOG.md) §5.3）；生产编码用 libopus/libvorbis，原生 opus/vorbis encoder（experimental）禁用于生产面（实测见 rt05-probes/probe-av-ocr.md 异常 #3）。**

### 4.6 convert 别名 → Adapter 层统一 magick 入口

- 实况：/usr/bin/convert 为 IM7 弃用 symlink，调用带 deprecation WARNING，本机无 IM6（实测见 rt05-probes/probe-pdf-image.md #6 #7、异常 #1）。
- **裁决：RT-1 Adapter 的命令 allowlist 只登记 magick，不登记 convert；杜绝 IM6 语义混用（-density/-units 差异）。**

### 4.7 PATH 残留面 → 受控 search roots

- 实况：4 个 emo 属主可写目录（~/.local/bin、~/.cargo/bin、pnpm/bin、flatpak exports）排在受信目录之前；5 个 PATH 元素位于 /tmp/.mount_zcode.0pveme 前缀；真实跨物理目录 shadow 零命中；exiftool 解析在受信集合外（实测见 rt05-probes/probe-path-audit.md verdict、#1 #2 #6）。
- **裁决（设计 §3.2）：Provider 探测与 Adapter 执行均用受控 search roots（受信前缀 + 固定候选目录），解析为绝对路径后验证身份；不裸信运行时 PATH、不接受请求参数提供的可执行路径；用户可写目录与 /tmp 挂载元素中的同名程序不得当作系统可信能力。**

## 5. 与既有语义的衔接

- 全部执行后端（原生/venv/Podman/VM）复用同一 Tool Registry、ToolRun、Artifact、Evidence、Trace 与预算/取消语义，不建立第二套事实源（策略 §4.1）。
- Runtime 健康检查、能力声明、版本信息统一暴露（策略 §4.1）——由 RT-0.5-F Provider 契约承载。
- 本文档为策略输入，不改变 WSW-1 现有批次与 WSW-1 → WSW-2 顺序（设计 §9；策略 §9）。
