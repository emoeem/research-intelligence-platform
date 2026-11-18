# RT-0.5-C：Host Capability Catalog（能力目录）

- 日期：2026-11-18
- 状态：已定稿（RT-0.5-C）。全字段数据模型按 [RT-0.5-HOST-CAPABILITY-DISCOVERY.md](RT-0.5-HOST-CAPABILITY-DISCOVERY.md) §3.1 冻结；fingerprint 公式冻结、具体指纹值由 RT-1 Capability Provider 首探时计算，本目录不预生成、不编造
- 证据来源：`rt05-probes/` 五份 2026-11-18 只读实测（引用格式：实测见 rt05-probes/probe-*.md #N）
- 总表与异常见 [RT-0.5-A-HOST-SOFTWARE-INVENTORY.md](RT-0.5-A-HOST-SOFTWARE-INVENTORY.md)；探测细节见 [RT-0.5-B-CAPABILITY-PROBES.md](RT-0.5-B-CAPABILITY-PROBES.md)

## 1. 目录定位

能力目录记录**环境事实与健康状态**：宿主机有什么软件、什么版本、什么子能力、处于什么状态。它不是第二套 Tool Registry——具体工具仍由现有 Manifest、Tool Registry 和 Tool Policy 管理（设计 §3.1 末段）。边界声明见 §6。

## 2. 数据模型字段映射（设计 §3.1 → 本目录字段）

| 设计 §3.1 字段 | 本目录字段 | 填充来源 |
|---|---|---|
| capability_id | capability_id | 稳定标识，同 [RT-0.5-A](RT-0.5-A-HOST-SOFTWARE-INVENTORY.md) §2 表 |
| provider | provider | 软件名 + 发行项目（包归属佐证） |
| executable_path | executable_path | `command -v` + `readlink -f` 归一的绝对路径（L0） |
| version | version（normalized + raw） | L1 版本探测回显 |
| binary_identity | binary_identity | sha256 前 16 位（取证口径）+ 文件类型 + 所有者/权限（L0） |
| package_provenance | package_provenance | `pacman -Qo`；无法确认标记 unknown |
| capabilities | capabilities（已探测能力集合） | L2 探测清单 |
| status | status | available / missing / incompatible / probe_failed / disabled 五态 |
| probe_timestamp | probe_timestamp | 2026-11-18（五份取证同日实测） |
| fingerprint | fingerprint | 见 §5 公式；本批不预生成具体值 |
| constraints | constraints | 版本/子能力/兼容性限制（异常清单推导） |
| security_profile | security_profile | 允许的调用方式、输入风险等级、隔离要求（详见 [RT-0.5-E-RISK-AND-ISOLATION.md](RT-0.5-E-RISK-AND-ISOLATION.md)） |

## 3. 能力目录（16 项全字段）

通用记法：`probe_timestamp = 2026-11-18`（全部条目）；`fingerprint` 待 Provider 计算；证据仅列关键条目号，全量见 rt05-probes/ 各 evidence 清单。

### 3.1 media.ffmpeg

- provider：FFmpeg（ffmpeg-full 项目构建）
- executable_path：/usr/bin/ffmpeg
- version：normalized `9.0.2`；raw `ffmpeg version 9.0.2`
- binary_identity：sha256 前 16 位 `09190fb791fb0db8` · ELF 64-bit LSB pie executable, x86-64, dynamically linked, stripped · root:root 755
- package_provenance：pacman · ffmpeg-full · 9.0.2-3.46（第三方仓库构建，非官方 core/extra）
- capabilities：248 encoders（libx264/libx265/libopenh264/h264_nvenc/h264_vaapi/aac/libfdk_aac/flac/libopus/libvorbis 等 27 行关键命中）；muxers mp4/matroska/webm（另有 ipod/psp/webm_chunk/webm_dash_manifest）；build 含 GPL+version3+nonfree+NVENC/CUDA/VAAPI/Vulkan
- status：available
- constraints：原生 opus/vorbis encoder 为 experimental（`A..X.D`），生产编码必须用 libopus/libvorbis；demuxer 全量面未清点，按需 L2 重探
- security_profile：可信低风险媒体输入可受限原生子进程；不可信媒体输入走隔离；禁自行联网拉流（Egress Policy 统一）
- 证据：实测见 rt05-probes/probe-av-ocr.md #3 #4 #5 #6 #10 #12

