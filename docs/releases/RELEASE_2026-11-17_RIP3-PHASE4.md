# RELEASE：RIP3 Phase 4 混合检索（阶段收口）

| 项 | 值 |
| --- | --- |
| 日期 | 2026-11-17 |
| 阶段 | RIP3 Phase 4（混合检索，R2 第六部分 6.1-6.8 + 第十四部分 Phase 4 行） |
| 批次 | P4-1（统一检索面 PG 侧）+ P4-2（跨库归并薄 crate + JSONB 过滤）+ P4-3（reranker + tokenizers + 缓存）+ P4-4（Benchmark + 收口，本批） |
| 阶段提交 | 29585fe7（P4-1）+ ee9ee79c（P4-2）+ e8682ba4/da46e7e7（P4-3 + CI 修复）+ P4-4 提交（随 push 回填） |
| 前置 | Phase 3 收口（83da42f3）；V0006 BM25 索引；ADR-0001 绞杀者路线 |
| CI | P4-1 run 43719162660 success（红三连实录）；P4-2 run 43720100522 success；P4-3 run 43729534846 failure（vendor 误伤）→ 43729823719 success（修复 da46e7e7）；docs 回填 43730193861 success；本收口提交结论随 push 回填 |

## 一、批次汇总

### P4-1（统一检索面 PG 侧）
- migrations_pg V0006（knowledge_cards BM25 索引 ParadeDB pg_search 0.26.6）+
  intel-db search 统一检索面（bm25_search @@@ + paradedb.score / vector_search
  pgvector 余弦 / rrf_fuse k=60 / mmr_diversify / hybrid_search）+ 权限前置谓词
  SQL 内（R2 11.4：workspace + approved-only + 硬标记排除 + 类型/JSONB 过滤
  动态占位符四态组装）+ rp-kc search 三模式 + 集成测试真容器实跑。

### P4-2（跨库归并 + JSONB）
- crates/unified-search 薄 crate（12-STORAGE C 层双库归并：SQLite 运营核知识
  面 FTS5/BM25 零改动 + PG 情报核面 hybrid 跨库 RRF k=60 + UnifiedHit store
  标注 + SQLite 通道失败降级如实标注）+ rp-search-unified CLI + 关键词拆分
  口径实录 + JSONB 包含过滤扩展（structured_content @> 显式 cast）。

### P4-3（reranker + tokenizers + 缓存）
- rerank 全链（rerank_search：hybrid + 正文 ANY 批读 + rerank_hits 容器排序 +
  8KiB UTF-8 截断；rp-kc --mode rerank；实测前后对比 hybrid RRF 分差 2 倍 →
  rerank 0.9993/0.5041/0.0）+ tokenizers 接入（R2 6.6 Tokenizer Adapter：
  context-broker tokenizer.rs，est_tokens 单点切换，EMBEDDING_TOKENIZER_DIR
  优先双缺席回落；vendor 增量 0.21.4 + fancy-regex 闭包 39 crate；calibration
  偏差 <5% 达标）+ 缓存升级（search_cache.rs 同构内容寻址骨架：query-vector
  1.493s→0.237s 实证 + retrieval 治理态指纹失效）+ selftest_phase4 17/17。

### P4-4（Benchmark + 收口，本批）
- 语料播种（RTIS verified 133 元数据面脱敏 + 合成 8 = ws-bench 141 卡，全
  APPROVED + 向量齐；POC 正文零复制红线）+ qrel 显式 10 查询（六形态）+
  bench.rs 指标纯函数（Recall@K/Precision@K/MRR/NDCG@K 单测钉值）+ rp-kc
  bench 子命令 + 四模式实跑（见 §三）+ JSONB 正式集成测试转正 + --sqlite-
  filters CLI 解析展开（fail-closed）+ **三平台级 bug 修复实录**（见 §四）。

## 二、Phase 4 验收对账（R2 第十四部分 Phase 4 行逐项）

