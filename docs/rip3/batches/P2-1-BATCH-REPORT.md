# RIP3 批次报告：P2-1（rip-parser 容器族 + Parser Registry + 解析管线 + Document IR 落库，§17.6 格式）

## A 完成内容

1. **rip-parser 容器族**（containerfiles/rip-parser/，ADR-0003 裁决 1）：基座
   `python:3.12-slim@sha256:4840d514…` 与各族同 digest；torch 2.6.0+cpu /
   torchvision 0.21.0+cpu 成对钉版（torchvision 无上界约束曾连带 torch 升到
   pypi 全量轮 2.15.0，首次构建实录，requirements `--extra-index-url` 钉 CPU
   源修复）+ docling==2.150.1 + pymupdf==1.28.2 + jsonschema==4.26.0 +
   tesseract-ocr 5.5.0（eng/chi_sim 构建期 apt 烘焙）；Docling 模型
   `docling-tools models download`（2.150 起命令形变更实录：旧
   download-artifacts 已移除）烘焙 /opt/docling-artifacts；构建期受控拉取
   pypi/pytorch-cpu/debian/hf-mirror 全 200 实测，运行期 network=none；产物
   digest `sha256:9a59eef52bf80aeb2562d9ca32cada455946c775fe92a0b0fb044a527dd8bf7a`
   （版本 0.1.2，digest 变更即版本变更纪律）三处同步（manifest yaml/json +
   containerfiles/README.md 台账行 + provider/测试运行时读取 manifest）。
2. **入口 parser_tools.py** 三 op（argv 分派，D-TC-03 帧协议）：detect（魔法数
   + OOXML/EPUB 细分 + ZIP 成员清单与安全判定：13 号冻结物第二节阈值
   条目数 10000 / 单条 512MiB / 总量 2GiB / 压缩比 100 / 路径穿越拒）；parse
   （格式路由引擎：pdf/docx/pptx/xlsx/html/image 走 Docling，epub 走 PyMuPDF，
   markdown/txt/code 走 native_markdown，实际引擎如实记录 IR
   parser_metadata.parser）；ocr（Tesseract TSV 兜底，词级置信度）。
   stdout 守卫（D-TC-03 帧 = stdout 第一行，库噪声改道 stderr）；
   IR 容器内 jsonschema 自检（双通道之一）后发帧。
3. **Document IR v1 schema**（schemas/document-ir/ir-v1.schema.json，04 号冻结
   物逐字实施）：全层 additionalProperties:false + conditional required
   if/then（heading_level/table/code/image_ref 联动与排他）+ 词表封闭；19
   校验例全绿（本机 jsonschema 4.26 预验 + Rust 侧 12 例）。
4. **intel-db 解析面**（ADR-0003 裁决 2/3/4）：ir.rs（schema include_str! 内嵌 +
   jsonschema_lite::validate_instance + 平台侧树门：parent 引用存在/环检测/
   root children 一致/全树连通 + revision 域 block/asset/ref id 重映射）；
   parser.rs（DocumentParser/OCRProvider trait + route_parse 路由表 +
   PodmanParserProvider：tool-engine submit → runtime-gateway Gateway::dispatch
   → execute_container_via_engine 全链复用，ops 运行库专用 enable_wal）；
   commit.rs（提交服务：CAS 锚定 IR 文本与资产 + document_blocks/assets 落库
   + revision 回填 parser_version/parse_state/extraction_quality/ir_content_hash
   + 幂等键 revision_id + IR 摘要 + 资产内容哈希 fail-closed 对账）。
5. **rp-doc 三子命令**：parse（幂等门：已提交版本跳过 replayed=true，--force
   强制重解析；CAS 读出 + 原始文件名保留 + 资产哈希配对）、parse-status
   （九态/质量/计数视图）、approve（REVIEW_PENDING → APPROVED）。
