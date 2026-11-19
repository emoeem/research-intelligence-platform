# DI-2 ADR：OCR / 媒体 / 归档三通道设计裁决（路由扩展、argv 模板、语言路由、帽值、四闸、错误映射、wrapper 双钉）

- 日期：2026-11-20
- 状态：已定稿（DI-2 设计裁决；实现口径冻结，对应实现任务书 [DI-2-TASK.md](DI-2-TASK.md)）
- 依据：DI-2 批次指派（三通道设计 + 负例矩阵扩全 + fixture 策略）；[DI-1-ADR-DOC-ROUTER.md](DI-1-ADR-DOC-ROUTER.md) D1-D8（含 D3/D7 两条调和裁决）；[DI-1-TASK.md](DI-1-TASK.md) §2 本机实测面；[RT-0.5-E-RISK-AND-ISOLATION.md](RT-0.5-E-RISK-AND-ISOLATION.md) §2 十四项 / §4.3 归档两阶段 / §4.5 ffmpeg 特化；[RT-0.5-B-CAPABILITY-PROBES.md](RT-0.5-B-CAPABILITY-PROBES.md) §1/§2/§3/§7（tesseract 3 语言包 / ffprobe 9.0.2 / 7z 26.06 格式面 / 7za 无 Rar / bsdtar 无 --formats）；[RT-0.5-C-CAPABILITY-CATALOG.md](RT-0.5-C-CAPABILITY-CATALOG.md) §3.1/3.2/3.3/3.8/3.11-3.15 security_profile；[RT-1-ADR-NATIVE-ADAPTER.md](RT-1-ADR-NATIVE-ADAPTER.md) D3（wrapper 双钉）/D4/D7/D8
- 代码现状锚点（2026-11-20 只读实读）：crates/document-router/src/{sniff,channel,router,normalize,error}.rs 全量 + tests/{channel_gate,sniff_boundary,three_channel_e2e,fixture_pins}.rs 全量；crates/tool-engine/src/adapter/native_rust.rs:592-640（verify_wrapper_target 静态形态解析）、:736-800（{{slot}} 整 token 精确匹配 + 混合 token 字面量透传）、:1051/:1133（cwd = ws.root）；crates/mcp-server/src/dispatch.rs:285-449（native_rust 分支承载范围守卫 + stat 前置帽）；crates/document-router/src/channel.rs:18-27（Channel 枚举）/80-117（channel_steps 对账镜像）
- 本机实测锚点（2026-11-20，/tmp/di2-probe，零网络、全部写 /tmp；原始命令与回显逐条见 [DI-2-TASK.md](DI-2-TASK.md) §2.2）：本文各裁决逐条引用
- 决策记录形态：D1 到 D8，每条含裁决、论据（引用在位证据）、否决项
- 归属登记（偏差修正）：DI-1 批次报告 G.2 与 DI-1 ADR D6 表将「归档两阶段防护」登记为 DI-3 面；DI-2 批次指派明确把**归档通道（7z 两阶段）**划入本批——本 ADR 为该归属变更的权威记录，DI-3 剩余面 = facade run 审计锚 / exiftool / rip.doc.parse 升级 / 统一输出信封化

## D1：三通道落位 document-router 既有成员；嗅探表扩 8 签名；zip_other 改路由归档通道（行为变更登记）

**裁决**：DI-2 三通道（OCR / 媒体 / 归档）全部落 `crates/document-router` 既有成员内扩展（不建新 crate、不拆模块边界），通道枚举 `Channel` 增 `Ocr | Media | Archive` 三值（channel.rs:19-27 扩展，统一输出 `channel` 字面量 `"ocr" | "media" | "archive"`）；通道表 Manifest 集 4 → 9（新增 `rip.doc.image.ocr` / `rip.media.probe` / `rip.media.frames` / `rip.archive.list` / `rip.archive.extract` 五份，`RouterManifests::from_dirs` REQUIRED 同步，dispatch.rs:296-311 承载范围守卫消息同步）。嗅探表（sniff.rs:173-194 判定序追加）新增八签名：

| 签名 | 判据（手写字节判定，2026-11-20 hexdump 实测） | 通道 |
|---|---|---|
| `7z` | 头 6 字节 `37 7A BC AF 27 1C`（demo.7z 实测） | 归档 |
| `wav` | 头 4 字节 `RIFF` 且 8-11 字节 `WAVE`（tone.wav 实测） | 媒体 |
| `avi` | 头 4 字节 `RIFF` 且 8-11 字节 `AVI ` | 媒体 |
| `mp4` | 4-7 字节 `ftyp`（small.mp4 实测 `....ftypisom`） | 媒体 |
| `webm` | 头 4 字节 `1A 45 DF A3`（EBML，v.mkv 实测） | 媒体 |
| `ogg` | 头 4 字节 `OggS` | 媒体 |
| `flac` | 头 4 字节 `fLaC` | 媒体 |
| `mp3` | 头 3 字节 `ID3` | 媒体 |

**行为变更（登记）**：`Magic::ZipOther`（`PK\x03\x04` 且无 `[Content_Types].xml`）由「DI-1 INPUT 拒绝（sniff.rs:76 `routed_channel() => None`）」改为**路由归档通道**——zip 是归档两阶段防护的第一频次输入（evil.zip 穿越负例 DI-1 即已入库），DI-1 D6 表「zip_other 显式不支持」行由本裁决取代（DI-1 ADR 历史文本不改写，行为面以本 ADR 为准）。Rar/Rar5/gzip/xz/bzip2/zstd/tar 不入嗅探表：gzip（`1F 8B`）、tar（@257 `ustar`）实测可判但**本批不路由**（fail-closed INPUT 拒绝，`ROUTER_UNSUPPORTED_TYPE`），归档通道本批仅承载 zip + 7z 两格式（7z 输入面 archive.7z 能力条目格式集 7z/zip 全覆盖；Rar 面留 DI-3 连同 bsdtar/tar 家族一并裁决，E §4.3.4「Rar → 仅 archive.7z」路由规则届时落地）。OCR 通道不新增嗅探签名：图像输入沿用 `Png | Jpeg`（sniff.rs:30-34），OCR 意图由**显式请求位**承载（D2）。

**论据**：

