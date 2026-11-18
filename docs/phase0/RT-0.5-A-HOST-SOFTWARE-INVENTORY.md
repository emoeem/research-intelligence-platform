# RT-0.5-A：宿主软件能力盘点总表

- 日期：2026-11-18
- 状态：已定稿（RT-0.5-A）。证据全部来自 `rt05-probes/` 五份 2026-11-18 只读实测取证；本批未安装/升级/卸载软件，未改 Manifest/Adapter/Cargo/数据库
- 上游设计：[RT-0.5-HOST-CAPABILITY-DISCOVERY.md](RT-0.5-HOST-CAPABILITY-DISCOVERY.md) §3.1/§3.2/§3.3/§7；策略依据 [NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md](NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md) §12
- 原始取证：`rt05-probes/probe-av-ocr.md`、`rt05-probes/probe-office-convert.md`、`rt05-probes/probe-pdf-image.md`、`rt05-probes/probe-archive-meta.md`、`rt05-probes/probe-path-audit.md`

## 1. 盘点口径裁决

1. **候选 ≠ 现状。** 设计 §1 的软件候选清单只定义"探测什么"；本表每一行都来自本机只读实测回显，不存在把候选写成已安装的条目（设计 §7 第 1 条验收）。
2. **16 项口径。** 五份取证共登记 17 个 capability 段落；其中 `image.convert` 与 `image.magick` 是同一文件（`/usr/bin/convert -> magick` symlink，sha256 相同，实测见 rt05-probes/probe-pdf-image.md #7），不构成独立能力，按别名并入 `image.magick`（弃用警告与 IM6 缺位记入异常清单 §5.2）。因此独立登记 **16 项：15 available + 1 missing**（`pdf.mutool`）。
3. **探测只读。** 全部探测命令 `timeout 5s`，未处理用户文件、未访问网络、未触发下载（实测见 rt05-probes/probe-av-ocr.md 头注、rt05-probes/probe-office-convert.md 头注、rt05-probes/probe-archive-meta.md 头注）。
4. **路径归一。** PATH 命中多为 `/usr/sbin/*`，经 `readlink -f` 归一为 `/usr/bin/*`（Arch usrmerge，`/usr/sbin -> bin`），非独立副本（实测见 rt05-probes/probe-path-audit.md #3、rt05-probes/probe-av-ocr.md #2）。

## 2. 16 项能力 8 字段总表

