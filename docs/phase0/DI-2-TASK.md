# DI-2 任务书：Document Router 三通道扩全（OCR / 媒体 / 归档两阶段）+ 负例矩阵扩全

- 日期：2026-11-20
- 状态：任务书已定稿（DI-2 设计裁决员产出；实现批次开工即按本任务书执行）
- 依据：DI-2 批次指派（三通道设计 + 负例矩阵扩全 + fixture 策略）；[DI-1-TASK.md](DI-1-TASK.md)（既有面与纪律模板）；[DI-1-ADR-DOC-ROUTER.md](DI-1-ADR-DOC-ROUTER.md) D1-D8；[RT-0.5-E-RISK-AND-ISOLATION.md](RT-0.5-E-RISK-AND-ISOLATION.md) §2 十四项 / §4.3 归档两阶段 / §4.5 ffmpeg 特化；[RT-0.5-B-CAPABILITY-PROBES.md](RT-0.5-B-CAPABILITY-PROBES.md) §1/§2/§3/§7；[RT-0.5-C-CAPABILITY-CATALOG.md](RT-0.5-C-CAPABILITY-CATALOG.md) §3.1/3.2/3.3/3.11 security_profile；[RT-1-ADR-NATIVE-ADAPTER.md](RT-1-ADR-NATIVE-ADAPTER.md) D3/D7/D8；[DI-1-BATCH-REPORT.md](DI-1-BATCH-REPORT.md) H 节 DI-2 扩展缝与 G 节遗留
- 设计裁决：本任务书不重复论证，实现口径一律以 [DI-2-ADR-CHANNELS.md](DI-2-ADR-CHANNELS.md) D1 到 D8 为准
- 前置：DI-1 已收口（47+5+20+42+11 测试面全绿 / lock 400 / vendor 零 diff，DI-1-BATCH-REPORT §A）；五 CLI 在位且钉值实测吻合（本任务书 §2.2 复测）

## 1. 目标

把 Router 通道面从 DI-1 三通道（pdf/office/image）扩全到六通道（+ocr/media/archive），每通道真实 run 经 tool-engine 引擎全链，负例矩阵按指派八类扩全：

1. **OCR 通道**：图像输入（`Png | Jpeg` 嗅探 + 显式 OCR 请求位）→ identify 门步（像素帽 50 MP）→ tesseract（chi_sim/eng 按 catalog 语言路由；zh-Hant/chi_tra 请求前置门显式 error 禁静默回退；TSV 置信度与语言入统一输出 `metadata.ocr`）。
2. **媒体通道**：ffprobe 元数据常步（时长/编码/分辨率/码率 → 时长 300s / 单帧 16 MP 门）+ ffmpeg 受限抽帧条件步（fps=2 / `-frames:v 32` / `-c:v png` 全常量；experimental encoder 禁用——原生 opus/vorbis 面 known_limits 登记）；纯音频 metadata-only + 诚实 warnings。
3. **归档通道**：7z 两阶段——先 `l -slt` 列目校验四闸（路径穿越/绝对路径/符号链接/条目数 512/解压总量 256 MiB+单条目 64 MiB）→ 再 `x -y -oout` 解包到本 run 独立 out/ + post-extract 符号链接终检。
4. **负例矩阵扩全**（§5 表 12 类）：畸形图像 / 超大图像 / 畸形媒体 / 超时长媒体 / 音频无视频流 / 穿越路径归档 / 压缩炸弹模拟 / zip 符号链接条目 / 条目数超帽 / 不支持格式 / chi_tra 请求 / 能力 missing。

不目标（本批不做）：PDF 渲染 OCR（pdftoppm 链，DI-3）；Rar/tar/gzip/xz 家族归档路由（嗅探门 fail-closed，DI-3）；音频转写；统一输出外层 facade run 信封化（DI-3）；不明来源输入的隔离档位路由（RT-3，红线 10）。

## 2. 现状锚点（2026-11-20 实测；代码为只读实读，CLI 为 /tmp/di2-probe 本机实测，样本 = 仓库入库 fixtures + 测试内构造）

### 2.1 代码锚点

