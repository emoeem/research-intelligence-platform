# RIP3 批次报告：P4-1（统一检索面 PG 侧：BM25 + 向量 + RRF/MMR + 权限前置，§17.6 格式）

## A 完成内容

1. **migrations_pg V0006**：knowledge_cards BM25 索引（ParadeDB pg_search 0.26.6，
   `USING bm25 (knowledge_id, title, content)`；key_field 自 0.26 弃用 no-op
   不提供——**P1 遗留 2 pg_search BM25 索引建演练清账**：真库建索引 + @@@ 查询
   + score 面实测通过）。
2. **intel-db search**（统一检索面，R2 6.1 唯一入口 + 6.2 流程）：
   - bm25_search（@@@ 词面 + paradedb.score 排序；Top-K 扫描注记：ORDER BY
     直用 pdb.score 形不用别名包装——性能注记实录）；
   - vector_search（pgvector 余弦 top-K；查询向量经 rip.embed.text 本地推理，
     `$1::vector` 显式 cast——CTE 未类型化参数推断 text 坑位同源实录）；
   - rrf_fuse（RRF k=60 跨通道并归，双通道命中卡 RRF 分最高——单测断言）；
   - mmr_diversify（λ 多样化，词面重叠代理——两两向量化属 Phase 8 规模面，
     注记如实）；
   - hybrid_search（BM25 + 向量 → RRF → MMR 入口）。
3. **权限前置谓词 SQL 内**（R2 11.4 + 13 号冻结物第五节）：workspace +
   review_status（approved-only 缺省，`--include-unapproved` 显式放开）+ 硬性
   风险标记排除（POSSIBLE_PROMPT_INJECTION 数组重叠排除）+ 类型过滤动态占位
   符——全部在 SQL 内，禁止取回后过滤。
4. **rp-kc search** 子命令（--mode bm25|vector|hybrid --query --limit --type
   --workspace --mmr-lambda --include-unapproved）。
5. **集成测试**（tests/search_hybrid.rs，1/1 真容器 + 真 PG 4.3s）：四卡语料
   （批准 SQL 主题 ×2 + 注入卡硬标记 + 未批准同主题）——BM25 命中批准卡 +
   注入卡前置排除 + 未批准卡前置排除 + 向量语义命中 + hybrid RRF/MMR 输出 +
   类型过滤 SQL 内生效。

## B 架构变化

- 统一检索面成立（intel-db::search，PG 情报核面）；跨库归并（SQLite 运营核
  知识面 + PG 面，12-STORAGE C 层「双库各自索引，统一 Search API 归并」）随
  P4-2 批（独立薄 crate，不破 intel-db 边界——预留注记）。
- 检索可见性面：cards 无独立 visibility 列（03 号 1.5 未冻结该列），本期以
  workspace 前置谓词承载；跨 workspace 检索（13 号第五节显式授权表）随
  Phase 5 批评估。

## C 文件变化

新增：migrations_pg/V0006__search_hybrid.sql；crates/intel-db/src/search.rs；
crates/intel-db/tests/search_hybrid.rs。
修改：crates/intel-db/src/lib.rs（search mod）；crates/intel-db/src/
embed_worker.rs（dispatch/ParserErr pub(crate)，检索面查询向量复用）；
crates/intel-db/src/bin/rp-kc.rs（search 子命令 + embed_cfg_from 提取）。

## D 数据库变化

PG：V0006（BM25 索引）；SQLite 零迁移。

## E 测试结果（全部实际执行）

| 门 | 命令 | 结果 |
|---|---|---|
| BM25 演练 | 真库 CREATE INDEX USING bm25 + @@@ 查询 | 命中 score 2.403/5.69（P1 遗留 2 清账） |
| 集成测试 | RP_TEST_PG_DSN=… --test search_hybrid | 1 passed（4.3s：前置过滤全断言） |
| 三模式实跑 | rp-kc search bm25/vector/hybrid | bm25 5.69 / vector 0.639 / hybrid rrf 0.0328（RRF 分量级正常） |
| search 单测 | rrf_fuse + mmr_diversify | 2 passed |
| 回归 | intel-db lib 24 + clippy -D warnings | 全绿 |

## F 性能结果

BM25 Top-K 扫描注记（ORDER BY pdb.score 形直用）；小语料秒级；检索延迟/
Recall@K 属 P4-4 Benchmark 面。

## E2 CI 结论

push 后 CI run 43717958891 首跑 failure（search_hybrid 测试重复 cfg 遮蔽
unused variable——本地收口漏跑全量 clippy，修复批 43718355732 二跑再红：
mcp-server run_tool_without_token 负载 flake 二连现，与 P3-2 同例）。根因加固
（换方法不再重跑赌运气）：send_raw stdin 管道破裂竞态合法化——fail-closed
子进程先于父进程写入退出属正常时序，写失败静默返回，退出码断言由 wait_exit
承载；本地五连跑 7/7 全过。本批同树重跑随加固批。

## G 遗留问题

1. 跨库归并（SQLite 知识面 + PG 面统一 Search API）→ P4-2 批（独立薄 crate）。
2. reranker worker（bge-reranker ONNX）→ P4-3 批；tokenizers 接入替换
   est_tokens → P4-3 批；缓存升级 → P4-3 批。
3. Benchmark（RTIS 语料播种，Recall@K/MRR/NDCG）→ P4-4 批。
4. JSONB 结构化过滤（structured_content 面）随 P4-2 扩展（本批类型/工作区/
   审核态/风险标记四维先行）。
5. 卡片级 visibility 列（03 号 1.5 未冻结）随 Phase 5 批评估。

## H 下一阶段依赖与顺序

P4-2（跨库归并统一 Search API 薄 crate + JSONB 过滤扩展）→ P4-3（reranker
+ tokenizers + 缓存升级）→ P4-4（Benchmark + Phase 4 收口）。
