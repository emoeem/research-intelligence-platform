# ERI P2 性能验证报告（10 万合成资源）

生成：2026-11-05T15:31:07Z　环境：本机 SQLite（WAL），隔离临时库，真实 resolver 代码路径

## 实测（毫秒，非估算）
| 指标 | 实测值 |
|---|---|
| 100k 行播种（含索引维护） | 1218.8 |
| resolve Top-5（2 关键词+profile） | 455.0 |
| resolve Top-10（4 关键词+skill） | 660.6 |
| SQL 多条件过滤 Top-K（grade+score） | 12.7 |
| 4 并发读（4 resolver 同时） | 3061.3（无错误） |
| 100k 库备份（sqlite backup API） | 30.5 |
| integrity_check | ok |

## 结论
- 10 万级：全部指标满足"基本查询测试"验收；单查询 <0.7s，并发读安全
- 瓶颈：LIKE %kw% 预筛无法用索引（全表扫描）。百万级建议：FTS5 虚表或迁移 PostgreSQL（存储抽象已保留：DB_PATH 可换，SQL 集中在 eri_p2.resolve 单点）
- 当前 81 条真实目录单查询 ~1.2ms，无性能压力
- 未迁移任何新数据库（计划 §8：现有方案满足就不引入）
