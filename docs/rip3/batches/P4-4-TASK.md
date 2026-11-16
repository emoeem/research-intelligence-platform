# RIP3 任务书：P4-4（Benchmark + Phase 4 收口批）

> 来源：HANDOVER 第七节 P4-4 速查；规格源 = R2 6.8 检索质量评估 + 6.7 Token
> 成本治理 + 第十四部分 Phase 4 行剩余项。**本批完成后 Phase 4 收口。**

## 范围

1. **语料播种**（R2 6.8「测试集优先从 RTIS 真实语料播种，辅以固定合成集」）：
   - scripts/p4_4_bench_seed.py：rt.py poc search --verified --json（只读，
     133 条元数据；POC 正文零复制——卡片 content 仅元数据句式，R2 红线）
     → rp-kc create 入隔离 workspace ws-bench + 合成集 8 条（版本差异/
     章节形态/中英混合）；
   - 批准（batch-review approve 双闸）+ embed-cards 补向量；
   - qrel 显式声明（查询 match 规则 → 相关卡 ID），不用检索结果自判；
     落 rip-data/bench/qrel.json（运行时数据不入库）。
2. **指标计算器**（intel-db/src/bench.rs 纯函数 + 单测钉值）：
   Recall@K / Precision@K / MRR / NDCG@K（二值相关；IDCG = 完整 qrel 集，
   未检出相关文档在分母扣分）；rp-kc bench 子命令（--qrel-file --k）。
3. **四模式基准实跑**：bm25 / vector / hybrid / rerank 逐查询 × 指标 +
   latency；聚合口径按模式平均（各查询 qrel 分母不同）。
4. **P4-2 遗留转正**：JSONB 包含过滤正式集成测试（structured_content @>
   SQL 内生效断言）；rp-search-unified --sqlite-filters "k=v[,k=v]" CLI
   解析展开（fail-closed：未知键显式报错）。
5. **基准报告** docs/rip3/batches/P4-4-BENCHMARK.md + Phase 4 阶段收口
   （RELEASE 登记 + HANDOVER 队列勾销 + 第七节换 Phase 5 开批）。

## 验收

- bench 单测钉值过 + 四模式实跑数据落报告；
- JSONB 集成测试 + search_hybrid 全绿；--sqlite-filters 实跑；
- selftest_phase4_search.sh 复跑全过；
- 五道门禁 + push CI 全绿（HEAD）。

## 非目标

- Latency 基线的规模压力面（1k/10k/100k）→ Phase 8；
- 覆盖 6.5 书籍检索专项（章节形态以合成卡近似，书籍域检索面随 Phase 6/8）；
- WebUI 检测入口 → Phase 5（计划 §4.6 Scan Center）。
