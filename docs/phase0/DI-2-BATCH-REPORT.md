# DI-2 批次验收报告：Document Router 三通道扩全（OCR / 媒体 / 归档两阶段）+ 负例矩阵扩全

- 日期：2026-11-20
- 状态：批次收口（S1→S8 全步执行；评审 通过 + 3 medium/2 low 修订全落地 + 定向门两轮内全绿；本地离线验收面全绿；CI 未跑未 watch，E2 待回填）
- 任务书：[DI-2-TASK.md](DI-2-TASK.md)（十二项判据、十条红线、fixture 三层策略）；设计裁决：[DI-2-ADR-CHANNELS.md](DI-2-ADR-CHANNELS.md)（D1-D8 + S2 内存帽阶梯定值回写注记）
- 前置：DI-1 已收口（[DI-1-BATCH-REPORT.md](DI-1-BATCH-REPORT.md)）；SEC-0 依赖安全插批（commit 76c6feb6）在本批开工前已改 lock 基线（见坏消息 1）
- 结构说明：按 A-K 十一节标准模板（同 DI-1/RT-1/SEC-0 批报口径），坏消息单列于文首；本报告会话 2026-11-20 独立复跑，E 节全部数字为终态代码（含评审修订 3 条补测）fresh 实测

## 坏消息（单列）

1. **lock 基线与任务书原口径不符（本批开工前即成立）**：任务书红线 1 原记 `[[package]]` 400 / `source =` 375；实测为 **399 / 374**。差异来源 = SEC-0 插批 commit 76c6feb6（DI-1 收口之后、本批开工之前）：hashlink 0.9.1→0.12.2 升级链中 hashbrown-0.14.5 随淘汰移除——本报告会话双向实测：`git show 76c6feb6^:Cargo.lock | grep -c` = **400 / 375**、`git show 76c6feb6:Cargo.lock` = **399 / 374**、当前工作树 = **399 / 374**（S1/S7 双点复核，零新增包名零 vendor diff）。任务书红线 1 已带勘误块（DI-2 评审修订 4 回填）；本批恒定 399/374。
2. **两条内存帽纸面初值在本机不可用，已按 ADR D5 预留的 S2 阶梯定值流程实测定值**（ADR D5 回写注记在案）：rip.media.probe 256Mi→**1536Mi**（FAIL@1152Mi / OK@1280Mi，本报告会话复验 exit 127/0/0 吻合），rip.media.frames 512Mi→**3Gi**（FAIL@2048Mi → OK@3072Mi，批内阶梯记录，本报告会话未复跑该档）。原因是 ffmpeg-full 约两百个动态库的映射全量计入 RLIMIT_AS 地址空间帽；128x64 抽帧 3Gi 上界主体是库映射面而非解码工作集。真实内存收紧缝（systemd-run MemoryMax / RSS 域强制）留 DI-3。
3. **任务书引用的 bomb.zip 探针值（解压量 67108864 = 64 MiB）恰好等于严格 `>` 判据的单条目帽、不触发闸 4**（ADR D4 表明文「单条目 Size > 64 MiB」）；fixture 重造为 64 MiB + 1 KiB（Size = **67109888**，本会话 `7z l -slt` 实测逐字在位），闸 4 触发面钉死，新钉值 258e1c3c… 入 fixture_pins。任务书 §2.2 引用的是设计裁决员的 /tmp 探针非入库钉值。
4. **workspace 全量回归中 context-broker 两套件的 Manifest 计数常数 29→34 被强制更新**（`acceptance_no_stuffing.rs` 1 处 + `catalog_sources.rs` 6 处）——S4 五份 Manifest 入列使登记面 29→34，该对账面在任务书 §3.1 落点清单之外但 workspace 全绿门（红线 9）强制；更新同时构成五份新件过九条 registry 校验的登记回执（34 件全 register 成功）。
5. **ARCHIVE_LIST_NO_OUTPUT 负例载具改 harness 注入**：零条目空 zip 的全部字节为 ASCII+NUL——NUL 是合法 UTF-8 码点，全 ASCII 空归档落嗅探 Text 签名（D6 判定序）到不了归档通道；载具改用 DI-1 pdfinfo_pages_unparsed 先例同款 harness（rip.archive.list 克隆 binary=/usr/bin/true，exit 0 但 stdout 恒空 → 头块解析失败路径，确定性）。零条目 7z 容器实测造不出（7z 至少写入一个目录条目）。
6. **SKIP 行本会话零实际产出**：本机五 CLI 全部 available（tesseract/ffprobe/ffmpeg/7z/magick），守卫走绿路径；SKIP 格式 `SKIP <test>: capability <id> status=<status>` 由代码锚承载（three_channel_e2e.rs:116-152 require_capability），本会话未触发即如实标注，异机语义与 DI-1 同构。
7. **本批改动尚在工作树（未 commit / 未 push）**：本报告会话实测 `git rev-parse HEAD` = `git rev-parse origin/main` = **76c6feb6**、`git status -sb` 首行 `## main...origin/main`（无 ahead/behind）——CI 见过 SEC-0 字节但从未见过本批任何字节，fresh-checkout 行为未验证（E 节为本地实跑口径）。

## A. 完成

**交付面**（S1→S8 全步落地）：