- 同 crate 扩展 = DI-1 ADR D1 论据的直接延续：Router 是文档域编排层，媒体/归档输入仍走「嗅探 → 通道表 → 帽门 → 规范化」同构管线（route() 前置门 bytes 帽 + 嗅探门 router.rs:170-186 三通道共用）；新 crate 只会复制 DriveCtx/drive_step/read_stdout 复用面，零收益。
- 新签名全部为**精确字节判定**（无启发式）：mp3 仅收 ID3 头形态（帧同步 `FF FB` 为启发式，误判率高，不收）；WEBP 形态（RIFF+`WEBP`）不收（图像 magick 面留 DI-3，避免 RIFF 子型判定面过宽）。
- zip_other 改路由的实测前提：7z 26.06 对 `../escape.txt`（列表逐字显示 `Path = ../escape.txt`）与绝对路径条目（`Path = /tmp/...` 逐字显示）在**列表阶段全部如实呈现**（本机 -slt 实测），Router 门校验所需信息在解包前完整可得——E §4.3.1「先列目录解析条目清单，经 Router 校验后再放行解包」结构成立。
- 负例行为变更对 DI-1 测试面有直接影响（three_channel_e2e.rs:558-569 判据 4 断言 epub/zip/evil.zip 嗅探门拒绝）：该三条负例按本裁决迁移——evil.zip → 归档 list 门拒绝负例（run 行留证）、sample.zip / sample.epub → 归档通道正例；嗅探门零 run 行负例集改由 tar（bsdtar 运行时生成）+ gzip（测试内字节）承载。**迁移是判据语义更新而非测试削弱**：负例矩阵按本批指派扩全（任务书 §5）。

**否决项**：

- 三通道落新 crate（`channels-ocr` 之类）：复制引擎驱动面成第二事实源，破 E §5:122「不建第二套事实源」，否决。
- tar/gzip/xz 本批入路由：bsdtar 无 `--formats` 清单（RT-0.5-B §7.2 实测）、tar 家族防护是独立裁决面（E §4.3 注 pax 扩展头），本批通道表五 Manifest 全部绑定 archive.7z 单能力，tar 面入列即引入第二能力未探测状态，否决；fail-closed 拒绝零风险。
- zip_other 维持 INPUT 拒绝（不扩归档通道）：违背本批指派的归档通道设计；且 zip 归档防护四闸（D4）实测信息完备，继续拒绝是把已具备防护能力的输入永久挡在门外，否决。

## D2：OCR 通道——identify 像素帽门步 + tesseract TSV 单步；语言路由表以 catalog 为唯一权威；chi_tra 请求前置门显式 error

**裁决**：OCR 通道两子 run 管线，仅由**显式 OCR 请求位**触发（`Png | Jpeg` 嗅探签名 + 请求位并存才路由 OCR；无请求位 → DI-1 图像通道 identify 单步行为零变化）：

| 步 | 工具 Manifest | capability_ref | argv 模板（2026-11-20 实测冻结） | 产物面 |
|---|---|---|---|---|
| 1 门步 | rip.doc.image.meta（DI-1 既有，零改） | image.magick | `["-ping","-quiet","-format","{{identify_fmt}}","{{input_path}}"]`（D3 既有） | stdout 单行 → 像素帽门 |
| 2 OCR 步 | rip.doc.image.ocr（新增） | ocr.tesseract | `["{{input_path}}", "stdout", "-l", "{{ocr_lang}}", "tsv"]` | stdout TSV → Router 解析 |

- **像素帽收紧（D7 #8 DI-2 值）**：OCR 步派生前像素门 = **50 MP**（DI-1 元数据路径 100 MP 是 `-ping` 仅头部读口径；OCR 全量解码，按 D7 #8 行内预注「DI-2 渲染/OCR 批在派生前按同表收紧」落 50 MP）。门步复用 identify 输出解析（normalize.rs ImageMeta::pixel_count），超帽链内 FAILED `ROUTER_PIXEL_BUDGET_EXCEEDED`（码沿用，消息注明 OCR 帽 50 MP）。
- **语言路由表（唯一权威 = capability 目录 ocr.tesseract 条目 language 集：chi_sim/eng/osd，RT-0.5-C §3.3）**：

| 请求语言（input.ocr.language） | tesseract `-l` 值 | catalog 判定 |
|---|---|---|
| `zh-Hans` / `zh-CN` / `zh` / `chi_sim` | `chi_sim` | 目录 language=chi_sim ✓ 放行 |
| `en` / `eng` | `eng` | ✓ 放行 |
| 缺省 / `auto` | `chi_sim+eng` | 双语言均在目录 ✓ 放行（组合形态本机实测 exit 0） |
| `zh-Hant` / `zh-TW` / `zh-HK` / `chi_tra` 等繁体面 | — | 目录无 chi_tra → **前置门拒绝** |
| 其余一切（ja/ko/fr/…） | — | 目录无对应 language → **前置门拒绝** |

- **chi_tra 请求 = 显式 error，禁静默回退 chi_sim**（RT-0.5-C §3.3 constraints 原文「禁止静默降级为 chi_sim 兜底」）：路由表查 catalog 缺失 → `Err(RouterReject)` category **DEPENDENCY**、code `ROUTER_OCR_LANG_MISSING`、retryable=false、消息注明「chi_tra 语言包缺失属环境持久事实，补装属用户决策不自动安装（RT-0.5-C §3.3/设计 §5）」、**零 run 行**（D7 前置门语义——拒绝发生在任何子 run 提交之前，与嗅探门/64 MiB 门同层）。第二道防线：绕过 Router 直呼 tesseract 时 `-l chi_tra` exit 1（stderr `Error opening data file /usr/share/tessdata/chi_tra.traineddata`，实测）——子 run 行 FAILED，不静默。
- **OCR 置信度与语言入输出 Schema（统一输出 metadata.ocr 块）**：