### 3.2 media.ffprobe

- provider：FFmpeg（ffmpeg-full）
- executable_path：/usr/bin/ffprobe
- version：normalized `9.0.2`；raw `ffprobe version 9.0.2 … libavformat 63.1.102`
- binary_identity：`a2ae3d0209aec7399` · ELF pie executable, x86-64, stripped · root:root 755
- package_provenance：pacman · ffmpeg-full · 9.0.2-3.46
- capabilities：媒体流/容器探测（-version 正常）
- status：available
- constraints：与 media.ffmpeg 同包同版本演进，指纹联动失效
- security_profile：同 media.ffmpeg；ffprobe 为只读探测面，输入风险容忍度高于 ffmpeg 转码面，仍按媒体输入风险路由
- 证据：实测见 rt05-probes/probe-av-ocr.md #7 #12

### 3.3 ocr.tesseract

- provider：Tesseract OCR（tesseract 项目；leptonica 后端）
- executable_path：/usr/bin/tesseract
- version：normalized `5.5.4`；raw `tesseract 5.5.4`（leptonica-1.87.0，AVX2/AVX/FMA/SSE4.1，OpenMP 202111）
- binary_identity：`44e4c41d9986e8b2` · ELF pie executable, x86-64, dynamically linked, stripped · root:root 755
- package_provenance：pacman · tesseract · 5.5.4-1.1（语言包 tesseract-data-* 2:4.1.0-5）
- capabilities：tessdata=/usr/share/tessdata/；语言包 3 个：chi_sim、eng、osd；**chi_tra 缺失（繁体中文 OCR 不可用）**
- status：available
- constraints：chi_tra 语言包缺失须显式报告，禁止静默降级为 chi_sim 兜底（设计 §3.4"语言数据缺失要明确报告"）；补装属用户决策，Discovery 只报告不安装（设计 §5）
- security_profile：输入为图像/PDF 渲染页 → 受限原生仅限可信低风险输入；像素数/页数/时长限额（OCR 内存随像素放大）；不明来源图像按隔离路由
- 证据：实测见 rt05-probes/probe-av-ocr.md #8 #9 #11

### 3.4 document.libreoffice

- provider：LibreOffice（The Document Foundation；Arch 包 libreoffice-fresh）
- executable_path：/usr/lib/libreoffice/program/soffice（sh wrapper）；真实二进制 /usr/lib/libreoffice/program/soffice.bin
- version：normalized `26.8.2.2`；raw `LibreOffice 26.8.2.2 680(Build:2)`
- binary_identity：wrapper `3d585422d7cdddf9`（POSIX shell script）· soffice.bin `baa182393db76be0`（ELF pie，BuildID 6c007680…）· 两文件 root:root 755
- package_provenance：pacman · libreoffice-fresh · 26.8.2-3
- capabilities：headless 可用（--help 第 57 行；--convert-to/--print-to-file/--cat 均 implies headless；未启动实例）；Writer（libswlo+libswuilo）/Calc（libsclo+libscuilo）/Impress（libsdlo+libsduilo）组件齐
- status：available
- constraints：headless 判定仅来自 --help，L3 smoke test 留待工具首次启用；**禁止并发共享同一用户 profile，运行时必须用隔离临时 profile**（设计 §3.4）；审计以 soffice.bin 为准
- security_profile：可信低风险 Office 文档可受限原生（隔离临时 profile + 非特权身份）；不明来源 Office 文档（宏/嵌入对象/ODF 漏洞面）优先 Podman 隔离；禁暴露宿主家目录/凭据
- 证据：实测见 rt05-probes/probe-office-convert.md #1-#6 #11

### 3.5 convert.pandoc

- provider：Pandoc（pandoc-cli，GHC 构建）
- executable_path：/usr/bin/pandoc
- version：normalized `3.11`；raw `pandoc 3.11`（Features: +server +lua，Lua 5.4）
- binary_identity：`effb83b40a825d86` · ELF shared object（GHC 常态，非 pie executable）· root:root 755
- package_provenance：pacman · pandoc-cli · 3.11-87
- capabilities：输入格式 51 条、输出格式 76 条；双侧含 docx/html/html4/html5/latex/markdown 及变体（markdown_github/markdown_mmd/markdown_phpextra/markdown_strict/biblatex/chunkedhtml）
- status：available
- constraints：文本类转换为主，不承担 PDF 输入解析（PDF 输入走 poppler）；Lua 过滤器面默认不启用
- security_profile：可信/低风险文本输入可受限原生；HTML 输入注意资源与脚本面（--lua-filter 默认禁用）；不明来源文档按 PDF/Office 同级路由评估
- 证据：实测见 rt05-probes/probe-office-convert.md #7-#10 #11