| 验收项 | 结论 | 证据 |
| --- | --- | --- |
| 统一 Search API（PG 面：pg_search BM25 + pgvector + JSONB 过滤） | ✅ | intel-db search 唯一入口（R2 6.1 禁私建检索），V0006 BM25 + pgvector + @>/动态占位符；JSONB 集成测试 3/3 |
| RRF/MMR 融合在 Rust 检索层（跨 SQLite 知识面与 PG 文档面） | ✅ | unified-search 跨库 RRF k=60 归并（双通道实测命中）；intel-db rrf_fuse/MMR 单测 + MMR 归位 rerank 分后 |
| reranker worker（bge-reranker ONNX） | ✅ | rip-rerank 容器族（digest 钉死台账三处同步）；四数量级分离 0.99969/0.68741 vs 3.7e-05；rerank 分 MRR 0.95 |
| tokenizers 接入替换 est_tokens | ✅ | HF tokenizers 0.21.4 单点切换 + calibration 2/2（偏差 <5%）；vendor --offline 全绿 |
| 缓存升级 | ✅ | query-vector（1.493s→0.237s）+ retrieval（治理态指纹失效 + v3 键修复）；search_cache 单测 2 |
| Benchmark（RTIS 语料播种，Recall@K/MRR/NDCG） | ✅ | 本报告 §三实测矩阵（hybrid 0.5812/0.9167/0.8212 全面超单通道） |

## 三、基准矩阵（P4-4 实测，k=10，10 查询 × 4 模式，141 卡）

| mode | Recall@10 | P@10 | MRR | NDCG@10 | latency(ms) |
|---|---|---|---|---|---|
| bm25 | 0.5415 | 0.3600 | 0.6500 | 0.6613 | 12.0 |
| vector | 0.5692 | 0.4500 | 0.8667 | 0.7771 | 2.5 |
| hybrid | 0.5812 | 0.4700 | 0.9167 | 0.8212 | 14.2 |
| rerank | 0.5812 | 0.4700 | 0.9500 | 0.8102 | 5325.5 |

## 四、本批三平台级 bug 修复实录（P4-4-BENCHMARK.md G 节详证）

1. **tool-engine 容器执行管道死锁**：大输出（>64KB 管道缓冲）容器 print
   阻塞 → 进程永不退出 → 死锁（实证 embed 23min 挂死）。修复 = stdout/
   stderr 并发读线程。修复后 141 卡嵌入一次通过。
2. **MMR 尺度错位**：RRF 分（~0.03）上做 MMR，多样性项压倒相关性（hybrid
   Recall 0.044）。修复 = MMR 归位 rerank 分（0-1）后（R2 6.2 流程原意）。
   修后 hybrid 0.5812。
3. **检索结果缓存键漏 query**：10 查询共用一键，后续查询全命中首查询缓存
   （recall 逐位复现 /10）。修复 = params 补 query + 缓存版本 v3。

## 五、门禁收口（实际执行，2026-11-17）

| 门 | 命令 | 结果 |
|---|---|---|
| workspace 测试 | cargo test --offline --workspace | 714 passed 0 failed（Phase 3 基线 707 → +7：bench 1 + jsonb 1 + search_hybrid rerank 1 + search_cache 2 + calibration 2） |
| clippy | cargo clippy --offline --workspace --all-targets -- -D warnings | exit 0 零告警 |
| unittest | python3 -m unittest discover -s workers/python/tests | OK（skipped=11） |
| web | npm run build + npm run test | build exit 0；vitest 124 passed |
| selftest phase4 | apps/server/selftest_phase4_search.sh | 17/17 PASS exit 0（收口复测） |
| selftest auth | apps/server/selftest_auth.sh | 11/11 PASS |
| push CI | （HEAD run 随 push 回填） | （回填） |

## 六、队列推进

Phase 4 收口 → **Phase 5 工作流与安全研究**（补充计划
RIP3-PHASE5-WEB-WORKBENCH-AND-KNOWLEDGE-DRIVEN-VERIFICATION-PLAN.md：P5-0
只读审计先于 P5-1；Yakit 参考图已落 docs/rip3/ui-reference/yakit/）。