| 交付物 | 内容 | 判据/裁决 |
|---|---|---|
| OCR 通道（D2） | `route_ocr` 显式请求位入口（route() 签名与 DI-1 三管线零改动）+ `route_ocr_chain`（identify 门步复用 rip.doc.image.meta 零改 → 50 MP OCR 派生像素帽 → tesseract TSV 单 run）；语言路由 `OCR_LANG_ROUTES`（catalog language 集唯一权威，chi_tra 面前置门 ROUTER_OCR_LANG_MISSING 零 run 行）；TSV 重组 / mean_conf / low_conf_words 入 metadata.ocr | 判据 1-5 |
| 媒体通道（D3） | route_media（ffprobe JSON 常步 → 畸形透传 / 时长 300 s / 视频流分支 / 单帧 16 MP / 帧数预算 min(⌈2×d⌉,32) 五门序 → ffmpeg 全常量抽帧条件步）；FfprobeMeta 字符串/整数形态实测解析；纯音频 metadata-only 诚实分支 | 判据 6-9 |
| 归档通道（D4） | route_archive（7z 两阶段：`l -slt` 列目 → 四闸（路径 / 链接 / 条目数 512 / 解压总量 256 MiB + 单条目 64 MiB）→ `x -y -oout` 解包本 run out/ → post-extract symlink 终检）；ArchiveListMeta 逐字 Path 原样解析；ARCHIVE_LIST_NO_OUTPUT 空清单面 | 判据 10-11 |
| 嗅探表扩 8 签名（D1） | 7z/wav/avi/mp4/webm/ogg/flac/mp3 精确字节判定 + **ZipOther → Archive 路由变更**（D1 行为变更登记，判据 4 迁移）；RIFF+WEBP 与 mp3 帧同步启发式不收（否决项维持）；tar/gzip/Rar fail-closed | D1 表 |
| dispatch 请求位（S3） | `input.ocr` 透传 route_ocr（dispatch.rs:378-414，object 形态可选 language，非法形态 VALIDATION 拒绝）；守卫消息九工具清单更新（dispatch.rs:309）；`RouterManifests::REQUIRED` 4→9（router.rs:73 `[&str; 9]`） | dispatch 面 |
| 五份 Manifest（S4） | rip.doc.image.ocr / rip.media.probe / rip.media.frames / rip.archive.list / rip.archive.extract（绝对路径 + 64hex 钉值；archive 两份 binary_target_sha256 双钉；result_mode file + network:none + resources）——本会话逐件 JSON 实测核验（E.6 表） | D2/D3/D4/D7 |
| 测试面（S5） | fixture 5 件钉值入库（fixture_pins 18 件面）+ 判据 1-12 逐项测试（e2e +13 测、sniff_boundary +4 测、fixture_pins +5 测、channel_gate +2 测、normalize +7 测）+ 判据 4 迁移（DI-1，ADR D1）+ dispatch 迁移与增 3 测 | 全部 |
| 批报 + 索引（S8） | 本报告（A-K 十一节）+ docs/README.md 三行索引（git diff 实测 +3 行在位）+ DI-3 缝登记（G/H 节） | S8 |

**本报告会话 fresh 复跑门禁（2026-11-20，仓库根，全部实跑实贴；终态代码 = 评审修订 3 条补测后）**：

```text
$ cargo test --offline -p document-router -- --test-threads=1
lib 26 + channel_gate 10 + fixture_pins 7 + sniff_boundary 11 + three_channel_e2e 26（48.17s）= 80 passed 0 failed
$ cargo test --offline -p mcp-server --test native_rust_dispatch -- --test-threads=1
8 passed 0 failed（2.11s；DI-1 5 保持 + DI-2 增 3：ocr_request_bit / chi_tra / zip_routes_to_archive_gate）
$ cargo test --offline -p tool-engine native_rust -- --test-threads=1
native_rust_e2e 20 passed 0 failed（6.16s；tool-engine 全 crate 零 diff 零回归门）
$ cargo clippy --offline -p document-router -p mcp-server --all-targets -- -D warnings
Finished `dev` profile（零告警）
$ rustfmt --edition 2021 --check <16 个本批 .rs 落点>
FMT-CLEAN
$ cargo test --offline --workspace
158 套件（grep -c 'test result: ok'）1049 passed 0 failed
```

**静态门 fresh 复核（本会话实测）**：`grep -c '^\[\[package\]\]' Cargo.lock` = **399** / `grep -c '^source = '` = **374** / `git status --porcelain -- vendor/` = **0 行** / `git status --porcelain -- integrations/rtis/ migrations/` = **0 行** / `git diff --name-only -- crates/tool-engine/ Cargo.lock vendor/` = **0 文件** / 五 CLI 钉值 sha256 复测与五份 Manifest 逐一吻合（E.6）/ `git status --porcelain` 全量 = **28 项**（C 节逐文件）。

注：批内记录 77 / 1046 与本报告 fresh 实测 80 / 1049 的差 = 评审修订补测 3 条（e2e +1、channel_gate +2），数字口径以本报告（终态代码）为准。

## B. 架构与复用（三通道 × document-router 既有成员扩展面，零 adapter 增量）

- **同 crate 扩展，不建新 crate**（ADR D1）：三通道全部落 `crates/document-router` 既有成员，复用 DI-1「嗅探 → 通道表 → 帽门 → 编排 → 规范化」同构管线；`Channel` 枚举增 `Ocr | Media | Archive` 三值、通道表 Manifest 集 4→9（router.rs:73 `REQUIRED: [&str; 9]`），channel_steps 对账镜像「两处同改」由 `d3_channel_table_rows_locked` / `channel_table_rows_match_d3` 既有锁定用例承载。
- **Router 零 `std::process::Command`**（红线 7，本会话 grep 实测）：document-router/src 全部命中仅 lib.rs:10 与 router.rs:6 两处 doc comment；三通道每一步均经 `execute_native_rust_via_engine`（drive_step 预写 in/ → submit_correlated 自锚定 → adapter 派发）。
- **七处既有面零复制复用**：drive_step/drive_step_at、read_stdout、terminal_reject、RouterReject 构造器防线、submit_reject、new_run_id、staged_name；route_pdf/route_office/route_image 三管线本体零 diff（红线 9，DI-1 全部既有测试在 80 绿内保持）。
- **tool-engine 全 crate 零 diff**（红线 8）：`-oout` cwd 相对裁决消费 adapter 既有 cwd=ws.root 事实（native_rust.rs:1051/:1133），不改 adapter——由行为级 e2e 钉死（判据 10 解包产物断言落本 run out/），adapter cwd 语义漂移即测试红。native_rust_e2e 20 全绿（含 wrapper_double_pin_dispatch）承载。
- **语言路由 catalog 唯一权威**（D2）：`resolve_ocr_language`（router.rs:694）经 CapabilityProvider::get(ocr.tesseract) 读 capabilities[].kind==Language 集（chi_sim/eng/osd）；条目缺席 fail-closed 拒绝；补装 chi_tra 后目录重探路由自动放行，Router 零改码。
- **`{{ocr_lang}}` 为整 token 槽位 + Manifest 二道收紧**：argv `["{{input_path}}","stdout","-l","{{ocr_lang}}","tsv"]`，语言值由 Router 经路由表生成后填 input（input_schema enum chi_sim|eng|chi_sim+eng），调用方永远不能把任意串送进 argv——对账钉 `ocr_manifest_lang_slot_locked_to_router_routes`（channel_gate）。
- **引擎既有预算面零新增**：OCR max_output_bytes 4 MiB（D5 #10 单点放宽）经 Manifest resources 自动生效（`resolved_budget` 既有传导），读帽/结果侧截断全部复用；混合 token 槽位扩展维持否决（`-o{{dir}}`/`{{fps}}` 类），mixed-token 实测否决矩阵（`-o` 空格 exit 7 / `-o=` 造字面目录）留档 ADR D4。