```jsonc
{
  "channel": "ocr",
  "text": "<TSV 按 (block,par,line) 重组正文>",   // 单 run 承载，非二次 OCR
  "metadata": {
    "format": "PNG", "dimensions": {"width": 460, "height": 80},   // 门步 identify 面沿用
    "ocr": {
      "language": "eng",                  // tesseract -l 实际参数
      "requested_language": "en",         // 路由输入原值（auto 形态记 "auto"）
      "words": 3,                         // TSV level=5 行计数
      "mean_conf": 93.6,                  // level=5 行 conf 均值（保留 1 位小数；conf 为 tesseract TSV 第 11 列实测浮点形态 92.203033）
      "low_conf_words": 0                 // conf < 60 的词数
    }
  },
  "page_anchors": [{"page": 1, "char_start": 0, "char_end": 13}],  // 单图单页锚
  "warnings": []
}
```

- TSV 解析面（本机实测冻结）：首行 header `level page_num block_num par_num line_num word_num left top width height conf text`；词行 = level=5；正文重组 = 按 (block_num,par_num,line_num) 分组、词间空格连接、行间 `\n` 连接（normalize 单测钉死重组规则；`tesseract <in> out txt tsv` 双文件形态实测会向 cwd 写 out.txt/out.tsv——cwd=ws.root 非 out/，破产物登记纪律，否决）；空结果（空白图 exit 0 零词行实测）= words=0 + warnings 如实记录「no words recognized」，不伪造置信度。
- mean_conf < 60（低置信）→ warnings 追加诚实记录，不拒绝不静默。

**论据**：

- 门步在 OCR 步之前是结构要求：identify -ping 仅读头部（DI-1 D3 实测 8ms），50 MP 判定零解码成本；oversized-pixels.png（121 MP，入库 fixture）在 OCR 门步即 FAILED，tesseract 永远见不到超帽输入——像素内存放大器（E §2 #8）在派生前封死。
- 语言路由以 catalog 为唯一权威而非硬编码清单：目录条目 capabilities[].kind=language 就是探测事实（RT-0.5-F §6 消费面），补装 chi_tra 后目录重探（RT-0.5-C §5.3 L2 变化即 stale 重探）路由表自动放行，Router 零改码。
- TSV 单 run 承载文本+置信度：plain stdout 与 tsv 各跑一次 = OCR 成本 ×2（OCR 是全通道最贵步），TSV 词行 text 列重组与 plain stdout 近等（本机实测同文「HELLO OCR 123」）。
- `{{ocr_lang}}` 为整 token 槽位（native_rust.rs:736-772 语法内）：值由 Router 经路由表生成后填 input（input_schema enum 三值 `chi_sim | eng | chi_sim+eng`），调用方永远不能把任意串送进 argv——语言枚举面在 Manifest input_schema 层二道收紧。

**否决项**：

- chi_tra 静默回退 chi_sim：RT-0.5-C §3.3 constraints 明文禁止（设计 §3.4「语言数据缺失要明确报告」），DI-0 链零静默降级纪律同源，否决。
- 语言路由硬编码进 Router 而不查 catalog：环境事实第二来源，目录重探后 Router 需同步改码，否决。
- OCR 步前无门步直呼 tesseract：超帽像素直达 OCR 解码面（内存放大器失控），且丢失 format/dimensions 元数据面，否决。
- `tesseract <in> stdout txt tsv` 双配置单 run：实测 stdout 只承载一个 config（双 config 走文件形态），且文件落 cwd=ws.root 不在 out/ 登记面，否决。
- PDF 渲染 OCR（pdftoppm → tesseract）：DI-1 批次报告 H 节曾预期 DI-2 消费 pdf.pdftoppm；本批指派明确 OCR 通道 = **图像输入 → tesseract**，PDF 渲染链（页数×像素乘积预算、页序 OCR 合并）留 DI-3 连同 facade run 一并承载。pdf.pdftoppm seed 条目本批零消费（sha256_full 回填也不做——只回填实际使用的四条）。

## D3：媒体通道——ffprobe JSON 常步 + ffmpeg 抽帧条件步；帽在派生前由 ffprobe 解析定案；experimental encoder 禁用于生产面

**裁决**：媒体通道两子 run（视频流）或一子 run（纯音频）管线：

| 步 | 工具 Manifest | capability_ref | argv 模板（2026-11-20 实测冻结） | 产物面 |
|---|---|---|---|---|
| 1 探测步 | rip.media.probe（新增） | media.ffprobe | `["-v","error","-print_format","json","-show_format","-show_streams","{{input_path}}"]` | stdout JSON → Router 解析 + 全部媒体门 |
| 2 抽帧步 | rip.media.frames（新增） | media.ffmpeg | `["-hide_banner","-loglevel","error","-i","{{input_path}}","-vf","fps=2","-frames:v","32","-c:v","png","out/frame-%03d.png"]` | out/frame-NNN.png（cwd 相对，Adapter cwd=ws.root 实锚） |

- **探测步后门序（全部在 ffmpeg 派生前定案，对齐 pdfinfo→pdftotext 门先于抽取的既有形态）**：
  1. 畸形媒体门：ffprobe exit != 0 → 子 run FAILED EXECUTION 透传退出码（fake.wav 实测 exit 1 `Invalid data found when processing input`）；
  2. 内容时长门：`format.duration`（**字符串形态** `"2.000000"`，实测）解析 > 300 s → 链内 FAILED `ROUTER_MEDIA_DURATION_EXCEEDED`；duration 缺席（某些流无时长）→ warnings 如实记录 + 门不伪装已定案（D7 页数缺席同款零静默口径）；
  3. 视频流存在性分支：streams 中 codec_type=video 缺席（纯音频 tone.wav 实测）→ **不失败不抽帧**：metadata-only 统一输出 + warnings 记录「no video stream, frame extraction skipped」，与 pdfinfo Pages 缺席同款诚实分支；
  4. 单帧像素门：`streams[video].width × height`（**整数形态** 128/64，实测）> 16 MP → FAILED `ROUTER_MEDIA_PIXEL_BUDGET_EXCEEDED`；
  5. 帧数预算：预期帧数 = min(⌈2×duration⌉, 32)，duration > 16 s 时抽帧步提取**前 32 帧**（`-frames:v 32` 实测帽生效：fps=2 源 20 帧请求 3 帧恰出 3 帧）+ warnings 记录 capped + metadata.frames 标记——**前缀采样语义非拒绝**（媒体抽帧是预览面非全量面，与 PDF 页数帽的全量抽取拒绝语义差异见 D5）。
