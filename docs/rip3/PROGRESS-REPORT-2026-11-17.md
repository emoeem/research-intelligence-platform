# RIP3 项目进度完成报告（截至 2026-11-17）

> 数据源：docs/HANDOVER-RIP3-CONTINUATION.md（第十一节实录）、docs/releases/
> RELEASE_2026-11-17_RIP3-PHASE4.md、docs/rip3/batches/（P1-1 至 P4-4 共 14 份
> 批次报告）、.auto/log.jsonl；全部数字来自实际执行输出。
> HEAD = 4ca1bdd8（CI run 43908428532 success）。

## 一、总体进度

**RIP 2.0 十阶段：全部收口（前置底账，队列 1-7 全勾销）。**

**RIP3（平台架构升级）：9 个阶段完成 5 个，主线程约完成 60%。**

| 阶段 | 状态 | 批次 | CI |
|---|---|---|---|
| Phase 0 架构审计与四冻结 | ✅ 完成 | afe6f5b（用户评审通过） | success |
| Phase 1 文档域与存储 | ✅ 完成 | P1-1 + P1-2 | success |
| Phase 2 文档智能 | ✅ 完成 | P2-1 + P2-2（0179f2bc/71c1f9a5） | success |
| Phase 3 知识治理 | ✅ 完成 | P3-1 ~ P3-4（e4dd5fe6 起） | success |
| Phase 4 混合检索 | ✅ **本周期收口** | P4-1 ~ P4-4（29585fe7 → 4ca1bdd8，14 批中最后 4 批） | success（含两次 CI 红修复实录） |
| Phase 5 工作流与安全研究 | ⬜ 未开 | P5-0 只读审计可先行（补充计划书已入库） | — |
| Phase 6 报告智能 | ⬜ 未开 | — | — |
| Phase 7 知识回流 | ⬜ 未开 | — | — |
| Phase 8 规模测试 | ⬜ 未开 | — | — |

收官闭环（R2 §17 最终验收 + D8 版本决策）待 Phase 8 后；实战校验三面维持用户门控暂缓。

## 二、Phase 4 收口详情（本周期核心交付）

### 2.1 四批内容

- **P4-1 统一检索面**：V0006 BM25 索引（ParadeDB pg_search 0.26.6）+ intel-db
  search 唯一入口（bm25/vector/rrf_fuse/mmr/hybrid）+ 权限前置谓词 SQL 内
  （R2 11.4）+ rp-kc search。
- **P4-2 跨库归并**：unified-search 薄 crate（SQLite 运营核 + PG 情报核跨库
  RRF k=60，12-STORAGE C 层双库各自索引统一归并）+ rp-search-unified CLI +
  JSONB 包含过滤。
- **P4-3 reranker/tokenizers/缓存**：rip-rerank 容器族（Xenova ONNX，四数量
  级分离 0.99969 vs 3.7e-05）+ rerank 全链 + Tokenizer Adapter（est_tokens
  单点切换 HF 真实计数，calibration 偏差 <5%）+ search_cache 双缓存（向量
  缓存 1.493s→0.237s 实证）。
- **P4-4 Benchmark（本批）**：RTIS 语料播种 141 卡（POC 正文零复制红线）+
  qrel 显式 10 查询 + 指标计算器 + 四模式实跑 + JSONB 集成测试转正 +
  sqlite-filters CLI 展开。

### 2.2 基准矩阵（k=10，10 查询 × 4 模式，141 卡语料）

| mode | Recall@10 | P@10 | MRR | NDCG@10 | latency |
|---|---|---|---|---|---|
| bm25 | 0.5415 | 0.3600 | 0.6500 | 0.6613 | 12.0ms |
| vector | 0.5692 | 0.4500 | 0.8667 | 0.7771 | 2.5ms |
| **hybrid** | **0.5812** | **0.4700** | 0.9167 | **0.8212** | 14.2ms |
| rerank | 0.5812 | 0.4700 | **0.9500** | 0.8102 | 5.3s |

结论：hybrid 全面优于单通道（RRF 融合有效），rerank 首位命中率最高（MRR
0.95）但成本为容器推理秒级——取舍面留给 Phase 5 Scan Center 策略。

### 2.3 平台级 bug 修复（基准过程实证暴露）

1. **tool-engine 容器管道死锁**：stdout piped 等退出才读，>64KB 输出死锁
   （embed 23min 挂死实证）→ 并发读线程修复，141 卡嵌入一次通过。
2. **MMR 尺度错位**：RRF 分（~0.03）上做 MMR，多样性项压倒相关性（hybrid
   Recall 0.044）→ 归位 rerank 分（0-1）后 0.5812（R2 6.2 流程原意）。