| # | capabilityId | executablePath | version | packageProvenance | binaryIdentity（sha256 前 16 位 · 类型） | permissions | status | capabilityProbe 摘要 |
|---|---|---|---|---|---|---|---|---|
| 1 | media.ffmpeg | /usr/bin/ffmpeg | 9.0.2 | ffmpeg-full 9.0.2-3.46（第三方仓库构建，非官方 core/extra 命名） | `09190fb791fb0db8` · ELF 64-bit LSB pie executable, x86-64, dynamically linked, stripped | root:root 755 | available | 248 个 encoders；关键 encoder 含 libx264/libx265/libopenh264/h264_nvenc/h264_vaapi/aac/libfdk_aac/flac/libopus/libvorbis 等 27 行命中；muxers 含 mp4/matroska/webm；build 含 GPL+version3+nonfree+NVENC/CUDA/VAAPI/Vulkan（实测见 rt05-probes/probe-av-ocr.md #3 #4 #5 #6） |
| 2 | media.ffprobe | /usr/bin/ffprobe | 9.0.2 | ffmpeg-full 9.0.2-3.46 | `a2ae3d0209aec7399` · ELF pie executable, x86-64, stripped | root:root 755 | available | `ffprobe -version` 正常：ffprobe 9.0.2，built with gcc 16，libavformat 63.1.102（实测见 rt05-probes/probe-av-ocr.md #7） |
| 3 | ocr.tesseract | /usr/bin/tesseract | 5.5.4 | tesseract 5.5.4-1.1 | `44e4c41d9986e8b2` · ELF pie executable, x86-64, stripped | root:root 755 | available | leptonica-1.87.0，AVX2/AVX/FMA/SSE4.1，OpenMP 202111；tessdata=/usr/share/tessdata/；语言包 3 个：chi_sim/eng/osd，**chi_tra 缺失**；语言包归属 tesseract-data-* 2:4.1.0-5（实测见 rt05-probes/probe-av-ocr.md #8 #9 #11） |
| 4 | document.libreoffice | /usr/lib/libreoffice/program/soffice（sh wrapper）；真实二进制同目录 soffice.bin | LibreOffice 26.8.2.2 680(Build:2) | libreoffice-fresh 26.8.2-3 | wrapper `3d585422d7cdddf9` · POSIX shell script；soffice.bin `baa182393db76be0` · ELF pie executable（BuildID 6c007680…） | root:root 755（wrapper 与 soffice.bin 同） | available | `--version` 正常；headless 可用（`--help` 第 57 行 `--headless`；第 120/142/144 行显示 `--convert-to`/`--print-to-file`/`--cat` 均 implies headless；仅凭 --help 判定，未启动实例）；Writer（libswlo/libswuilo）、Calc（libsclo/libscuilo）、Impress（libsdlo/libsduilo）组件 .so 全部在位（实测见 rt05-probes/probe-office-convert.md #3 #4 #5 #6） |
| 5 | convert.pandoc | /usr/bin/pandoc | pandoc 3.11（Features: +server +lua，Lua 5.4） | pandoc-cli 3.11-87 | `effb83b40a825d86` · ELF 64-bit LSB shared object（GHC 产物常态，非 pie executable） | root:root 755 | available | 输入格式 51 条、输出格式 76 条；双侧含 docx/html/latex/markdown（另有 markdown 变体、html4/html5、chunkedhtml、biblatex 等）（实测见 rt05-probes/probe-office-convert.md #7 #9 #10） |
| 6 | pdf.pdftotext | /usr/bin/pdftotext | 26.11.0 | poppler 26.11.0-1.1 | `f85872f94fb67433` · ELF pie executable（BuildID c4a62615…） | root:root 755 | available | `pdftotext -v` → pdftotext version 26.11.0 / Copyright 2005-2026 The Poppler Developers（实测见 rt05-probes/probe-pdf-image.md #4） |
| 7 | pdf.pdfinfo | /usr/bin/pdfinfo | 26.11.0 | poppler 26.11.0-1.1 | `3bdf68a79b9a3c22` · ELF pie executable（BuildID fa9767fc…） | root:root 755 | available | `pdfinfo -v` → pdfinfo version 26.11.0（实测见 rt05-probes/probe-pdf-image.md #4） |
| 8 | pdf.pdftoppm | /usr/bin/pdftoppm | 26.11.0 | poppler 26.11.0-1.1 | `ab0ef92375b618b8` · ELF pie executable（BuildID 21070f47…） | root:root 755 | available | `pdftoppm -v` → pdftoppm version 26.11.0（实测见 rt05-probes/probe-pdf-image.md #4） |
| 9 | pdf.mutool | — | — | unknown（`pacman -Q mupdf mupdf-tools` 两包均未找到） | — | — | **missing** | 无探测对象；`command -v mutool` 无输出 exit 1，/usr/bin/mutool 不存在（实测见 rt05-probes/probe-pdf-image.md #1 #11） |
| 10 | image.magick（别名 /usr/bin/convert → magick） | /usr/bin/magick | ImageMagick 7.1.2-35 Q16-HDRI x86_64 acaf7e418:20261031 | imagemagick 7.1.2.35-1.1 | `ff050139cbc6dbd9` · ELF pie executable（BuildID 15fe31bd…）；convert 为 symlink lrwxrwxrwx，同文件同 hash | root:root 755 | available | Features: Cipher DPC HDRI Modules OpenCL OpenMP；内建 delegates 含 bzlib/cairo/djvu/heic/jp2/jpeg/jxl/openexr/png/raw/rsvg/tiff/webp/wmf 等；PDF/PS delegate 实际调 `gs`（带 -dSAFER），gs 10.08.0 在位；doc/docx/odt/pptx/xlsx 等 office delegate 调 `libreoffice --convert-to pdf`；policy.xml 空壳（唯一生效 Undefined:none）（实测见 rt05-probes/probe-pdf-image.md #5 #8 #9 #10 #12） |
| 11 | archive.7z | /usr/bin/7z（sh wrapper，exec /usr/lib/7zip/7z） | 7-Zip 26.06 (x64)，Igor Pavlov，2026-11-06 | 7zip 26.06-1.1 | wrapper `f802383f13b8331a`（全值 f802383f13b8331aabaf779def00a356c8d2b8148255170b75b03028f4d23ad4）· POSIX shell script；真实 ELF /usr/lib/7zip/7z `d48e622e9009acdb` · ELF pie | root:root 755 | available | `7z i`：Libs `0 : 26.06 : /usr/lib/7zip/7z.so`；格式 7z/zip/tar/**Rar/Rar5**/gzip/xz/bzip2/zstd 全部在列，另有 APFS/Arj/Cab/Chm/Dmg/ISO/NSIS/PE 等约 30 种识别格式（实测见 rt05-probes/probe-archive-meta.md #2 #4 #5 #6 #7） |
| 12 | archive.7za | /usr/bin/7za（sh wrapper，exec /usr/lib/7zip/7za） | 7-Zip (a) 26.06 (x64) | 7zip 26.06-1.1 | wrapper `4566c6e82f8147dc` · POSIX shell script | root:root 755 | available | Formats = 7z/Cab/Split/bzip2/gzip/lzma/lzma86/tar/xz/zip/zstd/Hash；**无 Rar**（独立构建无 Rar 解码）（实测见 rt05-probes/probe-archive-meta.md #8） |
| 13 | archive.7zr | /usr/bin/7zr（sh wrapper，exec /usr/lib/7zip/7zr） | 7-Zip (r) 26.06 (x64)，Public domain | 7zip 26.06-1.1 | wrapper `1c5de14f5ac7ac74` · POSIX shell script | root:root 755 | available | Formats 仅 7z/Split/lzma/lzma86/xz/Hash；Codecs AES256CBC/7zAES/LZMA/LZMA2/Delta/Copy；无 zip/tar/rar（实测见 rt05-probes/probe-archive-meta.md #8） |
| 14 | archive.bsdtar | /usr/bin/bsdtar | bsdtar 3.8.11 · libarchive 3.8.11（zlib-ng/lzma 5.8.3/bz2/lz4 1.11.1/zstd 1.5.7/xml2/openssl 4.1.0） | libarchive 3.8.11-2.1 | `0081c8f3e9820eae` · ELF pie executable | root:root 755 | available | `--formats` 选项不存在（`bsdtar: Option --formats is not supported`，exit 1）；create 模式 `--format {ustar\|pax\|cpio\|shar}`；读侧格式集由 libarchive 后端提供，无格式清单子命令（实测见 rt05-probes/probe-archive-meta.md #9 #10） |
| 15 | archive.unzip | /usr/bin/unzip | UnZip 6.00 of 20 April 2009, by Info-ZIP | unzip 6.0-23.1 | `15f733fefed8de50` · ELF pie executable | root:root 755 | available | 无参数用法首行 banner 正常（实测见 rt05-probes/probe-archive-meta.md #11） |
| 16 | meta.exiftool | /usr/bin/vendor_perl/exiftool | 13.55 | perl-image-exiftool 13.55-1 | `80e0018c48f93244` · Perl script text executable | root:root **555** | available | `exiftool -ver` → 13.55，与包版本一致；**PATH 审计判定所在目录 /usr/bin/vendor_perl 不在受信前缀集合**（目录本身 root 属主 755 不可写）（实测见 rt05-probes/probe-archive-meta.md #12 #14、rt05-probes/probe-path-audit.md #6） |