- **编码器铁律（实测铁证）**：本机 ffmpeg 9.0.2（ffmpeg-full）**不显式指定编码器时自动选型 h264_rkmpp 硬件编码器并在 x86 宿主初始化失败**（MPP init 失败，mp4 输出零字节，实测复现）——媒体通道 argv 模板编码器一律显式：抽帧输出 `-c:v png`（PNG 原生编码器，无硬件选型面）；**experimental encoder 禁用于生产面**（原生 opus/vorbis，`A..X.D` 标记，RT-0.5-B §1/RT-0.5-C §3.1 constraints 原文「生产编码必须用 libopus/libvorbis」）——本批无音频编码面，该约束以 Manifest known_limits 登记（未来任何 ffmpeg 编码步的 argv 模板不得出现 `opus`/`vorbis` 裸名，只允许 `libopus`/`libvorbis`/显式无损面）。
- **媒体输出规范化 Schema**：

```jsonc
{
  "channel": "media",
  "text": "",
  "metadata": {
    "media": {
      "format_name": "wav",                  // format.format_name（mp4 实测逗号列表 "mov,mp4,m4a,3gp,3g2,mj2" 原样承载）
      "duration_s": 0.5,                     // format.duration 浮点化
      "bit_rate": 128704,                    // format.bit_rate（字符串形态实测，数值化）
      "size_bytes": 8044,
      "streams": [                           // 精简投影，全量 JSON 留 stdout.log
        {"codec_type": "audio", "codec_name": "pcm_s16le", "sample_rate": 8000, "channels": 1},
        {"codec_type": "video", "codec_name": "h264", "width": 128, "height": 64}
      ]
    },
    "frames": {                              // 仅视频流且抽帧步执行时在位
      "pattern": "frame-%03d.png", "extracted": 4, "fps": 2, "capped": false
    }
  },
  "page_anchors": [], "warnings": [], "runs": []
}
```

- 缺项零编造：bit_rate/duration 缺席键不产出（pdfinfo producer 同款口径）；`nb_frames`（字符串形态实测）不入 metadata（流级帧数与抽帧计数语义易混，抽帧计数以 out/ 实收为准）。

**论据**：

- 帽在 ffprobe 解析后、ffmpeg 派生前定案：300 s 内容时长门使 ffmpeg 实际工作量有界（≤32 帧 × ≤16 MP）；「超时长媒体」负例（400 s WAV 测试内构造）实测可走通时长门。
- ffprobe JSON 而非行解析：ffprobe `-print_format json` 本机实测原生支持（与 pdfinfo/magick identify 无 JSON 旗标被否决的 DI-1 前提相反），结构化解析零歧义。
- 抽帧步全常量（fps=2 / -frames:v 32 / -c:v png / 输出 pattern 均为字面量）：帽是 Router 属性不是调用方参数——`fps={{fps}}` 属混合 token（native_rust.rs 混合字面量透传实测），任何调用方可控速率参数都会绕开帽表；固定 fps=2 + 32 帧帽 = 单次抽帧工作量上界 32×16 MP 像素、32 件 Artifact 行，全部有界。
- 输出 pattern `out/frame-%03d.png` cwd 相对：cwd=ws.root（native_rust.rs:1051/:1133 实锚）→ 产物落本 run out/ 进 collect_out_artifacts 登记面（与归档 `-oout` 同一 cwd 相对裁决族，D4）。

**否决项**：

- 抽帧不显式 `-c:v`：rkmpp 自动选型失败实测（本机复现），编码器选型必须钉死，否决。
- 调用方可控 fps / 帧数参数：混合 token 不可组合 + 帽是 Router 属性，否决（`{{fps}}` 类槽位在未来 adapter 支持混合 token 组合前一律不做）。
- 时长超帽继续抽帧（截断式容忍）：内容时长是解析前可判定输入属性，与页数帽同级；300 s 以上的媒体不属于本批受限原生档目标面（转码面本就没有），否决。
- 音频转写（whisper 面）：catalog 无该能力条目，媒体通道 text 恒空串，否决。

## D4：归档通道——7z 两阶段（l -slt 列目四闸 → x 解包本 run out/）；解包产物 post-extract 符号链接终检

**裁决**：归档通道两子 run 管线（两阶段分别授权、分别预算，E §4.3.1 原文）：

| 阶段 | 工具 Manifest | capability_ref | argv 模板（2026-11-20 实测冻结） | 产物面 |
|---|---|---|---|---|
| 1 列目阶段 | rip.archive.list（新增） | archive.7z | `["l","-slt","{{input_path}}"]` | stdout 技术清单 → Router 四闸 |
| 2 解包阶段 | rip.archive.extract（新增） | archive.7z | `["x","-y","-oout","{{input_path}}"]` | out/ 解包产物 → post-extract 终检 |

- **`-oout` 相对 cwd 裁决（本 ADR 最关键实现面）**：7z 解包输出目录唯一形态 = `-o` 开关**直连路径**（实测矩阵：`-oex1` 附连 ✓ exit 0；`-o ex1` 空格 ✗ exit 7；`-o=ex3` ✗ 造出字面 `=ex3` 目录——三种实测一次性否决「空格/等号组合」直觉）。argv 模板槽位为整 token 精确匹配、混合 token 字面量透传（native_rust.rs:736-772，DI-1 D3 profile_uri 不可组合同源），`-o{{out_dir}}` 结构性不可行 → **模板用字面常量 `-oout`**：Adapter 子进程 cwd = `ws.root`（native_rust.rs:1133 `resource_command(..., &ws.root)` → :1051 `cmd.current_dir(cwd)` 实锚），`-oout` 即 `<root>/<run_id>/out` = 本 run 独立输出目录（E §2 #2），out/ 由 adapter prepare_workspace 预创建（:305 实锚）。**cwd=ws.root 依赖由行为级 e2e 钉死**（解包产物断言落位 out/），Adapter cwd 语义任何漂移即测试红。
- **列目清单解析面（`7z l -slt` 实测冻结）**：头块（`Path = <archive>` / `Type = zip|7z|…` / `Physical Size`）以 `----------` 分隔线与条目块分界；条目块字段 `Path = <条目名>`（穿越/绝对路径**逐字原样呈现**，`Path = ../escape.txt` 实测）、`Folder = +|-`（目录实测 `Folder = +`）、`Size = <n>`（压缩炸弹 64 MiB 条目实测 `Size = 67108864`）、`Attributes`（zip 符号链接条目实测 ` lrwxrwxrwx`，目录 `D ...`，普通文件 `V ...`）、`Symbolic Link = <目标>`（tar 形态实测）。畸形归档 `7z l` exit 2（fake.zip 实测）→ EXECUTION 透传。
- **防护四闸实现位（全部在 Router 列目阶段解析层，先于解包阶段提交；E §4.3.1「路径穿越、符号链接和压缩炸弹设独立防护」逐项落位）**：

