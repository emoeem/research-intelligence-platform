# RIP3 批次报告：P3-1（知识治理域落库 + KnowledgeCard/Relation + 质量分 + 审核面 + D2，§17.6 格式）

## A 完成内容

1. **migrations_pg V0004**（知识治理域，03-DOMAIN-MODEL 1.5/1.6）：knowledge_cards
   （knowledge_type 八值词表 / review_status 七态 / verification / evidence_level
   K0-K4 / source_trust 四值 / ai_derived 必标（原则 C）/ normalized_hash D2 键 /
   quality_flags 七类风险标记 TEXT[]）+ document_cards N:M（版本锚定）+
   knowledge_relations（十型词表 + UNIQUE(src,dst,type) 幂等 + relation_evidence/
   relation_source 必填）+ knowledge_review_log（append-only 审核日志，R2 5.4
   审核原因/历史/回退可追溯）。
2. **domain::kcard**（纯类型 + 词表 + 质量分纯函数，crates/domain/src/kcard.rs）：
   KnowledgeCardId/RelationId 新类型（宏模式沿用 document.rs 惯例）、八词表
   （KnowledgeType/ReviewStatus/VerificationStatus/RelationType/RiskFlag/
   EvidenceLevel/SourceTrust + QualityWeights/QualityInputs）+ `quality_score`
   六权重纯函数（R2 5.3 初始权重 20/15/20/15/15/15，validate 护栏权重和=1，
   输入 clamp 0-1）+ RiskFlag::is_hard_block（POSSIBLE_PROMPT_INJECTION 硬性
   隔离，13 号冻结物第三节）。DomainError 扩十变体。
3. **intel-db card_repo**（crates/intel-db/src/card_repo.rs）：
   - D2 规范化规则（normalize_text：CRLF 统一 / 行首全角空格剥离（CJK 排版
     空白，代码缩进保留）/ 行尾空白剥离 / 三连空行折叠 / 首尾空行剪除）+
     normalized_hash sha256；
   - create_card（DISCOVERED 落库 + 哈希随行）/ link_document（N:M 幂等）/
     add_relation（幂等 + **D5 硬性：CONTRADICTS 命中即 dst 打 CONTRADICTED
     标记 + APPROVED 回退 CANDIDATE + 审核日志留痕**）/ list_by_review（SQL 内
     workspace 前置谓词）/ get_card（关系 + 文档 + 审核日志详情）/ decide
     （转移矩阵 fail-closed：approve 被 `LOW_QUALITY_FLOOR=0.35` 地板与硬标记
     双闸阻断；日志 append-only）/ score_card（可追溯启发式子面：trust/K 分级
     映射表 + completeness/verifiability/timeliness/independence/accuracy 依
     卡片事实）/ dedup_scan_same_content（D2 扫描产 SAME_CONTENT 候选边，
     **不自动合并**——R2 5.2 合并走审核）。
4. **rp-kc CLI**（新 bin，rp-* 家族惯例）：create/relate/list/show/review/
   score/dedup-scan 七子命令，JSON stdout。
5. **集成测试**（crates/intel-db/tests/knowledge_governance.rs，5/5 真 PG 实跑）：
   迁移幂等到 V0004；D2 同内容建候选边 + 幂等二跑零新建；D5 CONTRADICTS 全链
   （标记 + 回退 + 日志 + 关系幂等）；审核双闸（硬标记拒 / 未计分拒 / 正常流
   APPROVED + 日志 + 分数过地板）；workspace 前置谓词（跨 workspace 不可见）。
6. selftest_phase1_pg.sh 迁移链钉值随 V0004 推进 3→4。

## B 架构变化

- intel-db 增治理域存取面（card_repo），与既有 document_repo/commit 并列；
  与 domain::knowledge（SK-1 SQLite 摄取面）边界如实登记：kcard = RIP3 PG 治理
  域，knowledge = 既有摄取面不动（03 号冻结物第四节）。
- 治理语义落点：**关系是候选不是合并**（D2/D3/D4 全部产边进审核）；低质量不进
  正式上下文 = 双闸（硬标记充分条件 + 质量分地板必要条件，R2 5.3「不能只凭总分
  判定可信」）。

## C 文件变化

新增：migrations_pg/V0004__knowledge_governance.sql；crates/domain/src/kcard.rs；
crates/intel-db/src/card_repo.rs；crates/intel-db/src/bin/rp-kc.rs；
crates/intel-db/tests/knowledge_governance.rs。
修改：crates/domain/src/error.rs（十新变体 + Display）；crates/domain/src/lib.rs
（kcard mod）；crates/intel-db/src/lib.rs（card_repo mod）；
crates/intel-db/src/document_repo.rs（timestamp_to_string pub(crate)）；
crates/intel-db/Cargo.toml（rp-kc bin 注册）；apps/server/selftest_phase1_pg.sh
（钉值 4）。

## D 数据库变化

- PG：V0004（knowledge_cards + document_cards + knowledge_relations +
  knowledge_review_log + 五索引，CHECK 词表与 domain 词表单一来源一致）；
  无 SQLite 新迁移（020 号仍空闲，运营核红线）。

## E 测试结果（全部实际执行）

| 门 | 命令 | 结果 |
|---|---|---|
| domain 单测 | cargo test --offline -p domain | 62 passed（+5 kcard 新例）+ bin 7 |
| intel-db lib | cargo test --offline -p intel-db --lib | 24 passed（+3 card_repo 新例） |
| 集成测试 | RP_TEST_PG_DSN=… --test knowledge_governance | 5 passed（真 PG 实跑） |
| rp-kc 冒烟 | create ×2 → dedup-scan（建边 1）→ score（0.57）→ approve | 全链 JSON 输出正常 |
| 回归 | intel-db 既有 3 目标（lib/document_domain/parser_pipeline） | 全绿 |

## F 性能结果

小样本秒级；治理扫描为 O(N) 分组 + 关系幂等，规模数字属 Phase 8 基准。

## E2 CI 结论

push 后 GitHub CI 全绿为硬门槛（本批 commit 与四 job 结论随 push 回填，同
P2 批口径 gh CLI）。

## G 遗留问题

1. D3 近似重复（datasketch MinHash+LSH 容器 worker）→ P3-2 批。
2. D4 语义候选（pgvector 卡片向量 + bge embedding 容器化）→ P3-3 批。
3. 审核工作流（workflow 定义复用 018 Human 节点 + 批量审核面）→ P3-4 批。
4. evidence_level 由导入面携带（00-审计缺口 4 的导入侧接线）→ 随 P3-2/3 批。
5. quality_score 子面为文档化启发式（可追溯映射表在 heuristic_inputs），
   权重可配置但 CLI 未暴露覆盖参数（按需后补）。

## H 下一阶段依赖与顺序

P3-2（D3 datasketch 容器 worker + dedup 面整合）→ P3-3（D4 语义候选）→
P3-4（审核工作流 + 批量审核）→ Phase 3 收口（selftest_phase3 + 五道门禁 +
RELEASE + CI 硬门槛）。