### 3.6 pdf.pdftotext

- provider：Poppler（The Poppler Developers）
- executable_path：/usr/bin/pdftotext
- version：normalized `26.11.0`；raw `pdftotext version 26.11.0 / Copyright 2005-2026 The Poppler Developers`
- binary_identity：`f85872f94fb67433` · ELF pie executable（BuildID c4a62615…）· root:root 755
- package_provenance：pacman · poppler · 26.11.0-1.1
- capabilities：PDF 文本抽取
- status：available
- constraints：与 pdf.pdfinfo/pdf.pdftoppm 同包同版本，指纹联动失效；PDF 解析器漏洞暴露面随版本更新
- security_profile：可信低风险 PDF 受限原生（页数/时长/输出上限）；不明来源 PDF 优先隔离（PDF 解析历史漏洞面）；禁止网络资源获取（PDF 可引用外部对象）
- 证据：实测见 rt05-probes/probe-pdf-image.md #2 #3 #4 #13

### 3.7 pdf.pdfinfo

- provider：Poppler
- executable_path：/usr/bin/pdfinfo
- version：normalized `26.11.0`；raw `pdfinfo version 26.11.0`
- binary_identity：`3bdf68a79b9a3c22` · ELF pie executable（BuildID fa9767fc…）· root:root 755
- package_provenance：pacman · poppler · 26.11.0-1.1
- capabilities：PDF 元数据/页数探测
- status：available
- constraints：同包联动（见 3.6）
- security_profile：只读探测面；可信 PDF 受限原生，不明来源 PDF 仍按隔离路由（解析面与 pdftotext 同源）
- 证据：实测见 rt05-probes/probe-pdf-image.md #3 #4 #13

### 3.8 pdf.pdftoppm

- provider：Poppler
- executable_path：/usr/bin/pdftoppm
- version：normalized `26.11.0`；raw `pdftoppm version 26.11.0`
- binary_identity：`ab0ef92375b618b8` · ELF pie executable（BuildID 21070f47…）· root:root 755
- package_provenance：pacman · poppler · 26.11.0-1.1
- capabilities：PDF 页面渲染（位图，OCR 前置）
- status：available
- constraints：渲染像素/页数必须由调用方限额（分辨率×页数即内存放大器）；同包联动（见 3.6）
- security_profile：同 3.6；渲染输出写入独立工作区，非特权身份
- 证据：实测见 rt05-probes/probe-pdf-image.md #3 #4 #13

### 3.9 pdf.mutool

- provider：MuPDF（Artifex）—— **未安装**
- executable_path：无
- version：—
- binary_identity：—
- package_provenance：unknown（`pacman -Q mupdf mupdf-tools` 两包均未找到）
- capabilities：无
- status：**missing**
- constraints：缺失项；成熟替代已裁决——文本抽取 pdf.pdftotext、元数据 pdf.pdfinfo、渲染 pdf.pdftoppm + gs 10.08.0（见 [RT-0.5-D-TOOL-MAPPING.md](RT-0.5-D-TOOL-MAPPING.md) §4.1）
- security_profile：—（补装属用户决策，Discovery 只报告不安装，设计 §5）
- 证据：实测见 rt05-probes/probe-pdf-image.md #1 #11

### 3.10 image.magick（别名 image.convert）

