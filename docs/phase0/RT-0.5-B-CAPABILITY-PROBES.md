# RT-0.5-B：能力探测详情

- 日期：2026-11-18
- 状态：已定稿（RT-0.5-B）。所有数字为 rt05-probes/ 五份取证中的实测回显原文，未编造、未补测；探测命令每条 timeout 5s
- 上游设计：[RT-0.5-HOST-CAPABILITY-DISCOVERY.md](RT-0.5-HOST-CAPABILITY-DISCOVERY.md) §3.4 关键软件能力断言
- 总表见 [RT-0.5-A-HOST-SOFTWARE-INVENTORY.md](RT-0.5-A-HOST-SOFTWARE-INVENTORY.md)；本文件展开"能力面"细节

## 1. media.ffmpeg — 编码器与封装器面（设计 §3.4 断言：不能只检测命令存在）

- **版本与构建**：`ffmpeg version 9.0.2`；build configuration 首段 `--prefix=/usr --enable-lto --disable-rpath --enable-gpl --enable-version3 --enable-nonfree …`；关键 enable：`--enable-libx264 --enable-libx265 --enable-libfdk-aac --enable-libopus --enable-libvorbis --enable-cuda-nvcc --enable-nvenc --enable-vaapi --enable-vulkan`（实测见 rt05-probes/probe-av-ocr.md #3）。
- **encoders 总数**：`ffmpeg -encoders | grep -c '^ [VAS]'` → **248**（实测见 rt05-probes/probe-av-ocr.md #4）。
- **关键 encoder（264/265/hevc/aac/flac/vorbis/opus 共 27 行命中）**：libx264、libx264rgb、libopenh264、h264_nvenc、h264_amf、h264_qsv、h264_v4l2m2m、h264_vaapi、h264_vulkan、h264_rkmpp、libx265、libsvt_hevc、hevc_nvenc、hevc_amf、hevc_qsv、hevc_v4l2m2m、hevc_vaapi、hevc_vulkan、hevc_rkmpp、libkvazaar、aac、libfdk_aac、flac、opus（原生，标记 experimental `A..X.D`）、libopus、vorbis（原生，experimental）、libvorbis（实测见 rt05-probes/probe-av-ocr.md #5）。
- **muxers**：`E mp4`、`E matroska`、`E webm`（另有 ipod、psp、webm_chunk、webm_dash_manifest）（实测见 rt05-probes/probe-av-ocr.md #6）。
- **生产约束**：原生 opus/vorbis 为 experimental，生产编码用 libopus/libvorbis（实测见 rt05-probes/probe-av-ocr.md 异常 #3）。

## 2. media.ffprobe — 媒体探测面

- `ffprobe -version` → ffprobe version 9.0.2，built with gcc 16，libavformat 63.1.102（实测见 rt05-probes/probe-av-ocr.md #7）。
- 用途定位：媒体容器/流探测（demux 面未在本批单独清点，demuxer/muxer 全量清单留待 RT-1 按需 L2 重探）。

## 3. ocr.tesseract — 语言包面（设计 §3.4 断言：语言数据缺失要明确报告）

- **版本**：tesseract 5.5.4 / leptonica-1.87.0；指令集 AVX2/AVX/FMA/SSE4.1；OpenMP 202111（实测见 rt05-probes/probe-av-ocr.md #8）。
- **tessdata 路径**：/usr/share/tessdata/。
- **已安装语言包：共 3 个** —— `--list-langs` → `List of available languages in "/usr/share/tessdata/" (3): chi_sim eng osd`（实测见 rt05-probes/probe-av-ocr.md #9）。
- **chi_tra 缺失**：繁体中文 OCR 不可用；语言包归属：eng/osd/chi_sim 均由 `tesseract-data-* 2:4.1.0-5` 拥有（`pacman -Qo /usr/share/tessdata/chi_sim.traineddata` → tesseract-data-chi_sim 2:4.1.0-5）（实测见 rt05-probes/probe-av-ocr.md #11）。补装属用户决策，Discovery 只报告不安装（设计 §5）。

## 4. document.libreoffice — headless 与组件面（设计 §3.4 断言：禁并发共享 profile，运行时用隔离临时 profile）

