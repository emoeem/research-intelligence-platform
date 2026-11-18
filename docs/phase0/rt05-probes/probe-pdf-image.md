# RT-0.5 盘点原始取证 · PDF 与图像（0.5-A/B）

- 盘点员子代理实测，只读；主机 CachyOS；2026-11-18
- 未解析任何用户 PDF/图片；每条探测命令 timeout 5s

## pdf.pdftotext

| 字段 | 值 |
|---|---|
| capabilityId | pdf.pdftotext |
| executablePath | /usr/bin/pdftotext（command -v 得 /usr/sbin/pdftotext，readlink -f 解析） |
| version | 26.11.0 |
| packageProvenance | poppler 26.11.0-1.1（pacman -Qo 命中） |
| binaryIdentity | f85872f94fb67433 / ELF 64-bit LSB pie executable, x86-64, dynamically linked, stripped, BuildID c4a626152c128bcfa8555a168515094dc606b813 |
| permissions | root:root 755 |
| status | available |
| capabilityProbe | `pdftotext -v` → "pdftotext version 26.11.0 / Copyright 2005-2026 The Poppler Developers" |

## pdf.pdfinfo

| 字段 | 值 |
|---|---|
| capabilityId | pdf.pdfinfo |
| executablePath | /usr/bin/pdfinfo（同上解析自 /usr/sbin/pdfinfo） |
| version | 26.11.0 |
| packageProvenance | poppler 26.11.0-1.1 |
| binaryIdentity | 3bdf68a79b9a3c22 / ELF 64-bit LSB pie executable, x86-64, dynamically linked, stripped, BuildID fa9767fc01522ba7d14a3ecb69282271fd91c74a |
| permissions | root:root 755 |
| status | available |
| capabilityProbe | `pdfinfo -v` → "pdfinfo version 26.11.0" |

## pdf.pdftoppm

| 字段 | 值 |
|---|---|
| capabilityId | pdf.pdftoppm |
| executablePath | /usr/bin/pdftoppm（同上解析自 /usr/sbin/pdftoppm） |
| version | 26.11.0 |
| packageProvenance | poppler 26.11.0-1.1 |
| binaryIdentity | ab0ef92375b618b8 / ELF 64-bit LSB pie executable, x86-64, dynamically linked, stripped, BuildID 21070f4767d77e63466711a1942930eb6b4e52bf |
| permissions | root:root 755 |
| status | available |
| capabilityProbe | `pdftoppm -v` → "pdftoppm version 26.11.0" |

## pdf.mutool

| 字段 | 值 |
|---|---|
| capabilityId | pdf.mutool |
| executablePath | 无（command -v mutool 无输出，exit 1；/usr/bin/mutool 不存在） |
| version | — |
| packageProvenance | unknown（pacman -Q mupdf mupdf-tools → 两包均未找到） |
| binaryIdentity | — |
| permissions | — |
| status | missing |
| capabilityProbe | 无探测对象；未编造任何版本信息 |

## image.magick

| 字段 | 值 |
|---|---|
| capabilityId | image.magick |
| executablePath | /usr/bin/magick |
| version | ImageMagick 7.1.2-35 Q16-HDRI x86_64 acaf7e418:20261031 |
| packageProvenance | imagemagick 7.1.2.35-1.1 |
| binaryIdentity | ff050139cbc6dbd9 / ELF 64-bit LSB pie executable, x86-64, dynamically linked, stripped, BuildID 15fe31bdcd68fcb49b5b8178b2860dc0eaff5cd6 |
| permissions | root:root 755 |
| status | available |
| capabilityProbe | `magick -version` → Version: ImageMagick 7.1.2-35 Q16-HDRI；Features: Cipher DPC HDRI Modules OpenCL OpenMP；Delegates (built-in): bzlib cairo djvu fftw fontconfig freetype heic jbig jng jp2 jpeg jxl lcms lqr ltdl lzma openexr pangocairo png raqm raw rsvg tiff uhdr webp wmf x xml zip zlib zstd；Compiler: gcc (16.2)。`magick -list delegate` 前 20 行如实记录：delegates.xml 位于 /etc/ImageMagick-7/delegates.xml，含 blender/bpg/c2pa/clipboard/dng(darktable-cli)/doc…docx/dot/dvi/emf 等条目；PDF/PS 面实际由 `gs` 承担（eps<=>pdf、eps<=>ps、pdf<=>eps、pdf<=>ps、ps<=>pdf 均调 'gs'，带 -dSAFER），office 系 doc/docx/emf/epub/odg/odt/ppt/pptx/vsd/vsdx/wmf/xls/xlsx 均调 'libreoffice --convert-to pdf' |
| policy | /etc/ImageMagick-7/policy.xml 存在（644, 8138 字节）；**PDF/PS 未被禁用**：模块限制示例（MSL,MVG,PDF,PS,SVG,TXT,URL,XPS → write-only）位于第 189-190 行注释内；全文件唯一生效行是第 83 行 `<policy domain="Undefined" rights="none"/>`（其余 `<policy` 行均在注释块/示例中）；`magick -list policy` 实测生效策略仅 Undefined:none（/etc/ImageMagick-7/policy.xml + [built-in] 各一条）。delegate 'gs' 所需 Ghostscript 实际在位：/usr/bin/gs，版本 10.08.0 |

## image.convert