## C. 文件清单（`git status --porcelain` 实测逐文件，2026-11-20 本报告会话，28 项 = 15 M + 13 ??）

**修改（15）**：

```text
 M crates/context-broker/tests/acceptance_no_stuffing.rs   # Manifest 计数 29→34 一处（坏消息 4，§3.1 外强制）
 M crates/context-broker/tests/catalog_sources.rs          # Manifest 计数 29→34 六处（坏消息 4，§3.1 外强制）
 M crates/document-router/src/channel.rs                   # Channel +Ocr/Media/Archive、帽常量族、OCR_LANG_ROUTES、镜像扩行、内联测试 2→3
 M crates/document-router/src/error.rs                     # D6 表十个新码构造器、内联测试 4→5
 M crates/document-router/src/normalize.rs                 # TessTsvMeta/FfprobeMeta/ArchiveListMeta 三解析面、内联测试 6→13
 M crates/document-router/src/router.rs                    # route_ocr/route_media/route_archive 三管线 + resolve_ocr_language + 四闸 + 终检 + REQUIRED 4→9
 M crates/document-router/src/sniff.rs                     # 八新签名 + ZipOther→Archive 路由变更（D1 行为变更）+ 家族提示扩展；内联测试 5→5 零增
 M crates/document-router/tests/channel_gate.rs            # 镜像扩行 + 帽值锁定 + 评审修订 2 条 manifest↔常量对账（8→10）
 M crates/document-router/tests/fixture_pins.rs            # 钉值面 13→18 + 五件新形状测试（2→7）
 M crates/document-router/tests/sniff_boundary.rs          # 八签名/tar-gzip fail-closed/Rar-7za-7zr 零命中/mismatch 新家族（7→11）
 M crates/document-router/tests/three_channel_e2e.rs       # 判据 4 迁移 + di2_* 13 新测（含修订补测 16MP 门）（13→26）
 M crates/mcp-server/src/dispatch.rs                       # input.ocr 请求位透传（:378-414）+ 守卫消息九工具（:309）
 M crates/mcp-server/tests/native_rust_dispatch.rs         # zip 门测迁移 + OCR/chi_tra 增 3 测 + 守卫消息断言更新（5→8）
 M docs/README.md                                          # DI-2 任务书/ADR/批报三行索引（git diff +3 行实测在位）
 M manifests/capabilities/host-catalog-seed.json           # 四条 sha256_full 实测回填（E.6 核验）
```

**新增（13）**：

```text
?? crates/document-router/tests/fixtures/text-ocr.png      # OCR 正例锚 460x80（钉值 2c558f8d…）
?? crates/document-router/tests/fixtures/tone.wav          # 8044 B 手写 PCM WAV（钉值 fc736cfe…）
?? crates/document-router/tests/fixtures/evil2.zip         # 闸 1 穿越负例锚（钉值 ea1c7fbe…）
?? crates/document-router/tests/fixtures/bomb.zip          # 闸 4 解压总量负例锚（钉值 258e1c3c…）
?? crates/document-router/tests/fixtures/symlink.zip       # 闸 2 符号链接负例锚（钉值 063d1827…）
?? docs/phase0/DI-2-TASK.md                                # 任务书（评审修订 4 勘误回填后版本）
?? docs/phase0/DI-2-ADR-CHANNELS.md                        # 设计裁决 D1-D8 + S2 内存帽回写注记
?? docs/phase0/DI-2-BATCH-REPORT.md                        # 本文件
?? manifests/tools/rip.doc.image.ocr.json                  # tesseract TSV 单步（ocr.tesseract，1Gi/120s/4MiB）
?? manifests/tools/rip.media.probe.json                    # ffprobe JSON 常步（media.ffprobe，1536Mi S2 定值）
?? manifests/tools/rip.media.frames.json                   # ffmpeg 抽帧条件步（media.ffmpeg，3Gi S2 定值）
?? manifests/tools/rip.archive.list.json                   # 7z l -slt（archive.7z，双钉）
?? manifests/tools/rip.archive.extract.json                # 7z x -y -oout（archive.7z，双钉）
```

**porcelain 面 vs 任务书 §3.1 预期**：§3.1 全部落点在位；两处增量 = context-broker 两测试文件（坏消息 4，workspace 全绿门强制）+ DI-2-TASK/ADR 两份设计批文档（批次账目）。零其它漂移（migrations/、vendor/、integrations/rtis/、crates/tool-engine/、生产库 porcelain 均零条目实测）。

## D. 迁移与兼容

**零迁移，逐项实测**：

- `git status --porcelain -- migrations/` = **0 行**：无任何 DB schema/迁移面改动，无新迁移文件；生产库零触碰——`migrations/platform*.db` 五文件 mtime 实测 2026-11-06/07（platform.db 11-07 00:53），全部早于批次日。
- 零新成员零新外部依赖：Cargo.lock `[[package]]` 恒 399 / `source =` 恒 374（嗅探/解析全部手写，未引 mime_guess/file-type/infer/zip 等任何新依赖）；`git diff --name-only -- Cargo.lock` = 0 文件。
- **零 API 破坏**：route() 公开签名与 DI-1 三管线零改动（DI-1 全部既有测试名保持，E.4 回归账）；image 通道无 OCR 请求位时行为不变（`di2_ocr_channel_end_to_end_explicit_request_bit` 与 DI-1 `image_channel_end_to_end_png_and_generated_jpeg` 同图不同路并存于 80 绿内）。

