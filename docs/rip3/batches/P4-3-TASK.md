# RIP3 任务书：P4-3（reranker worker + tokenizers 接入 + 缓存升级）

> 来源：HANDOVER-RIP3-CONTINUATION 第七节开批速查；规格源 = R2 第六部分
> 6.2/6.6 + 第十四部分 Phase 4 行剩余项。上一会话 WIP 快照（6a3ad9a9）为
> 起点：rip-rerank 容器族与 intel-db rerank 面已建，本批收口剩余四项。

## 范围

1. **rerank 全链收口**（R2 6.2 Reranking）：
   - `rerank_search` 从 unimplemented! 落成全链：hybrid 检索 top-N（N ≤ 64
     容器帽）→ 正文批量读取（单条 SQL IN）→ rerank_hits 容器排序；
   - `rerank_hits` 正文 8KiB UTF-8 安全截断（RERANK_TEXT_CAP_BYTES，与容器
     MAX_TEXT_BYTES 对齐）；
   - rp-kc search 扩 `--mode rerank`（--rerank-manifest-dir）；
   - 集成测试：rerank 前后排序对比断言（source=rerank + 分降序 + 相关主题卡
     置顶 + 无关卡不居首）。
2. **tokenizers 接入**（R2 6.6 Tokenizer Adapter）：
   - context-broker 新 tokenizer.rs：EMBEDDING_TOKENIZER_DIR env 优先 →
     缺省 models/bge-small-zh-v1.5/tokenizer.json → 双缺席回落 est_tokens
     保守口径（库 crate 不强制运行时文件）；
   - est_tokens 单点切换：adapter 在场走 HF tokenizers 真实计数，全部调用点
     行为一致性由单函数承载（不逐点改写）；est_tokens_formula 保留为校准
     对照面；
   - vendor 增量：tokenizers 0.21.4（default-features=false + fancy-regex
     正则后端，纯 Rust 零 C 依赖）+ 闭包 39 crate；
   - calibration 抽样测试：切换一致性 + 保守公式偏差 < 5%（ROADMAP §5.5
     「校准只修常数不改结构」）。
3. **缓存升级**（R2 6.6）：
   - intel-db 新 search_cache.rs（context-broker cache.rs:117-207 内容寻址
     骨架同构移植）：查询向量缓存 kind=query-vector（key = sha256(模型指纹
     ‖ query)，命中跳过容器派发）+ 检索结果缓存 kind=retrieval（fingerprint
     = 索引 schema 版本 + workspace 治理态 COUNT/MAX(updated_at)：审核跃迁/
     建卡即失效；cache.rs:17-28 七类触发器的 index_schema + governance 分量
     映射）；失败静默降级直查不阻断检索；
   - vector_search / hybrid_search 接入读写两侧。
4. **selftest**：apps/server/selftest_phase4_search.sh（沿 phase3 骨架：
   容器族 digest + manifest 三处同步 + 隔离 PG + 四模式检索 + rerank 前后
   对比 + 缓存命中 + calibration + 集成测试 + 生产库指纹红线 + 清理）。

## 验收

- cargo test --offline -p intel-db -p context-broker 全绿（含 rerank 集成 +
  calibration）；
- rerank 容器冒烟 + rerank 前后排序对比实测数据落批次报告；
- 缓存命中二跑计时对比落批次报告；
- selftest_phase4_search.sh 全过 exit 0；
- 五道门禁 + push + CI 全绿。

## 非目标（后续批次）

- Benchmark（RTIS 语料播种 + Recall@K/MRR/NDCG）→ P4-4；
- JSONB 过滤正式集成测试 + sqlite-filters CLI 解析展开 → P4-4；
- MMR 语义相似度代理的向量化（两两相似）→ Phase 8 规模面。
