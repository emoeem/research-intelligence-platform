# RIP3 Phase 8：Scale Testing & Benchmark

状态：完成（2026-11-23）。本轮在隔离测试 PostgreSQL 实例和临时 SQLite 文件上完成 1k / 10k / 100k 文档、百万级 Chunk 规模测试。测试创建的 schema、恢复数据库与临时 SQLite 文件均为临时数据，不触碰生产数据。

## 测试环境与数据集

- Host：CachyOS Linux 7.3.5，x86_64，16 CPU，物理内存约 32.84 GB。
- PostgreSQL：17.11，`shared_buffers=7829MB`。
- SQLite：3.54.1，WAL 模式，FTS5。
- Python：3.14.10。
- 语料：100,000 文档、1,000,000 Chunk（每文档 10 Chunk），另有 4,000 条合成运行事件。
- PostgreSQL 使用 GIN `tsvector` 索引；SQLite 使用 FTS5 外部内容索引。
- 读取压测：100 次查询，16 个并发读取者；写入压测：8 个并发写入者，每人 500 条，50 条一事务。
- PostgreSQL benchmark 使用独立 schema，完成后 DROP；备份恢复在独立临时数据库执行并逐表计数核验。

## PostgreSQL 实测

原始数据：`benchmarks/phase8-scale-20261123T082804Z.json`
可读报告：`benchmarks/phase8-scale-20261123T082804Z.md`

| 指标 | 实测 |
|---|---:|
| 文档 / Chunk | 100,000 / 1,000,000 |
| 1k 档新增文档 / Chunk 导入 | 0.003 s / 0.093 s |
| 10k 档增量新增 9k / 90k | 0.015 s / 0.821 s |
| 100k 档增量新增 90k / 900k | 0.132 s / 8.043 s |
| GIN 索引构建 | 5.594 s |
| GIN 索引重建 | 5.604 s |
| 检索 p50 / p95 / max | 6.588 / 7.368 / 7.833 ms |
| 16 并发读取吞吐 / p95 | 990.85 q/s / 18.881 ms |
| 8 并发写入 4k 事件 | 0.186 s，21,490.79 rows/s |
| 总关系存储 / Heap / Index | 527,728,640 / 453,386,240 / 74,145,792 bytes |
| Additive DDL + Index migration probe | 0.017 s |
| pg_dump | 0.862 s，10,809,205 bytes |
| pg_restore | 14.340 s，100k / 1M / 4k 行全部核验一致 |

检索 `EXPLAIN ANALYZE` 计划确认命中 `chunks_search_gin`（Bitmap Index Scan 实际约 0.009 ms，整个查询执行约 0.023 ms，不包含客户端连接建立）；端到端延迟统计包含每次建立本地 TCP 数据库连接的开销。

内存采样：`shared_buffers=7829MB`；压测完成后当前 PostgreSQL backend memory context 合计约 2.52 MB（158 contexts）；Host 总内存 32.84 GB、当时可用约 23.07 GB。该 backend context 是单连接采样，不代表 PostgreSQL 全部进程 RSS，不能与 SQLite 进程峰值 RSS 直接比较。

## SQLite WAL + FTS5 对照

原始数据：`benchmarks/phase8-sqlite-20261123T082842Z.json`

| 指标 | 实测 |
|---|---:|
| 文档 / Chunk | 100,000 / 1,000,000 |
| 1k 档新增文档 / Chunk 导入 | 0.001 s / 0.008 s |
| 10k 档增量新增 9k / 90k | 0.008 s / 0.080 s |
| 100k 档增量新增 90k / 900k | 0.099 s / 0.797 s |
| FTS5 索引构建 | 1.719 s |
| FTS5 索引重建 | 3.747 s |
| 检索 p50 / p95 / max | 0.169 / 0.253 / 0.258 ms |
| 16 并发读取吞吐 / p95 | 2,750.89 q/s / 10.401 ms |
| 8 并发写入 4k 事件 | 0.039 s，102,564.1 rows/s |
| 数据库文件大小 | 190,050,304 bytes |
| Python + SQLite 进程峰值 RSS | 29,077,504 bytes |
| SQLite backup / restore | 0.178 / 0.161 s，100k / 1M 行全部核验一致 |

SQLite 内存指标包含 Python benchmark 进程，非独立 SQLite 引擎 RSS。WAL 仍是单写者模型；本轮 8 writer / 4k 事件压力没有测出足以证明需要迁移的写锁瓶颈，但不能据此推断任意更高写入负载。

## 规模结果解释

1. PostgreSQL GIN 检索计划正确，百万 Chunk 下 p95 约 7.4 ms；索引重建约 5.60 s，备份恢复完整通过。
2. SQLite 在本机嵌入式 FTS5 读取、写入和备份恢复上明显更快、文件更小。两者路径不同：SQLite 是进程内调用，PostgreSQL 是客户端/服务端 TCP；因此端到端数值不能视为纯数据库引擎的绝对性能排名。
3. PostgreSQL 存储体积更大，原因包括 stored `tsvector`、GIN 与额外 B-tree 索引；它换取服务端多连接、事务隔离、集中治理和未来多节点能力。
4. 本轮并未读取或写入生产 `platform*.db`，也未将运营数据迁移到 PostgreSQL。

## 运营核数据库决策

**决策：运营核继续保留 SQLite，不在 Phase 8 迁移到 PostgreSQL。**

理由：

- 既定职责边界仍然成立：情报核（Document / Chunk / KnowledgeCard / Finding / Report / 向量与检索）使用 PostgreSQL 17；运营核（tool_run / agent_run / workflow_run / plugin / runtime_node / audit）保留 SQLite。
- 对本机嵌入式读多写少工作负载，SQLite 在本次同规模合成数据对照中延迟、吞吐、存储与恢复耗时均更优。
- 本次 8 writer 的合成事件负载没有呈现足以支持迁移的锁争用证据；为追求统一数据库而迁移会增加运行时依赖、部署复杂度和回滚风险。
- PostgreSQL 保留给需要集中式多客户端、跨主机共享、强并发写入、统一权限治理或高可用需求的情形。

重新评估触发条件：

- 实际运营库持续出现 SQLite `SQLITE_BUSY` / 写锁等待，且 p95 写入等待超过 50 ms 或失败/重试率持续超过 1%；
- 多主机并发访问或高可用故障切换成为明确需求；
- 实际运行数据的保留、审计、查询或并发需求超过 SQLite 当前能力。

重新评估前，必须以脱敏生产数据的规模分布和真实读写比例补做 shadow benchmark、双写差异解释与回滚演练。当前结论不授权切库。

## 可重复命令

PostgreSQL 100k / 1M 全量基准：

```bash
RP_BENCH_DSN="$RP_TEST_PG_DSN" \
python3 scripts/rip3_phase8_scale_bench.py \
  --docs 100000 --chunks-per-doc 10 --queries 100 --workers 16 \
  --output-dir docs/rip3/benchmarks
```

SQLite WAL 对照：

```bash
python3 scripts/rip3_phase8_sqlite_compare.py \
  --docs 100000 --chunks-per-doc 10 --queries 100 --workers 16 \
  --output-dir docs/rip3/benchmarks
```

阶段复验入口：`apps/server/selftest_p7p8.sh`。设置 `RUN_PHASE8_SCALE_BENCH=1` 会重新运行两套全量 benchmark；默认只验证报告与 PostgreSQL 集成门禁。
