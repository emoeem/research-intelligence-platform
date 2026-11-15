# RIP3 批次报告：P4-3（reranker worker + tokenizers 接入 + 缓存升级，§17.6 格式）

> 起点 = 上一会话 WIP 快照（6a3ad9a9：rip-rerank 容器族建成 + intel-db rerank
> 面 半成品编译过未实跑）；本批收口剩余四项。任务书 =
> docs/rip3/batches/P4-3-TASK.md。

## A 完成内容

1. **rerank 全链收口**（R2 6.2 Reranking，WIP 半成品转正）：
   - `rerank_search` 从 unimplemented! 落成全链：hybrid 检索 top-N → 正文
     批量读取（单条 SQL `ANY($1)`）→ `rerank_hits` 容器排序；
   - `rerank_hits` 正文 8KiB UTF-8 安全截断（`RERANK_TEXT_CAP_BYTES` = 容器
     MAX_TEXT_BYTES，截断点回退字符边界）；
   - rp-kc `search --mode rerank`（`--rerank-manifest-dir` 指定容器 manifest）。
2. **tokenizers 接入**（R2 6.6 Tokenizer Adapter）：
   - context-broker 新 `tokenizer.rs`：`EMBEDDING_TOKENIZER_DIR` env 优先 →
     缺省 `models/bge-small-zh-v1.5/tokenizer.json` → 双缺席回落保守口径
     （库 crate 不强制运行时文件）；OnceLock 全局单例，装载失败不重试；
   - `est_tokens` 单点切换：adapter 在场走 HF tokenizers 真实计数，全部调用
     点行为一致性由单函数承载（不逐点改写调用面）；`est_tokens_formula`
     保留为校准对照面；
   - vendor 增量：tokenizers 0.21.4（default-features=false，正则后端选
     fancy-regex 纯 Rust——onig 需编译 C 库不用）+ 闭包 crate，cargo vendor
     重排（单版本裸名 → 多版本带后缀目录重排 82 M + 新增 130 目录，lock 对
     齐零漂移）。
3. **缓存升级**（R2 6.6）：
   - intel-db 新 `search_cache.rs`（context-broker cache.rs:117-207 内容寻址
     骨架同构移植——cache.rs 在 context-broker crate 内，intel-db 不依赖该
     crate，同构移植非跨 crate 引用）：原子写 tmp+rename / 读时校验三元组 +
     TTL mtime 兜底 / 失败不落缓存无负缓存 / 坏条目视同未命中；
   - 查询向量缓存 kind=query-vector：key = sha256(模型指纹 ‖ query)，命中
     跳过 rip.embed.text 容器派发（秒级推理 → 磁盘读）；
   - 检索结果缓存 kind=retrieval：fingerprint = 索引 schema 版本（pg-search
     -v6，随迁移演进更新）+ workspace 治理态（COUNT + MAX(updated_at)：审核
     跃迁/建卡即失效——cache.rs:17-28 七类触发器的 index_schema + governance
     分量映射）+ filters 全量（approved/type/json/limit/mmr_lambda）；
   - 缓存失败静默降级直查（不阻断检索）。
4. **selftest**：apps/server/selftest_phase4_search.sh（9 节全过，见 E2）。

## B 架构变化

- 检索流程面补全 R2 6.2 缺口：Query → … → RRF → **Reranking** → Dedup →
  MMR 中 Reranking 段落位（rerank_search = hybrid 后置排序面，排序不改检
  索集）。
- Token Adapter 语义：est_tokens 全库单点切换，adapter 缺席回回落口径——
  预算语义（BRIEF_TOKEN_SOFT_CAP 等帽）在真实计数下行为由同函数保证。
- 缓存面新增 intel-db 侧目录口径：`$RIP_DATA_DIR/runtime/search-cache/`。

## C 文件变化

新增：crates/context-broker/src/tokenizer.rs；crates/context-broker/tests/
tokenizer_calibration.rs；crates/intel-db/src/search_cache.rs；
apps/server/selftest_phase4_search.sh；docs/rip3/batches/P4-3-TASK.md。
修改：crates/intel-db/src/search.rs（rerank_search 全链 + 截断 + 缓存接入 +
SearchHit Deserialize）；crates/intel-db/src/bin/rp-kc.rs（--mode rerank）；
crates/intel-db/src/lib.rs；crates/context-broker/src/estimate.rs（est_tokens
切换 + est_tokens_formula）；crates/context-broker/src/lib.rs（tokenizer 导
出）；crates/context-broker/Cargo.toml（tokenizers 依赖）；Cargo.lock；
vendor/（增量 39 crate + 目录重排）。

