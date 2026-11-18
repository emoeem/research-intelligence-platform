# RT-0.5 批次报告：Host Capability Discovery 与宿主能力目录

- 日期：2026-11-18
- 批次范围：只读盘点（0.5-A/B）→ 能力目录（0.5-C）→ 工具映射（0.5-D）→ 风险与隔离（0.5-E）→ Provider 契约（0.5-F）→ 独立复核（0.5-G），对应 [RT-0.5-HOST-CAPABILITY-DISCOVERY.md](RT-0.5-HOST-CAPABILITY-DISCOVERY.md) §6 步骤表全七步
- 产物：6 份定稿文档（RT-0.5-A~F）+ 5 份原始取证（rt05-probes/）+ 本报告；全部位于 `docs/phase0/`，未 commit/push（设计 §8"不提交或推送 Git"）
- 复核结论：九条验收 **9/9 通过**；评审 4 条 issues（2 medium + 2 low）修订全部 applied，rejected=[]

## 执行路径说明（单列）

1. **盘点与六文档由主会话 Agent 子代理完成**：rt05-probes/ 五份只读取证与 RT-0.5-A~F 六份文档的撰写均为主会话 Agent 子代理产出；驱动方式为 dynamic-workflow。驱动过程中出现**三次 Model creation failed，对应账户并发上限 3**（并发受限账户下子代理串行驱动为可行路径）。
2. **复核、评审修订、本报告与收口由本 workflow run 完成**：九条验收判定、三项抽读复现（ffmpeg/magick/exiftool 路径·包归属·sha256 前 16 位·版本·权限逐字段）、四类典型错误专项检查、4 条评审修订落稿（含 NATIVE-FIRST §8.2 时点标注连带修订）、本批次报告撰写与本节所列【本会话复核】命令，均为本 workflow run 实测执行。
3. 证据链：原始取证（rt05-probes/ `evidence 清单` 逐条编号）→ 六文档逐行引用（格式"实测见 rt05-probes/probe-*.md #N"）→ 本报告收口复核（命令原文 + 回显原文）。

## A. 完成情况

### A.1 设计 §6 步骤逐项完成

| 步骤 | 内容 | 产出 | 状态 |
|---|---|---|---|
| 0.5-A | 只读盘点宿主软件与包来源 | [RT-0.5-A-HOST-SOFTWARE-INVENTORY.md](RT-0.5-A-HOST-SOFTWARE-INVENTORY.md) 16 项 8 字段总表 + 5 项指定必录异常 + 9 项补充异常 | 完成 |
| 0.5-B | 盘点关键可选能力 | [RT-0.5-B-CAPABILITY-PROBES.md](RT-0.5-B-CAPABILITY-PROBES.md) 11 节能力面展开（encoders/语言包/headless/格式/delegates/policy/PATH 审计） | 完成 |
| 0.5-C | 形成 Host Capability Catalog | [RT-0.5-C-CAPABILITY-CATALOG.md](RT-0.5-C-CAPABILITY-CATALOG.md) 16 项全字段目录 + 五态统计 + 缓存/失效策略 + Registry 边界声明 | 完成 |
| 0.5-D | 建立候选工具映射 | [RT-0.5-D-TOOL-MAPPING.md](RT-0.5-D-TOOL-MAPPING.md) 现有 19+1 Manifest 逐行映射 + 12 行未来工具面映射 + 2 项缺失替代裁决 | 完成 |
| 0.5-E | 安全与性能风险评估 | [RT-0.5-E-RISK-AND-ISOLATION.md](RT-0.5-E-RISK-AND-ISOLATION.md) 三档路由表 + 7 格式域默认档位 + 14 条资源限制 + 7 条本机特化裁决 | 完成 |
| 0.5-F | 设计 Capability Provider 契约 | [RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md](RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md) CapabilityEntry 十二字段 + trait 签名冻结 + 缓存 TTL/失效事件 + 错误信封五态 | 完成 |
| 0.5-G | 独立复核与文档验收 | 九条验收判定 9/9 + 抽读复现 + 本报告 | 完成 |

### A.2 九条验收判定（设计 §7）