- provider：ImageMagick Studio LLC
- executable_path：/usr/bin/magick；兼容别名 /usr/bin/convert（symlink → magick）
- version：normalized `7.1.2-35`；raw `ImageMagick 7.1.2-35 Q16-HDRI x86_64 acaf7e418:20261031`
- binary_identity：`ff050139cbc6dbd9` · ELF pie executable（BuildID 15fe31bd…）· root:root 755；convert 为 lrwxrwxrwx symlink 同文件同 hash
- package_provenance：pacman · imagemagick · 7.1.2.35-1.1
- capabilities：Features Cipher/DPC/HDRI/Modules/OpenCL/OpenMP；内建 delegates bzlib/cairo/djvu/fftw/fontconfig/freetype/heic/jbig/jng/jp2/jpeg/jxl/lcms/lqr/ltdl/lzma/openexr/pangocairo/png/raqm/raw/rsvg/tiff/uhdr/webp/wmf/x/xml/zip/zlib/zstd；PDF/PS delegate 调 gs（-dSAFER，gs 10.08.0 在位）；office 系 delegate 调 libreoffice --convert-to pdf
- status：available
- constraints：**policy.xml 空壳**（唯一生效 Undefined:none，PDF/PS/URL/MSL/MVG 高危面默认开放）；convert 别名带 IM7 deprecation WARNING 且本机无 IM6，Adapter 层统一走 magick；delegate 表含 curl/c2patool/darktable-cli 注入面
- security_profile：**不可信图像/PDF 禁走宿主 magick 直连**（无 policy 兜底），走 Podman/VM 隔离；可信低风险图像可受限原生但由调用方硬限额（策略无保护）；禁 delegate 自行联网（URL coder 开放状态下尤其要收紧）
- 证据：实测见 rt05-probes/probe-pdf-image.md #5-#10 #12、异常 #1 #2 #3

### 3.11 archive.7z

- provider：7-Zip（Igor Pavlov 官方 Linux 移植；p7zip 为虚拟依赖解析到同包）
- executable_path：/usr/bin/7z（sh wrapper → /usr/lib/7zip/7z）
- version：normalized `26.06`；raw `7-Zip 26.06 (x64) : Copyright (c) 1999-2026 Igor Pavlov : 2026-11-06`
- binary_identity：wrapper `f802383f13b8331a`（全值 f802383f13b8331aabaf779def00a356c8d2b8148255170b75b03028f4d23ad4，POSIX shell script）· 真实 ELF /usr/lib/7zip/7z `d48e622e9009acdb` · root:root 755
- package_provenance：pacman · 7zip · 26.06-1.1
- capabilities：Formats 7z/zip/tar/Rar/Rar5/gzip/xz/bzip2/zstd 全部在列，另有约 30 种识别格式（APFS/Arj/Cab/Chm/Compound/Cpio/CramFS/Dmg/ELF/Ext/FAT/ISO/HFS/NTFS/NSIS/PE 等）；Libs /usr/lib/7zip/7z.so
- status：available
- constraints：唯一具备 Rar/Rar5 解码的 7-Zip 入口（经 7z.so 外挂库）；wrapper 哈希不等于真实 ELF 哈希，指纹必须覆盖两者；列目录与解包分开授权
- security_profile：不明来源归档禁在宿主直解——路径穿越/符号链接/压缩炸弹独立防护后仍按隔离优先；可信低风险归档可受限原生（只读输入、独立输出目录、解压大小预算）
- 证据：实测见 rt05-probes/probe-archive-meta.md #2 #4-#7 #13

### 3.12 archive.7za

- provider：7-Zip（独立静态构建）
- executable_path：/usr/bin/7za（sh wrapper → /usr/lib/7zip/7za）
- version：normalized `26.06`；raw `7-Zip (a) 26.06 (x64)`
- binary_identity：wrapper `4566c6e82f8147dc`（POSIX shell script）· root:root 755
- package_provenance：pacman · 7zip · 26.06-1.1
- capabilities：Formats 7z/Cab/Split/bzip2/gzip/lzma/lzma86/tar/xz/zip/zstd/Hash；无 Rar
- status：available
- constraints：Rar 不可用（选 7za 时 Rar 输入必须显式失败并路由到 archive.7z 或拒绝，不静默降级）
- security_profile：同 3.11；格式子集更窄，适合低风险常规归档
- 证据：实测见 rt05-probes/probe-archive-meta.md #8

### 3.13 archive.7zr

- provider：7-Zip（精简版，Public domain）
- executable_path：/usr/bin/7zr（sh wrapper → /usr/lib/7zip/7zr）
- version：normalized `26.06`；raw `7-Zip (r) 26.06 (x64)`
- binary_identity：wrapper `1c5de14f5ac7ac74`（POSIX shell script）· root:root 755
- package_provenance：pacman · 7zip · 26.06-1.1
- capabilities：Formats 仅 7z/Split/lzma/lzma86/xz/Hash；Codecs AES256CBC/7zAES/LZMA/LZMA2/Delta/Copy
- status：available
- constraints：无 zip/tar/rar 面
- security_profile：同 3.11；仅 7z 原生格式场景
- 证据：实测见 rt05-probes/probe-archive-meta.md #8