**唯一行为变更（ADR D1 登记）与兼容承载**：`Magic::ZipOther` 由 DI-1 INPUT 拒绝改为路由归档通道。兼容面三件事：

1. DI-1 判据 4 断言迁移（非削弱）：`unsupported_types_rejected_at_gate_with_zero_rows`（three_channel_e2e.rs:559）负例集改 gzip 魔数（0x1F8B）/非 UTF-8 blob/伪 .pdf 文本承载零行拒绝（本会话源读核验），evil.zip → 归档闸 1 负例（DI-1 fixture 复用）、sample.zip/sample.epub → 归档正例（判据 10 迁移正例）——正负例双侧覆盖。
2. route() unsupported_type 消息文案更新（「zip_other requires the DI-3 archive two-phase defense」字样不再准确），消息文本变更非行为变更，channel_gate 断言 code/category 不受影响。
3. dispatch 面同族迁移：`native_rust_dispatch_gate_rejects_unsniffed_magic_without_rows`（gzip 承载）+ `native_rust_dispatch_zip_routes_to_archive_gate`（新路由正向覆盖）。

工具协议零变化：五份新 Manifest 全部走 RT-1 既有 native_rust runtime 形状（validate 既有分支），sk3/registry 九条校验既有面（34 件全过），无 schema/additive 字段变更。

## E. 测试真实结果（本报告会话 2026-11-20 独立复跑，命令原文 + 回显原文）

### E.1 三通道 run 回显（e2e 断言源读核验 + CLI 实测锚）

**OCR（判据 1/2）**——`di2_ocr_channel_end_to_end_explicit_request_bit`（5 OCR 测组 2.29s）断言面源读核验：runs.len()==2 且 runs[0].tool_id=rip.doc.image.meta、runs[1].tool_id=rip.doc.image.ocr，两行 run_row_status 均 SUCCEEDED；统一输出 channel="ocr"、text="HELLO OCR 123"（TSV 重组）、metadata.ocr{language:"eng", requested_language:"en", words:3, mean_conf>90, low_conf_words:0}、单页锚 {page:1, char_start:0, char_end:13}。缺省语言测 `di2_ocr_default_language_routes_chi_sim_plus_eng`：`-l chi_sim+eng`、requested_language="auto"。CLI 实测锚（本会话）：

```text
$ tesseract crates/document-router/tests/fixtures/text-ocr.png stdout tsv 2>/dev/null | awk -F'\t' '$1==5{n++;s+=$11} END{printf "words=%d mean_conf=%.1f\n", n, s/n}'
words=3 mean_conf=93.6
$ tesseract --list-langs 2>&1 | tail -4
List of available languages in "/usr/share/tessdata/" (3):
chi_sim
eng
osd
```

**媒体（判据 6/7）**——CLI 实测锚（本会话，ffprobe JSON 形态）：

```text
$ ffprobe -v error -print_format json -show_format -show_streams <tone.wav> </dev/null | <投影>
format_name= wav duration= 0.500000 bit_rate= 128704
codec= pcm_s16le sample_rate= 8000 channels= 1        ← duration/bit_rate/sample_rate 字符串形态、channels 整数形态
```

e2e：`di2_media_tone_wav_metadata_only_no_frames_step`（ffprobe 单子 run SUCCEEDED、metadata.media{format_name wav, duration_s≈0.5, streams 含 pcm_s16le}、warnings "no video stream"、frames 块不在位）；`di2_media_video_frames_extracted_with_budget`（small.mp4 运行时 libx264 生成 → probe+frames 两子 run SUCCEEDED → out/frame-001..004.png、metadata.frames{extracted:4, capped:false, fps:2}）。

**归档（判据 10/11）**——CLI 实测锚（本会话，`7z l -slt` 违规字段逐字在位）：

```text
$ 7z l -slt <evil2.zip> | grep '^Path '
Path = ok.txt
Path = ../escape.txt                                  ← 闸 1 穿越条目逐字原样
Path = /tmp/di2-fix/abs-escape.txt                    ← 闸 1 绝对路径条目逐字原样
$ 7z l -slt <bomb.zip> | grep -E '^(Path|Size) '
Path = zeros.bin
Size = 67109888                                       ← 64 MiB+1KiB，严格 > 判据触发面（坏消息 3）
$ 7z l -slt <symlink.zip> | grep -E '^(Path|Attributes) '
Path = link-to-etc.txt
Attributes =  lrwxrwxrwx                              ← 闸 2 链接条目
```

e2e：`di2_archive_channel_end_to_end_zip_and_sevenzip`（demo.zip/demo.7z 运行时生成 + DI-1 迁移正例 sample.zip/sample.epub → list+extract 两子 run 均 SUCCEEDED、解包产物落本 run out/——`-oout` cwd 相对裁决行为级钉死）。

### E.2 chi_tra 显式报错（判据 3，三层证据）

1. **Router 前置门**：`di2_ocr_chi_tra_request_rejected_at_gate_without_rows`（zh-Hant/chi_tra/zh-TW 三请求形态）断言源读核验——Err：category=DEPENDENCY、code=ROUTER_OCR_LANG_MISSING、retryable=false、消息含 "user decision" + "silent fallback" + chi_tra、**不含 "fallback to chi_sim"**、`run_row_count(&engine)==0` 零 run 行、chi_sim 兜底零出现；dispatch 面 `native_rust_dispatch_ocr_chi_tra_rejected_without_rows` 同款。
2. **CLI 直呼第二道防线**（本会话实测）：`tesseract text-ocr.png stdout -l chi_tra` → **exit 1**，stderr 原文 `Error opening data file /usr/share/tessdata/chi_tra.traineddata`（不静默）。
3. **语言包事实**（本会话实测）：`tesseract --list-langs` = 3 语言（chi_sim/eng/osd），chi_tra 缺席 = 环境持久事实，只报错不补装（红线 3）。