| # | 验收标准 | 判定 | 关键证据 |
|---|---|---|---|
| 1 | 结论全部来自本机只读实测；候选不冒充已安装 | 通过 | 设计候选表显式免责"不代表当前机器已安装"（RT-0.5-HOST-CAPABILITY-DISCOVERY.md:13）；A§2 表 16 行每行带"实测见 rt05-probes/*#N"引用；pdf.mutool 如实 missing（A:27）；本会话抽读三项复现一致（见 E.2 节） |
| 2 | 每项记录绝对路径/版本/包来源/能力探测 | 通过 | A§2 表 8 字段齐备（RT-0.5-A:19-34）；mutool 行以 —/unknown 如实留空且 pacman -Q mupdf mupdf-tools 双包未找到落账（probe-pdf-image.md evidence #11） |
| 3 | 路径不信任/PATH 劫持/权限异常/探测失败有明确状态 | 通过 | exiftool candidateTrust=NO 非受信前缀（A§5.1 A3、probe-path-audit.md candidateTrust 表）；4 个 emo 可写目录 + 5 个 /tmp 挂载元素（A§5.2 A12）；555 权限（A11）；usrmerge 15 名假 shadow 排除、真实跨物理目录 shadow 零命中（A6）；五态区分定义于 C§4/F§7，本批实测仅 missing |
| 4 | 七类软件全检查，缺失如实标记 | 通过 | 16 项覆盖 FFmpeg/FFprobe/LibreOffice/Tesseract/Poppler/MuPDF/Pandoc/ImageMagick/7z×3/bsdtar/unzip/ExifTool（A§2）；MuPDF missing + 替代裁决（D§4.1）、chi_tra 缺失 + 禁静默回退裁决（D§4.2） |
| 5 | 能力目录与 Tool Registry 边界清楚，无第二套事实源 | 通过 | C§6 边界表（环境事实 vs 工具事实，capability_ref 单向引用）；F§1/§6"Provider 不注册工具、Registry 不探测环境"；CapabilityEntry 无 input_schema/output_schema/ToolRun 字段（RT-0.5-F:71-84）；D§2 确认现有 19+1 Manifest 无一因此改 runtime |
| 6 | Runtime Policy 综合宿主能力/信任/输入风险/依赖复杂度/隔离 | 通过 | 设计 §2 六因素；E§1 三档路由 = Trust Level × 格式解析器风险 + 7 格式域默认档位（RT-0.5-E:11-29）；D§1 六条映射原则引策略 §3 矩阵 |
| 7 | 恶意文档/媒体输入有明确隔离和资源限制策略 | 通过 | E§2 十四条资源限制（只读输入/独立输出/非特权/页数/像素/解压大小/输出上限/网络/环境白名单等，RT-0.5-E:31-48）；E§4 七条本机特化裁决；ImageMagick policy 空壳裁决写死（E§4.1，RT-0.5-E:80）；PS/EPS 强制 VM 档（E:29） |
| 8 | 未安装/升级/卸载软件，未改系统配置/Manifest/Adapter/Cargo/数据库 | 通过 | git status --porcelain 全部条目为 docs/phase0/（1 M + 6 ?? + ?? rt05-probes/），零 manifests/crates/migrations/Cargo 条目；五份 probe 头注均声明只读/timeout 5s |
| 9 | WSW-1 工作树及 WSW-1→WSW-2 顺序不受影响 | 通过 | HEAD=077a8dd4 "WSW-1 message model and WSW-2 runtime foundation"（2026-11-18 19:18，收口提交含 migrations/023_wsw_body_blob.sql）；Cargo.toml/Cargo.lock/crates/wsw-message/migrations/023_wsw_body_blob.sql 四路径 porcelain 为空；RT-0.5 文档全部 untracked |

### A.3 评审与修订

- 独立复核：九条验收全通过；抽读 ffmpeg/magick/exiftool 三项路径·包归属·sha256 前 16 位·版本·权限逐字段与文档一致；数字断言 248/51/76/26/3 语言包/mutool-missing 全部本机复现；四类典型错误（候选清单当机器现状/"命令存在"写成"能力可用"/PATH 劫持与权限异常漏查/能力目录越界成第二套 Registry）均未发现；六文档与五份取证间数字零矛盾。
- 评审 issues 4 条 → 修订 **4 applied / 0 rejected**：
  - [medium] E§2 条目 4/5 原生档 CPU/内存强制机制锚缺失 → 补 systemd-run --user --property=CPUQuota=/MemoryMax=（cgroup v2）为主、prlimit RLIMIT_CPU/RLIMIT_AS 兜底，"机制不可用即拒绝派发，不裸跑"写死（现 RT-0.5-E:38-39、注 A :50）；连带 F§8 条目 4 补交叉引用（RT-0.5-F:264）。
  - [medium] E§1 高风险行 podman_vm 未就绪期处置未定义 → 补"podman_vm 未启用/未实现时一律拒绝派发并报告（错误信封注明 VM 未就绪），禁止降级 podman_host 或受限宿主原生"（现 RT-0.5-E:17）。
  - [low] C§3.10 delegate 串 ffw 笔误 → fftw（RT-0.5-C:157；三方一致：本机 magick -version、probe-pdf-image.md #5、B§6.1）。
  - [low] NATIVE-FIRST §8.2 WSW-1 未提交快照句补时点标注（现 :211）+ 前置行连带修订（现 :6）；git diff = 1 file changed, 2 insertions(+), 2 deletions(-)。

### A.4 本批明确不做（按设计边界，非遗漏）

L3 smoke 未执行（设计 §3.3：仅安装/升级验收或工具首次启用）；Provider/Adapter 未实现（F§1.4/§6.4）；fingerprint 未预生成（C§5.1）；未安装任何软件；未 commit/push（设计 §8）。

## B. 架构与复用

### B.1 事实分层架构

```text
环境事实层  Host Capability Catalog（RT-0.5-C）+ CapabilityProvider（RT-0.5-F 契约）
                │  capability_refs 单向引用（工具 → 所需能力）
工具事实层  Tool Registry / Manifest / ToolRun / Artifact / Evidence / Trace（现有，零改动）
```