| 闸 | 判定（对 -slt 条目块） | 违规 code |
|---|---|---|
| 闸 1 路径安全 | 条目 Path 含 `..` 段 / 以 `/` 起头 / 含 `\`（Windows 语义分隔符保守 fail-closed） | `ROUTER_ARCHIVE_ENTRY_INVALID` |
| 闸 2 符号链接 | `Symbolic Link = ` 字段非空，或 Attributes 含 `l` 权限前缀（`lrwxrwxrwx`） | `ROUTER_ARCHIVE_ENTRY_SYMLINK` |
| 闸 3 条目数 | 条目块计数 > 512（含目录条目） | `ROUTER_ARCHIVE_ENTRY_BUDGET_EXCEEDED` |
| 闸 4 解压总量 | Σ Size > 256 MiB 或单条目 Size > 64 MiB（单条目帽对齐 64 MiB 输入帽量级） | `ROUTER_ARCHIVE_BUDGET_EXCEEDED` |

- 四闸任一触发 = 链内 FAILED，**解包阶段永不提交**（已存 list 行保持真实终态留证——负例断言 runs.len()==1 且 tool_id=rip.archive.list）；违规条目名如实进错误消息。
- **post-extract 符号链接终检（E §4.3.2「解包后拒绝保留符号链接形态进入下游处理」）**：解包步 Succeeded 后 Router 对本 run out/ 做 `symlink_metadata` 遍历，任何符号链接 → 链内 FAILED `POLICY_DENIED ROUTER_ARCHIVE_SYMLINK_SURVIVED`（闸 2 之后的第二道防线：7z 自身若对某形态不剥离而闸 2 解析面失察，终检兜底）。
- **7z 自身净化行为 = 第二道防线，不作契约**：实测 7z 26.06 对 `../escape.txt` 剥离前导段落位 `<target>/escape.txt`、对绝对路径条目落位 `<target>/tmp/...` 相对路径——该行为属第三方实现细节非规格承诺，闸 1 的 Router 判定是唯一权威面（fail-closed 不依赖工具善意）。
- **归档输出规范化 Schema**：

```jsonc
{
  "channel": "archive",
  "text": "",
  "metadata": {
    "archive": {
      "format": "zip",                        // 头块 Type = 行
      "entries": 2, "directories": 0,
      "total_uncompressed_bytes": 23,
      "files": [{"path": "a.txt", "size": 11}, {"path": "b.txt", "size": 12}]
    }
  },
  "page_anchors": [], "warnings": [], "runs": []
}
```

- 解包产物字节经解包 run 信封 artifacts 面 sha256/size 登记（collect_out_artifacts 既有面）；统一输出 files 清单来自列目阶段（清单=授权面记录），与 out/ 实收产物对账（条目数/字节数不一致 → warnings 如实记录）。

**论据**：

- `-slt` 技术清单是唯一机器可解析面：`7z l` 表格列（-ba bare 形态实测）空格分列遇含空格文件名即歧义，-slt 逐字段 `Key = Value` 无歧义且五闸所需字段（Path/Size/Folder/Attributes/Symbolic Link）全覆盖。
- 两子 run 结构 = E §4.3.1「两阶段分别授权、分别预算」的直接承载：list 与 extract 各自独立 ToolRun（各自 resources/network/timeout 预算），闸失败时 extract 预算零消耗。
- zip 与 7z 同管道：archive.7z 能力条目格式集 7z/zip 全覆盖（RT-0.5-C §3.11），同 argv 模板零分支；Type 字段区分容器（实测 `Type = zip`）。
- 解包阶段 `-y`（默认覆盖确认抑制）：闸 3/4 保证 out/ 产物有界（≤512 条目 × ≤64 MiB 单条目帽、Σ≤256 MiB），Artifact 行数同界。

**否决项**：

- 单阶段直解（无列目门）：E §4.3.1 明文两阶段，否决。
- Router 手写 zip 解析替代 7z l 列目：DI-1 嗅探器条目扫描（sniff.rs:121-170）只读 LFH 名不读 Size/Attributes，手写全量解析 = 第二套归档实现且 Rar/7z 容器格式各自为坑，否决。
- `-o{{out_dir}}` 槽位组合：混合 token 字面量透传实测（`-o=ex3` 造出字面 `=ex3` 目录为一次性反证），扩展 adapter 槽位语法破 D4 封顶纪律，否决；`-oout` + 行为级测试钉死为正解。
- 解包目标 tmp/：tmp/ 用后即焚（cleanup_tmp），产物登记面在 out/（collect_out_artifacts 只扫 out/），落 tmp/ 产物不可审计，否决。
- post-extract 终检落 Router 外（独立扫描工具 run）：又一子 run 成本且终检属链内裁决面，否决。

## D5：帽值表（对齐 RT-0.5-E §2 十四项，DI-2 三通道逐项定稿）

**裁决**：E §2 十四项在 DI-2 域的承载如下（#1-#3/#12-#14 沿用 Adapter/Engine 既有面零改动；#4-#11 为本批 Manifest/Router 定稿值）。内存/时长为**初定值**——DI-1 教训 K-1「纸面帽值必须实测定值」：实现批 S2 以 50 MP OCR 样本 / 16 MP 帧 / 256 MiB 解包样本过一遍本机阶梯，偏差按调和裁决条目回写本 ADR。

| E §2 # | 限制项 | DI-2 帽值 | 承载层 |
|---|---|---|---|
| 1 | 只读输入 | 输入经 Router 预写进子 run in/（DI-1 D5 复用），`{{input_path}}` 前缀校验 | adapter 既有（native_rust.rs:1322-1330） |
| 2 | 独立输出 | 每 run 独立 `<root>/<run_id>/{in,out,tmp}`；归档解包落**本 run** out/（`-oout` cwd 相对，D4）；抽帧 pattern `out/frame-%03d.png` 同族 | adapter 既有 + 模板常量 |
| 3 | 非特权身份 | env_clear 最小集（PATH/TMPDIR/LANG），零继承；7z sh wrapper 核心工具链经 PATH=/usr/bin 可用（soffice 同款实测先例） | adapter 既有（D4） |
| 4 | CPU | 五 Manifest `resources.cpu: "1"` → prlimit 兜底 | manifests + adapter 既有 |
| 5 | 内存 | rip.doc.image.ocr **1Gi**（tesseract/leptonica 50 MP 上界样本阶梯定稿）*；rip.media.probe 256Mi；rip.media.frames **512Mi**（16 MP 单帧上界）*；rip.archive.list 256Mi；rip.archive.extract **512Mi**（Σ256 MiB 解包）* | manifests resources.memory（* = S2 实测阶梯待定稿项） |
| 6 | 时长 | ocr 120s（50 MP 密集文本 OCR 上界）；media.probe 30s；media.frames 60s；archive.list 60s；archive.extract 120s（256 MiB 解包上界） | manifests resources.timeout_seconds |
| 7 | 页数（域对应物） | 媒体帧数帽 **32**（`-frames:v 32` 字面常量，D3）；归档条目帽 **512**（闸 3）；媒体内容时长帽 **300 s**（ffprobe format.duration 门） | 模板常量 + Router 门 |
| 8 | 像素 | OCR 像素帽 **50 MP**（identify 门步，DI-1 100 MP 元数据口径收紧）；媒体单帧 **16 MP**（ffprobe width×height 门） | Router 门 |
| 9 | 解压大小 | 归档解压总量 **256 MiB**（Σ Size）+ 单条目 **64 MiB**（闸 4）；闸 1/2 路径与链接防护 | Router 列目门（D4） |
| 10 | 输出上限 | 子 run `max_output_bytes`：rip.doc.image.ocr **4 MiB**（TSV 解析面实测行宽估算，50 MP 密集文本 TSV 上界；1 MiB 惯例对 TSV 过窄会造成诚实失败误伤面）/ 其余四 Manifest 1 MiB；统一输出信封限界沿用 DI-1 D7 #10 注（外层信封化随 DI-3 facade run） | manifests + engine 既有 |
| 11 | 网络 | 五 Manifest `network.mode: "none"`；嗅探前置使 ffprobe/ffmpeg/7z/tesseract 的网络面结构性不可达（媒体 URL 输入 / 归档远程对象无入口） | manifests + Router |
| 12 | 环境白名单 | adapter D4 既有；五 Manifest `runtime.env` 零追加 | adapter 既有 |
| 13 | 文件系统边界 | in/ 前缀校验 + 解包产物仅落本 run out/ + post-extract symlink 终检（D4）；家目录/凭据/生产库零暴露 | adapter 既有 + Router 终检 |
| 14 | 状态统一 | 全部子 run 走 ToolRun 状态机 + 九类信封；链级失败 = D6 映射 reject + 子 run 行留证（DI-1 D8 同款） | engine |

- 输入文件大小帽 64 MiB：全通道同门（route() 前置门 channel.rs MAX_INPUT_FILE_BYTES 既有，DI-2 零改动）——归档/媒体超帽输入在嗅探门同层拒绝。
- 帧数帽语义裁决（前缀采样非拒绝）：PDF 页数帽拒绝是因为 pdftotext 语义 = 全文抽取；媒体抽帧语义 = 预览采样，`-frames:v 32` 前缀提取 + capped 警告如实呈现（metadata.frames.capped）即可满足零静默纪律——域语义差异是本条的存在理由，不是对页数帽拒绝语义的偏离。

**DI-2 实现批 S2 内存帽阶梯定值回写（2026-11-20 实测，`prlimit --as` RLIMIT_AS 口径；纸面初值为上表 * 项，本回写为 K-1「纸面帽值必须实测定值」流程的偏差回写，历史初值保留不改写）**：

| 工具 | 纸面初值 | 实测阶梯 | 定值 |
|---|---|---|---|
| rip.media.probe | 256Mi | FAIL@1152Mi（`libSvtJpegxs.so.0: failed to map segment`）→ FAIL@1024Mi → **OK@1280Mi** → OK@1536Mi | **1536Mi**（1.2× 余量；ffmpeg-full 约两百动态库全映射计入 RLIMIT_AS） |
| rip.media.frames | 512Mi | FAIL@1152Mi（lib）→ FAIL@1280Mi（h264 decoder open）→ FAIL@1536Mi（packet submit）→ FAIL@2048Mi（swscaler）→ **OK@3072Mi** | **3Gi**（128x64 抽帧 3Gi 上界主体是库映射面非解码工作集） |
| rip.archive.list / extract | 256Mi / 512Mi | 128Mi 起 OK（小样本；解包帽 256 MiB 预算内 512Mi 充裕） | 维持初值 |
| rip.doc.image.ocr | 1Gi | 256Mi 起 OK（36800 像素样本；50 MP 上界留 1Gi 余量） | 维持初值 |

- 实测命令形态：`prlimit --as=$((N*1024*1024)) -- /usr/bin/ffprobe -v error -print_format json -show_format tone.wav`（stdin 须 </dev/null 关闭——ffmpeg 无 -nostdin 且 stdin 常开时挂起为探针自身坑，adapter 面写毕即关不踩）。
- 本机承载机制 = prlimit 兜底（RLIMIT_AS 地址空间帽，systemd-run MemoryMax 不可用）；地址空间帽与 RSS 语义差异如实登记。

**否决项**：

- 媒体时长/帧数照抄 PDF 页帽（2000 页）量级：帧×像素工作量为 32×16 MP=512 MP 上界，与 2000 页文本抽取完全不同域，否决混用。
- OCR 像素沿用 DI-1 100 MP：D7 #8 行内预注明文「DI-2 渲染/OCR 批按同表收紧」，OCR 是全量解码面，否决。
- 五 Manifest max_output_bytes 一律 1 MiB：TSV 解析面（OCR 唯一数据载体）实测行宽下 1 MiB ≈ 1.7 万词，密集页会诚实失败误伤；4 MiB 单点放宽并留 known_limits 注记，否决一律化。

## D6：错误五态映射沿用（九类词表唯一）；DI-2 新增 code 表

**裁决**：RT-1 ADR D7 五态映射 + DI-1 D8 总表**沿用不改**（九类 category 唯一词表 envelope.rs:16-24；ProbeError 五态 → DEPENDENCY/POLICY_DENIED/TIMEOUT/EXECUTION/INFRA 一一对应不变；Router 自定义 category 仍为否决项）。DI-2 新增事件全部映射进既有 category，code 走信封 error.code 自由面（`ROUTER_*` 前缀，envelope::validate 对 code 无词表约束——DI-1 D8 同款）：

| 事件 | category | code | 时点与留证 |
|---|---|---|---|
| OCR 请求语言 catalog 缺失（chi_tra 面） | DEPENDENCY | `ROUTER_OCR_LANG_MISSING` | 前置门拒绝，**零 run 行**，retryable=false |
| OCR 像素超 50 MP | INPUT | `ROUTER_PIXEL_BUDGET_EXCEEDED`（沿用 DI-1 码，消息注明 OCR 帽） | 链内 FAILED，identify 行留证 |
| 媒体内容时长 > 300 s | INPUT | `ROUTER_MEDIA_DURATION_EXCEEDED` | 链内 FAILED，ffprobe 行留证 |
| 媒体单帧像素 > 16 MP | INPUT | `ROUTER_MEDIA_PIXEL_BUDGET_EXCEEDED` | 链内 FAILED，ffprobe 行留证 |
| 归档路径穿越/绝对路径/反斜杠条目 | INPUT | `ROUTER_ARCHIVE_ENTRY_INVALID` | 链内 FAILED，list 行留证，解包步未提交 |
| 归档符号链接条目 | INPUT | `ROUTER_ARCHIVE_ENTRY_SYMLINK` | 同上 |
| 归档条目数 > 512 | INPUT | `ROUTER_ARCHIVE_ENTRY_BUDGET_EXCEEDED` | 同上 |
| 归档解压总量 > 256 MiB / 单条目 > 64 MiB | INPUT | `ROUTER_ARCHIVE_BUDGET_EXCEEDED` | 同上 |
| 解包后 out/ 残留符号链接 | POLICY_DENIED | `ROUTER_ARCHIVE_SYMLINK_SURVIVED` | 链内 FAILED，post-extract 终检 |
| 列目成功但清单零条目 / 头块解析失败 | EXECUTION | `ARCHIVE_LIST_NO_OUTPUT` | 链内 FAILED（OFFICE_CONVERT_NO_OUTPUT 同位语义） |
| 媒体无视频流（抽帧步跳过） | （非错误） | — | metadata-only + warnings 诚实记录（D3 分支） |
| sniff 无签名（tar/gzip/rar/未知魔数） | INPUT | `ROUTER_UNSUPPORTED_TYPE`（沿用） | 前置门，零 run 行 |
| capability missing / stale / 钉值不符 / 非零退出 / 读帽触顶 / 超时 / 取消 | 既有五态映射 | RT-1 既有 NATIVE_* 码表 | 既有路径零改动 |

**论据**：RT-0.5-F §7「禁止把 timeout/probe_failed 报成 missing」由映射唯一性保证（DI-1 D8 同款论据）；OCR 语言缺失报 DEPENDENCY 而非 INPUT 的归因 = chi_tra traineddata 文件缺席是**环境持久事实**（F §7 missing 行定义「capability 目录 missing」同族——语言包是 ocr.tesseract 的子能力面），补装属用户决策（RT-0.5-C §3.3），而「请求里没有语言」才属 INPUT。

**否决项**：`ROUTER_*` 自定义 category：超出九类词表 envelope::validate 直接拒绝（DI-1 D8 否决项沿用），否决。

## D7：wrapper 双钉对 7z 包装脚本的适用性——静态 exec 形态走 Manifest 双钉（与 office 变量形态分层承载成对照）

**裁决**：`/usr/bin/7z` 为 sh 包装脚本且 **exec 目标为静态形态**（实测文件内容全文：`#!/bin/sh` + `exec /usr/lib/7zip/7z "$@"`——单条 `exec <绝对路径>` 行），命中 RT-1 `verify_wrapper_target`（native_rust.rs:592-640）的解析器要求（`exec ` 前缀 + 首 token 以 `/` 起头 + 唯一目标）。故 archive 通道两份 Manifest（rip.archive.list / rip.archive.extract）走**全量 Manifest 双钉**：

