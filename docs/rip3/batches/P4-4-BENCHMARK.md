# RIP3 P4-4 基准报告（Retrieval Benchmark，R2 6.8；Phase 4 收口批，§17.6 格式）

## A 完成内容

1. **语料播种**（RTIS 真实语料 + 固定合成集）：
   - scripts/p4_4_bench_seed.py：rt.py poc search --verified --json（只读，
     133 条元数据；search 面不含 POC 正文）→ rp-kc create 入隔离
     workspace ws-bench（141 卡 = RTIS 133 + 合成 8；卡片 content 仅元数据
     句式——类别/类型/严重级/验证状态 + RTIS 引用码，POC 正文与利用细节
     零复制，R2 红线）；score + batch-review approve（141/141，双闸）+
     embed-cards（141/141 向量）；
   - qrel 显式声明：10 查询（R2 6.8 六形态：英文类别词 ×4 / 中文语义 ×3 /
     多语言 ×1 / 版本差异 ×1 / 章节形态 ×1），match 规则生成相关卡集合
     （1~45 卡/查询），落 rip-data/bench/qrel.json（运行时数据不入库）。
2. **指标计算器**（crates/intel-db/src/bench.rs）：Recall@K / Precision@K /
   MRR / NDCG@K 纯函数（二值相关；IDCG = 完整 qrel 集 min(|rel|,k) 项，
   未检出相关文档在分母扣分）+ 单测钉值（完美序列 1.0 / 部分命中 / 零命中 /
   空判据 / K 截断五面）；rp-kc bench --qrel-file --k 子命令（行级 JSON）。
3. **四模式基准实跑**（k=10，10 查询 × 4 模式，数据见 F）。
4. **P4-2 遗留转正**：JSONB 包含过滤正式集成测试
   （jsonb_contains_filter_applies_in_sql：标记卡唯一命中 + 无过滤对照两卡
   在场——过滤确实在 SQL 内生效）；rp-search-unified --sqlite-filters
   "k=v[,k=v]" CLI 解析展开（grade/license/maintenance/resource-type 四维，
   未知键 fail-closed 显式报错退出；实测 "grade=A" 双通道归并 + sqlite 通道
   schema v1→v2 降级如实标注）。
5. **三个平台级 bug 修复**（基准过程中暴露，见 G 实录）。

## B 架构变化

- MMR 归位（R2 6.2 流程）：hybrid_search 止于 RRF 融合；MMR 移至
  rerank_search 的 rerank 分（0-1 尺度）之后——原实现把 MMR 直接做在 RRF
  分（~0.03 尺度）上，多样性项（title Jaccard 0-1）压倒相关性项。
- 检索结果缓存键补 query（缓存正确性）。
- tool-engine 容器执行器 stdout/stderr 并发读（平台执行面修复）。

## C 文件变化

新增：crates/intel-db/src/bench.rs；scripts/p4_4_bench_seed.py；
docs/rip3/batches/P4-4-TASK.md；本报告。
修改：crates/tool-engine/src/adapter/podman_host.rs（管道并发读）；
crates/intel-db/src/search.rs（MMR 归位 + 缓存键补 query + 缓存版本 v3）；
crates/intel-db/src/lib.rs；crates/intel-db/src/bin/rp-kc.rs（bench 子命令）；
crates/intel-db/tests/search_hybrid.rs（JSONB 集成测试）；
crates/unified-search/src/bin/rp-search-unified.rs（--sqlite-filters 解析）。

## D 数据库变化

零迁移（ws-bench 语料为运行时数据面，经公开接口写入隔离 workspace）。

## E 测试结果（全部实际执行）

| 门 | 命令 | 结果 |
|---|---|---|
| 指标单测 | cargo test -p intel-db --lib bench | 1/1（钉值五面） |
| JSONB 集成 | cargo test -p intel-db --test search_hybrid | 3/3（bm25/vector/hybrid + rerank 全链 + jsonb） |
| unified-search | cargo test -p unified-search | 1/1（rrf_merge） |
| intel-db lib | cargo test -p intel-db --lib | 29/0（含 search_cache 2） |
| sqlite-filters 实跑 | rp-search-unified --sqlite-filters … | 未知键 fail-closed 报错 + 合法过滤双通道归并 |
| 播种全链 | p4_4_bench_seed.py + 补丁命令 | 141 卡 create/approve/embed 全过，qrel 10 查询 |
| selftest | apps/server/selftest_phase4_search.sh | （收口复测回填） |