- **版本**：`LibreOffice 26.8.2.2 680(Build:2)`，包 libreoffice-fresh 26.8.2-3（实测见 rt05-probes/probe-office-convert.md #3 #4）。
- **headless 判定**：`soffice --help` 第 57 行含 `--headless  Starts in "headless mode"…`；第 120/142/144 行显示 `--convert-to`/`--print-to-file`/`--cat` 均 implies `--headless` → headless 转换面可用。**仅凭 --help 判定，未启动 soffice 实例、未执行任何转换**（取证头注声明）（实测见 rt05-probes/probe-office-convert.md #6）。
- **组件面（program 目录，8 路径全部存在）**：
  - Writer：libswlo.so + libswuilo.so
  - Calc：libsclo.so + libscuilo.so
  - Impress：libsdlo.so + libsduilo.so
  - soffice wrapper + soffice.bin
  （实测见 rt05-probes/probe-office-convert.md #5）
- **真实二进制**：/usr/lib/libreoffice/program/soffice.bin（wrapper 为 POSIX shell script）；指纹/权限审计以 soffice.bin 为准（实测见 rt05-probes/probe-office-convert.md 异常 #1、#2 #4）。
- L3 未做：headless 转换 smoke test 留待 RT-1 工具首次启用时以内置 fixture 执行（设计 §3.3）。

## 5. Poppler 26.11.0 — 三命令面（设计 §3.4 断言：具体命令、版本和渲染/抽取能力）

- 三命令同一包 poppler 26.11.0-1.1，版本行一致 26.11.0（实测见 rt05-probes/probe-pdf-image.md #3 #4）：

| 命令 | 用途 | 版本探测 |
|---|---|---|
| pdftotext | PDF 文本抽取 | `pdftotext -v` → pdftotext version 26.11.0 / Copyright 2005-2026 The Poppler Developers |
| pdfinfo | PDF 元数据/页数 | `pdfinfo -v` → pdfinfo version 26.11.0 |
| pdftoppm | PDF 页面渲染（位图） | `pdftoppm -v` → pdftoppm version 26.11.0 |

- **pdf.mutool 缺位**：MuPDF 工具面缺失，Poppler 三命令 + gs 承担其职责（缺失裁决见 [RT-0.5-D-TOOL-MAPPING.md](RT-0.5-D-TOOL-MAPPING.md) §4.1）（实测见 rt05-probes/probe-pdf-image.md #1 #11）。

## 6. image.magick — delegate 与安全策略实况（设计 §3.4 断言：版本、delegate/codec 能力与安全策略限制）

### 6.1 版本与 codec 面

- `magick -version` → Version: ImageMagick 7.1.2-35 Q16-HDRI x86_64 acaf7e418:20261031；Features: Cipher DPC HDRI Modules OpenCL OpenMP；Delegates (built-in): bzlib cairo djvu fftw fontconfig freetype heic jbig jng jp2 jpeg jxl lcms lqr ltdl lzma openexr pangocairo png raqm raw rsvg tiff uhdr webp wmf x xml zip zlib zstd；Compiler: gcc (16.2)（实测见 rt05-probes/probe-pdf-image.md #5）。

### 6.2 delegate 实况（delegates.xml 位于 /etc/ImageMagick-7/delegates.xml）

- **PDF/PS 全部由 gs 承担**：`magick -list delegate` 中 eps<=>pdf、eps<=>ps、pdf<=>eps、pdf<=>ps、ps<=>pdf 均调 `'gs'`，命令带 `-sstdout=%%stderr -dQUIET -dSAFER -dBATCH -dNOPAUSE … -sDEVICE=pdfwrite`；gs 实际在位：/usr/bin/gs，版本 **10.08.0**（实测见 rt05-probes/probe-pdf-image.md #8 #12）。
- **office 系转 PDF 由 libreoffice 承担**：doc/docx/emf/epub/odg/odt/ppt/pptx/vsd/vsdx/wmf/xls/xlsx 均调 `libreoffice --convert-to pdf`（实测见 rt05-probes/probe-pdf-image.md #5 capabilityProbe）。
- **注入面提示**：内置 delegate 表另含 curl https:decode、c2patool、darktable-cli（dng）等命令面；放开图像管道需评估 delegate 参数注入面（实测见 rt05-probes/probe-pdf-image.md 异常 #3）。