6. **解析管线 = workflow 定义**（workflows/parse-pipeline-v1.json，九态映射表
   补 Phase 0 审计任务 5 交付）：start→validating(detect)→parsing(parse)→
   quality_gate(condition：overall=poor 或 POSSIBLE_PROMPT_INJECTION)→
   review(human，018 awaiting_approval)→approved_end/quarantined_end；
   rp-workflow 驱动扩展 podman_host manifest 分派（+manifest_dirs 默认挂
   rip-parser 族 + run 级 input_host_files 工作区预置，工具 input_schema 零
   污染 + manifest resources.memory 随帽）。
7. **解析质量检查**（任务书 5 项）：容器侧五检项（toc_present/page_coverage/
   empty_ratio/font_glyphs/table_integrity）+ overall 判级 + LOW_EXTRACTION_
   CONFIDENCE / POSSIBLE_PROMPT_INJECTION 风险标记；poor 或注入命中 →
   REVIEW_PENDING 分支（人工 approve 面在位）。
8. **selftest_phase2_parser.sh** 22/22 exit 0（隔离容器 + 生产库指纹红线）。

## B 架构变化

- intel-db 从纯存储面扩为 Document Intelligence 子系统承载面（ADR-0003 裁决 2）：
  ir/parser/commit 三模块；SQLite 边界修订 = parser Provider 打开专用 ops 运行库
  （tool_run/registry 状态面，RIP_DATA_DIR 口径，显式 enable_wal），生产
  platform*.db 不经任何路径（红线保持，selftest 指纹断言）。
- 执行面零新造：容器派发全走既有 tool-engine/Gateway T1 podman_host 链
  （隔离全套/看门狗/预算帽/FamilyLimiter/tool_run 审计/trace 关联继承）。
- workflow-engine 二进制驱动从 python_worker 单面扩为按 manifest runtime 分派
  （python_worker 逻辑零变更抽出复用；podman_host 新面）。
- 幂等口径：驱动层幂等门（R2 4.6.2 content_hash 键的驱动侧落地）+ 提交侧
  revision_id + IR 摘要重放（事务内 DELETE+INSERT，同 IR 零重复行）。

## C 文件变化

新增：containerfiles/rip-parser/（Containerfile + requirements.txt +
parser_tools.py + 三 manifest yaml/json）+ schemas/document-ir/ir-v1.schema.json
+ migrations_pg/V0003__parser_pipeline.sql + crates/intel-db/src/{ir,parser,commit}.rs
+ crates/intel-db/tests/parser_pipeline.rs + workflows/parse-pipeline-v1.json
+ apps/server/selftest_phase2_parser.sh + docs/rip3/batches/P2-1-TASK.md +
docs/rip3/decisions/ADR-0003-parser-execution-face-and-pipeline-commit.md。
修改：Cargo.toml（postgres with-serde_json-1 特性）/Cargo.lock（零新外部
crate，特性激活元数据 8 行）/containerfiles/README.md（台账两行）/
crates/intel-db/{Cargo.toml,src/lib.rs,src/bin/rp-doc.rs}（+222 行）/
crates/workflow-engine/src/bin/rp-workflow.rs（+285 行）。
vendor：零增量（249 目录不变）。

## D 数据库变化

- PG：refinery 链 V0003（document_assets 表 + document_revisions 增列
  ir_content_hash/extraction_quality JSONB/parse_state CHECK 12 词 +
  document_blocks.structured_payload JSONB + 三索引），每文件事务 +
  checksum；SQLite 运营核零新迁移（020 号仍空闲）。
- 兼容：全部新增式（ADD COLUMN/新表）；本机 dev PG 因 V0003 批内修正
  （structured_payload 列漏项）重置（refinery checksum 纪律，冒烟数据可弃，
  已在报告如实登记）。

## E 测试结果（全部实际执行）