## F 基准数据（实际执行，k=10，10 查询，141 卡语料）

| mode | Recall@10 | P@10 | MRR | NDCG@10 | latency(ms/查询) |
|---|---|---|---|---|---|
| bm25 | 0.5415 | 0.3600 | 0.6500 | 0.6613 | 12.0 |
| vector | 0.5692 | 0.4500 | 0.8667 | 0.7771 | 2.5（查询向量缓存命中） |
| hybrid | **0.5812** | **0.4700** | 0.9167 | **0.8212** | 14.2 |
| rerank | **0.5812** | **0.4700** | **0.9500** | 0.8102 | 5325.5（容器推理） |

读法：
- hybrid 全面优于单通道（Recall +0.012/+0.040，MRR +0.05/+0.27）——双通道
  并归有效；
- rerank 把 MRR 推到 0.9500（首位即相关的概率最高），NDCG 略降 0.011（重排
  在深位次序的取舍）；rerank 成本 = 容器推理秒级 vs hybrid 毫秒级——预算
  面取舍进 Scan Center 策略（Phase 5）；
- 跨语言语义查询（中文 → 英文类别卡）由 vector/rerank 承担，bm25 零命中
  场景的召回靠融合面补齐；
- Token Cost 口径（R2 6.7）：查询向量缓存命中后 vector 通道零容器派发
  （lat 2.5ms 实证），rerank 为最大成本项，重复查询二跑开销趋近 bm25 面。

## G 坑位实录（本批三修复，逐条带证据）

1. **tool-engine 容器执行管道死锁（平台级）**：stdout/stderr piped 后主
   循环只 try_wait 等退出、退出后才 read——64 卡批嵌入输出 ~260KB > 64KB
   管道缓冲 → 容器侧 print 阻塞 → 进程永不退出 → 死锁（实证：embed 容器
   Up 23min，容器内 result frame 已产出在 logs，宿主侧 vectors=0）。修法
   = spawn 后立即起双读线程排空管道，退出后 join。修复后 141 卡 3 批
   embed 一次通过（embedded=141）。历史批（≤16KB 输出）从未触发。
2. **MMR 尺度错位**：RRF 分（~0.03）直接做 MMR，title Jaccard（0-1）主导
   排序，同类别相关卡被挤掉（hybrid Recall@10 0.044 vs 单通道 0.54）。修
   法 = MMR 移至 rerank 分（0-1）之后（R2 6.2 流程原意）。修后 hybrid
   0.5812。
3. **检索结果缓存键漏 query**：params 只含 json/limit/lambda，10 查询共用
   一键 → 后续查询全命中首查询缓存（平均 recall = 单查询值/10，逐位复现；
   平均 latency 2.6ms 佐证）。修法 = params 首段补 query + 缓存版本 v3。
   （注：缓存版本语义递增是设计行为——修复后旧条目整体失效。）

## G2 遗留问题

1. qrel 判定为播种映射生成（category/前缀规则）——人工深判定集（逐卡
   relevance grade）随 Phase 5 Scan Center 实测面扩充；
2. Latency 基线的规模压力（1k/10k/100k 文档）→ Phase 8；
3. rerank 批内 64 条帽的扩展策略（>64 候选分批归并）随 Phase 8。

## H 下一阶段依赖与顺序

**Phase 4 收口**（四批全绿 + 本报告 + RELEASE）→ Phase 5（工作流与安全研究，
补充计划 docs/rip3/RIP3-PHASE5-WEB-WORKBENCH-AND-KNOWLEDGE-DRIVEN-
VERIFICATION-PLAN.md：P5-0 只读审计先于 P5-1）。