### 6.3 policy.xml 空壳实况

- /etc/ImageMagick-7/policy.xml 存在（644, 8138 字节）。
- **PDF/PS 未被禁用**：模块限制示例（MSL,MVG,PDF,PS,SVG,TXT,URL,XPS → write-only）位于第 189-190 行**注释内**。
- **全文件唯一生效行**：第 83 行 `<policy domain="Undefined" rights="none"/>`；其余 `<policy` 行均在注释块/示例中。
- `magick -list policy` 实测生效策略仅 Undefined:none（/etc/ImageMagick-7/policy.xml 与 [built-in] 各一条）（以上实测见 rt05-probes/probe-pdf-image.md #9 #10、异常 #2）。
- 裁决后果：宿主 magick 处理不可信 PDF/PS/图像时无 policy 兜底，仅依赖 gs 自身 -dSAFER → 不可信输入禁走宿主 magick 直连（见 [RT-0.5-E-RISK-AND-ISOLATION.md](RT-0.5-E-RISK-AND-ISOLATION.md) §4.1）。

## 7. 归档工具 — 7z/7za/7zr/bsdtar/unzip 格式面（设计 §3.4 断言：列目录与解包能力分开探测，路径穿越/符号链接/压缩炸弹独立防护）

### 7.1 三套 7-Zip 格式差异（Rar 仅 7z）

| 能力 | archive.7z | archive.7za | archive.7zr |
|---|---|---|---|
| 版本 | 7-Zip 26.06 (x64) | 7-Zip (a) 26.06 (x64) | 7-Zip (r) 26.06 (x64) |
| Formats | 7z/zip/tar/**Rar/Rar5**/gzip/xz/bzip2/zstd 全部在列，另有 APFS/APM/Ar/Arj/Cab/Chm/Compound/Cpio/CramFS/Dmg/ELF/Ext/FAT/ISO/HFS/NTFS/NSIS/PE 等约 30 种识别格式 | 7z/Cab/Split/bzip2/gzip/lzma/lzma86/tar/xz/zip/zstd/Hash（**无 Rar**） | 仅 7z/Split/lzma/lzma86/xz/Hash（无 zip/tar/rar） |
| Codecs | （经 7z.so 外挂库） | — | AES256CBC/7zAES/LZMA/LZMA2/Delta/Copy |
| 形态 | sh wrapper → /usr/lib/7zip/7z（ELF，sha256 `d48e622e9009acdb`） | sh wrapper → /usr/lib/7zip/7za | sh wrapper → /usr/lib/7zip/7zr |

（实测见 rt05-probes/probe-archive-meta.md #2 #4 #5 #6 #7 #8、异常 #1 #3）

- 裁决：Rar 解码能力仅 archive.7z 具备（经 /usr/lib/7zip/7z.so 外挂编解码库）；7za 用于无 Rar 需求的轻量场景；7zr 仅 7z 原生格式。列目录（`7z l`）与解包（`7z x`）在 RT-1 Adapter 中分开授权，本批未解包任何归档（取证头注）。

### 7.2 bsdtar

- bsdtar 3.8.11 - libarchive 3.8.11（zlib/1.3.1.zlib-ng liblzma/5.8.3 bz2lib/1.0.8 liblz4/1.11.1 libzstd/1.5.7 libxml2/2.15.4 openssl/4.1.0）（实测见 rt05-probes/probe-archive-meta.md #9）。
- **`--formats` 选项不存在**：`bsdtar: Option --formats is not supported`（exit 1）→ 无格式清单子命令，L2 格式探测对 bsdtar 不适用；create 模式可用格式仅 `--format {ustar|pax|cpio|shar}`；读侧格式集由 libarchive 后端提供（实测见 rt05-probes/probe-archive-meta.md #10、异常 #4）。

### 7.3 unzip

- UnZip 6.00 of 20 April 2009, by Info-ZIP；包 unzip 6.0-23.1（实测见 rt05-probes/probe-archive-meta.md #11）。

## 8. convert.pandoc — 格式转换面