- Capability Catalog 记录**环境事实**（宿主机有什么软件/版本/子能力/健康状态），Tool Registry 记录**工具事实**（哪些工具可调用/Schema/运行记录）——边界表见 RT-0.5-C §6；Provider 不注册工具、Registry 不探测环境（F§1/§6）。
- CapabilityEntry 十二字段与设计 §3.1 一一映射（F§2；C§2），无任何工具语义字段——防"第二套 Tool Registry"的结构性保证。

### B.2 复用清单

| 复用对象 | 复用方式 | 证据 |
|---|---|---|
| 宿主已装软件 16 项 | 零重复打包，全部复用（设计 §1 Host Software First）；仅 missing 1 项走替代裁决 | A§2 总表；D§3 能力→工具面映射 |
| Tool Registry/ToolRun/Artifact/Evidence/Trace | 全部执行后端（原生/venv/Podman/VM）共用同一套，不建第二事实源 | E§5（RT-0.5-E:123） |
| Podman 挂载形态 | `/task/in (ro)` + `/task/out (rw)` 作为隔离档资源限制实践佐证 | E§2 条目 1/2 |
| 健康检查面 | Runtime 健康检查统一暴露 capability 摘要（id/status/version/fingerprint） | F§6.3（策略 §4.1） |
| Egress 统一出网 | 解析器默认禁网，出网走统一 Egress Policy（egress_proxy 白名单） | E§2 条目 11 |
| Worker Protocol v1 / 现有派发分支 | python_worker/podman_host 语义不变，Provider 接入发生在派发前置校验层 | F§6.2（引 RT-0 §4 dispatch.rs 代码证据） |

### B.3 映射结论（D 文档）

现有 19+1 Manifest **无一因宿主能力目录而改变 runtime**（RT-0.5-D:46）：16 python_worker → 保持 RT-2 venv；2 native_rust → 保持受控子进程；1 podman_host（nuclei）→ 保持隔离；rip.ops.worker-runtime 元数据条目不迁移不派发。宿主能力的映射价值在**未来工具面**（D§3 表 12 行：PDF 抽取/渲染、Office→PDF、OCR、图像、归档、元数据、媒体、格式转换 → RT-1 新增 native CLI 工具候选）。

## C. 文件清单（git status --porcelain 实测逐文件）

`git -C research-platform status --porcelain` 实测（收口时点，本报告写入后）：

```text
 M docs/phase0/NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md
?? docs/phase0/RT-0.5-A-HOST-SOFTWARE-INVENTORY.md
?? docs/phase0/RT-0.5-B-CAPABILITY-PROBES.md
?? docs/phase0/RT-0.5-C-CAPABILITY-CATALOG.md
?? docs/phase0/RT-0.5-D-TOOL-MAPPING.md
?? docs/phase0/RT-0.5-E-RISK-AND-ISOLATION.md
?? docs/phase0/RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md
?? docs/phase0/RT-0.5-BATCH-REPORT.md
?? docs/phase0/rt05-probes/
```

逐文件说明（行数以 `wc -l` 实测）：

| 文件 | 行数 | 状态 | 说明 |
|---|---:|---|---|
| docs/phase0/RT-0.5-A-HOST-SOFTWARE-INVENTORY.md | 98 | untracked | 16 项能力 8 字段总表 + 缺失清单 + 5 指定必录异常 + 9 补充异常 + 取证索引 |
| docs/phase0/RT-0.5-B-CAPABILITY-PROBES.md | 122 | untracked | 能力面展开：ffmpeg 编码器/语言包/headless/poppler/delegates/policy/归档格式/pandoc/PATH 审计摘要 |
| docs/phase0/RT-0.5-C-CAPABILITY-CATALOG.md | 303 | untracked | 16 项全字段能力目录 + 五态统计（15/1/0/0/0）+ 指纹公式/TTL/失效事件 + Registry 边界声明 |
| docs/phase0/RT-0.5-D-TOOL-MAPPING.md | 89 | untracked | 19+1 Manifest 逐行映射（全部"保持"）+ 12 行未来工具面 + mutool/chi_tra 替代裁决 + 四象限汇总 |
| docs/phase0/RT-0.5-E-RISK-AND-ISOLATION.md | 125 | untracked | 三档路由 + 7 格式域默认档位 + 14 条资源限制（含注 A CPU/内存锚）+ 7 条本机特化裁决 |
| docs/phase0/RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md | 265 | untracked | CapabilityEntry 十二字段 Rust 伪码 + trait 签名冻结 + probe 分层语义 + TTL/失效事件 + 错误信封五态 + RT-1 消费清单 |
| docs/phase0/RT-0.5-BATCH-REPORT.md | — | untracked | 本报告 |
| docs/phase0/NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md | — | **M**（tracked 修改） | 评审 [low] 修订落稿：前置行 + §8.2 时点标注（2 insertions/2 deletions，git diff --stat 实测） |
| docs/phase0/rt05-probes/probe-av-ocr.md | 78 | untracked | 取证：media.ffmpeg/ffprobe/ocr.tesseract（248 encoders/27 关键命中/3 语言包） |
| docs/phase0/rt05-probes/probe-office-convert.md | 61 | untracked | 取证：document.libreoffice/convert.pandoc（headless 面/51 in 76 out） |
| docs/phase0/rt05-probes/probe-pdf-image.md | 110 | untracked | 取证：poppler 三命令/pdf.mutool(missing)/image.magick（delegates/policy 空壳/gs） |
| docs/phase0/rt05-probes/probe-archive-meta.md | 121 | untracked | 取证：7z/7za/7zr/bsdtar/unzip/exiftool（wrapper 形态/Rar 仅 7z/--formats 不适用） |
| docs/phase0/rt05-probes/probe-path-audit.md | 99 | untracked | 取证：PATH 26 项审计/shadow/candidateTrust（横切面） |