```jsonc
"runtime": {
  "type": "native_rust",
  "binary": "/usr/bin/7z",                                  // sh wrapper
  "binary_sha256": "f802383f13b8331aabaf779def00a356c8d2b8148255170b75b03028f4d23ad4",   // wrapper 全量钉（2026-11-20 实测，与 seed sha256_full 逐字吻合）
  "binary_target_sha256": "d48e622e9009acdba4f8e29645ce791fef1ecb2c41bf385e960852a8c902fad9", // exec 目标 ELF 全量钉（2026-11-20 实测）
  "capability_ref": "archive.7z",
  "argv_template": ["…", "{{input_path}}"],
  "result_mode": "file"
}
```

- PREPARING 期 verify_wrapper_target 自动核验：目标路径唯一性 + 目标文件存在性 + 实测 sha256 对照钉值，不符即 `NATIVE_IDENTITY_MISMATCH` fail-closed——**wrapper 钉死即传递性钉死其全部 exec 行为**（RT-0.5-C §5.1 wrapper 指纹必须覆盖 wrapper 与 exec 目标双方的实测结论在 Manifest 层全额兑现）。
- **与 DI-1 D3 调和裁决 B（office）的关系 = 同一安全意图的两种实测形态分支**：soffice wrapper exec 目标为变量形态（`exec $GDBTRACECHECK …`，静态目标 0 个）→ binary_target_sha256 在场即 fail-closed 全灭 → 退守分层承载（Manifest 钉 wrapper hash + Catalog 目录钉 resolved target）；7z wrapper 静态形态 → **不需要退守，Manifest 双钉全额适用**。两分支共享同一判据 = exec 目标静态形态存在与否（`verify_wrapper_target` 解析器行为），未来 7za/7zr（同为 sh wrapper，本机实测形态同构）入通道时按同一判据复用本裁决。
- seed 回填：archive.7z 条目 wrapper_target.sha256_full 回填实测值（d48e622e…902fad9）+ ocr.tesseract / media.ffprobe / media.ffmpeg 三条 sha256_full 回填实测值（44e4c41d…c35d0 / a2ae3d02…38eff1 / 09190fb7…1708889，2026-11-20 实测，前 16 位与 seed 既有 prefix 逐一吻合）；archive.7za / archive.7zr / pdf.pdftoppm 本批零消费不回填（RT-0.5-C §5.1「不预生成、不编造」纪律）。
- 7za / 7zr 不入本批通道表：zip+7z 输入面 archive.7z 单能力全覆盖（D1），7za（无 Rar）/7zr（仅 7z 原生）无独立消费面；E §4.3.4「Rar → 仅 archive.7z」的路由规则随 Rar 签名面留 DI-3。