3. **检索缓存键漏 query**：多查询共用一键全命中首查询缓存（recall 逐位 /10
   复现）→ params 补 query + 缓存版本 v3。
4. **TokenizerAdapter truncation 防御**：显式 with_truncation(None) + 长文本
   全量计数单测（收口后补丁，防御换 tokenizer 目录场景）。

## 三、质量面（实际执行数字）

| 门 | 结果 |
|---|---|
| cargo test --workspace | 714 passed 0 failed（Phase 3 基线 707 → 净增 7）+ 收口后补丁单测 |
| clippy -D warnings | 零告警（rustc 1.99 本地 + 1.100 CI 双版本口径过） |
| python unittest | OK（skipped=11） |
| web build + vitest | build exit 0 + 124 passed |
| selftest | phase2 22/22 / phase3 19/19 / phase4 17/17 / auth 11/11 / phase10 29/29 |
| 生产库红线 | platform*.db 指纹前后一致（selftest 断言，零写） |
| RTIS 对账 | 零回归（检索面只读 rt.py，POC 正文零复制） |

CI 记录：P4-1 红三连（修复实录）→ P4-2 绿 → P4-3 红（vendor 误伤）→ 修复绿
→ P4-4 绿 → 补丁红（rustc 1.100 新 lint）→ 修复绿。**全部失败均为环境/工具
链面，无一为业务逻辑缺陷。**

## 四、交付物清单（Phase 1-4 累计）

- **容器族 ×5**：rip-pg17（PG17+pgvector+pg_jieba+pg_search）/ rip-parser
  0.1.2 / rip-gov 0.1.0 / rip-embed 0.1.0 / rip-rerank 0.1.0（digest 钉死，
  台账三处同步）。
- **迁移**：migrations_pg V0001-V0006（documents/chunks/vectors/治理域/
  card_vectors/BM25 索引）；SQLite 020+ 运营面小迁移。
- **CLI**：rp-doc / rp-kc（create/relate/list/show/review/score/dedup-scan/
  batch-review/minhash-scan/embed-cards/semantic-scan/search 四模式/bench）/
  rp-search-unified / rp-context / rp-tool / rp-workflow。
- **工作流**：wf-review-card-v1（审核双闸两分支）/ wf-parse-pipeline-v1
  （解析管线两分支）。
- **领域资产**：KnowledgeCard 十型关系 / 质量分六权重 / D1-D5 五级去重 /
  Context Broker L0-L3 + Tokenizer Adapter + 内容寻址缓存 / Trace 六 ID。
- **基准语料**：ws-bench 141 卡 + qrel 10 查询（可重跑播种脚本）。

## 五、剩余工作（按序）

1. **Phase 5**（P5-0 只读审计 → P5-1~P5-9）：Web 工作台升级（Research
   Workspace 从 Placeholder 转正、Scan Center、Coverage 七维、Workflow 模板
   十三步）+ 知识驱动验证引擎 RIP-VRVE（Knowledge Matcher → Planner →
   Provider Adapter：RTIS POC / Nuclei / ZAP）+ 五态结果 + 去重缓存。
   - 前置素材：Yakit 参考图已落 ui-reference/yakit/；**A 组用户当前 UI
     截图两张待提供**（P5-1 前）。
2. **Phase 6 报告智能**：Report IR + typst PDF + 多格式渲染 + Diagram Engine。
3. **Phase 7 知识回流**：Finding→KnowledgeCard 流水线 + Evidence 溯源。
4. **Phase 8 规模测试**：1k/10k/100k 文档基准 + 运营核移植 PG 决策。
5. 遗留清账候选：qrel 深判定集扩充（随 Phase 5）/ rerank 64 条帽扩展策略
   （随 Phase 8）/ 书籍检索专项（随 Phase 6/8）/ RTIS 侧 rtis.py 自测
   10 过 7 败（RTIS 批次，非平台引入）。

## 六、过程资产沉淀

- HANDOVER-RIP3-CONTINUATION.md：第十一节实录 + 第七节 Phase 5 开批速查 +
  五节坑位清单（跨会话逐批带修法沉淀）。
- autoresearch 性能守卫：assemble_ms 中位数 1.863（best，-22.4% vs 基线
  2.399）+ dispatch 0.408；不回归门 assemble>2.879 / dispatch>0.569，每批
  收口跑一次对照（segment 1 进度 3/20，三次全过门）。
- 环境事实记忆：podman 直用 / vendor 全量重写口径 / .gitignore models/ 坑
  位 / 容器并发挂死 / CI rustc 未钉版 / WebUI 测试栈口径。

---

**结论：RIP3 主线 Phase 0-4 五个阶段全部收口且 HEAD CI 全绿；下一动作 =
用户提供 A 组截图后启动 Phase 5 P5-0 只读审计。**
