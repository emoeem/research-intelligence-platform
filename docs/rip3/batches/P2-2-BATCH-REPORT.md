# RIP3 批次报告：P2-2（run 产物接续 + 全格式回归 + Phase 2 收口，§17.6 格式）

## A 完成内容

1. **run 产物 → 提交服务接续**（P2-1 遗留 1 关闭，ADR-0003 裁决 4 取回通道）：
   `rp-doc parse --from-run <workflow_run_id>`：读 ops 库
   workflow_run_node.output_json（parsing 节点，tool 信封 result 数据含 IR）+
   tool_run(trace_id = workflow run_id, status=SUCCEEDED) 工作区 out 目录资产
   （哈希即身份，缺失 fail-closed）→ 提交服务落库。真容器实跑：新 workflow
   run（wfr-7e65c06c，rip-parser 0.1.2）→ from-run 提交 → APPROVED，3 blocks
   落库（tool_run_id 链接与 ws_out 在列）。旧 E2E run 的 IR 被树门拒
   （children 语义修复前的容器产物）如实佐证校验器 fail-closed 行为。
2. **EPUB 段落形状观察收口**（P2-1 遗留 3 观察面）：当前镜像 EPUB 样本
   5 blocks（synthetic title 根 + 2 heading + 2 paragraph），toc 标题去重
   生效（标题重复段落 0），overall=good。
3. **全格式回归冒烟**：pdf/docx/pptx/xlsx/html/ocr/zip 正负例在 0.1.2 镜像
   复跑全过（P2-1 矩阵结论在终镜像上稳定）。
4. **Phase 2 收口文档面**：docs/rip3/05-DOCUMENT-INTELLIGENCE.md（实施面
   path 级登记）+ docs/README.md 增 RIP3 阶段账段 + 阶段总账
   docs/releases/RELEASE_2026-11-15_RIP3-PHASE2.md + HANDOVER-RIP3-CONTINUATION
   状态行。

## B 架构变化

零新架构面：--from-run 为 ADR-0003 裁决 4 既定提交模式的取回通道补全
（workflow_run_node.output_json 为 run 内产物的事实载体，tool_run 不持久
result 信封的机制如实登记于报告 E 节）。

## C 文件变化

修改：crates/intel-db/src/bin/rp-doc.rs（parse_from_run 函数 + --from-run/
usage）；docs/README.md；docs/HANDOVER-RIP3-CONTINUATION.md；docs/rip3/
batches/P2-1-BATCH-REPORT.md（E2 CI 结论回填，独立 docs 批 aae7effa）。
新增：docs/rip3/05-DOCUMENT-INTELLIGENCE.md；docs/rip3/batches/
P2-2-BATCH-REPORT.md；docs/releases/RELEASE_2026-11-15_RIP3-PHASE2.md。

## D 数据库变化

零迁移（P2-2 纯取回通道与文档面；PG 链仍为 V0001-V0003）。

## E 测试结果（全部实际执行）

| 门 | 命令 | 结果 |
|---|---|---|
| from-run 实跑 | rp-workflow run（0.1.2 镜像）→ rp-doc parse --from-run → parse-status | APPROVED，3 blocks，replayed=false |
| 幂等门 | 已提交版本再 from-run | replayed=true（revision + IR 摘要键命中） |
| 树门 fail-closed | 旧语义容器 run 的 IR 提交 | 拒（children 不一致），佐证校验器在位 |
| EPUB 观察 | 容器 parse sample.epub | 5 blocks，标题重复段落 0，overall=good |
| 门禁五道 | 见阶段总账第三节（入库当日复测） | 全绿 |

## E2 CI 结论

入库 push 后 GitHub CI 全绿为硬门槛（本批 commit 与四 job 结论见阶段总账
RELEASE 登记；push 与 watch 同 P2-1 口径 gh CLI）。

## F 性能结果

零新增资源面（--from-run 复用既有 ops 库与工作区；无新容器/镜像）。

## G 遗留问题

1. 长解析 Parallel 分片调度实测（Phase 8 规模批，P2-1 遗留延续）。
2. /documents 页解析状态卡（Phase 5/6 页面族统一扩展，P1 遗留延续）。
3. docling 中文段落 heading 误判与 HTML 链接拆段（观察面，IR 如实记录）。
4. CI 无 PG service（Phase 4 批评估，P1 遗留延续）。

## H 下一阶段依赖与顺序

Phase 2 收口（RELEASE 登记 + push + CI 硬门槛）→ Phase 3 知识治理开批
（窗口 3：D1-D5 去重 + KnowledgeRelation 10 型 + 质量分六权重 + 审核工作流
复用 018 控制面 + RTIS 对账零回归 15155 口径）。
