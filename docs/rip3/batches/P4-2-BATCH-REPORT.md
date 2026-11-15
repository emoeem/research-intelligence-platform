# RIP3 批次报告：P4-2（跨库归并统一 Search API 薄 crate + JSONB 过滤扩展，§17.6 格式）

## A 完成内容

1. **crates/unified-search**（新薄 crate，12-STORAGE C 层「双库各自索引，统一
   Search API 归并」落地）：SQLite 运营核知识面（search-engine FTS5/BM25，
   2.0 既有面复用零改动）+ PG 情报核面（intel-db hybrid：BM25 + 向量 + RRF +
   MMR）跨库 RRF 归并（k=60 与 intel-db::search 常数一致）；UnifiedHit 携
   store 标注（pg-intel | sqlite-knowledge）与来源库补字段；SQLite 通道失败
   降级为空通道 + channels.sqlite_error 如实标注（索引为可重建派生物，缺席
   不阻断跨库归并）。
2. **rp-search-unified CLI**（新 bin）：--query/--workspace/--limit/--type/
   --index-db（缺省 migrations/platform-search.db）/--sqlite-filters 预留/
   --include-unapproved。
3. **关键词拆分口径**（rp-search-index 同源：空白/逗号分隔多词，FTS OR 语义
   ——整句单词传参致 sqlite 通道零命中的坑位实录，实测 "hacking awesome"
   双通道命中修复）。
4. **JSONB 包含过滤扩展**（intel-db search，R2 6.4 结构化面）：filters
   .json_contains → `structured_content @> $n::jsonb`（显式 cast——PG 未类型化
   参数推断失败坑位，::vector 同源实录）；bm25_search 动态占位符四态组装
   （type/json 在场组合 × LIMIT 序号——未引用占位符 PG 拒推断 E42P18 坑位
   实录，P4-1 动态参数教训的完整化）。
5. **JSONB 过滤实测**：临时集成面验证（命中卡唯一 + 未命中卡排除 + 清理），
   正式测试并入 P4-4 Benchmark 语料面（见 G 节）。
6. **全链实跑**：FTS 索引重建（rp-search-index build，81 docs schema v2）→
   跨库双通道验证（"hacking awesome"：pg-intel 1 + sqlite-knowledge 6，RRF
   归并排序输出）+ "SQL 注入"（pg 1 + sqlite 0，语义无交集如实零）。

## B 架构变化

- 新 crate 边界：unified-search 只做归并编排不持有存储实现（PG 面 = intel-db
  ::search，SQLite 面 = search-engine 2.0 既有面零改动——RTIS 对账红线不触
  碰）；RRF 常数跨面一致（k=60）。
- 检索可见面：rp-kc search（PG 面）+ rp-search-unified（跨库面）双 CLI 并存，
  统一入口约束（R2 6.1）由两 CLI 均走本 crate/intel-db 既有面承载。

## C 文件变化

新增：crates/unified-search/（lib.rs + bin/rp-search-unified.rs + Cargo.toml）。
修改：Cargo.toml（workspace 成员 + intel-db/unified-search 依赖）；
crates/intel-db/src/search.rs（json_contains 过滤 + 显式 ::jsonb cast + bm25
四态动态占位符）；crates/intel-db/src/bin/rp-kc.rs（BOOL_FLAGS 扩
include-unapproved——布尔旗标坑位三现实录）。

## D 数据库变化

零迁移（BM25 索引 V0006 已建；FTS 索引为可重建派生物 /tmp 验证）。

## E 测试结果（全部实际执行）

| 门 | 命令 | 结果 |
|---|---|---|
| unified-search 单测 | cargo test -p unified-search | rrf_merge 跨库并归 + 截断 1 passed |
| JSONB 过滤实测 | 临时集成面（命中唯一 + 未命中排除） | 过（已清理，正式测试随 P4-4 语料面） |
| FTS 重建 | rp-search-index build | 81 docs schema v2（P10 既有面复用） |
| 跨库实跑 | rp-search-unified 三查询 | 双通道命中归并（pg 1 + sqlite 6）+ 无交集如实零 + 缺席降级标注 |
| 回归 | intel-db lib 24 + search 2 + clippy -D warnings | 全绿 |

## F 性能结果

FTS 重建 9.7ms（81 docs）；跨库归并为内存 RRF（秒级内）；规模数字属 Phase 8。

## E2 CI 结论

push 后 GitHub CI 全绿为硬门槛（随 push 回填，同前批口径）。

## G 遗留问题

1. JSONB 过滤正式集成测试随 P4-4 Benchmark 语料面（临时面已验证语义）。
2. reranker worker + tokenizers + 缓存升级 → P4-3 批。
3. Benchmark → P4-4 批（含跨库归并的 Recall@K 面）。
4. sqlite-filters 参数面 CLI 解析未展开（结构体透传已备，随 Benchmark 批按
   RTIS 播种面需求展开）。

## H 下一阶段依赖与顺序

P4-3（reranker + tokenizers + 缓存升级）→ P4-4（Benchmark + Phase 4 收口）。