**论据**：

- 静态形态实测是裁决的全部前提：`cat /usr/bin/7z` 全文两行（见上），与 soffice 变量形态（DI-1 ADR D3 调和裁决触发事实）逐字对照——同一解析器两种输入形态两种承载分支，无新增 Adapter 代码（verify_wrapper_target 既有）。
- 7z sh wrapper 在 env_clear 白名单下可用：核心工具链仅需 `/usr/bin` 内 sh/d dirname 类基础工具（wrapper 全文无外部命令调用，`exec` 为 shell 内建），PATH=/usr/bin 覆盖——DI-1 soffice 同款结论（PATH=钉值二进制所在目录）的直接复用。

**否决项**：

- 7z 沿用 office 分层承载（binary_target_sha256 不入 Manifest）：静态形态实测成立时分层承载是**无理由的安全面削弱**（目标 ELF 不钉 = wrapper 升级静默换实现），否决。
- 直呼 /usr/lib/7zip/7z 绕过 wrapper：真实 ELF 在 /usr/lib/7zip/（不在 capability 条目 executable_path），目录身份由 Catalog 承载、调用面以 /usr/bin/7z 为登记入口（RT-0.5-E §4.3.5「以绝对路径调用 /usr/bin/7z 等入口，指纹校验覆盖 wrapper 与真实 ELF 双方」原文），否决。
- 本批顺带入 7za/7zr Manifest：零消费面登记 = 死 Manifest 面（登记即承诺维护），否决。