| 锚点 | 事实 | 出处 |
|---|---|---|
| 通道枚举 | `Channel { Pdf, Office, Image }` 三值；`channel_steps` 为对账镜像非编排来源，扩展「两处同改」纪律 | crates/document-router/src/channel.rs:18-38、:8-16、:80-117 |
| 嗅探判定序 | `%PDF-` / PNG / JPEG / ZIP 条目扫描 / 全量 UTF-8 / unknown；`ZipOther => None`（DI-2 改路由点）；ZIP 扫描上限 256 条目 + 4 MiB fail-closed | sniff.rs:173-194、:76、:104-106 |
| 前置门 | bytes > 64 MiB → `ROUTER_INPUT_TOO_LARGE`；无通道签名 → `ROUTER_UNSUPPORTED_TYPE`；均零 run 行 | router.rs:170-186 |
| 子 run 驱动 | drive_step/drive_step_at（预写 in/ → submit_correlated 自锚定 → execute_native_rust_via_engine）+ read_stdout 回读 | router.rs:483-577 |
| 链级失败 | terminal_reject → ChainOutcome::failed，已存子 run 行保持真实终态 | router.rs:582-597、:618-640 |
| 错误映射 | RouterReject 构造器族 + 九类词表防线（词表外 category panic）；code 自由面 ≤128 透传 | error.rs:105-156 |
| 帽常量 | MAX_INPUT_FILE_BYTES 64 MiB / MAX_PAGES 2000 / MAX_PIXELS 100MP / SUBRUN_MAX_OUTPUT_BYTES 1 MiB | channel.rs:125-137 |
| 像素解析 | ImageMeta::pixel_count()（缺尺寸返回 None 不臆测）；PdfinfoMeta 页数缺席 → warnings 零静默 | normalize.rs:139-143；router.rs:255-266 |
| 槽位语法 | `{{slot}}` 整 token 精确匹配；混合 token 字面量透传；`{{input_path}}` in/ 前缀校验 | native_rust.rs:736-800、:1405-1415 |
| cwd 实锚 | 子进程 cwd = ws.root（`resource_command(..., &ws.root)` → `cmd.current_dir`）；out/ 预创建 | native_rust.rs:1133、:1051、:305 |
| wrapper 静态核验 | verify_wrapper_target：`exec <abs>` 唯一目标 + sha256 对照 binary_target_sha256，不符 NATIVE_IDENTITY_MISMATCH | native_rust.rs:592-640 |
| dispatch 守卫 | native_rust 分支承载范围 = RouterManifests.contains(tool_id)；stat 前置帽在 fs::read 之前 | dispatch.rs:295-346 |
| fixture 钉值面 | 13 件语料 sha256 逐件钉值 + oversized-pixels.png 形态前提单测（11000x11000=121MP 完整 PNG） | tests/fixture_pins.rs:39-98、:102-126 |
| 判据 4 现行断言 | epub/zip/evil.zip 嗅探门拒绝零 run 行——**DI-2 按行为变更迁移**（ADR D1） | three_channel_e2e.rs:558-569 |

### 2.2 本机 CLI 实测（/tmp/di2-probe，2026-11-20；样本 = 入库 fixtures + 探针内构造，零网络）