### 3.14 archive.bsdtar

- provider：libarchive 项目（bsdtar 前端）
- executable_path：/usr/bin/bsdtar
- version：normalized `3.8.11`；raw `bsdtar 3.8.11 - libarchive 3.8.11 zlib/1.3.1.zlib-ng liblzma/5.8.3 bz2lib/1.0.8 liblz4/1.11.1 libzstd/1.5.7 libxml2/2.15.4 openssl/4.1.0 libb2/bundled libacl/2.4.0`
- binary_identity：`0081c8f3e9820eae` · ELF pie executable · root:root 755
- package_provenance：pacman · libarchive · 3.8.11-2.1
- capabilities：tar 读面由 libarchive 后端提供（zlib/lzma/bz2/lz4/zstd/xml2/openssl）；create 模式 `--format {ustar|pax|cpio|shar}`；**无 --formats 清单子命令**
- status：available
- constraints：L2 格式探测对 bsdtar 不适用（`Option --formats is not supported`，exit 1）；能力声明以 libarchive 版本行承载
- security_profile：同 3.11；tar 家族的路径穿越/符号链接防护是第一风险（pax 扩展头可携带绝对路径/链接目标）
- 证据：实测见 rt05-probes/probe-archive-meta.md #9 #10

### 3.15 archive.unzip

- provider：Info-ZIP
- executable_path：/usr/bin/unzip
- version：normalized `6.00`；raw `UnZip 6.00 of 20 April 2009, by Info-ZIP. Maintained by C. Spieler.`
- binary_identity：`15f733fefed8de50` · ELF pie executable · root:root 755
- package_provenance：pacman · unzip · 6.0-23.1
- capabilities：zip 解包（banner 探测通过；无格式清单子命令）
- status：available
- constraints：2009 年发布的旧实现；zip64/新压缩方法支持面未知，未见 L2 清单；与 archive.7z 的 zip 面功能重叠
- security_profile：同 3.11；zip 路径穿越（`../` 条目）与炸弹防护必须独立实现；上量场景优先 archive.7z（格式识别更全）
- 证据：实测见 rt05-probes/probe-archive-meta.md #11 #3

### 3.16 meta.exiftool

- provider：ExifTool（Phil Harvey；perl-image-exiftool 包）
- executable_path：/usr/bin/vendor_perl/exiftool
- version：normalized `13.55`；raw `13.55`（`-ver`）
- binary_identity：`80e0018c48f93244` · Perl script text executable · root:root **555**
- package_provenance：pacman · perl-image-exiftool · 13.55-1
- capabilities：版本探测通过；图片/音视频/文档元数据读取面（设计 §1 用途，本批未对样本实测）
- status：available
- constraints：**所在目录 /usr/bin/vendor_perl 不在受信前缀集合**（PATH 审计 candidateTrust=NO；目录本身 root 属主 755 不可写）；Perl 脚本形态，指纹审计覆盖脚本文件本体
- security_profile：调用必须固定绝对路径 + 包归属校验，禁止裸 PATH 解析；元数据读取面对不可信文件的解析漏洞暴露面真实存在 → 不明来源文件按隔离路由；输出上限约束（某些格式元数据可膨胀）
- 证据：实测见 rt05-probes/probe-archive-meta.md #12 #14、rt05-probes/probe-path-audit.md #6

### 3.17 其余五态归零说明

incompatible / probe_failed / disabled 三态在本批为 0（见 §4 统计）；`rip.ops.worker-runtime` 式的 disabled 属 Tool Registry 工具事实，不属本目录。

## 4. 五态统计

| status | 计数 | 条目 |
|---|---:|---|
| available | **15** | media.ffmpeg、media.ffprobe、ocr.tesseract、document.libreoffice、convert.pandoc、pdf.pdftotext、pdf.pdfinfo、pdf.pdftoppm、image.magick、archive.7z、archive.7za、archive.7zr、archive.bsdtar、archive.unzip、meta.exiftool |
| missing | **1** | pdf.mutool |
| incompatible | **0** | — |
| probe_failed | **0** | — |
| disabled | **0** | — |