## 3. 探测方法与分层对应（设计 §3.3）

| 层级 | 方法 | 本批实测落点 |
|---|---|---|
| L0 静态发现 | 受控路径查找（`command -v` + `readlink -f` 归一）、`file`/`stat`/`sha256sum` 身份、`pacman -Qo` 包归属、组件文件 `ls` | 全部 16 项的路径/权限/hash/包归属均 L0 实测（实测见 rt05-probes/probe-av-ocr.md #1 #2 #10 #12；rt05-probes/probe-office-convert.md #1 #2 #4 #11；rt05-probes/probe-pdf-image.md #1 #2 #3 #7 #13；rt05-probes/probe-archive-meta.md #1 #3 #4 #5 #14；rt05-probes/probe-path-audit.md #2 #3 #6） |
| L1 轻量探测 | 固定版本参数：`--version`/`-v`/`-ver`/无参 banner | ffmpeg/ffprobe/tesseract/soffice/pandoc/poppler 三命令/magick/convert/7z 系/bsdtar/unzip/exiftool/gs 全部 L1 通过（实测见 rt05-probes/probe-av-ocr.md #3 #7 #8；rt05-probes/probe-office-convert.md #3 #7；rt05-probes/probe-pdf-image.md #4 #5 #6 #12；rt05-probes/probe-archive-meta.md #2 #9 #11 #12） |
| L2 能力探测 | 只读清单命令：`ffmpeg -encoders/-muxers`、`tesseract --list-langs`、`pandoc --list-input/output-formats`、`7z i`/`7za i`/`7zr i`、`magick -list delegate/-list policy`、`soffice --help`（组件与 headless 面） | 实测见 rt05-probes/probe-av-ocr.md #4 #5 #6 #9；rt05-probes/probe-office-convert.md #5 #6 #9 #10；rt05-probes/probe-pdf-image.md #8 #9 #10；rt05-probes/probe-archive-meta.md #6 #7 #8 #10 |
| L3 功能验证 | 内置 fixture 无副作用 smoke test | **本批未执行**（设计 §3.3：仅安装/升级验收或工具首次启用时运行，不得在每次服务启动时运行） |

