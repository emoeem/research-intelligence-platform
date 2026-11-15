# RELEASE：RIP3 Phase 2 文档智能（阶段收口）

| 项 | 值 |
| --- | --- |
| 日期 | 2026-11-15 |
| 阶段 | RIP3 Phase 2（文档智能，R2 第四部分 + 第十四部分 Phase 2 行；窗口 2 收口） |
| 批次 | P2-1（rip-parser 容器族 + Parser Registry + 解析管线 + Document IR 落库，4139ed58）+ P2-2（run 产物接续 + 全格式回归 + 收口文档面） |
| 阶段提交 | 4139ed58（P2-1）→ aae7effa（E2 CI 回填）→ 本收口提交 |
| 前置 | Phase 1 收口（aabc3894）；四冻结物实施化判据（04 号 IR / 13 号安全 / 03 号域模型）；ADR-0003（解析执行面与管线提交模式） |
| CI | P2-1 批 CI run 43708220905 success 2m44s（四 job 全绿）；本收口提交 CI 结论见 E2 节 |

## 一、批次汇总

### P2-1（批次报告 docs/rip3/batches/P2-1-BATCH-REPORT.md）

- rip-parser 容器族（版本 0.1.2，digest sha256:9a59eef5 台账三处同步）：
  python:3.12-slim 同各族基座 + torch 2.6.0+cpu/torchvision 0.21.0+cpu 成对
  钉版 + docling==2.150.1/pymupdf==1.28.2/jsonschema==4.26.0 +
  tesseract 5.5.0（eng/chi_sim）+ Docling 模型 `docling-tools models download`
  烘焙（2.150 命令形变更实录）；构建期受控拉取（pypi/pytorch-cpu/debian/
  hf-mirror 全 200 实测），运行期 network=none；镜像 3.42 GB。
- parser_tools.py 三 op（detect/parse/ocr，D-TC-03 帧协议 + stdout 守卫 +
  容器内 jsonschema 自检）；全格式冒烟 10 格式（pdf/docx/pptx/xlsx/html/
  epub/md/txt/image/zip）正负例过；ZIP 安全判定（13 号冻结物第二节阈值）。
- Document IR v1 schema（04 号冻结物逐字实施，19 校验例）+ intel-db
  ir/parser/commit 三模块（双通道校验 + 平台侧树门 + revision 域 ID 重映射 +
  PodmanParserProvider 经 tool-engine/Gateway 全链 + 提交服务幂等重放）。
- migrations_pg V0003（document_assets + revisions 解析三列 parse_state
  CHECK 12 词 + blocks structured_payload）；rp-doc parse/parse-status/
  approve 三子命令（幂等门 + --force）。
- 解析管线 = workflow 定义 workflows/parse-pipeline-v1.json（九态映射，补
  Phase 0 审计任务 5 交付）；rp-workflow 驱动 podman_host 分派扩展
  （input_host_files 预置 + resources.memory 随帽）；两分支真容器全通
  （干净样本 approve 直通；注入样本 quality_gate → human 停等 → resume →
  quarantine end）。
- 解析质量面：五检项 + LOW_EXTRACTION_CONFIDENCE/POSSIBLE_PROMPT_INJECTION
  → REVIEW_PENDING 分支 → approve 人工面。
- selftest_phase2_parser.sh 22/22 exit 0（隔离容器 digest 钉死 + 生产库指纹
  红线）。

### P2-2（批次报告 docs/rip3/batches/P2-2-BATCH-REPORT.md）

- run 产物 → 提交服务接续（rp-doc parse --from-run，ADR-0003 裁决 4 取回
  通道）：workflow_run_node.output_json 取 IR + tool_run 工作区资产（哈希
  即身份）→ 落库；真容器实跑 APPROVED 3 blocks。
- EPUB 形状观察收口（toc 标题去重生效，标题重复段落 0）+ 全格式回归冒烟
  （0.1.2 终镜像稳定）。
- 收口文档面：docs/rip3/05-DOCUMENT-INTELLIGENCE.md + docs/README.md RIP3
  阶段账段 + 本 RELEASE + HANDOVER 状态行。

## 二、P2 验收要点对账（R2 第十四部分 Phase 2 行）