## 生效与关系

- 本 ADR 为 DI-2 实现批次的设计依据；实现任务书 = [DI-2-TASK.md](DI-2-TASK.md)（S0→S8 步骤、判据表、负例矩阵、fixture 策略、红线）。
- 与 DI-1 的关系：通道表/嗅探表/帽表/错误映射/预写回读/submit_correlated 全部复用 DI-1 既有面；channel.rs `channel_steps` 对账镜像随 router.rs 管线扩展**两处同改**（channel.rs:8-16 模块文档纪律），锁定用例 `channel_table_rows_match_d3` / `d3_channel_table_rows_locked` 同步扩行。
- 与 DI-3 的边界：PDF 渲染 OCR（pdftoppm 链）、Rar/tar 家族归档格式路由、exiftool、facade run 审计锚、统一输出外层信封化、RT-3 Policy 判定移交仍留 DI-3；本批承接的归档两阶段防护为 DI-1 批次报告 G.2 遗留项的**归属变更落地**（批头登记）。
- 与 RT-3 的边界：本批仍只承载「可信低风险档受限原生」面（Manifest permission_level + 嗅探签名承载，DI-1 红线 10 沿用）；不明来源归档/媒体/图像的隔离路由不做、不实现、不绕过。

## 复现（本 ADR 实测面命令原文，/tmp/di2-probe）

```bash
mkdir -p /tmp/di2-probe && cd /tmp/di2-probe
# 钉值复测（预期：44e4c41d… / a2ae3d02… / 09190fb7… / f802383f…23ad4 / d48e622e…902fad9）
sha256sum /usr/bin/tesseract /usr/bin/ffprobe /usr/bin/ffmpeg /usr/bin/7z /usr/lib/7zip/7z
# tesseract 面（预期：3 语言包；TSV exit 0；chi_tra exit 1）
/usr/bin/tesseract --list-langs
/usr/bin/magick -size 460x80 xc:white -font /usr/share/fonts/liberation/LiberationSans-Regular.ttf \
  -pointsize 36 -fill black -gravity center -annotate +0+0 "HELLO OCR 123" text.png
/usr/bin/tesseract text.png stdout tsv | head -1            # 预期 TSV header 行
/usr/bin/tesseract text.png stdout -l chi_tra >/dev/null 2>&1; echo $?   # 预期 1
# ffmpeg 面（预期：无显式编码器 → rkmpp 失败零字节；显式 png 抽帧恰 3 帧）
/usr/bin/ffmpeg -hide_banner -loglevel error -f lavfi -i testsrc=duration=2:size=128x64:rate=10 -y bad.mp4; echo $?   # 预期 1
/usr/bin/ffmpeg -hide_banner -loglevel error -f lavfi -i testsrc=duration=2:size=128x64:rate=10 \
  -c:v libx264 -pix_fmt yuv420p -y small.mp4 && mkdir -p fr \
  && /usr/bin/ffmpeg -hide_banner -loglevel error -i small.mp4 -vf fps=2 -vframes 3 -c:v png -y fr/frame-%03d.png \
  && ls fr/                                                  # 预期 frame-001..003
# 7z 面（预期：附连 -o exit 0；空格 exit 7；-o= 造 "=dir" 字面目录；穿越条目逐字入清单）
printf 'alpha\n' > a.txt && /usr/bin/7z a -tzip demo.zip a.txt >/dev/null && mkdir -p ex1 out
/usr/bin/7z x -y -oex1 demo.zip >/dev/null 2>&1; echo $?    # 预期 0
/usr/bin/7z x -y -o ex1 demo.zip >/dev/null 2>&1; echo $?   # 预期 7
/usr/bin/7z x -y -oout demo.zip >/dev/null 2>&1 && ls out/  # 预期 out/ 内 a.txt（cwd 相对裁决）
/usr/bin/7z l -slt demo.zip | grep '^Type '                 # 预期 Type = zip
```