- pandoc 3.11（Features: +server +lua，Scripting engine Lua 5.4）；包 pandoc-cli 3.11-87（实测见 rt05-probes/probe-office-convert.md #7 #8）。
- **输入格式 51 条**：`--list-input-formats | wc -l` → 51；抽样含 docx、html、latex、markdown（另有 markdown_github/markdown_mmd/markdown_phpextra/markdown_strict/biblatex）（实测见 rt05-probes/probe-office-convert.md #9 #10）。
- **输出格式 76 条**：`--list-output-formats | wc -l` → 76；抽样含 docx、html/html4/html5、latex、markdown（另有 chunkedhtml/biblatex 等）（实测见 rt05-probes/probe-office-convert.md #9 #10）。

## 9. meta.exiftool — 元数据面

- exiftool 13.55（`-ver`），包 perl-image-exiftool 13.55-1；Perl script text executable；权限 555（实测见 rt05-probes/probe-archive-meta.md #12 #14）。
- 读取面（图片/音视频/文档元数据）为设计 §1 预期用途；本批仅完成版本与身份探测，未对样本文件读元数据（取证头注：只读探测）。

## 10. PATH 26 项审计摘要（横切面，全部能力路径可信性的依据）

- **规模**：PATH 共 26 项，空元素 0 个；UID=1000(emo)（实测见 rt05-probes/probe-path-audit.md 头注、#1）。
- **受信前缀**：/usr/local/bin、/usr/sbin、/usr/bin、/sbin、/bin 共 5 项（后四者经 symlink 验证同为 /usr/bin 物理目录，deref 755 root:root）（实测见 rt05-probes/probe-path-audit.md #3 #5）。
- **可写位**：全部 26 项有效权限中 other/group 位无写位；唯一写位来自 4 个 emo 属主目录的 owner 位（~/.local/bin、~/.cargo/bin、pnpm/bin、flatpak exports/bin，均 755 ownerW），且排在受信目录之前（标准用户 bin 设计，需先持有 emo 身份方可利用）（实测见 rt05-probes/probe-path-audit.md #2、verdict）。
- **/tmp 面**：/tmp 本身与 cwd 不在 PATH；5 个 PATH 元素位于 /tmp/.mount_zcode.0pveme 前缀（AppImage/squashfs 挂载，root 属主 755，当前用户不可写）（实测见 rt05-probes/probe-path-audit.md #1 #8）。
- **shadow**：15 个候选名（ffmpeg/ffprobe/tesseract/soffice/pandoc/pdftotext/pdfinfo/pdftoppm/magick/convert/7z/7za/7zr/bsdtar/unzip）按字面在 4 目录命中构成 potential shadow，但验证为同一物理目录 /usr/bin 的 symlink 别名；**真实跨物理目录 shadow 零命中**（实测见 rt05-probes/probe-path-audit.md shadowHits 表、#4）。
- **candidateTrust**：16 个候选名中 15 个解析到受信路径；**exiftool → /usr/bin/vendor_perl/exiftool 判 trusted=NO**（目录 root 属主 755 不可写，但不在受信前缀集合）；mutool NOT_FOUND（实测见 rt05-probes/probe-path-audit.md candidateTrust 表、#6 #7）。
- **verdict**：探测面基本不受 PATH 劫持影响；残留面仅上述用户可写目录排序与 /tmp 挂载元素。裁决：Provider 实装时仍用受控 search roots（设计 §3.2），不裸信运行时 PATH（见 [RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md](RT-0.5-F-CAPABILITY-PROVIDER-CONTRACT.md) §4）。

## 11. 探测纪律声明

- 每条探测命令 `timeout 5s`；未转码、未解析、未解包、未启动 soffice 实例、未处理任何用户文件、未访问网络、未安装任何内容（实测见五份 probe 头注）。
- L3 功能验证本批未执行（设计 §3.3：仅安装/升级验收或工具首次启用时运行）。
- 探测失败五态区分（missing/permission_denied/timeout/unsupported_output/probe_failed）本批实测分布：仅 missing 命中（pdf.mutool、chi_tra 语言包）；无 permission_denied/timeout/unsupported_output/probe_failed 案例（五份 probe 的 missing/anomalies 清单）。