| 字段 | 值 |
|---|---|
| capabilityId | image.convert |
| executablePath | /usr/bin/convert → symlink → /usr/bin/magick（ls -la: `convert -> magick`） |
| version | ImageMagick 7.1.2-35 Q16-HDRI（与 magick 同一二进制） |
| packageProvenance | imagemagick 7.1.2.35-1.1（pacman -Qo /usr/bin/convert 命中） |
| binaryIdentity | 与 magick 同文件（symlink，sha256 同 ff050139cbc6dbd9…） |
| permissions | symlink lrwxrwxrwx root:root；目标 root:root 755 |
| status | available（IM6 不存在，见 anomalies） |
| capabilityProbe | `convert -version` → 输出前有 WARNING "The convert command is deprecated in IMv7, use \"magick\" instead of \"convert\" or \"magick convert\""，随后版本与 delegate 信息与 magick 完全一致（7.1.2-35 Q16-HDRI） |

## missing 清单

- pdf.mutool：mutool 不在 PATH；mupdf、mupdf-tools 均未安装（pacman 查询报"软件包未找到"）

## anomalies 清单

1. **convert 不是 IM6**：/usr/bin/convert 是 IM7 的 deprecated 兼容 symlink 指向 magick，本机无独立 ImageMagick 6（/etc/ImageMagick-6/policy.xml、/etc/ImageMagick/policy.xml 均不存在）。调用 convert 会得到 IM7 行为 + 弃用 WARNING，混用 IM6 语义（如 -density/-units 差异）需注意。
2. **ImageMagick 安全策略为空壳**：/etc/ImageMagick-7/policy.xml 中针对 PDF/PS/SVG/MVG/URL/XPS 等高危 coder 的模块限制、resource 限额、path 限制全部处于注释状态，唯一生效的是 `Undefined: none`；即 PDF/PS coder 与 gs delegate 均未禁用，且 URL/HTTP/MSL/MVG 等历史高危面默认开放（HTTP coder 的 none 限制也只是注释示例）。magick 处理不可信 PDF/PS 时依赖 gs 自身 -dSAFER。
3. **PDF 渲染链完整但分散**：magick PDF 走外部 gs 10.08.0（delegates.xml 命令带 -dSAFER -dBATCH -dNOPAUSE）；同时内置 delegate 表含 libreoffice 转 PDF 与 curl https:decode 等命令面，若放开图像管道需评估这些 delegate 的参数注入面（如 c2patool/darktable-cli）。
4. poppler 三命令与 magick/convert 均由 /usr/sbin 入口（PATH 别名）解析到 /usr/bin，属 Arch 系 usrbin 合并布局，非异常。

## evidence 清单

1. `command -v pdftotext pdfinfo pdftoppm mutool magick convert` → `/usr/sbin/pdftotext /usr/sbin/pdfinfo /usr/sbin/pdftoppm /usr/sbin/magick /usr/sbin/convert`（mutool 无行，单独查 exit 1）
2. `readlink -f /usr/sbin/pdftotext /usr/sbin/pdfinfo /usr/sbin/pdftoppm /usr/sbin/magick /usr/sbin/convert` → `/usr/bin/pdftotext /usr/bin/pdfinfo /usr/bin/pdftoppm /usr/bin/magick /usr/bin/magick`
3. `pacman -Qo /usr/bin/pdftotext /usr/bin/pdfinfo /usr/bin/pdftoppm /usr/bin/magick` → 均命中：poppler 26.11.0-1.1 ×3，imagemagick 7.1.2.35-1.1 ×1
4. `pdftotext -v` → `pdftotext version 26.11.0`（pdfinfo/pdftoppm 同版本行）
5. `magick -version` → `Version: ImageMagick 7.1.2-35 Q16-HDRI x86_64 acaf7e418:20261031`；Delegates (built-in): bzlib cairo djvu … zstd
6. `convert -version` → `WARNING: The convert command is deprecated in IMv7…` + 同一 7.1.2-35 版本行
7. `ls -la /usr/bin/convert` → `lrwxrwxrwx 1 root root 6 ... /usr/bin/convert -> magick`
8. `magick -list delegate | grep -iE 'pdf|gs|ps:'` → `eps<=>pdf  "'gs' -sstdout=%%stderr -dQUIET -dSAFER -dBATCH -dNOPAUSE ... -sDEVICE=pdfwrite ..."`（及 pdf<=>ps / ps<=>pdf / pdf<=>eps 同型 gs 命令）
9. `grep -n '<policy ' /etc/ImageMagick-7/policy.xml` → 唯一未注释行 `83: <policy domain="Undefined" rights="none"/>`（PDF 限制示例在 189-190 行注释内）
10. `magick -list policy` → 仅 `Policy: Undefined / rights: None`（/etc/ImageMagick-7/policy.xml 与 [built-in] 各一条）
11. `pacman -Q mupdf mupdf-tools` → `错误：软件包 'mupdf' 未找到 / 错误：软件包 'mupdf-tools' 未找到`（exit 1）；`ls /usr/bin/mutool` → 不存在
12. `gs --version` → `10.08.0`（magick PDF/PS delegate 依赖在位）
13. `stat -c '%U:%G %a %n' /usr/bin/pdftotext /usr/bin/pdfinfo /usr/bin/pdftoppm /usr/bin/magick` → 四者均 `root:root 755`