### E.3 负例矩阵（12 类 runs.len() 留证形态对账）

| # | 负例类 | 承载测试（全部实跑通过） | runs.len() 断言 |
|---|---|---|---|
| 1 | 畸形图像 | `di2_ocr_malformed_image_execution_passthrough_no_tesseract` | 1（identify 行 FAILED 留证，tesseract 未拉起） |
| 2 | 超大图像 121 MP | `di2_ocr_pixel_budget_chain_fails_with_identify_evidence` | 1（identify 行 SUCCEEDED 留证，消息含 121000000/50000000） |
| 3 | 畸形媒体 | `di2_media_malformed_execution_passthrough_and_gzip_gate` 前半 | 1（ffprobe exit 1 EXECUTION 透传） |
| 4 | 超时长媒体 400 s | `di2_media_duration_exceeded_chain_fails_with_probe_evidence` | 1（probe 行 SUCCEEDED 留证，消息含 400/300，ffmpeg 未拉起） |
| 5 | 媒体单帧超 16 MP（修订补测） | `di2_media_pixel_budget_exceeded_chain_fails_with_probe_evidence` | 1（probe 行 SUCCEEDED 留证，消息含 16810000/16000000，ffmpeg 未拉起；载具 = ffprobe stdout harness 免编码 stub——adapter D4 白名单 PATH 下外部命令结构性不可用，stub 纯 shell 内建 echo） |
| 6 | 音频无视频流 | `di2_media_tone_wav_metadata_only_no_frames_step` | 1（非错误：metadata-only + warnings 诚实分支） |
| 7 | 归档穿越/绝对路径/反斜杠 | `di2_archive_four_gates_negative_matrix`（evil2.zip + abs.zip + backslash.zip 三载具） | 1（list 行 SUCCEEDED 留证；消息含条目原名 + 对应 reason 文案："absolute path entry" / "contains a backslash separator (Windows semantics, fail-closed)"——修订补测 2，minimal_zip_named 测试内构造零入库） |
| 8 | 压缩炸弹 | 同上（bomb.zip） | 1（消息含 67109888） |
| 9 | zip 符号链接 | 同上（symlink.zip） | 1（消息含 lrwxrwxrwx） |
| 10 | 条目数超帽 513 | 同上（minimal_zip_entry_count(ARCHIVE_MAX_ENTRIES+1)） | 1（消息含 513） |
| 11 | 列目零输出 | 同文件 harness 段（binary=/usr/bin/true） | 1（EXECUTION ARCHIVE_LIST_NO_OUTPUT，list 行留证） |
| 12 | 不支持格式 + 能力 missing | `di2_unsupported_tar_gate_and_missing_capability_ref`（t.tar 嗅探门零 run 行 + missingcap PREPARING DEPENDENCY 行 FAILED）+ `di2_media_malformed…` 后半 gzip 门 + `unsupported_types_rejected_at_gate_with_zero_rows`（迁移负例集） | 0（嗅探门/语言前置门类）或 1（PREPARING 期信封行） |

四闸统一断言面（源读核验）：category=INPUT、闸码精确、消息含违规事实、"extract phase never submitted"、runs[0].tool_id=rip.archive.list——**解包阶段零提交**。全类负例 runs.len() 断言在 80 绿内实跑通过。

### E.4 DI-1 回归账（per-file `#[test]` 计数本会话实测，HEAD=76c6feb6 vs 工作树）

| 文件 | HEAD（DI-1 终态） | 现在 | 增量归属 |
|---|---|---|---|
| tests/three_channel_e2e.rs | 13 | 26 | +13 全部 di2_*（DI-1 13 测名逐一保持，diff 实测零删除） |
| tests/channel_gate.rs | 8 | 10 | +2 = 评审修订 manifest↔常量对账两测（DI-1 8 测保持） |
| tests/fixture_pins.rs | 2 | 7 | +5 = 五件新 fixture 形状测（corpus 钉值测 13→18 件扩行） |
| tests/sniff_boundary.rs | 7 | 11 | +4 = 八签名/tar-gzip fail-closed/Rar-7za-7zr 零命中/mismatch 新家族 |
| src/channel.rs（内联） | 2 | 3 | +1（语言路由/帽常量） |
| src/error.rs（内联） | 4 | 5 | +1（新码构造器形状） |
| src/normalize.rs（内联） | 6 | 13 | +7（TSV/ffprobe/-slt 解析） |
| src/sniff.rs（内联） | 5 | 5 | 0（DI-1 嗅探单测零改） |
| **合计 document-router** | **47** | **80** | DI-1 47 全保留 + DI-2 新增 33 |

- **唯一行为断言变更** = ADR D1 登记的判据 4 迁移（测试名保持、负例集更换，D 节）；DI-1 三通道 e2e（pdf 两 run / office 三 run 级联 / image 含运行时 JPEG / 页帽 / 像素帽 / stdout 帽 / wrapper env 白名单 / missing capability / sniff 权威 / xlsx 反例）全部在 80 绿内实跑通过。
- **tool-engine 零 diff 回归门**：native_rust_e2e 20 passed（17 RT-1 + 3 DI-1 file_mode）——`git diff --name-only -- crates/tool-engine/` = 0 文件，20 全绿证明 adapter 触碰零增量。
- **dispatch 回归**：8 passed = DI-1 5（gate/input_too_large/pdf chain/tool_outside_channel_table/other_rejection_surface）+ DI-2 3。
- **workspace 全量**：158 套件 1049 passed 0 failed（context-broker 计数对账两套件随批更新后全绿，坏消息 4）。

### E.5 clippy / rustfmt / 静态门（本会话实测）

