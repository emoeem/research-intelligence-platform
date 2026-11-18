# RT-0.5 盘点原始取证 · 音视频与 OCR（0.5-A/B）

- 盘点员子代理实测，只读；主机 CachyOS (linux 7.3.4-1-cachyos-bore-lto)；2026-11-18
- 未转码/未解析/未安装任何内容；每条探测命令 timeout 5s

## media.ffmpeg

| 字段 | 值 |
|---|---|
| capabilityId | media.ffmpeg |
| executablePath | /usr/bin/ffmpeg（which 命中 /usr/sbin/ffmpeg，readlink -f 解析为 /usr/bin；/usr/sbin -> bin 符号链接） |
| version | 9.0.2 |
| packageProvenance | ffmpeg-full 9.0.2-3.46（pacman -Qo 确认，非官方 core/extra 命名，第三方仓库构建） |
| binaryIdentity | sha256 前 16 位 `09190fb791fb0db8`；ELF 64-bit LSB pie executable, x86-64, dynamically linked, stripped |
| permissions | root:root 755 |
| status | available |
| capabilityProbe | build configuration 前 40 字符：`--prefix=/usr --enable-lto --disable-rpa`（关键 enable：--enable-gpl --enable-version3 --enable-nonfree --enable-libx264 --enable-libx265 --enable-libfdk-aac --enable-libopus --enable-libvorbis --enable-cuda-nvcc --enable-nvenc --enable-vaapi --enable-vulkan）；encoders 总数 248；含 264/265/hevc/aac/flac/vorbis/opus 的 encoder 共 27 行：libx264、libx264rgb、libopenh264、h264_nvenc、h264_amf、h264_qsv、h264_v4l2m2m、h264_vaapi、h264_vulkan、h264_rkmpp、libx265、libsvt_hevc、hevc_nvenc、hevc_amf、hevc_qsv、hevc_v4l2m2m、hevc_vaapi、hevc_vulkan、hevc_rkmpp、libkvazaar、aac、libfdk_aac、flac、opus（原生，标记 experimental `A..X.D`）、libopus、vorbis（原生，experimental）、libvorbis；muxers 命中：mp4、matroska、webm（另有 ipod、psp、webm_chunk、webm_dash_manifest） |

## media.ffprobe

| 字段 | 值 |
|---|---|
| capabilityId | media.ffprobe |
| executablePath | /usr/bin/ffprobe |
| version | 9.0.2 |
| packageProvenance | ffmpeg-full 9.0.2-3.46 |
| binaryIdentity | sha256 前 16 位 `a2ae3d0209aec7399`；ELF 64-bit LSB pie executable, x86-64, dynamically linked, stripped |
| permissions | root:root 755 |
| status | available |
| capabilityProbe | `ffprobe -version` 正常：ffprobe version 9.0.2，built with gcc 16，libavformat 63.1.102 |

## ocr.tesseract

| 字段 | 值 |
|---|---|
| capabilityId | ocr.tesseract |
| executablePath | /usr/bin/tesseract |
| version | 5.5.4 |
| packageProvenance | tesseract 5.5.4-1.1 |
| binaryIdentity | sha256 前 16 位 `44e4c41d9986e8b2`；ELF 64-bit LSB pie executable, x86-64, dynamically linked, stripped |
| permissions | root:root 755 |
| status | available |
| capabilityProbe | `--version`：tesseract 5.5.4 / leptonica-1.87.0，指令集 AVX2/AVX/FMA/SSE4.1，OpenMP 202111；tessdata 路径 /usr/share/tessdata/；`--list-langs` 全部语言包（3）：chi_sim、eng、osd。**chi_tra 缺失**；语言包归属：eng/osd/chi_sim 均由 tesseract-data-* 2:4.1.0-5 拥有 |

## missing 清单

- tesseract 简繁中文仅 chi_sim 已装，`tesseract-data-chi_tra` 未安装（繁体中文 OCR 不可用）；eng、chi_sim、osd 均在位。
- 无二进制 missing：ffmpeg / ffprobe / tesseract 全部存在且探测通过。

## anomalies 清单

- PATH 命中路径为 /usr/sbin/*：因 /usr/sbin -> bin 符号链接，真实路径 /usr/bin/*，非权限或版本异常，仅记录避免盘点口径分歧。
- 包归属为 ffmpeg-full（9.0.2-3.46 的 `-3.46` 修订号非 Arch 官方 ffmpeg 包命名，属第三方仓库构建），provenance 明确非 unknown，仅备注来源仓库非官方 core/extra。
- ffmpeg 原生 `opus` 与 `vorbis` encoder 带 experimental 标记（`A..X.D`）；生产编码建议用 libopus / libvorbis。
- ffmpeg 9.0.2 系带 nonfree（libfdk_aac）+ NVENC/CUDA/VAAPI/Vulkan 全开的 full 构建，能力高于官方 ffmpeg 包，符合音视频管道需求。
- 无权限异常（三个二进制均 root:root 755）、无包归属 unknown、无输出异常。

## evidence 清单

1. `which ffmpeg ffprobe tesseract` → `/usr/sbin/ffmpeg`、`/usr/sbin/ffprobe`、`/usr/sbin/tesseract`
2. `readlink -f /usr/sbin/ffmpeg /usr/sbin/ffprobe /usr/sbin/tesseract` → `/usr/bin/ffmpeg`、`/usr/bin/ffprobe`、`/usr/bin/tesseract`
3. `ffmpeg -version` → `ffmpeg version 9.0.2`；configuration 首段 `--prefix=/usr --enable-lto --disable-rpath --enable-gpl --enable-version3 --enable-nonfree ...`
4. `ffmpeg -encoders | grep -c '^ [VAS]'` → `248`
5. `ffmpeg -encoders | grep -Ei '(264|265|hevc|aac|flac|vorbis|opus)'` → 27 行，含 `libx264`、`libx265`、`aac`、`libfdk_aac`、`flac`、`libopus`、`libvorbis` 等
6. `ffmpeg -muxers | grep -Ei '(mp4|matroska|webm)'` → `E mp4`、`E matroska`、`E webm`（另有 webm_chunk、webm_dash_manifest）
7. `ffprobe -version` → `ffprobe version 9.0.2 Copyright (c) 2007-2026`
8. `tesseract --version` → `tesseract 5.5.4` / `leptonica-1.87.0`
9. `tesseract --list-langs` → `List of available languages in "/usr/share/tessdata/" (3): chi_sim eng osd`
10. `pacman -Qo /usr/bin/ffmpeg` → `ffmpeg-full 9.0.2-3.46`；`pacman -Qo /usr/bin/tesseract` → `tesseract 5.5.4-1.1`
11. `pacman -Qo /usr/share/tessdata/chi_sim.traineddata` → `tesseract-data-chi_sim 2:4.1.0-5`
12. `sha256sum /usr/bin/ffmpeg` → `09190fb791fb0db81fb2616a59b9c4a1...`；`stat -c '%U:%G %a' /usr/bin/ffmpeg` → `root:root 755`（三二进制一致）

## 复现

```
which ffmpeg ffprobe tesseract; ffmpeg -version | head -1; ffmpeg -encoders | grep -c '^ [VAS]'; ffprobe -version | head -1; tesseract --version | head -1; tesseract --list-langs | head -5; pacman -Qo /usr/bin/ffmpeg /usr/bin/tesseract
```
预期回显：三路径在位、`ffmpeg version 9.0.2`、`248`、`ffprobe version 9.0.2`、`tesseract 5.5.4`、`chi_sim eng osd`、`ffmpeg-full 9.0.2-3.46` 与 `tesseract 5.5.4-1.1`。