| 探针 | 命令（原文要点） | 实测回显要点 |
|---|---|---|
| 钉值复测 | `sha256sum /usr/bin/tesseract /usr/bin/ffprobe /usr/bin/ffmpeg /usr/bin/pdftoppm /usr/bin/7z /usr/lib/7zip/7z` | `44e4c41d…c35d0` / `a2ae3d02…38eff1` / `09190fb7…1708889` / `ab0ef923…8afcb` / `f802383f…23ad4` / `d48e622e…902fad9`（与 seed 前缀逐一吻合） |
| tesseract 语言包 | `tesseract --list-langs` | 3 个：chi_sim eng osd；tessdata=/usr/share/tessdata/；版本 5.5.4/leptonica-1.87.0 |
| tesseract 文本面 | `magick -size 460x80 xc:white -font …/LiberationSans-Regular.ttf -annotate +0+0 "HELLO OCR 123" text.png` + `tesseract text.png stdout` | exit 0；stdout `HELLO OCR 123` |
| tesseract TSV 面 | `tesseract text.png stdout tsv` | exit 0；header `level page_num block_num par_num line_num word_num left top width height conf text`；词行 level=5 conf 浮点（92.203033/92.232201/96.249260，均值 93.6） |
| tesseract 组合语言 | `tesseract text.png stdout -l chi_sim+eng` | exit 0 `HELLO OCR 123` |
| tesseract 缺失语言包 | `tesseract text.png stdout -l chi_tra` | **exit 1**；stderr `Error opening data file /usr/share/tessdata/chi_tra.traineddata` / `Failed loading language 'chi_tra'` |
| tesseract 空白图 | `magick xc:white blank.png` + `tesseract blank.png stdout` | exit 0 零词（诚实空结果） |
| tesseract 双配置 | `tesseract text.png out txt tsv` | 写 out.txt + out.tsv 两文件（cwd 形态，ADR D2 否决依据） |
| ffprobe WAV | 手工字节造 8044B PCM WAV + `ffprobe -v error -print_format json -show_format -show_streams tone.wav` | exit 0；format.duration `"0.500000"`（**字符串**）、bit_rate `"128704"`、format_name `"wav"`；streams[0] codec_name `pcm_s16le`、sample_rate `"8000"`、channels 1（**整数**） |
| ffprobe MP4 | `ffmpeg -f lavfi -i testsrc=duration=2:size=128x64:rate=10 -c:v libx264 -pix_fmt yuv420p small.mp4` + ffprobe | exit 0；duration `"2.000000"`、format_name `"mov,mp4,m4a,3gp,3g2,mj2"`（逗号列表）、codec h264、width/height **整数** 128/64、nb_frames `"20"`（字符串） |
| ffprobe 畸形输入 | `printf 'not audio at all\n' > fake.wav` + ffprobe | **exit 1** `Invalid data found when processing input` |
| ffmpeg 自动选型 | `ffmpeg -f lavfi -i testsrc… small.mp4`（无 -c:v） | **自动选 h264_rkmpp → MPP init 失败，exit 1，零字节产物**（显式编码器铁律实测铁证） |
| ffmpeg 受限抽帧 | `ffmpeg -hide_banner -loglevel error -i small.mp4 -vf fps=2 -vframes 3 -c:v png -y fr/frame-%03d.png` | exit 0；**恰 3 帧**（帽生效）；identify 确认 `PNG 128x64` |
| ffmpeg 音频抽帧 | `ffmpeg -i tone.wav -vf fps=1 -vframes 2 -y fr2-%03d.png` | **exit 234** `Output file does not contain any stream` |
| 7z 列目 | `7z a -tzip demo.zip a.txt b.txt` + `7z l -ba` / `7z l -slt demo.zip` | a exit 0；-ba 裸行；-slt 头块 `Path/Type = zip/Physical Size` + `----------` + 条目块 `Path/Folder = +/-/Size/Attributes` |
| 7z 解包 -o 矩阵 | `7z x -y -oex1 demo.zip` / `-o ex1`（空格）/ `-o=ex3` / `-oout`（相对） | 附连 **exit 0**；空格 **exit 7**；`-o=` **造出字面 `=ex3` 目录**；相对 `-oout` **exit 0 产物落 out/** |
| 7z 穿越条目列表 | python zipfile 造 `../escape.txt` + `7z l -slt` | `Path = ../escape.txt` **逐字原样**（闸 1 判定输入完整可得） |
| 7z 绝对路径条目 | zipfile 造 `/tmp/di2-probe/z/abs-escape.txt` | 列表逐字原样；`7z x` 自身落位 target 下相对路径（第二道非契约） |
| 7z 穿越条目解包 | `7z x -y -ott evil2.zip` | 7z 自身剥离前导段落位 `tt/escape.txt`（第二道防线，Router 闸 1 仍唯一权威） |
| 压缩炸弹清单 | zipfile 造 64 MiB zeros（compresslevel=9）+ `7z l -slt bomb.zip` | zip 仅 65348 B；条目块 **`Size = 67108864`** / Packed 65232（闸 4 判定输入） |
| zip 符号链接条目 | zipfile external_attr 0o120777 + `7z l -slt symlink.zip` | 链接条目 **`Attributes =  lrwxrwxrwx`**；普通文件 `V 01800000 …`；目录 `D …` + `Folder = +` |
| tar 符号链接条目 | python tarfile SYMTYPE + `7z l -slt links.tar` | **`Symbolic Link = /etc/passwd`** 字段形态（闸 2 双判据输入） |
| 畸形归档 | `printf 'garbage\n' > fake.zip` + `7z l -slt` | **exit 2**（EXECUTION 透传） |
| 7z wrapper 形态 | `cat /usr/bin/7z` | 全文两行：`#!/bin/sh` + `exec /usr/lib/7zip/7z "$@"`（**静态形态**，ADR D7 双钉前提） |
| 魔数 hexdump | `xxd` demo.7z / small.mp4 / v.mkv / tone.wav / a.txt.gz / t.tar | `377a bcaf 271c` / `…6674 7970`（@4 ftyp）/ `1a45 dfa3` / `RIFF…WAVE`（@8）/ `1f8b`（不路由）/ `ustar`@257（不路由） |

## 3. 范围

### 3.1 允许改动（新增/修改落点）

| 落点 | 内容 | 步骤 |
|---|---|---|
| `crates/document-router/src/sniff.rs` | 八新签名判定（7z/wav/avi/mp4/webm/ogg/flac/mp3）+ `ZipOther → Archive` 路由变更 + 通道归属表更新 | S2 |
| `crates/document-router/src/channel.rs` | `Channel` 增 Ocr/Media/Archive + `channel_steps` 对账镜像扩行（两处同改纪律）+ 帽常量新增（OCR_PIXELS 50MP / MEDIA_DURATION 300s / MEDIA_FRAME_PIXELS 16MP / MEDIA_FRAMES 32 / ARCHIVE_ENTRIES 512 / ARCHIVE_TOTAL_BYTES 256Mi / ARCHIVE_ENTRY_BYTES 64Mi / OCR_LANG_ROUTES 语言路由表） | S2 |
| `crates/document-router/src/router.rs` | route_ocr/route_media/route_archive 三管线（复用 drive_step/read_stdout/前置门）+ OCR 请求位入口（route() 零改动）+ 列目四闸 + post-extract symlink 终检 | S3 |
| `crates/document-router/src/normalize.rs` | FfprobeMeta/TessTsvMeta/ArchiveListMeta 解析与 metadata 投影 + TSV 正文重组规则 | S3 |
| `crates/document-router/src/error.rs` | D6 新码构造器（ROUTER_OCR_LANG_MISSING / ROUTER_MEDIA_* / ROUTER_ARCHIVE_* / ARCHIVE_LIST_NO_OUTPUT） | S3 |
| `crates/document-router/tests/*` | 四套既有测试按行为变更修订（channel_gate 表行/帽值、sniff_boundary 新签名与 zip_other 迁移、three_channel_e2e 判据 4 迁移 + 新通道 e2e、fixture_pins 新钉值） | S5 |
| `crates/document-router/tests/fixtures/` | 新增钉值 fixture（§4 fixture 策略表） | S5 |
| `manifests/tools/rip.doc.image.ocr.json`、`rip.media.probe.json`、`rip.media.frames.json`、`rip.archive.list.json`、`rip.archive.extract.json`（新增） | D2/D3/D4 表五 Manifest（绝对路径 + 64hex 钉值 + archive 双钉 binary_target_sha256 + capability_ref + argv_template + result_mode:"file" + resources + network:none） | S4 |
| `manifests/capabilities/host-catalog-seed.json` | 四条 sha256_full 实测回填（ocr.tesseract / media.ffprobe / media.ffmpeg / archive.7z wrapper_target） | S4 |
| `crates/mcp-server/src/dispatch.rs` | native_rust 分支 OCR 请求位透传（input.ocr → route_ocr）+ 守卫消息九工具清单更新 | S3 |
| `docs/phase0/DI-2-BATCH-REPORT.md` + `docs/README.md` | 批次报告 + 索引登记行 | S8 |

### 3.2 不修改

- `crates/tool-engine/`（adapter/capability/envelope 全零 diff）：DI-2 全部落在 D3/D4/D7 既有面（result_mode file / verify_wrapper_target 静态核验 / cwd=ws.root / prlimit 兜底），零新增 adapter 增量——DI-1 D4 封顶纪律延续。
- `crates/document-router/src/router.rs` 既有 route() 签名与三管线行为（零回归门 = DI-1 四套测试既有断言全绿，判据 4 断言按 ADR D1 登记的行为变更迁移除外）。
- `manifests/tools/rip.doc.{pdf.info,pdf.text,office.to_pdf,image.meta}.json`、`rip.doc.parse.json`、其余 15 份工具 Manifest、`migrations/`、`vendor/`、Cargo.lock 依赖面（本批零新成员零新外部依赖）。
- 生产 `platform*.db`、`platform-dryrun.db`、`integrations/rtis/`。

## 4. fixture 策略（指派要求：写明选择与理由）

**三层裁决：字节确定性归入库钉值；跨容器/时间戳不确定性归运行时生成（能力守卫 SKIP）；负例放大与畸形面归测试内构造。**

| 层 | fixture | 生成方式 | 理由 |
|---|---|---|---|
| ① 入库钉值（+ fixture_pins.rs sha256 逐件钉值，漂移即红） | `text-ocr.png`（460x80 白底黑字 HELLO OCR 123） | magick `label:` 等价面一次生成，生成后字节入库钉值 | OCR 正例锚：tesseract 词行/置信度断言锚定同一字节基线；字体渲染版本相关 → 不在测试内实时生成（换机器字体版本漂移），生成一次钉字节即跨机稳定 |
| ① | `tone.wav`（8044 B PCM 16-bit 单声道 8 kHz 0.5 s 440 Hz 正弦） | 手写字节（RIFF/fmt /data 头 + 正弦样本，探针已产出同构字节） | RIFF/WAVE 判定与 ffprobe JSON 形态锚；手工字节完全确定性，不依赖任何生成器版本 |
| ① | `evil2.zip`（ok.txt + `../escape.txt` + abs.txt，316 B） | python zipfile 一次生成（ZipInfo 缺省 date_time=1980-01-01 固定 → 字节确定），入库钉值 | 闸 1 穿越负例锚：条目名序列是断言对象本身，必须逐字节稳定；7z 无法创建穿越名条目（工具自身拒绝），只能程序化构造 |
| ① | `bomb.zip`（zeros.bin 解压 64 MiB，65348 B） | 同上（ZIP_DEFLATED compresslevel=9 + 64 MiB 零字节） | 闸 4 解压总量负例锚；小 zip 大 Size 是唯一可入库形态（解压产物不入库） |
| ① | `symlink.zip`（real.txt + link-to-etc.txt 符号链接条目） | 同上（external_attr 0o120777 + create_system=3） | 闸 2 zip 符号链接面锚：zip 符号链接形态只能程序化构造 |
| ② 运行时生成（capability 守卫 + SKIP 显式标注，DI-1 e2e 先例） | `demo.zip` / `demo.7z`（2 条目文本文件） | `7z a -tzip demo.zip …` / `7z a demo.7z …`（archive.7z） | 7z 容器内嵌创建时间戳 → 字节非确定，不可钉值；生成器 = 被测能力自身（7z a 面），守卫沿用 `require_capability` |
| ② | `small.mp4`（2 s 128x64 10fps） | `ffmpeg -f lavfi -i testsrc=… -c:v libx264 …`（media.ffmpeg） | mp4 容器时间戳/编码器元数据非确定；testsrc 内容确定（same host 同版本字节近稳）但容器层不定 → 行为断言（帧数/尺寸）不字节断言 |
| ② | `t.tar`（1 文件） | `bsdtar -cf t.tar a.txt`（archive.bsdtar） | 「不支持格式」负例：tar 不入嗅探表 → 嗅探门 INPUT 拒绝；bsdtar 生成证明负例真实性（真 tar 而非伪字节） |
| ③ 测试内构造（纯代码，零入库零下载） | 513 条目 zip（条目数帽负例） | Rust 测试内最小合法 zip 构造器（LFH + central directory + EOCD，~40 行，DI-1 multi_page_pdf 先例同形态） | 512+1 条目放大面；入库会是无意义大文件，运行时 7z a 造 513 文件又慢又脆 |
| ③ | 400 s WAV（8 kHz 16-bit 零 PCM，6.4 MB 内存字节不落盘全量） | 测试内手写字节组装（data 长度字段 = 6.4 MB，Rust Vec 直接组装） | 超时长媒体负例：ffprobe 只读头部算时长，零 PCM 合法；6.4 MB 内存态免磁盘放大 |
| ③ | 伪 PNG（魔数 + 垃圾体）/ 伪 WAV / gzip 字节 / 畸形 zip | 测试内字面字节 | 畸形面负例：嗅探后工具层拒绝路径，字面字节即精确 |

- **入库理由汇总**：fixture_pins.rs 纪律（评审 2 修订）——语料不入库 CI fresh checkout 必红；本批新增 5 件钉值语料全部为「断言对象 = 字节本身」或「生成器字节非确定但一次性生成后字节稳定」面。
- **运行时生成理由汇总**：7z/mp4 容器时间戳字段使字节钉值跨次生成即漂移（钉值纪律不成立）；DI-1 已有 magick 生成 JPEG + soffice 造超帽 PDF 先例，守卫/SKIP 语义沿用。
- **测试内构造理由汇总**：负例放大（条目数/时长）按定义是超帽形态，入库即把放大物带进仓库；DI-1 判据 8 的 2001 页 PDF 纯代码构造先例直接沿用。

## 5. 完成标准（判据表，S5 逐项有测试、S8 逐项有实测输出；负例为硬性红线）

| # | 判据 | 要求（负例为硬性红线） |
|---|---|---|
| 1 | OCR 通道端到端 | text-ocr.png + 请求 language=en → identify + tesseract 两子 run SUCCEEDED；统一输出 channel="ocr"、text="HELLO OCR 123"（TSV 重组）、metadata.ocr{language:"eng", requested_language:"en", words=3, mean_conf>90}、page_anchors 单页锚 |
| 2 | OCR 缺省语言 | 无请求位 → `-l chi_sim+eng`（目录双语言放行）；同一 fixture 文本一致；requested_language="auto" |
| 3 | chi_tra 请求显式 error | 请求 zh-Hant → 前置门 Err：category=DEPENDENCY、code=ROUTER_OCR_LANG_MISSING、**零 run 行**、消息注明禁静默回退与补装属用户决策；chi_sim 兜底面零出现 |
| 4 | OCR 像素帽 | oversized-pixels.png（121 MP）→ OCR 管线 identify 行 SUCCEEDED 留证 + 链内 FAILED `ROUTER_PIXEL_BUDGET_EXCEEDED`（消息含 121000000 与 50 MP OCR 帽）；tesseract 未拉起 |
| 5 | 畸形图像 | PNG 魔数 + 垃圾体 → identify 子 run exit!=0 → EXECUTION 透传 FAILED（子 run 行留证），tesseract 未拉起 |
| 6 | 媒体端到端（音频） | tone.wav → ffprobe 单子 run SUCCEEDED；metadata.media{format_name:"wav", duration_s≈0.5, streams 含 pcm_s16le}；warnings 记录 no-video-stream 分支；抽帧步未提交 |
| 7 | 媒体端到端（视频） | small.mp4（运行时生成）→ ffprobe + ffmpeg 两子 run；out/ 内帧 PNG（2s@fps2 = 4 帧 ≤ 32 帽）；metadata.frames{extracted, capped:false}；stdout.log JSON 有 format.duration |
| 8 | 超时长媒体 | 400 s WAV（测试内构造）→ ffprobe 行 SUCCEEDED 留证 + 链内 FAILED `ROUTER_MEDIA_DURATION_EXCEEDED`；ffmpeg 未拉起 |
| 9 | 畸形媒体 / 媒体嗅探 | fake.wav（垃圾）→ ffprobe exit 1 → EXECUTION 透传；gzip 字节 → 嗅探门 INPUT 拒绝零 run 行（魔数 1f8b 不入表） |
| 10 | 归档端到端（zip 与 7z） | demo.zip / demo.7z（运行时生成）→ list + extract 两子 run 均 SUCCEEDED；解包产物落**本 run out/**（行为级断言，-oout cwd 相对裁决钉死）；metadata.archive{format, entries, total_uncompressed_bytes, files}；两子 run 各自独立预算行 |
| 11 | 归档四闸负例 | evil2.zip → `ROUTER_ARCHIVE_ENTRY_INVALID`（runs.len()==1 仅 list 行，解包未提交）；bomb.zip → `ROUTER_ARCHIVE_BUDGET_EXCEEDED`（Size=67108864 进消息）；symlink.zip → `ROUTER_ARCHIVE_ENTRY_SYMLINK`；513 条目 zip → `ROUTER_ARCHIVE_ENTRY_BUDGET_EXCEEDED`——四闸任一触发解包阶段零提交 |
| 12 | 不支持格式 + 能力 missing | t.tar（运行时 bsdtar 生成）→ 嗅探门 INPUT `ROUTER_UNSUPPORTED_TYPE` 零 run 行；Rar/7za/7zr 面 grep 零命中；harness 注入缺失 capability_ref（pdf.mutool 形态，DI-1 判据 5 同款）→ PREPARING DEPENDENCY 显式信封 run 行 FAILED |

## 6. 测试面（收口命令，与 CI 同门）

```bash
cd /media/emo/code/pensting/research-platform

# 定向三套（document-router 全量 + dispatch；office/soffice 套件维持 --test-threads=1 纪律）
cargo test --offline -p document-router -- --test-threads=1
cargo test --offline -p mcp-server --test native_rust_dispatch -- --test-threads=1
cargo test --offline -p tool-engine native_rust -- --test-threads=1    # 帧模式零回归门（20 保持）

# 静态检查门（触碰面）
cargo clippy --offline -p document-router -p mcp-server --all-targets -- -D warnings

# 格式门（scoped：本批落点文件级 rustfmt --check，DI-1 教训 K-9；全仓 661 处 HEAD 既有漂移不属本批）
rustfmt --edition 2021 --check <本批 .rs 落点逐个>

# 跨 crate 回归（S7）
cargo test --offline --workspace
```

基线记录（本任务书写作时点 2026-11-20 实测）：DI-1 收口门禁终态 = document-router 47 / dispatch 5 / tool-engine native_rust 20 / tool-registry 42 / capability_catalog 11 / workspace 154 套件 946 passed（DI-1-BATCH-REPORT §A，本会话未复跑全量，S1 以复跑为准）；lock 400 / source=registry 375 / vendor 零 diff。（**勘误**：lock 口径为 DI-1 收口时点值，SEC-0 插批 commit 76c6feb6 后实际基线 399/374——见红线 1 勘误与 DI-2-BATCH-REPORT 坏消息 1。）

纪律：全部命令带 `--offline`；fixture 零下载；任一 CLI 在目标环境缺失（capability 目录 status != available 或二道 stat executable_path 缺席）时该用例 SKIP + 显式标注行（`SKIP <test>: capability <id> status=<status>`，DI-1 同款）；红线 4/5 生产库与 RTIS 零触碰。

## 7. 红线（十条，违反任一即停批上报）

1. **零 vendor / 零 lock 增量**：`vendor/` 零 diff；Cargo.lock `[[package]]` 恒 400、`source = ` 恒 375（S1/S4/S7 三点复核）；本批零新成员零新外部依赖（嗅探/解析全部手写，禁 mime_guess/file-type/infer/zip/serde_json 之外的任何新依赖——serde_json 已在成员依赖面）。
   > **勘误（2026-11-20，DI-2 评审修订）**：上文 400/375 是任务书写作时点的 DI-1 收口口径；SEC-0 插批 commit 76c6feb6（hashlink 升级链中 hashbrown-0.14.5 随淘汰移除）后实际基线为 **`[[package]]` 399 / `source = ` 374**（`git show 76c6feb6^:Cargo.lock` 400 → `git show 76c6feb6:Cargo.lock` 399 实测；DI-2 全程恒定 399/374，S1/S7 双点复核）。后续批次照抄本条前先对齐实际基线并注明来源 commit，详见证 [DI-2-BATCH-REPORT.md](DI-2-BATCH-REPORT.md) 坏消息 1（开工前即成立、诚实记账非隐瞒）。
2. **禁网络**：运行与测试全程离线（`--offline`）；fixture 零下载；ffprobe/ffmpeg/7z/tesseract 的网络面被嗅探前置 + network:none 双闸阻断（ffmpeg 不带任何输入协议 URL 形态——argv 模板只接受 `{{input_path}}` 槽位）；证据归档零外联。
3. **能力缺失按目录 status 跳过并标注**：SKIP + 显式标注；缺失能力只报告不安装（chi_tra 语言包缺失同理——只报错不补装）。
4. **生产库零触碰**：测试内存库/临时库。
5. **RTIS 零接触**：`integrations/rtis/` 零 diff。
6. **soffice 套件单线程沿用**：DI-1 office 通道既有测试保持 `--test-threads=1`；本批新增 e2e 无共享态互斥面（各用例独立 workspace_root）但套件整体仍单线程收口。
7. **不建第二事实源**：ToolRun/Artifact/Evidence/Trace/预算/取消全部复用引擎既有面；Router 零 `std::process::Command`；错误信封只用九类词表 + RT-1 NATIVE_* 码表 + D6 表新增 code（category 不新增）；列目四闸/post-extract 终检在 Router 解析层实现（零新子进程类型）。
8. **adapter 触碰封顶零增量**：tool-engine 全 crate 零 diff（DI-2 全部消费 RT-1/DI-1 既有公开面）；`-oout` cwd 相对裁决依赖 adapter 既有 cwd=ws.root 事实，由行为级 e2e 钉死而非改 adapter。
9. **既有面零回归**：DI-1 三通道行为零变化（route() 既有管线零 diff；image 通道无 OCR 请求位时行为不变）；`cargo test --offline --workspace` 全绿为回归门；DI-1 判据 4 断言迁移按 ADR D1 登记执行（唯一允许的行为断言变更，迁移后 zip/epub/evil.zip 仍各有负例/正例覆盖）。
10. **档位不越权**：只承载「可信低风险档受限原生」面；不明来源归档/媒体/图像的隔离路由不做、不实现、不绕过（RT-0.5-E §1 三档判定留 RT-3）；归档两阶段防护是对**已放行输入**的纵深防御，不构成信任判定。

## 8. 回滚

- 每步独立 commit（S2/S3/S4/S5/S8 各一）；`git revert <批次 commit 集>` 整体撤销。
- 嗅探/通道/错误/normalize 新增面为纯 additive（ZipOther 路由变更单点回退 = routed_channel() 一行还原，DI-1 判据 4 断言随批还原）；五 Manifest 为新增文件，随批回滚；seed 回填值为只读数据面。
- 测试载具/fixture 随代码回滚；生产库/RTIS/宿主软件零操作；Cargo.lock 零增量（无 lock 回滚面）。

## 9. 交付物

1. 代码：document-router 扩展（sniff/channel/router/normalize/error + tests 修订与新增）+ dispatch OCR 请求位（S2/S3/S5）。
2. 数据：五份新工具 Manifest + seed 四条 sha256_full 回填（S4）。
3. 文档：本任务书、[DI-2-ADR-CHANNELS.md](DI-2-ADR-CHANNELS.md)、DI-2 批次验收报告（S8）+ docs/README.md 登记。
4. 证据：OCR/媒体/归档真实 run 的 ToolRun 行 + 统一输出实例（metadata.ocr / metadata.media / metadata.archive）+ 负例 12 类实测输出（S8 对照表）+ 列目四闸逐闸违规消息原文。
5. 前置输出：DI-3 可消费的登记缝——Rar/tar 家族嗅探签名位、pdf.pdftoppm 渲染 OCR 管线位、facade run 审计锚位、统一输出外层信封化位（DI-1 批次报告 H 节清单沿用 + 本批归属变更注记）。

## 10. 实施步骤（S1 → S8，串行，每步独立可验证；无 S0 登记面修订步——pack 存根 exit_conditions 已含 DI-2 解锁语句，本批零登记面改写）

- **S1 门禁基线核验**：`grep -c '^\[\[package\]\]' Cargo.lock`（399，SEC-0 插批后基线——红线 1 勘误）/ `grep -c '^source = ' Cargo.lock`（374）/ `git status --short`（干净）；§6 基线命令复跑；§2.2 五条钉值 sha256 复测对照（漂移即 stale 上报，不静默换值）；五 CLI 在位复测（tesseract --version / ffprobe -version / ffmpeg -version / 7z 版本行 / magick -version）。
- **S2 嗅探与通道表扩展**：sniff.rs 八签名 + ZipOther 路由变更（sniff_boundary 同步）；channel.rs 枚举/镜像行/帽常量/语言路由表（channel_gate 同步）；lock/vendor 复核零 diff。
- **S3 Router 三管线 + dispatch 请求位**：route_ocr/route_media/route_archive（前置门复用 + drive_step 复用）+ normalize 三解析面 + error 新码构造器 + 列目四闸 + post-extract 终检 + dispatch OCR 请求位透传与守卫消息更新；DI-1 既有套件零回归复跑。
- **S4 Manifest 登记与 seed 回填**：五份 Manifest 按 D2/D3/D4 表 + 实测钉值（archive 两份带 binary_target_sha256 双钉）+ seed 四条回填；`cargo test -p tool-registry`（sk3 面）+ Registry 登记通过；每 Manifest validate 回执入批报。
- **S5 测试面落地**：§4 fixture 三层落位（5 件钉值 fixture + fixture_pins 扩行）+ §5 判据 12 项逐项测试 + channel_gate/sniff_boundary/three_channel_e2e 行为变更迁移（ADR D1）。
- **S6 判据逐项自验**：§5 判据 1-12 逐项复跑核对 + 判据对照表预填（S8 批报输入）；SKIP 行格式核对（带测试名）；负例 12 类错误码/留证形态逐条对账（runs.len() 断言、解包步零提交断言）。
- **S7 收口门**：§6 全部命令 + lock/vendor/生产库/RTIS 复核 + `git status` 只含 §3.1 落点。
- **S8 批次收口**：DI-2-BATCH-REPORT.md 落库（命令原文 + 输出摘要 + 判据对照表 + 实测钉值 + 内存帽 S2 阶梯实测定值记录 + 遗留登记）+ docs/README.md 索引行 + DI-3 缝登记（§9.5）。

## 复现

```bash
cd /media/emo/code/pensting/research-platform
# 门禁（预期：document-router 全绿（DI-1 47 + DI-2 新增）、dispatch 5+、native_rust 20 保持、clippy 零告警）
cargo test --offline -p document-router -- --test-threads=1
cargo test --offline -p mcp-server --test native_rust_dispatch -- --test-threads=1
cargo clippy --offline -p document-router -p mcp-server --all-targets -- -D warnings
# 静态门（预期：399 / 374 / 0 / 0——SEC-0 插批 76c6feb6 后基线，见红线 1 勘误）
grep -c '^\[\[package\]\]' Cargo.lock
grep -c '^source = ' Cargo.lock
git status --porcelain -- vendor/ integrations/rtis/ migrations/ | wc -l
# 实测锚复现（预期回显见 §2.2 表）
sha256sum /usr/bin/tesseract /usr/bin/7z /usr/lib/7zip/7z
tesseract --list-langs 2>&1 | head -5
```