```text
$ cargo clippy --offline -p document-router -p mcp-server --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.44s   （零告警）
$ rustfmt --edition 2021 --check <5 src + 4 router tests + dispatch + native_rust_dispatch + 2 context-broker>
FMT-CLEAN
$ grep -c '^\[\[package\]\]' Cargo.lock && grep -c '^source = ' Cargo.lock
399
374
$ git status --porcelain -- vendor/ | wc -l
0
```

### E.6 登记回执（S4，本会话逐件核验）

**五份 Manifest 实测核验表**（python 逐件 JSON 读取，与源码常量对账）：

| Manifest | binary | binary_sha256 | target 双钉 | capability_ref | argv_template | mem | timeout | max_output |
|---|---|---|---|---|---|---|---|---|
| rip.doc.image.ocr | /usr/bin/tesseract | 44e4c41d9986e8b2 | — | ocr.tesseract | `["{{input_path}}","stdout","-l","{{ocr_lang}}","tsv"]` | 1Gi | 120s | **4 MiB** |
| rip.media.probe | /usr/bin/ffprobe | a2ae3d0209aec739 | — | media.ffprobe | `["-v","error","-print_format","json","-show_format","-show_streams","{{input_path}}"]` | **1536Mi**（S2 定值） | 30s | 1 MiB |
| rip.media.frames | /usr/bin/ffmpeg | 09190fb791fb0db8 | — | media.ffmpeg | `["-hide_banner","-loglevel","error","-i","{{input_path}}","-vf","fps=2","-frames:v","32","-c:v","png","out/frame-%03d.png"]` | **3Gi**（S2 定值） | 60s | 1 MiB |
| rip.archive.list | /usr/bin/7z | f802383f13b8331a | **d48e622e9009acdb** | archive.7z | `["l","-slt","{{input_path}}"]` | 256Mi | 60s | 1 MiB |
| rip.archive.extract | /usr/bin/7z | f802383f13b8331a | **d48e622e9009acdb** | archive.7z | `["x","-y","-oout","{{input_path}}"]` | 512Mi | 120s | 1 MiB |

- 五 CLI 钉值本会话 sha256sum 实测逐一吻合：tesseract 44e4c41d…c35d0 / ffprobe a2ae3d02…38eff1 / ffmpeg 09190fb7…1708889 / 7z wrapper f802383f…23ad4 / 7z exec 目标 d48e622e…902fad9；`/usr/bin/7z` 全文两行 `#!/bin/sh` + `exec /usr/lib/7zip/7z "$@"`（静态形态，D7 双钉前提）本会话 cat 复验。
- **seed 四条 sha256_full 回填核验**（python 逐条读 host-catalog-seed.json）：ocr.tesseract / media.ffprobe / media.ffmpeg binary_identity.sha256_full 三条 + archive.7z wrapper_target.sha256_full 一条，与实测值逐字吻合；pdf.pdftoppm / archive.7za 保持 EMPTY（零消费不回填不编造）。
- **registry 登记回执**：context-broker `acceptance_no_stuffing_full_surface_and_rtis_scale`（计数 34）+ `p4_2_registry_cards_and_briefs_cover_registered_surface` 两套件随 workspace 全绿实跑通过——五份新件过九条校验的执行性回执（坏消息 4 同源）。
- **fixture 钉值**：fixture_pins.rs 18 件面（DI-1 13 + DI-2 5），五件新钉值本会话 sha256sum 独立复核逐一吻合（2c558f8d…/fc736cfe…/ea1c7fbe…/258e1c3c…/063d1827…）；运行时生成面（demo.zip/demo.7z/7z a、small.mp4/libx264、t.tar/bsdtar）带 require_capability 守卫（SKIP 语义见坏消息 6），负例放大面（513 条目 zip、400 s WAV）测试内构造零入库。

## E2. CI（待 push watch 回填）

- **本批未 commit、未 push**（坏消息 7）：HEAD = origin/main = **76c6feb6**，CI 见过 SEC-0 字节但从未见过本批任何字节；`.github/workflows/ci.yml` 在位（ls 实测）。
- **回填口径**：push 后单 commit（回退 = `git revert` 一体撤销 router/dispatch/tests/五 Manifest/seed/文档，嗅探路由单点回退 = routed_channel() 一行还原，任务书 §8）；CI 终态 + 异机暴露面随 watch 记账于本节。
- **预期暴露面（本地经验预判，未验证）**：五 CLI 钉值面在 CI runner 与本机 Arch 构建不同（RT-1 批 CI 先例：capability_catalog 三测曾因 runner 无 poppler/7z 钉值不同而红，已加 host_pin_guard）——本批同族面 = 五份 Manifest 钉值 + fixture 钉值（fixture 已入库不红）+ tesseract 语言包面；守卫沿用 require_capability/host_pin_guard 口径应接住，watch 时核验。

## F. 性能（通道延迟面，无吞吐基线，如实）

- **本批未建吞吐基线、未跑任何吞吐/派发延迟基准（未建即写未建）**：下列数字全部是测试载具墙钟（libtest 自报 finished in），包含引擎/工作区 setup + CLI spawn，**不是** dispatch 延迟或生产吞吐数字，只作通道工作量级参考。
- **分通道 e2e 墙钟（本会话实测，libtest 自报）**：OCR 5 测组 **2.29s** / MEDIA 5 测组 **3.12s** / ARCHIVE 2 测组 **5.29s**（归档组含运行时 7z 生成 + 双解包正例，是三通道最重面）；全 e2e 26 测 48.17s（`--test-threads=1` 纪律下的串行口径）。
- **门步成本锚（本会话实测）**：identify `-ping` 对 text-ocr.png 三次实测 8/8/7 ms——OCR 像素帽门步是毫秒级头部读，50 MP 判定零解码成本（D2 论据）的量级印证。
- **dispatch / adapter 面**：native_rust_dispatch 8 测 2.11s、native_rust_e2e 20 测 6.16s（含 ≥2.5s 超时击杀窗口）——量级参考非基准。
- **内存帽面**：prlimit RLIMIT_AS 阶梯本会话复验 ffprobe FAIL@1152Mi（exit 127）/ OK@1280Mi / OK@1536Mi（Manifest 定值 1536Mi = 1.2× 余量）；ffmpeg 3Gi 档为批内阶梯记录（FAIL@2048Mi → OK@3072Mi），本报告会话未复跑该档，如实标注。RLIMIT_AS 地址空间帽与 RSS 语义差异 = ffmpeg-full 两百动态库映射面，真实收紧留 DI-3（G 节）。
- 相邻面区分（防冒认）：autoresearch assemble_ms 1.863 等数字属 context-broker tokenizer 计量链（.auto 记账），与本批三通道面零交集，不作本批性能证据。

