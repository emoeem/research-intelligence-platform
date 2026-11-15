# RIP3 批次报告：P3-4（审核工作流 + 批量审核面 + Phase 3 收口批，§17.6 格式）

## A 完成内容

1. **审核工作流定义**（workflows/review-card-v1.json，R2 5.4 + HANDOVER 队列
   「审核工作流（复用 018 控制面 + Human 节点）」）：hard_gate condition 节点
   （硬性风险标记命中 → 自动隔离分支 end）+ human_review 节点（018
   awaiting_approval，批准参数 = {decision, reason, reviewer}）→
   human_decided_end；checkpoint/断点恢复继承。两分支真跑：注入卡
   （hard_block=true）自动隔离分支 succeeded（human skipped）；干净卡 human
   停等 paused → resume 注入批准参数 → succeeded。PG 写面由 rp-kc review 在
   run 终态后执行（ADR-0003 裁决 4 同款提交模式；审核日志 knowledge_review_
   log 由 rp-kc 落）。
2. **rp-kc batch-review**（批量审核面，R2 5.4 批量审核）：--decision
   approve|quarantine|reject + --status（缺省 CANDIDATE）+ --min-score 覆盖
   地板 + --dry-run 预览；硬标记与质量分地板双闸逐卡预检（与 decide 内闸一
   致），跳过面逐卡列原因（fail-closed 不静默）。
3. **rp-kc 启动迁移引导**（run_migrations refinery 幂等，治理表面随启动就绪
   ——V0004/V0005 表面缺失即治理 CLI 全面失效的坑位收口）+ RP_DEBUG 错误链
   （postgres 参数序列化面根因定位）。
4. **selftest_phase3_governance.sh** 19/19 exit 0（治理全链：D2/D3/D4 扫描 +
   审核双闸 + 批量审核 dry-run/实批 + 审核工作流两分支 + 三集成目标 + 生产库
   指纹红线）。

## B 架构变化

零新面：审核门 = workflow 定义（复用 018 Human），PG 写 = 治理 CLI（同
ADR-0003 驱动侧提交模式）；批量审核 = card_repo::decide 既有闸的批量编排。

## C 文件变化

新增：workflows/review-card-v1.json；apps/server/selftest_phase3_governance.sh；
docs/rip3/batches/P3-4-BATCH-REPORT.md；docs/releases/
RELEASE_2026-11-15_RIP3-PHASE3.md（阶段总账）。
修改：crates/intel-db/src/bin/rp-kc.rs（batch-review + 迁移引导 + RP_DEBUG +
BOOL_FLAGS 扩 dry-run——布尔旗标坑位二现实录，先例 --force）；
docs/HANDOVER-RIP3-CONTINUATION.md；docs/README.md。

## D 数据库变化

零迁移（V0004/V0005 已承载；审核工作流定义落 workflow_definition 016 面）。

## E 测试结果（全部实际执行）

| 门 | 命令 | 结果 |
|---|---|---|
| 工作流两分支 | rp-workflow put + run ×2 + resume | 硬标记自动隔离 succeeded；human 停等 → resume succeeded |
| batch-review | dry-run（DISCOVERED 面 ≥1 卡）+ 实批 | applied ≥1，注入卡 QUARANTINED 在库 |
| selftest | bash apps/server/selftest_phase3_governance.sh | 19/19 PASS exit 0 |
| 定义校验 | rp-workflow validate review-card-v1 | PASSED（5 节点 4 边） |
| 门禁五道 | 见阶段总账第三节 | 全绿 |

## F 性能结果

零新增资源面（定义 + CLI 编排；无新容器/迁移）。

## E2 CI 结论

push 后 GitHub CI 全绿为硬门槛（随 push 回填，同前批口径）。

## G 遗留问题

1. 审核工作流的 PG 写面为 run 终态后驱动执行（与 ADR-0003 一致）；run 内
   直写节点的必要性随 Phase 5 批次评估（Evidence/Finding 节点族）。
2. 批量审核的审核人留痕为 CLI 参数（--reviewer）；多审核人工作流化随
   Phase 5 Human 面增强。
3. D4 阈值校准（0.83-0.92 区间数据点已落库）随 Phase 4 Benchmark。
4. evidence_level 导入侧接线（导入面产卡携带 K 分级）——P3 批次内卡片级
   evidence_level 列与 CLI 参数已在位，外部导入面（source_sync 泛化）接线
   随 Phase 4/7 批（00-审计缺口 4 的完整清账节点）。

## H 下一阶段依赖与顺序

Phase 3 收口（RELEASE + push + CI 硬门槛）→ Phase 4 混合检索开批（窗口 3：
统一 Search API + RRF/MMR 融合 + reranker + tokenizers + Benchmark）。