## D 数据库变化

零迁移（V0006 BM25 索引 P4-1 已建；缓存为文件面派生物，不入库）。

## E 测试结果（全部实际执行）

| 门 | 命令 | 结果 |
|---|---|---|
| rerank 容器冒烟 | podman run rip-rerank 三文本 | 相关 0.99969/0.68741 vs 无关 3.7e-05（四数量级分离） |
| rerank 集成测试 | cargo test -p intel-db --test search_hybrid | 2/2 passed（7.48s，含 rerank 前后对比 + source/降序/置顶断言） |
| rerank CLI 实跑 | rp-kc search --mode rerank | hybrid 前 RRF 0.0328/0.0323/0.0159（分差 2 倍）→ rerank 后 0.9993/0.5041/0.0（无关卡归零） |
| calibration | cargo test -p context-broker --test tokenizer_calibration | 2/2（切换一致 + 公式偏差 <5% 达标，无需修常数） |
| context-broker 回归 | cargo test -p context-broker | 66 passed 0 failed |
| intel-db lib 回归 | cargo test -p intel-db --lib | 28 passed（含 search_cache 往返/失效/TTL/坏条目 2 测试） |
| 缓存命中实测 | 同查询二跑 | 向量面 1.493s → 0.237s（容器派发 → 磁盘读）；search-cache 条目落盘 2（query-vector + retrieval），二跑零新建 |
| selftest phase4 | apps/server/selftest_phase4_search.sh | 17/17 PASS exit 0（首跑 16/1：集成测试容器派发并发 flake 挂死 25min，杀进程后改 `--test-threads=1` 静场串行 + timeout 300s 保护，二跑全过——坑位实录见 G4） |
| selftest auth | apps/server/selftest_auth.sh | 11/11 PASS |
| workspace 回归 | cargo test --offline --workspace | 712 passed 0 failed（基线 707 + 本批净增 5：search_cache 2 + calibration 2 + rerank 集成 1） |
| clippy | cargo clippy --offline --workspace --all-targets -- -D warnings | exit 0 零告警 |
| unittest | python3 -m unittest discover -s workers/python/tests | OK（skipped=11） |
| web | npm run build + npm run test | build exit 0；vitest 124 passed |

## F 性能结果

- rerank 成本：64 条单批容器帽内 pairwise 推理（CPU 2 线程），实测 3 文本
  容器端到端秒级；规模数字属 Phase 8。
- 缓存收益：查询向量命中路径省容器派发（1.493s → 0.237s，含进程启动）；
  检索结果命中路径双通道直查全免。

## E2 CI 结论

（push 后回填）

## G 遗留问题

1. Benchmark（RTIS 语料播种 + Recall@K/MRR/NDCG + JSONB 过滤正式集成测试 +
   sqlite-filters CLI 解析展开）→ P4-4。
2. rerank 输入正文帽 8KiB 截断为保守口径（长卡截断后 rerank 分只反映前
   8KiB）；全文重排随 Phase 8 规模面评估分块策略。
3. MMR 多样化相似度仍为 BM25 词面代理（本批未动，Phase 8）。
4. tokenizers 校准为抽样口径（6 语料中英混排 + 代码）；正式校准集随 P4-4
   Benchmark 语料面。
5. **容器派发并发 flake 实录**（selftest 首跑 16/1）：cargo test 默认双线程
   并发跑 unified + rerank 两集成测试，podman run --rm -i 容器（rip-embed
   embed）挂死 25min（podman run 客户端 futex 死等，容器本体已消失，conmon
   侧无残留），杀进程后 `--test-threads=1` 静场串行 + timeout 300s 保护二
   跑 17/17 全过。与 HANDOVER 第五节「cargo/vitest/视觉门并发跑有负载
   flake，收口复测静场串行」同族，selftest 脚本内已固化串行口径。

## H 下一阶段依赖与顺序

P4-4（Benchmark + Phase 4 收口）→ Phase 5（工作流与安全研究）。