## G. 遗留与风险

1. **CI 未 watch**（坏消息 7 / E2）：push 后 watch 回填为批次最终判据缺口；五 CLI 钉值异机面随 watch（E2 预判）。
2. **隔离面留 RT-3**（红线 10 维持）：本批只承载「可信低风险档受限原生」；不明来源归档/媒体/图像的隔离档位路由不做、不实现、不绕过——归档四闸/post-extract 终检是对**已放行输入**的纵深防御，不构成信任判定。
3. **UI 接入留 DI-3**：dispatch 请求位（input.ocr）与九工具承载面已就绪，但 Web 工作台无任何 DI-2 工具的 UI 面（I 节零截图）；三通道从 UI 发起 + 统一输出信封化（facade run 审计锚 + 外层 envelope）一并留 DI-3。
4. **内存域真实收紧留 DI-3**：prlimit RLIMIT_AS 库映射面问题（坏消息 2），systemd-run MemoryMax（RSS 域）替换路径 RT-0.5-E §2 注 A 既有登记。
5. **chi_tra 补装后路由放行未实测**：catalog 重探自动放行路径（D2 论据）本批未跑（补装属用户决策，红线 3 只报不装）；代码路径与 chi_sim 同构（resolve_ocr_language 同一候选集判定）。
6. **tesseract 50 MP 上界内存未实测**：1Gi 帽以 36800 像素样本可用 + 初值维持；50 MP 密集文本 OCR 真实内存峰值未测（判据 2/4 验帽门不验内存上界），DI-3 渲染 OCR 批时一并阶梯。
7. **两条诚实分支无 e2e（评审 low，不阻塞收口）**：OCR mean_conf<60 warnings 分支（router.rs:649-655）与帧预算 capped=true 分支（router.rs:821-825，16 s<duration≤300 s）——与已测分支同构、解析级单测在位（tesseract_tsv_counts_low_conf_words / 帽常量锁），DI-3 建渲染 OCR/长视频批时各补一条正例。
8. **SKIP 分支零实际产出**（坏消息 6）：异机语义未实测；emptydir 7z 零条目分支由 harness 承载（坏消息 5），真实零条目输入面留观察。

## H. 下一批依赖（WSW-3 History+Repeater 或 RT-2）

**本批产物可消费面**：DI-3 可直接消费的登记缝（ADR「生效与关系」节 + A.表 S8 行）——① Rar/tar 家族嗅探签名位（sniff.rs 判定序追加位在八签名之后、UTF-8 检查之前；四闸管线已建，落地 = Magic::Rar/Tar 变体 + routed_channel 分支 + E §4.3.4「Rar → 仅 archive.7z」）；② pdftoppm 渲染 OCR 管线位（route_ocr 请求位形态已建，pdf.pdftoppm seed sha256_full 留空待首用时回填）；③ facade run 审计锚位；④ 统一输出外层信封化位；⑤ 媒体域扩展缝（转码白名单/experimental 禁令已在 rip.media.frames known_limits 登记）；⑥ 内存域缝（G.4）；⑦ 诚实分支正例位 + DI-3 负例矩阵登记缝（G.7）。

- **WSW-3（HTTP History + Repeater）**：定义 = docs/rip3/WEB-SECURITY-WORKBENCH-PLAN.md:605（History 列表详情、过滤搜索、请求编辑、重放、Tab 历史）；承接缝 = WSW-2-ADR D4（`wsw_exchange` 元数据表留 WSW-3）与 D9（audit-engine Emit 接线留 WSW-3）。与 DI-2 **零直接耦合**（WSW-3 不消费三通道面）；共享写面只有 CI 门禁与（若新增依赖）Cargo.lock/vendor——DI-2 零 lock 增量无冲突遗留，新依赖按 SEC-0 七步 vendor + deny 双门。
- **RT-2（Python Worker 原生环境）**：定义 = docs/phase0/PHASE-ROADMAP.md:786（项目级 venv、依赖锁、启动探测、版本/哈希记录、协议一致性测试）。与 DI-2 零耦合（DI-2 不触 python_worker 档）；RT-1→DI-1→DI-2 确立的「runtime 档位逐个真实化 + 门禁命令原文入报告 + 帽值实测定值」纪律模板直接复用。
- **DI-3（若先行）**：消费上列七缝；lock 基线以 **399/374** 为准并注明来源 commit 76c6feb6（任务书红线 1 勘误块口径，防 stale 假阳性）。

## I. 浏览器截图

**本批无 UI 面，零截图（如实）**：改动面 = router/sniff/channel/normalize/error 源码 + dispatch 请求位 + 测试 + 五 Manifest + seed + 文档（C 节实测清单），porcelain 无任何 web/ 前端条目；无浏览器 e2e 用例故无截图可附。三通道唯一输出面 = 统一输出 JSON + ToolRun 行（E 节命令行实测回显承载）。web 工作台接入三通道工具面属 DI-3（G.3）。

## J. RTIS 对账

**零接触，实测**：

- `git status --porcelain -- integrations/rtis/ migrations/` = **0 行**（本会话实测）；全仓 porcelain 无任何 `rtis` 命中（红线 5 达成）。
- mtime 全部早于批次日：`integrations/rtis/knowledge.db` = 2026-11-18 05:36、`integrations/rtis/rtis.py` = 2026-11-09 09:45（stat 实测）。
- 本批不触知识摄取/检索链路；RTIS 旧链条数对账**未复跑**（无触碰面故无需，同 DI-1/SEC-0 §J 口径），本报告不以其为门。

## K. 教训

