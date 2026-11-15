# P2-1 任务书：rip-parser 容器族 + Parser Registry + 解析管线 + Document IR 落库

> 状态：生效中（2026-11-15 开批）；判据源 = R2 第四部分 + 第十四部分 Phase 2 行 +
> 四冻结物（03/04/12/13）实施化 + 交接文档第七节。批次报告落
> `docs/rip3/batches/P2-1-BATCH-REPORT.md`（R2 §17.6 A-H 格式）。

## 一、范围（交接文档第七节五项，逐项实施化）

### 1. containerfiles/rip-parser/ 容器族

- 基座：`python:3.12-slim@sha256:4840d514c583a4d3d8cad3940e68dee6f0f6f3635c12fcd0a3d74c21a5b93a54`
  （与 rip-tool 各族同基座，同 digest）。
- 组件：Docling 2.x（PDF/DOCX/PPTX/XLSX/HTML/图片主解析 + OCR 先行）、PyMuPDF
  （EPUB 专用 + PDF 页码辅助；AGPL 已按用户裁决记录于 ADR-0002 台账）、
  Tesseract（OCR 兜底，tessdata eng + chi_sim 构建期烘焙）。
- 模型烘焙：Docling artifacts 构建期下载进镜像（HF_ENDPOINT=https://hf-mirror.com
  受控拉取，DOCLING_ARTIFACTS_PATH 固定路径），运行期零网络（network=none 实测断言）。
- 入口脚本：`parser_tools.py` argv op 分派（沿 core_tools.py/src_tools.py 惯例），
  输出 D-TC-03 帧协议（stdout 一行 JSON，Tool Contract result 信封），IR 即
  `schemas/document-ir/ir-v1.schema.json` 实例。
- Manifest 双份：`rip.parser.docling.yaml/json`（多格式 parse + detect + ocr op）；
  digest 台账三处同步（manifest yaml/json + containerfiles/README.md 台账行）。
- 构建纪律：受控拉取（pypi 官源 200 实测、hf-mirror 200 实测、debian apt 实测在批内
  核对）；构建后 `podman inspect` 实取 digest 登记；宿主 `pacman -Q` 基线零变化。

### 2. Parser Registry + Provider 接口（R2 10.3）

- 落点：`crates/intel-db/src/parser/`（document domain 同 crate，验收骨架
  `cargo test --offline -p intel-db --test parser_pipeline` 随之落位）。
- Trait：`DocumentParser`（parse → DocumentIR）与 `OCRProvider`（ocr → 文本/块），
  领域层不写死供应商；`ParserRegistry` 按 magic bytes + 扩展名双证据路由格式到
  parser id（docling/pymupdf/tesseract/native_markdown），词表与 IR parser_metadata
  一致。
- 执行面：解析容器经既有 T1 podman_host 通道派发（继承 podman_host 隔离全套：
  read-only rootfs / cap-drop ALL / network none / 资源帽 / tmpfs nosuid；继承
  看门狗与预算帽），不复刻第二套容器执行逻辑（原则 G）。
- CLI：`rp-doc parse`（触发解析）与 `rp-doc parse-status`（查进度）子命令。

### 3. 解析管线 = workflow 定义（R2 4.6）

- 九态状态机映射为 workflow 定义（definition-v1），复用既有 checkpoint/断点恢复/
  retry/Human 节点，不新建队列系统。九态到节点映射表（补 Phase 0 审计任务 5 交付物）：

| 九态 | workflow 落点（节点/机制） | 状态载体 |
|---|---|---|
| DISCOVERED | 导入完成即为此态（parser_version 空）；触发解析 = 创建 workflow run | document_revisions.parse_state + run pending |
| VALIDATING | tool 节点 `detect`（magic bytes/格式词表/大小帽） | 节点 succeeded |
| QUEUED | run created 后执行前（FamilyLimiter 并发帽在 tool 派发层） | run 状态 pending/running |
| PARSING | tool 节点 `parse`（rip.parser.docling 按 registry 路由） | 节点 succeeded |
| NORMALIZING | tool 节点后置 Rust 面 IR 校验 + 规范化（fail-closed） | 提交服务 |
| EXTRACTING | 与 PARSING 合并（IR blocks 已含结构抽取；知识抽取属 Phase 3 面不占节点） | 说明项 |
| QUALITY_CHECK | condition 节点（extraction_quality.overall 判定分支） | 节点分支 |
| REVIEW_PENDING | human 节点（poor/degraded 或风险标记进审核，018 awaiting_approval 继承） | awaiting_approval |
| APPROVED | 提交服务回填 parser_version/parse_state/blocks/assets | end 节点 + 回填 |
| FAILED/CANCELLED/QUARANTINED | run failed/cancelled 与 quarantine 分支 end；失败原因进 run 节点日志 | run 终态 |