探测纪律（设计 §3.2）落实情况：探测命令固定、参数固定、无 Shell 拼接、每条 `timeout 5s`、只读、不处理用户文件、不联网；未信任请求参数提供的路径，全部经 `command -v`/`readlink -f` 归一为绝对路径后验证（实测见五份 probe 头注与各 evidence 清单）。

## 4. 缺失清单

| 项 | 状态 | 成熟替代 | 证据 |
|---|---|---|---|
| pdf.mutool（mutool / MuPDF 工具面） | missing：mutool 不在 PATH，mupdf、mupdf-tools 均未安装 | 文本抽取走 `pdf.pdftotext`、元数据走 `pdf.pdfinfo`、页面渲染走 `pdf.pdftoppm`（poppler 26.11.0 三命令全部 available）与 gs 10.08.0（ImageMagick PDF/PS delegate 依赖在位）；Poppler 已覆盖 MuPDF 在本目录的核心用途，不构成静默降级 | 实测见 rt05-probes/probe-pdf-image.md missing 清单、#4 #11 #12 |
| ocr.tesseract chi_tra 语言包 | 缺失：`tesseract-data-chi_tra` 未安装（仅 chi_sim/eng/osd 在位）；繁体中文 OCR 不可用 | eng/chi_sim/osd 已在位；chi_tra 补装属**用户决策**，Discovery 只报告不安装（设计 §5：用户明确授权后才可安装） | 实测见 rt05-probes/probe-av-ocr.md missing 清单、#9 #11 |

无其他二进制缺失：ffmpeg/ffprobe/tesseract/soffice/pandoc/poppler 三命令/magick/7z 系/bsdtar/unzip/exiftool 全部存在且探测通过（实测见 rt05-probes/probe-av-ocr.md missing 清单、rt05-probes/probe-office-convert.md missing 清单、rt05-probes/probe-archive-meta.md missing 清单）。

## 5. 异常清单

### 5.1 指定必录异常（五项）