1. **纸面帽值必须实测定值，且地址空间帽的失真方向是「显著偏小」**：ffprobe 纸面 256Mi 实测 1152Mi 才能跑（ffmpeg-full 约两百动态库映射全量计入 RLIMIT_AS），偏 6 倍——DI-1 教训 K-1 的再次兑现；凡是「进程 + 动态库」型工具，内存帽阶梯以 FAIL/OK 边界实测定值，纸面值只能当起点。
2. **计数型基线不变量必须带来源 commit**：任务书红线 1 写 400/375（DI-1 收口口径），SEC-0 插批在批次间隙合法改掉它——照抄旧口径会造成假阳性门禁。任何 lock/计数/钉值不变量落任务书时注明来源 commit 与时点（本批勘误块 + ADR 回写为标准动作）。
3. **设计探针值 ≠ 入库钉值，边界判据要用 off-by-one 探针复核**：bomb.zip 引用值 64 MiB 恰好等于严格 `>` 帽不触发闸 4——「等于帽」与「超帽」差 1 KiB；引用它处数值作 fixture 时先算边界再入库，闸的触发面用重造值（67109888）钉死。
4. **argv 模板槽位与工具 CLI 语法冲突时，先跑语法矩阵再定模板形态**：`-o` 空格 exit 7 / `-o=` 造字面目录 / 附连唯一合法的实测矩阵一次性否决了「空格/等号组合」直觉；混合 token 不可组合的 adapter 语义下，字面常量 + 行为级 e2e 钉死（cwd=ws.root）是唯一正解，不为模板便利扩 adapter。
5. **负例载具必须跑过嗅探判定序再选**：空 zip 全 ASCII+NUL 落 Text 签名到不了归档通道（NUL 是合法 UTF-8 码点）——「负例类真实可产」与「负例类可达目标门」是两件事；不可构造输入（零条目 7z）用 harness 注入承载并如实登记载具变更。
6. **双源事实（Manifest argv ↔ Router 常量）必须钉对账测试**：帽常量锁只锁代码侧，manifest 漂移时 metadata 会如实报告错误事实而实际按漂移值执行——恒等关系本身要有一条测试（评审修订补测 3），否则两源漂移零红灯。
7. **行为级 e2e 的 run 行计数断言有真实抓缺陷能力**：route_archive 成功路径漏 push(extract) 被判据 10 的 runs.len()==2 直接命中——子 run 账目断言不是形式，是编排层缺陷的第一道网。
8. **评审覆盖缺口可用 harness 注入当批闭环**：16 MP 媒体门补测用 ffprobe stdout stub（pdfinfo_pages_unparsed 先例形态）免编码即得，负例门只消费探测输出——为负例门造真实大输入是浪费，stub 注入 + 一次真实负例可产性验证（4100x4100 实造）组合即可。

---

## 复现

```bash
cd /media/emo/code/pensting/research-platform
# 门禁（预期：80 / 8 / 20 全绿 + clippy 零告警 + fmt clean）
cargo test --offline -p document-router -- --test-threads=1
cargo test --offline -p mcp-server --test native_rust_dispatch -- --test-threads=1
cargo test --offline -p tool-engine native_rust -- --test-threads=1
cargo clippy --offline -p document-router -p mcp-server --all-targets -- -D warnings
rustfmt --edition 2021 --check crates/document-router/src/*.rs crates/document-router/tests/*.rs crates/mcp-server/src/dispatch.rs crates/mcp-server/tests/native_rust_dispatch.rs crates/context-broker/tests/acceptance_no_stuffing.rs crates/context-broker/tests/catalog_sources.rs
cargo test --offline --workspace          # 预期 158 套件 1049 passed 0 failed
# 静态门（预期：399 / 374 / 0 / 0 / 0 / 28）
grep -c '^\[\[package\]\]' Cargo.lock
grep -c '^source = ' Cargo.lock
git status --porcelain -- vendor/ | wc -l
git status --porcelain -- integrations/rtis/ migrations/ | wc -l
git diff --name-only -- crates/tool-engine/ Cargo.lock vendor/ | wc -l
git status --porcelain | wc -l
# 基线偏移实证（预期父提交 400/375 → 76c6feb6 399/374）
git show 76c6feb6^:Cargo.lock | grep -c '^\[\[package\]\]'
git show 76c6feb6:Cargo.lock | grep -c '^\[\[package\]\]'
# 实测锚（预期 words=3 mean_conf=93.6；chi_tra exit 1 + stderr Error opening data file）
tesseract crates/document-router/tests/fixtures/text-ocr.png stdout tsv 2>/dev/null | awk -F'\t' '$1==5{n++;s+=$11} END{printf "words=%d mean_conf=%.1f\n", n, s/n}'
tesseract crates/document-router/tests/fixtures/text-ocr.png stdout -l chi_tra >/dev/null 2>&1; echo $?
# 归档违规字段（预期 ../escape.txt / abs-escape.txt / Size = 67109888 / lrwxrwxrwx）
7z l -slt crates/document-router/tests/fixtures/evil2.zip | grep '^Path '
7z l -slt crates/document-router/tests/fixtures/bomb.zip | grep -E '^(Path|Size) '
7z l -slt crates/document-router/tests/fixtures/symlink.zip | grep -E '^(Path|Attributes) '
# 内存阶梯复验（预期 1152 exit=127 FAIL / 1280 exit=0 / 1536 exit=0）
prlimit --as=$((1152*1024*1024)) -- /usr/bin/ffprobe -v error -print_format json -show_format crates/document-router/tests/fixtures/tone.wav </dev/null >/dev/null 2>&1; echo $?
prlimit --as=$((1536*1024*1024)) -- /usr/bin/ffprobe -v error -print_format json -show_format crates/document-router/tests/fixtures/tone.wav </dev/null >/dev/null 2>&1; echo $?
# 钉值复现（预期五值与 fixture_pins.rs 逐一吻合）
sha256sum crates/document-router/tests/fixtures/{text-ocr.png,tone.wav,evil2.zip,bomb.zip,symlink.zip}
```