- 提交模式（ADR-0003）：workflow run 内 Tool 节点产出 IR artifact；PG 落库由驱动侧
  提交服务在 run 终态后执行（幂等键 = revision_id + IR content_hash；checkpoint 续跑
  与重试继承 workflow-engine，落库可重放不重解析）。
- 长解析分片：Parallel 节点按 PDF 页段/文档分片扇出（Phase 2 首版为单分片直通 +
  扇出骨架在位，多分片调度随规模实测批补）。

### 4. Document IR v1 落库（04 号冻结物实施）

- `schemas/document-ir/ir-v1.schema.json` 按 04 号文档第二节至第三节逐字落盘；
  Rust 侧 include_str! 内嵌 + `tool_registry::jsonschema_lite` 校验（Tool Contract
  同口径）；Python 侧容器内 jsonschema 自检（双通道）。
- 校验负例八类（04 §六.1）：未知字段 / 未知 block_type / 缺失 content / heading
  级缺失 / table 缺 rows / 坏 sha256 形 / 环（平台侧树校验拒）/ 坏版本。
- 落库面：blocks 进 `document_blocks`（V0002 已在位）；PG-0003 新增
  `document_assets`（as- 前缀，content_hash FK content_objects，提取物进 CAS）+
  `document_revisions` 增列（`ir_content_hash` FK content_objects = IR JSON 凭证进
  CAS 可重放；`extraction_quality` JSONB；`parse_state` CHECK 词表九态 + FAILED/
  CANCELLED/QUARANTINED）。IR JSON 文本本体进 Object Storage（cas://）。

### 5. 解析质量检查（R2 4.4 extraction_quality + 5.3 风险标记）

- 容器侧 parser_tools.py 产出 self_check 五检项：toc_present / page_coverage /
  empty_ratio / font_glyphs / table_integrity；overall = good/degraded/poor。
- 风险标记：LOW_EXTRACTION_CONFIDENCE（confidence 均值低于阈）与
  POSSIBLE_PROMPT_INJECTION（指令式祈使句形态检测，13 号冻结物第三节）随 IR flags
  输出；poor/风险命中 → REVIEW_PENDING 分支（QUARANTINED 候选进人工审核）。

## 二、验收（R2 第十四部分 Phase 2 行 + 交接文档第七节骨架）

| 验收要点 | 证据载具 |
|---|---|
| PDF/EPUB/MD/DOCX/PPTX/XLSX/HTML/TXT/图片/ZIP 十格式统一入库 | selftest_phase2_parser.sh 容器冒烟逐格式 + IR 校验过 |
| 结构正确（父子不压平/章节树/表格结构） | parser_pipeline 集成测试 roundtrip 断言 |
| 页码可定位（source_mapping/page_number） | PDF/EPUB 样本页码对账断言 |
| 失败可重试 | workflow retry 面 + 幂等键落库重放断言 |
| 解析器版本可记录 | parser_metadata + document_revisions.parser_version 回填断言 |
| 与业务 API 隔离 | 解析面全部经 rp-doc/容器通道，rp_server 业务 API 零改动断言 |
| network=none | manifest network.mode=none + podman_host_e2e 既有断言 + 冒烟容器内无网探针 |

- 门禁：五道门静场串行 + selftest_phase2_parser.sh exit 0 + 生产库指纹红线
  （platform*.db 零触碰）+ RTIS 零触碰（B5 守卫在位）。
- CI：push 后全绿硬门槛；intel-db 集成测试 CI 侧 V-9 跳过（DSN/镜像缺席口径）。

## 三、依赖与坑位（交接文档第五节逐条适用）

- podman 直连宿主（/usr/sbin/podman 在位）；rip-pg17 与 python:3.12-slim 基座镜像
  本地在库。
- vendor 增量：intel-db 新增 tool-engine/tool-registry 依赖若引入新外部 crate，走
  手工增量 vendor 口径（嵌套 .gitignore 清理 + checksum 三方一致，CI #63 实录）。
- dash guard：中文文档禁 em/en dash。
- rtis 审计 B5 守卫：跑 rtis.py test 后 git status 核对还原。
- 门禁静场串行（时序敏感断言 flake 口径）。
- 手动会话 commit/push 待用户明确要求（R2 §17.5 双轨制）。

## 四、批内不做（边界）

- Chunking/向量列/BM25（Phase 3/4）；KnowledgeCard 抽取（Phase 3）；Zip 深度递归
  批量导入编排（Phase 2 仅安全解包 + 逐成员入库骨架）；多分片调度实测（骨架在位）；
  /documents 页解析状态 UI（随 Phase 5/6 页面族）。
