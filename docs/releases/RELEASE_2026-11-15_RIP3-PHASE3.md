# RELEASE：RIP3 Phase 3 知识治理（阶段收口）

| 项 | 值 |
| --- | --- |
| 日期 | 2026-11-15 |
| 阶段 | RIP3 Phase 3（知识治理，R2 第五部分 + 第十四部分 Phase 3 行；窗口 3 开批） |
| 批次 | P3-1（治理域落库 + 质量分 + 审核面 + D2，e4dd5fe6）+ P3-2（D3 容器 worker + 编排面）+ P3-3（D4 语义候选 + 向量面）+ P3-4（审核工作流 + 批量审核） |
| 阶段提交 | e4dd5fe6（P3-1）+ flake 记录/修复两批 + P3-2/P3-3/P3-4 各批（提交短哈希见 git log 与各批次报告） |
| 前置 | Phase 2 收口（0179f2bc/71c1f9a5）；03 号冻结物 1.5/1.6/1.8 实施化；ADR-0003 |
| CI | P3-1 批 run 43713355070 success 3m16s（红二连修复实录：unused import + const 断言）；P3-2 批 run 43714615323 success（mcp-server 负载 flake，本地静场 7/7 复跑全过，rerun 权限缺失如实登记后同树重跑）；P3-3 批 run 43715885192 success 3m0s；本收口提交结论随 push 回填 |

## 一、批次汇总

### P3-1（治理域落库）
- migrations_pg V0004：knowledge_cards（类型八值/审核七态/验证/K0-K4/source_trust
  四值/ai_derived 必标原则 C/normalized_hash D2/quality_flags 七类风险标记）+
  document_cards N:M + knowledge_relations 十型（UNIQUE 幂等）+
  knowledge_review_log（append-only 审核日志）。
- domain::kcard（八词表 + quality_score 六权重纯函数 20/15/20/15/15/15 +
  RiskFlag::is_hard_block）+ DomainError 十变体。
- intel-db card_repo（D2 normalize 规则：CRLF 统一/行首全角剥离/行尾剥离/
  空行折叠/首尾剪除，代码缩进保留；D5 硬性 CONTRADICTS 命中即标记 + APPROVED
  回退 CANDIDATE + 日志；审核双闸 LOW_QUALITY_FLOOR=0.35 + 硬标记；dedup-scan
  候选边不自动合并）+ rp-kc CLI 七子命令。
- 集成测试 5/5（真 PG）。

### P3-2（D3 容器 worker）
- rip-gov 容器族（datasketch==2.0.0，ADR-0002 台账落地；char-bigram ∪ 词元
  shingling；条目 5000/单条 64KiB 帽；只产候选）+ intel-db gov_worker 编排面
  （同 parser 链）+ rp-kc minhash-scan；D1 复用确认（content_objects 面零改动）。
- 集成测试 1/1 + 全链实跑（scanned=4 pairs=2 created=2）。

### P3-3（D4 语义候选 + 向量面）
- migrations_pg V0005（knowledge_card_vectors + HNSW vector_cosine_ops）+
  rip-embed 容器族（onnxruntime 1.32.0/tokenizers 0.23.3/numpy 2.5.4 钉版 +
  bge-small-zh-v1.5 模型构建期烘焙 + M8 embedding_worker 真实后端类零复制导入
  + 无回退面语义不静默降 hash）+ intel-db embed_worker（embed_cards 幂等 +
  semantic_scan 余弦近邻候选）+ rp-kc embed-cards/semantic-scan。
- 独立判别力实证：同义改写对 cosine 0.8358 仅语义面召回（D3 minhash 零召回），
  无关句 0.325。
- 集成测试 1/1 + 全链实跑。

### P3-4（审核工作流 + 批量审核）
- workflows/review-card-v1.json（hard_gate 自动隔离分支 + human 018 复用）两
  分支真跑全通；rp-kc batch-review（双闸逐卡预检 + dry-run + 跳过面列因）；
  rp-kc 迁移引导 + RP_DEBUG 错误链。
- selftest_phase3_governance.sh 19/19 exit 0。

## 二、P3 验收要点对账（R2 第十四部分 Phase 3 行）