- 上游设计文档 docs/phase0/RT-0.5-HOST-CAPABILITY-DISCOVERY.md（179 行）与 NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md 为前置批次产物，本批仅后者有 2 行评审修订；RT-0.5-HOST-CAPABILITY-DISCOVERY.md 本身零改动（不在 porcelain 清单）。
- 零 manifests/、零 crates/、零 migrations/、零 Cargo.toml/lock、零 web/ 条目（porcelain 全清单对照）。
- 父仓 pensting 视角：research-platform 子仓呈 M 状态，源于本批未提交产物（会话起始 gitStatus 快照可见）。

## D. 迁移与兼容

**本批零迁移、零代码。**

1. **代码零改动**：git status --porcelain 全部条目位于 docs/phase0/；零 manifests/tools/*.json、零 crates/**、零 migrations/**、零 Cargo.toml/Cargo.lock 条目——验收条 8 的直接实测证据。
2. **数据库零迁移**：migrations/ 链头未动；WSW-1 的 migrations/023_wsw_body_blob.sql 已在收口提交 077a8dd4 内（验收条 9），本批不触碰。
3. **Manifest 零迁移**：D§2 逐行映射全部为"保持"裁决（候选路径列明确标注"映射设计，不是机器现状裁决，更不是迁移执行"，RT-0.5-D:4）；逐项迁移归 RT-4 按实测分批裁决（策略 §9）；Manifest 新增 capability_refs 可选字段属 RT-1/RT-4 事项（F§6/§8.3），本批不改 Manifest。
4. **兼容性（契约设计而非实现）**：CapabilityProvider/CapabilityEntry 均为 Rust 伪码设计稿（F§2/§3 签名冻结），无 struct 落地、无 Adapter 接线、无 Cargo 变更；现有 MCP dispatch 的 python_worker/podman_host 分支语义不变（F§6.2）；Provider 接入位置在派发前置校验层，不改既有协议。
5. **兼容红线**：stale 条目不可供 Runtime Policy 决策使用（F§5.3）；五态区分硬要求——禁止把 timeout/probe_failed 报成 missing（F§7）；能力查找失败时路由终止并输出明确错误信封，不降级到低质量实现（E§3）。

## E. 实测结果

### E.1 盘点命令与回显摘要（引自 rt05-probes/ 取证）

| # | 命令 | 回显摘要 | 取证引用 |
|---|---|---|---|
| 1 | `command -v mutool` / `ls /usr/bin/mutool` / `pacman -Q mupdf mupdf-tools` | 无输出 exit 1 / 不存在 / 两包均未找到 | probe-pdf-image.md #11、missing 清单 |
| 2 | `ffmpeg -encoders \| grep -c '^ [VAS]'` | **248** | probe-av-ocr.md #4 |
| 3 | `ffmpeg -encoders` 关键行过滤（264/265/hevc/aac/flac/vorbis/opus） | 27 行命中：libx264…libvorbis（原生 opus/vorbis 带 `A..X.D` experimental 标记） | probe-av-ocr.md #5 |
| 4 | `ffmpeg -muxers` | `E mp4` / `E matroska` / `E webm`（另有 ipod/psp/webm_chunk/webm_dash_manifest） | probe-av-ocr.md #6 |
| 5 | `tesseract --list-langs` | `List of available languages in "/usr/share/tessdata/" (3): chi_sim eng osd` | probe-av-ocr.md #9 |
| 6 | `soffice --help`（headless 面判定） | 第 57 行 `--headless`；第 120/142/144 行 `--convert-to`/`--print-to-file`/`--cat` 均 implies headless（未启动实例） | probe-office-convert.md #6 |
| 7 | `pandoc --list-input-formats \| wc -l` / `--list-output-formats \| wc -l` | **51** / **76** | probe-office-convert.md #9 |
| 8 | `pdftotext -v` / `pdfinfo -v` / `pdftoppm -v` | 三命令均 version 26.11.0（poppler 26.11.0-1.1） | probe-pdf-image.md #4 |
| 9 | `magick -version` | ImageMagick 7.1.2-35 Q16-HDRI；Delegates (built-in) 含 bzlib/cairo/djvu/**fftw**/…/zstd | probe-pdf-image.md #5 |
| 10 | `magick -list delegate`（PDF/PS 行） | eps/pdf/ps 互转均调 `'gs'`，带 `-sstdout=%%stderr -dQUIET -dSAFER -dBATCH -dNOPAUSE -sDEVICE=pdfwrite`；gs 10.08.0 在位 | probe-pdf-image.md #8 #12 |
| 11 | `magick -list policy` | 生效策略仅 Undefined:none 两行（/etc/ImageMagick-7/policy.xml + [built-in]）；policy.xml 8138 字节中高危 coder 限制全在注释内 | probe-pdf-image.md #9 #10 |
| 12 | `7z i` / `7za i` / `7zr i` | 7z：7z/zip/tar/**Rar/Rar5**/gzip/xz/bzip2/zstd + 约 30 种识别格式；7za：无 Rar；7zr：仅 7z/Split/lzma/lzma86/xz/Hash | probe-archive-meta.md #6 #7 #8 |
| 13 | `bsdtar --formats` | `bsdtar: Option --formats is not supported`（exit 1）——如实记录非编造格式清单 | probe-archive-meta.md #10 |
| 14 | `readlink -f` PATH 归一（16 候选名） | /usr/sbin 命中全部归一为 /usr/bin/*（usrmerge symlink，非独立副本） | probe-path-audit.md #3、probe-av-ocr.md #2 |
| 15 | PATH 审计（26 项逐条） | 26 项、空元素 0；受信前缀 5 项；4 个 emo 可写目录排受信目录之前；5 项位于 /tmp/.mount_zcode.* 前缀；真实跨物理目录 shadow 零命中 | probe-path-audit.md pathDirs 表、shadowHits 表、verdict |
| 16 | `pacman -Qo` 包归属（16 候选） | ffmpeg/ffprobe→ffmpeg-full 9.0.2-3.46；tesseract→5.5.4-1.1；soffice→libreoffice-fresh 26.8.2-3；pandoc→pandoc-cli 3.11-87；poppler 三命令→26.11.0-1.1；magick→7.1.2.35-1.1；7z 系→7zip 26.06-1.1；bsdtar→libarchive 3.8.11-2.1；unzip→6.0-23.1；exiftool→perl-image-exiftool 13.55-1；mutool→两包未找到（unknown 如实留空） | 五份 probe 各 capabilityProvenance 字段 |

### E.2 本会话复核（本 workflow run 收口实测，命令原文 + 回显原文）

| 命令 | 回显（原文摘要） | 与文档对照 |
|---|---|---|
| `command -v mutool; ls /usr/bin/mutool; pacman -Q mupdf mupdf-tools` | `exit=1`；`ls: 无法访问 '/usr/bin/mutool': 没有那个文件或目录`；`错误：软件包 'mupdf' 未找到 / 错误：软件包 'mupdf-tools' 未找到` | 与 probe-pdf-image.md #11、A§2 行 9 一致 |
| `timeout 5 tesseract --list-langs` | `(3): chi_sim eng osd` | 与 probe-av-ocr.md #9 一致（3 语言包，chi_tra 缺失） |
| `readlink -f /usr/sbin/ffmpeg; sha256sum /usr/bin/ffmpeg \| cut -c1-16; pacman -Qo /usr/bin/ffmpeg` | `/usr/bin/ffmpeg`；`09190fb791fb0db8`；`由 ffmpeg-full 9.0.2-3.46 所拥有` | 与 A§2 行 1 五字段全部一致 |
| `sha256sum /usr/bin/magick \| cut -c1-16; pacman -Qo /usr/bin/magick; magick -version \| head -1` | `ff050139cbc6dbd9`；`由 imagemagick 7.1.2.35-1.1 所拥有`；`ImageMagick 7.1.2-35 Q16-HDRI x86_64 acaf7e418:20261031` | 与 A§2 行 10 一致 |
| `exiftool -ver; stat -c '%a %U:%G' /usr/bin/vendor_perl/exiftool; pacman -Qo /usr/bin/vendor_perl/exiftool` | `13.55`；`555 root:root`；`由 perl-image-exiftool 13.55-1 所拥有` | 与 A§2 行 16、A11/A3 一致（555 + 非受信前缀） |
| `echo -n "$PATH" \| tr ':' '\n' \| cat -n` | 26 项（.grok/.strix MISSING 目录在位；#3/4/5/17 为 emo 可写；#13/14/24/25/26 位于 /tmp/.mount_zcode.0pveme） | 与 probe-path-audit.md"PATH 共 26 项，空元素 0"一致 |
| `command -v systemd-run prlimit; systemd-run --user --scope -p CPUQuota=50% -p MemoryMax=64M true` | `/usr/sbin/systemd-run`、`/usr/sbin/prlimit`；`Running as unit: run-….scope`，exit 0 | 与 E§2 注 A"2026-11-18 本机核验"一致（原生档 CPU/内存强制锚在位） |
| `git status --porcelain`（含 Cargo.toml Cargo.lock crates/wsw-message migrations/023_wsw_body_blob.sql 定向检查） | 全部条目仅 docs/phase0/；四 WSW-1 路径输出为空 | 与验收条 8/9 一致 |

数字断言全部本机复现：248 encoders / 51+76 formats / 26 PATH 项 / 3 语言包 / mutool missing；六文档与五份取证间零矛盾。

## E2. CI（E2）

**待 watch 回填。**

- 本批零代码零构建面：无 Cargo/lock 变更、无测试面新增、无 Manifest/migrations 变更 → 既有门禁（cargo test/clippy/deny、web build/selftest）无新增需求，无需本批新增 CI 项。
- 批次产物未 commit（设计 §8"不提交或推送 Git"）；`gh run watch` 语义仅在后续收口提交后适用，届时终态回填本节（沿用 WSW-0 批次报告 E2 口径：红则修复批）。

## F. 性能

**本批无吞吐/延迟/内存基线，探测开销面如实登记（未逐一计量单命令耗时——取证未记录 per-command wall time，未验证项如实标注）。**

- **探测开销面构成**：五份取证全部只读命令，每条 `timeout 5s`（probe 头注实测口径）；覆盖 17 个 capability 段落（别名归并后 16 项）+ 26 项 PATH 审计 + 包归属/身份/hash 查询。L0/L1/L2 三层全部只读，L3 smoke 未执行（设计 §3.3：禁入启动路径）。
- **运行时开销零引入**：本批零代码，服务启动路径、派发路径均无任何新逻辑。
- **RT-1 预算锚（F§3 建议值，RT-1 定稿）**：L0 ≤1s（纯 stat/pacman 查询）、L1 ≤5s、L2 ≤10s（按需或后台）；L3 按工具登记。
- **缓存防启动开销（C§5.2/F§5.2）**：L0/L1 启动时仅 stat+sha256 轻量指纹校验，TTL 24h；L2 后台/按需重探，TTL 7d，绝不阻塞启动；L3 不进常规缓存。
- **原生档 CPU/内存锚开销（E§2 注 A）**：per-dispatch `systemd-run --user --scope` 单元创建存在额外进程开销，RT-1 落实时需计量——**未验证**（本批未测 scope 创建耗时）。
- **fingerprint 未预生成**：全量 sha256 由 RT-1 Provider 首探时计算（C§5.1），本批取证口径为前 16 位（仅 archive.7z wrapper 有全值），避免目录文档与运行时指纹双口径。

## G. 遗留与风险（裁决与待办全录）

### G.1 已裁决遗留（本批裁决写死，待 RT-1 落实）

| # | 项 | 裁决 | 证据 |
|---|---|---|---|
| 1 | **pdf.mutool missing**（mutool 不在 PATH，mupdf/mupdf-tools 均未安装） | **替代已裁决：不补装、不重复打包**——文本抽取 pdf.pdftotext、元数据 pdf.pdfinfo、渲染 pdf.pdftoppm + gs 10.08.0（Poppler 覆盖 MuPDF 在本目录核心用途，非静默降级）；MuPDF 特有能力（clean 结构化重写/JS 面）不在需求面，未来需求走用户授权安装 | D§4.1；probe-pdf-image.md #4 #11 #12 |
| 2 | **chi_tra 语言包缺失**（仅 chi_sim/eng/osd 3 个） | **补装属用户决策，Discovery 只报告不安装**；补装前 chi_tra 请求显式报错，禁止静默回退 chi_sim（繁简字形差异大，回退即静默降级）；可粘贴命令 `sudo pacman -S tesseract-data-chi_tra` 由用户决定 | D§4.2；probe-av-ocr.md #9 #11 |
| 3 | **ImageMagick policy.xml 空壳**（唯一生效 Undefined:none，PDF/PS/URL/MSL/MVG 高危面默认开放） | **隔离裁决：不可信图像/PDF 一律不走宿主 magick 直连，走 Podman/VM 隔离**；可信低风险图像受限原生时由调用方硬限额（policy 无兜底）；URL/MSL/MVG 类 coder 输入宿主原生一律拒绝；gs -dSAFER 仅为缓解非边界 | E§4.1（RT-0.5-E:80）；probe-pdf-image.md #9 #10 |
| 4 | **exiftool 非受信前缀**（/usr/bin/vendor_perl 不在受信集合，candidateTrust=NO；Perl 脚本 555） | **固定绝对路径 /usr/bin/vendor_perl/exiftool + 包归属校验，禁止裸 PATH 解析**；不误判"非受信前缀"为"目录被劫持"（目录 root 属主 755 不可写）；指纹覆盖脚本本体 + perl 解释器包归属 | E§4.4；probe-path-audit.md #6、probe-archive-meta.md #12 #14 |
| 5 | PATH 残留面（4 个 emo 可写目录排受信目录之前；5 个 /tmp/.mount 挂载元素） | Provider 探测与 Adapter 执行均用受控 search roots（受信前缀 + 固定候选目录），解析绝对路径后验证身份；用户可写目录与 /tmp 元素中的同名程序不得当作系统可信能力；真实跨物理目录 shadow 零命中 | E§4.7；probe-path-audit.md verdict |
| 6 | ffmpeg-full 第三方仓库构建（-3.46 非 Arch 官方命名，nonfree 全开） | 纳入指纹监控 + stale 重探（包版本/hash 变化即失效）；生产编码用 libopus/libvorbis，原生 opus/vorbis（experimental）禁用于生产面 | E§4.5、A5/A8 |
| 7 | convert 弃用别名（IM7 symlink + WARNING，本机无 IM6） | RT-1 Adapter 命令 allowlist 只登记 magick，不登记 convert | E§4.6、A2 |
| 8 | LibreOffice headless 仅凭 --help 判定（未启动实例） | L3 smoke test 留待 RT-1 工具首次启用时以内置 fixture 执行；运行时禁并发共享 profile，用隔离临时 profile | B§4、E§4.2 |
| 9 | 归档工具能力缺口如实落账：bsdtar 无 --formats 清单；7za 无 Rar；7zr 无 zip/tar/rar；unzip 2009 旧实现 | 格式路由按能力面（Rar 仅 7z）；7za/7zr 遇 Rar 显式报错不静默降级；列目录与解包两阶段分开授权 + 路径穿越/符号链接/炸弹独立防护 | E§4.3、B§7、A10/A13 |
| 10 | magick delegate 注入面（libreoffice --convert-to pdf、curl https:decode、c2patool、darktable-cli） | Adapter 层固定参数模板，禁止把用户输入拼进 delegate 命令 | E§4.1、A14 |
| 11 | podman_vm 设计保留未实现（NATIVE-FIRST §2.2/§8.1） | E§1 高风险行修订：podman_vm 未启用/未实现时一律拒绝派发并报告（错误信封注明 VM 未就绪），禁止降级 podman_host 或受限宿主原生 | E§1（RT-0.5-E:17，评审 medium#2 修订 applied） |
| 12 | 原生档 CPU/内存强制入口（manifests resources.cpu/memory 属容器/python_worker 语境，不覆盖宿主原生子进程） | E§2 注 A：systemd-run --user --scope --property=CPUQuota=/MemoryMax=（cgroup v2）为主、prlimit RLIMIT_CPU/RLIMIT_AS 兜底；两种机制均不可用 = 拒绝派发不裸跑；本机核验 systemd-run/prlimit 在位且 scope 启动 exit 0 | E§2 条目 4/5 + 注 A（RT-0.5-E:38-39/50，评审 medium#1 修订 applied；本会话复核见 E.2） |
| 13 | 指纹值未预生成（目录只记 sha256 前 16 位） | 完整 fingerprint 由 RT-1 Capability Provider 首探计算（公式冻结于 C§5.1/F§5.1），目录文档不预生成不编造 | C§5.1 |

### G.2 批次级遗留

- 本批产物未 commit/push（设计 §8 边界）；入库属后续收口决策。
- CI 终态未回填（E2 节，待 watch）。
- demuxer 全量面未清点（ffprobe -version 通过即可用，demuxer/muxer 全量清单留待 RT-1 按需 L2 重探，B§2）。
- 评审 2 medium 属设计完备性缺口（非数据错误）：初稿 E§2 条目 4/5 缺原生档强制锚、E§1 高风险档路由未定义态——均已修订 applied；rejected=[]。

## H. 下一批依赖

**下一批：RT-1 Native Rust Adapter（通用受控原生执行）。RT-0.5 是其前置设计输入（设计 §9：避免 Native Rust Adapter 只围绕自带二进制设计，之后再返工支持系统软件）。**

RT-1 消费清单（F§8 全文五条）：

1. 实现 `CapabilityProvider`（进程内缓存 + pacman 归属校验 + 受控 search roots），落地 C§5 缓存/失效与 F§5 失效事件。
2. 以 RT-0.5-C 目录 **16 条为种子数据**做首次 L0/L1 全量校验（对照本批取证 hash/版本）。
3. 为 Manifest 增加 capability_refs 可选字段解析（不改现有 19 个 Manifest 既有字段）。
4. Native CLI Adapter 按 entry.executable_path + security_profile.allowed_call_modes 执行，落实 E§2 十四条资源限制（**原生档 CPU/内存强制机制锚与"机制不可用即拒绝派发"行为见 E§2 注 A**）。
5. L3 smoke 用例随工具首次启用逐个补充，不进启动路径。

消费的契约面（本批冻结、实现归 RT-1）：

- `CapabilityEntry` 十二字段 + `CapabilityStatus` 五态 + `ProbeError` 五态（F§2/§7）；
- `CapabilityProvider` trait 签名：probe/get/list/fingerprint/invalidate/refresh（F§3）；
- probe(level) 分层聚合规则：请求 L2 必须先完成 L0+L1，任一层失败返回对应 ProbeError 不返回半截 Entry（F§4）；
- search roots 规则：受信前缀 + 排除 cwd//tmp/用户可写目录优先级（F§4 末段）；
- 错误信封五态映射表（F§7：Missing→拒绝派发+成熟替代；Timeout/UnsupportedOutput/ProbeFailed→拒绝派发+stale 重探/审计）；
- stale 条目禁供 Runtime Policy 决策（F§5.3）。

下游批次链（设计 §9）：RT-1 Adapter → RT-2 Python Worker 项目级 venv → RT-3 Runtime Policy Resolver（纳入 Capability/Trust/InputRisk/Dependency/Isolation 五因素）→ RT-4 逐项迁移评估（不得批量改 Manifest）。**RT-0.5 不改变 WSW-1 收口后 WSW-2 的顺序**（验收条 9；NATIVE-FIRST §8.2 修订后口径）。

## I. 浏览器截图

**本批无 UI 面，无截图。** 批次产物全部为 markdown 文档与命令行取证回显；零浏览器操作、零页面交互；web/ 零改动（git status porcelain 无 web/ 条目）；既有 web/qa/plugins 截图属此前批次产物，本批未触碰。运行证据全部为命令行实拍（rt05-probes/ 五份取证）。

## J. RTIS 对账

**零接触。** integrations/rtis 在 git status --porcelain 全清单中无任何条目（零 diff）；本批未调用任何 RTIS 工具面、未读写 RTIS 数据；能力目录、Provider 契约、隔离裁决均不含 RTIS 面；RTIS 私有数据继续列入解析器禁区（E§2 条目 13"文件系统边界"明确禁止暴露）。

## K. 教训

1. **候选清单 ≠ 机器现状，必须每行实测落证。** 设计把"候选软件不得冒充已安装"立为验收第一条（§7），候选表自带免责行（设计 :13）；评审四类典型错误专项检查确认未犯——防错机制前置（免责声明 + 每行证据引用）比事后纠错有效。
2. **wrapper 形态的指纹必须覆盖 wrapper 与真实 ELF 双方。** 7z/7za/7zr 为 sh wrapper、soffice 落点也是 wrapper，只记 wrapper 哈希则升级校验落空（A1/A7）；指纹公式为此显式含 wrapper_target（F§2 BinaryIdentity）。
3. **"命令存在" ≠ "能力可用"，"文件存在" ≠ "策略生效"。** LibreOffice 仅 --help 判定且如实声明"未启动实例"；bsdtar `--formats` 不适用如实记录而非编造格式清单；/etc/ImageMagick-7/policy.xml 存在 8138 字节但唯一生效行是 Undefined:none——安全兜底必须以运行时清单实测为准（`magick -list policy`），不能以配置文件存在为据。
4. **探测失败五态先冻结再实装。** 本批仅 missing 命中，但五态语义（missing/permission_denied/timeout/unsupported_output/probe_failed）与错误信封映射先在 F§7 定稿——避免 RT-1 实现时把 timeout/probe_failed 报成 missing（设计 §3.2"探测失败不等于软件不存在"）。
5. **策略条目必须自带"可操作实现入口"。** 评审 2 medium 同源：资源限制表引用了容器/python_worker 语境的实践（manifests resources.cpu/memory），但宿主原生子进程档位没写强制机制——设计完备性缺口在评审暴露后以"机制锚 + 机制不可用即拒绝派发"补齐（E§2 注 A）。写限制清单时要问一句"这一条在目标档位由谁、用什么机制强制"。
6. **历史快照句要带时点标注。** NATIVE-FIRST §8.2"WSW-1 未提交修改"是写作时点快照，WSW-1 收口后即成误导（后来者会误判前置未满足）——修订补时点标注 + 前置行同步，门语句式（"须在收口后启动"）则保持不变。
7. **并发受限账户下 dynamic-workflow 的驱动路径。** 本批盘点期间三次 Model creation failed（账户并发上限 3）：子代理并发扩展受限时，主会话 Agent 子代理串行驱动是可行路径；执行路径单列入账（本报告"执行路径说明"节）保证后续批次可复用该经验。

## 坏消息单列

1. **宿主能力缺失两项**：pdf.mutool missing（mupdf/mupdf-tools 双包未安装）；tesseract chi_tra 语言包缺失（繁体中文 OCR 不可用）——替代/补装裁决见 G.1 #1/#2，均未安装（按设计 §5 Discovery 只报告不安装）。
2. **安全面三项实况不佳**：ImageMagick policy.xml 空壳（高危 coder 面默认开放）；exiftool 位于非受信前缀（candidateTrust=NO）；PATH 残留面（4 个 emo 可写目录排受信目录之前 + 5 个 /tmp/.mount 挂载元素）。
3. **初稿两处设计完备性缺口（评审 2 medium）**：E§2 原生档 CPU/内存限制无可操作强制入口；E§1 高风险档在 podman_vm 未就绪期路由未定义——均已修订 applied，但属初稿质量问题如实登记。
4. **能力判定深度受限**：LibreOffice headless 仅凭 --help 判定未启动实例；bsdtar 无格式清单能力；demuxer 全量面未清点——L3/深探均推迟到 RT-1（按设计 §3.3 边界，非遗漏）。
5. **dynamic-workflow 三次 Model creation failed**（账户并发上限 3）——驱动路径已写入"执行路径说明"。
6. **CI 终态未回填**（E2 待 watch）；**批次未 commit/push**（按设计 §8 属边界内行为，产物以 untracked 状态存在于工作树，入库待后续收口决策）。