| # | 异常 | 事实 | 影响 | 证据 |
|---|---|---|---|---|
| A1 | 7z/7za/7zr 均为 sh 包装脚本而非 ELF | `/usr/bin/{7z,7za,7zr}` 为 `#!/bin/sh` + `exec /usr/lib/7zip/{7z,7za,7zr} "$@"`；binaryIdentity 记录的是 wrapper 哈希，真实 ELF 在 `/usr/lib/7zip/`（7z/7za/7zr/7zCon.sfx/7z.so 五文件），archive.7z 真实 ELF sha256 前 16 位 `d48e622e9009acdb` | 指纹/权限审计必须以 wrapper 指向的真实 ELF 为准，否则升级校验落空；路径穿越/炸弹防护见 [RT-0.5-E-RISK-AND-ISOLATION.md](RT-0.5-E-RISK-AND-ISOLATION.md) §4.3 | 实测见 rt05-probes/probe-archive-meta.md 异常 #1、#4 #5 |
| A2 | convert 是 IM7 弃用别名而非 IM6 | `/usr/bin/convert -> magick` symlink（lrwxrwxrwx），`convert -version` 先输出 deprecation WARNING 再给出与 magick 完全一致的 7.1.2-35 版本行；本机无独立 ImageMagick 6（/etc/ImageMagick-6/policy.xml、/etc/ImageMagick/policy.xml 均不存在） | 调用 convert 得到 IM7 行为 + 弃用警告；混用 IM6 语义（如 -density/-units 差异）有风险；Adapter 层应统一走 `magick` | 实测见 rt05-probes/probe-pdf-image.md 异常 #1、#6 #7 |
| A3 | exiftool 位于非受信前缀且为 Perl 脚本 | 实际路径 /usr/bin/vendor_perl/exiftool，PATH 审计 candidateTrust 判 `trusted=NO`（不在 /usr/local/bin、/usr/bin 等受信集合内；目录本身 root 属主 755 不可写）；权限 555、Perl script text executable 而非 ELF | 调用必须固定绝对路径 + 包归属校验，禁止裸 PATH 解析；不把"非受信前缀"误判为"目录被劫持"（该目录 root 属主、不可写，残留面见 §5.3） | 实测见 rt05-probes/probe-path-audit.md #6、rt05-probes/probe-archive-meta.md 异常 #5、#12 #14 |
| A4 | ImageMagick 安全策略为空壳 | /etc/ImageMagick-7/policy.xml 存在（644, 8138 字节）但 PDF/PS/SVG/MVG/URL/XPS 等高危 coder 的模块限制、resource 限额、path 限制全部在注释内；唯一生效行为第 83 行 `<policy domain="Undefined" rights="none"/>`；`magick -list policy` 实测仅 Undefined:none 两条（/etc/ImageMagick-7/policy.xml + [built-in]） | PDF/PS coder 与 gs delegate 均未禁用，URL/HTTP/MSL/MVG 等历史高危面默认开放；宿主 magick 处理不可信输入无 policy 兜底，只能依赖 gs 自身 -dSAFER → 不可信图像/PDF 禁走宿主 magick 直连（裁决见 [RT-0.5-E-RISK-AND-ISOLATION.md](RT-0.5-E-RISK-AND-ISOLATION.md) §4.1） | 实测见 rt05-probes/probe-pdf-image.md 异常 #2、#9 #10 |
| A5 | ffmpeg-full 为第三方仓库构建 | `pacman -Qo /usr/bin/ffmpeg` → `ffmpeg-full 9.0.2-3.46`；`-3.46` 修订号非 Arch 官方 ffmpeg 包命名；build 带 nonfree（libfdk_aac）+ NVENC/CUDA/VAAPI/Vulkan 全开 | provenance 明确非 unknown，但来源仓库非官方 core/extra → 纳入 fingerprint 监控与 stale 重探范围（缓存策略见 [RT-0.5-C-CAPABILITY-CATALOG.md](RT-0.5-C-CAPABILITY-CATALOG.md) §5） | 实测见 rt05-probes/probe-av-ocr.md 异常 #2 #4、#10 |

### 5.2 补充异常（实测发现，非指定必录）