| 验收要点 | 结论 | 证据 |
| --- | --- | --- |
| 跨来源重复可识别 | 过 | D1 content_objects（P1 面）+ D2 normalized_hash 候选边（dedup-scan 集成断言）+ D3 NEAR_DUPLICATE（近似对召回）+ D4 语义候选（同义改写对独立召回 0.8358） |
| 不同知识类型不误合并 | 过 | 词表封闭 + 关系是候选不是合并（D2/D3/D4 全部产边；卡片本体行数不变断言） |
| 来源关系完整 | 过 | 关系必带 relation_evidence + relation_source（deducer 名+版本），UNIQUE 幂等 |
| 低质量不进正式上下文 | 过 | decide 双闸（硬标记充分 + 地板 0.35 必要）集成断言（硬标记拒/未计分拒/弱分拒路径单测） |
| 审核可追溯 | 过 | knowledge_review_log append-only（from/to/decision/reason/reviewer 全链断言）+ 工作流 run 留痕 |
| RTIS 对账零回归 | 过 | integrations/rtis 零触碰（B5 守卫 CI 常驻）；15155 摄取面 provenance 测试随 workspace 套件全绿 |

## 三、门禁（阶段收口复测，2026-11-15）

| 门 | 结果 |
| --- | --- |
| Rust 全量 | cargo test --offline --workspace：全绿（P2 基线 688 + 治理域增量；收口复测当日执行） |
| clippy | --workspace --all-targets -D warnings：零告警（const 断言/non_snake_case 两坑位修复实录） |
| Worker unittest | 232 OK（skipped=11） |
| Web | build exit 0；vitest 124 passed（本批零 web 改动） |
| selftest | selftest_phase3_governance.sh 19/19 + phase1_pg（钉值 5）16/16 + phase2_parser 22/22 + phase10_release 29/29 + auth 11/11 |
| 集成 | knowledge_governance 5/5 + gov_dedup 1/1 + gov_semantic 1/1（真容器 + 真 PG） |
| 生产库红线 | platform*.db 指纹一致（selftest 断言） |
| CI | P3 各批 run 全绿（43713355070 / 43714615323 / 43715885192 + 收口提交随 push 回填） |

## 四、已知取舍

1. 关系候选面（D2/D3/D4 全部产边）的合并判定走审核——自动合并被 R2 5.2 硬性
   禁止，本期不提供任何自动合并路径。
2. 质量子面为文档化启发式（映射表 heuristic_inputs 可追溯），权重可配置但
   CLI 未暴露覆盖参数。
3. D4 阈值区间 0.83-0.92 有实测数据点，定值校准随 Phase 4 Benchmark
   （Recall@K 面）。
4. 审核工作流 PG 写面为 run 终态后驱动执行（ADR-0003 模式）；run 内直写节点
   随 Phase 5 批评估。
5. evidence_level 导入侧完整接线（00-审计缺口 4）随 Phase 4/7 批（卡片级列
   与 CLI 参数本期已在位）。
6. CI 负载 flake 一例（mcp-server，P3-2 批）：本地静场复跑全过，记录入批次
   报告；gh run rerun 权限缺失（integration token）如实登记。

## 五、回滚方式

Phase 3 无 SQLite 迁移、无生产库触碰：回滚 = revert 治理面（intel-db
card_repo/gov_worker/embed_worker + rp-kc + domain kcard/error + migrations_pg
V0004/V0005 + containerfiles/rip-gov,rip-embed + workflows/review-card-v1 +
selftest_phase3 随批 revert）；治理容器两族 podman rmi 即清；rip-data/parse/
为运行时数据。RTIS 生产行为零改动（B5 守卫 CI 常驻）。

## 六、队列位置

RIP3 阶段推进：Phase 0（afe6f5b）→ Phase 1（RELEASE_2026-11-15_RIP3-PHASE1）→
Phase 2（RELEASE_2026-11-15_RIP3-PHASE2）→ Phase 3（本 RELEASE）→ Phase 4
混合检索待开批（统一 Search API + RRF/MMR 融合 + reranker + tokenizers +
Benchmark，PG service job 评估随批）。