补充（不占 capability 条目）：ocr.tesseract 本体 available，但子能力 chi_tra 语言包 missing（实测见 rt05-probes/probe-av-ocr.md missing 清单）。合计 16 项：15 available + 1 missing。

## 5. 缓存与失效策略

### 5.1 指纹公式（冻结）

```text
fingerprint = sha256( canonical_json({
    bin_sha256,        // executable 真实 ELF/脚本的全量 sha256（wrapper 场景另含 target_sha256）
    version,           // normalized 版本串
    capset,            // capabilities 排序后 canonical 序列化（如 encoders 清单/语言包清单/格式清单）
}) ) 前 16 位十六进制
```

- 本批取证记录的是二进制 sha256 **前 16 位**（仅 archive.7z wrapper 有全值），完整指纹由 RT-1 Capability Provider 首探时以全量 sha256 重算生成；本目录不预生成、不编造具体指纹值。
- wrapper 形态（7z/7za/7zr、soffice）的指纹必须同时覆盖 wrapper 文件与其 exec 目标（实测见 rt05-probes/probe-archive-meta.md 异常 #1、rt05-probes/probe-office-convert.md 异常 #1）。

### 5.2 缓存层级与 TTL（建议值，RT-1 实装时定稿）

| 层级 | 缓存内容 | 生命周期 |
|---|---|---|
| L0/L1（静态+版本） | 路径、身份、包归属、版本 | 服务启动时轻量指纹校验；命中即用；TTL 建议 24h |
| L2（能力面） | encoders/语言包/格式清单/delegates/policy | 按需或后台重探；TTL 建议 7d；结果只读、短超时、输出上限 |
| L3（smoke test） | fixture 验证结论 | 仅安装/升级验收或工具首次启用时执行，不进常规缓存刷新（设计 §3.3） |

### 5.3 失效事件（变化即 stale 重探）

以下任一变化即将旧条目标记 stale 并重新验证（设计 §3.2）：

1. executable_path 指向的文件 sha256 变化（含 wrapper 与其 exec 目标任一）。
2. 包版本/包归属变化（pacman 升级，如 ffmpeg-full 第三方仓库滚动更新——实测见 rt05-probes/probe-av-ocr.md 异常 #2）。
3. L2 能力集合变化（如 tesseract 语言包增删、ImageMagick policy.xml 生效面变化、delegates.xml 变化）。
4. 探测失败或返回形态不符（五态区分：missing / permission_denied / timeout / unsupported_output / probe_failed）。
5. policy/config 文件 mtime+sha256 变化（/etc/ImageMagick-7/policy.xml、/etc/ImageMagick-7/delegates.xml）。

stale 条目不可供 Runtime Policy 决策使用；重探失败回退为对应错误态，不沿用旧结果（设计 §3.2"探测失败不等于软件不存在"）。

## 6. 与 Tool Registry 的职责边界声明

| 维度 | Capability Catalog（本目录 / RT-0.5-F Provider） | Tool Registry（现有） |
|---|---|---|
| 事实类型 | **环境事实**：宿主机有什么软件、什么版本、什么子能力、健康状态 | **工具事实**：哪些工具可调用、Manifest、input/output schema、ToolRun |
| 数据单位 | CapabilityEntry（capability_id 如 media.ffmpeg） | Manifest（tool id 如 rip.doc.parse） |
| 变更主体 | 宿主软件安装/升级/卸载（用户或受控部署流程） | 工具登记/迁移（RT-4 逐项裁决） |
| 消费方 | Tool Engine / Runtime Policy Resolver（RT-3）/ RT-1 Adapter 能力解析 | Agent/Workflow/Tool Engine |

- 不建立第二套工具注册、运行记录、Artifact 或 Evidence 数据库（设计 §8；策略 §4.2）。
- 目录条目不携带工具语义（无 input_schema/output_schema/ToolRun），Registry 条目不携带环境探测事实；两者通过"工具 → 所需能力（capability_ref）"单向引用连接（契约见 [RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md](RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md) §6）。
- Runtime 迁移决策（RT-4）引用目录作为证据输入，但不因目录存在就自动改写 Manifest。