| 验收要点 | 结论 | 证据 |
| --- | --- | --- |
| PDF/EPUB/MD/DOCX/PPTX/XLSX/HTML/TXT/图片/ZIP 统一入库 | 过 | selftest 全格式冒烟 + 容器矩阵逐格式 IR 产出（zip 走 detect 成员清单 + 平台侧逐成员编排口径，R2 4.2） |
| 结构正确（父子不压平/章节树） | 过 | ir.rs 树门（环/引用/root children/连通）+ 回读对账（heading→paragraph 父链一致） |
| 页码可定位 | 过 | PDF/EPUB page_number + location_metadata bbox + source_mapping 落库回读一致 |
| 失败可重试 | 过 | workflow retry 面继承 + 驱动幂等门（replayed=true）+ 提交侧 revision+IR 摘要重放（块数零翻倍断言）+ --force 重解析 |
| 解析器版本可记录 | 过 | parser_metadata（引擎+版本如实）+ document_revisions.parser_version 回填断言 |
| 与业务 API 隔离 | 过 | 解析面全部经 rp-doc/容器通道；rp_server 业务 API 零改动（本批零 web/src 改动，vitest 基线持平） |
| network=none | 过 | manifest network.mode=none（Registry 校验）+ podman_host 既有断言 + 冒烟容器内无网实测 |

## 三、门禁（阶段收口复测，2026-11-15）

| 门 | 结果 |
| --- | --- |
| Rust 全量 | cargo test --offline --workspace：688 passed 0 failed 2 ignored（116 目标；P1 基线 667 + ir/parser/管线面 21） |
| clippy | --workspace --all-targets -D warnings：零告警 |
| Worker unittest | 232 tests OK（skipped=11；P1 基线 211 + parser_tools 容器面不进宿主 unittest，增量来自既有面） |
| Web | build exit 0；vitest 124 passed（基线持平，本批零 web 改动）；prettier 不涉及 |
| selftest | selftest_phase2_parser.sh 22/22 + selftest_phase1_pg.sh（P1 面，回归）+ selftest_phase10_release.sh 29/29 + selftest_auth.sh 11/11 |
| 生产库红线 | platform*.db 指纹一致 + journal_mode=delete 断言（selftest 内） |
| 集成测试 | RP_TEST_PG_DSN=… parser_pipeline 5/5（真容器 + 真 PG 实跑；CI 侧 V-9 跳过口径） |
| CI | P2-1 批 run 43708220905 success 2m44s + P2-2 收口提交 0179f2bc run 43709329153 success（四 job 全绿：Rust workspace 2m48s / parity 1m3s / python workers 23s / web 38s） |

## 四、已知取舍

1. workflow run 产物 → 提交服务接续为取回通道（workflow_run_node.output_json
   为事实载体；tool_run 不持久 result 信封为 005 既有机制，未扩表）。
2. 单解析工具多引擎（rip.parser.docling 名下 Docling/PyMuPDF/native 三引擎，
   实际引擎如实记录 IR parser_metadata.parser）——ADR-0003 一镜多 op 裁决。
3. docling 对部分 11pt 中文段落误判 heading / HTML 链接文本拆段：IR 如实
   记录不遮蔽，观察面（改进随 Phase 4 检索质量批综合评估）。
4. 长解析 Parallel 分片：定义骨架在位，多分片调度实测随 Phase 8 规模批。
5. /documents 页解析状态卡随 Phase 5/6 页面族统一扩展（P1 遗留延续）。
6. CI 无 PG service：intel-db 集成测试 CI 侧 V-9 跳过，本地真容器实跑为准
   （PG service job 评估随 Phase 4 批）。
7. dev PG 因 V0003 批内修正重置（refinery checksum 纪律；冒烟数据可弃）。

## 五、回滚方式

Phase 2 无 SQLite 迁移、无生产库触碰：回滚 = revert 解析面（intel-db
ir/parser/commit + rp-doc parse 面 + migrations_pg V0003 + containerfiles/
rip-parser + workflows/ + rp-workflow 驱动扩展随批 revert）；rip-parser 镜像
卸载即清（podman rmi）；rip-data/parse/ 为运行时数据（删除即清）。RTIS 生产
行为零改动（integrations/rtis/ 本阶段零触碰，B5 守卫在 CI 常驻）。

## 六、队列位置

RIP3 阶段推进：Phase 0（afe6f5b）→ Phase 1（RELEASE_2026-11-15_RIP3-PHASE1）→
Phase 2（本 RELEASE）→ Phase 3 知识治理待开批（窗口 3：D1-D5 去重 +
KnowledgeRelation 10 型 + 质量分六权重 + 审核工作流复用 018 控制面 +
RTIS 对账零回归 15155 口径）。