| # | 异常 | 证据 |
|---|---|---|
| A6 | /usr/sbin → /usr/bin usrmerge：15 个候选名在 /usr/sbin:/usr/bin:/sbin:/bin 四目录同时命中，经 `readlink`/`stat -L` 验证为同一物理目录的 symlink 别名，非真实双副本；真实跨物理目录 shadow 零命中 | 实测见 rt05-probes/probe-path-audit.md shadowHits 表、#3 #4 #5 |
| A7 | soffice 为多级符号链接落点上的 shell wrapper，真实二进制是 soffice.bin；指纹/权限审计应以 soffice.bin 为准 | 实测见 rt05-probes/probe-office-convert.md 异常 #1、#2 #4 |
| A8 | ffmpeg 原生 `opus`/`vorbis` encoder 带 experimental 标记（`A..X.D`）；生产编码应用 libopus/libvorbis | 实测见 rt05-probes/probe-av-ocr.md 异常 #3 |
| A9 | pandoc file 类型为 ELF shared object 而非 pie executable（GHC 产物常态），仅记录 | 实测见 rt05-probes/probe-office-convert.md 异常 #2 |
| A10 | bsdtar 无 `--formats` 清单子命令，格式探测不适用；读侧格式集由 libarchive 3.8.11 提供 | 实测见 rt05-probes/probe-archive-meta.md 异常 #4、#10 |
| A11 | exiftool 权限 555（perl-image-exiftool 包正常行为），与其余 755 不同 | 实测见 rt05-probes/probe-archive-meta.md 异常 #5、#14 |
| A12 | PATH 残留面：4 个 emo 属主可写目录（~/.local/bin、~/.cargo/bin、pnpm/bin、flatpak exports）排在受信目录之前；5 个 PATH 元素位于 /tmp/.mount_zcode.0pveme 前缀（root 属主 755，当前用户不可写）；空 PATH 元素 0、cwd 与 /tmp 本身不在 PATH | 实测见 rt05-probes/probe-path-audit.md verdict、#1 #2 #8 |
| A13 | p7zip 无独立包：`pacman -Q p7zip` 经 provides/虚拟依赖解析到 `7zip 26.06-1.1`；banner 为 `7-Zip 26.06`（Igor Pavlov 官方移植）而非 p7zip 17.x | 实测见 rt05-probes/probe-archive-meta.md 异常 #2、#13 |
| A14 | ImageMagick delegate 表含 libreoffice 转 PDF 与 curl https:decode、c2patool、darktable-cli(dng) 等命令面；放开图像管道需评估 delegate 参数注入面 | 实测见 rt05-probes/probe-pdf-image.md 异常 #3、#5 #8 |

## 6. 原始取证索引

| 取证文件 | 覆盖 capabilityId | 状态 |
|---|---|---|
| rt05-probes/probe-av-ocr.md | media.ffmpeg、media.ffprobe、ocr.tesseract | 3 available；chi_tra 缺失 |
| rt05-probes/probe-office-convert.md | document.libreoffice、convert.pandoc | 2 available |
| rt05-probes/probe-pdf-image.md | pdf.pdftotext、pdf.pdfinfo、pdf.pdftoppm、pdf.mutool、image.magick（含 image.convert 别名） | 5 available + 1 missing |
| rt05-probes/probe-archive-meta.md | archive.7z、archive.7za、archive.7zr、archive.bsdtar、archive.unzip、meta.exiftool | 6 available |
| rt05-probes/probe-path-audit.md | （横切）PATH 26 项审计、shadow、candidateTrust | 全部能力路径可信性依据 |

## 7. 验收对照（设计 §7 相关条目）

- 结论全部来自本机只读实测，候选软件未冒充已安装：§1 裁决 1，§2 表 9 行 missing 如实标记。
- 每项能力记录绝对路径、版本、包来源与能力探测结果：§2 表（8 字段齐备）。
- 路径不信任、PATH 劫持、权限异常有明确状态：§5.1 A3、§5.2 A6/A11/A12；探测失败区分五态见 [RT-0.5-C-CAPABILITY-CATALOG.md](RT-0.5-C-CAPABILITY-CATALOG.md) §4（本批无 probe_failed）。
- 七类关键软件全部检查、缺失如实标记：§2/§4（FFmpeg、LibreOffice、Tesseract、Poppler/MuPDF、Pandoc、ImageMagick、归档工具；MuPDF 缺失已如实标记并给替代）。
- 未安装/升级/卸载软件、未改系统配置、未改 Manifest/Adapter/Cargo/数据库：头注状态行。