| 门 | 命令 | 结果 |
|---|---|---|
| Rust 单测 | cargo test --offline -p intel-db | 26 passed（ir 12 + parser manifest 3 + rp-doc 5 + object_store 5 + entity 1） |
| 集成测试 | RP_TEST_PG_DSN=… cargo test -p intel-db --test parser_pipeline | 5 passed（roundtrip/幂等重放/注入 REVIEW_PENDING→approve/九态词表门/真容器 E2E） |
| selftest | bash apps/server/selftest_phase2_parser.sh | 22/22 PASS exit 0 |
| workflow 管线 | rp-workflow put + run ×2 + resume | 干净样本 approve 直通 SUCCEEDED；注入样本 quality_gate→human 停等→resume→quarantined_end SUCCEEDED |
| 门禁 1 | cargo test --offline --workspace | 688 passed 0 failed 2 ignored（116 目标） |
| 门禁 2 | cargo clippy --offline --workspace --all-targets -D warnings | 零告警 |
| 门禁 3 | python3 -m unittest discover -s workers/python/tests | 232 OK（skipped=11） |
| 门禁 4 | cd web && npm run build | exit 0 |
| 门禁 5 | cd web && npm run test | 124 passed（19 文件，基线持平，本批零 web 改动） |
| 生产库红线 | selftest 内指纹断言 | platform*.db 零触碰 |

## F 性能结果

- rip-parser 镜像 3.42 GB（`podman images` 实测）；`podman system df` 镜像总量
  9.724 GB / 50 GiB 预算内（本机含既有四族 + PG 族）。
- 解析时延（CPU 容器单跑）：PDF（docling 首载含权重加载）约 8.6 s；
  markdown/txt native 路径亚秒；EPUB（pymupdf）秒级；OCR（tesseract）秒级。
  规模数字属 Phase 8 基准，不在本批。

## E2 CI 结论

入库提交 4139ed58，push 后 GitHub CI run 43708220905 **success（2m44s）**，
四 job 全绿（用户明确授权入库，2026-11-15）：
Rust workspace（测试与 clippy 零警告）✓ 2m41s / 协议一致性载具（REST vs MCP
双通道）✓ 1m32s / Python worker 测试 ✓ 23s / Web 工作台（构建与 vitest）✓ 39s。
intel-db 集成测试 CI 侧按 V-9 跳过（DSN/镜像缺席口径，本地实跑为准）；
CI 无 PG service 遗留口径同 P1。

## G 遗留问题

1. workflow run 产物 → 提交服务的自动接续（--from-run 面）未接：P2-1 两载具
   分离验证（rp-workflow run 面 + rp-doc parse 驱动面），接续随 P2-2 批
   （ADR-0003 裁决 4 的提交服务已就位，差 artifact 取回通道）。
2. 长解析 Parallel 分片扇出：定义与骨架在位（任务书说明项），多分片调度实测
   随规模实测批。
3. 解析质量已知形状：docling 对部分 11pt 中文段落误判 heading（layout 模型
   分类行为，如实记录不遮蔽）；EPUB 页面重排标题去重已做（toc 标题完全一致
   行剔除），HTML 链接文本拆段为 docling HTML 后端行为，P2-2 观察面。
4. /documents 页解析状态视图未挂（页面族统一扩展随 Phase 5/6，P1 已知遗留
   延续）。
5. CI 无 PG service（P1 遗留延续，PG service job 评估仍随 Phase 4 批）。
6. vendor 零增量（postgres with-serde_json-1 纯特性激活），手工增量口径
   经验条目延续。

## H 下一阶段依赖与顺序

P2-2 批（Phase 2 收口候选）：run 产物 → 提交服务接续（--from-run）+ EPUB
段落形状观察 + /documents 解析状态卡（与页面族批次协调）→ Phase 2 收口
（五道门禁复测 + RELEASE 登记 + push + CI 硬门槛，红不进下一阶段）→
Phase 3 知识治理开批（窗口 3：D1-D5 去重 + KnowledgeRelation + 质量分）。
