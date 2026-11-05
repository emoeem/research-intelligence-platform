# ERI P2 ResourceResolver 报告

生成：2026-11-05T15:31:07Z　实现：eri_p2.resolve（纯 SQL+规则，零 LLM）

## 权重配置（初始可调）
| 维度 | 权重 |
|---|---|
| keyword | 0.25 |
| category | 0.20 |
| tech | 0.15 |
| profile | 0.15 |
| relation | 0.15 |
| quality | 0.10 |

- 无证据维度不计分并在 relevance_reasons 声明 missing，权重按有证据维度归一（不虚构匹配分）。
- 关键词为词元级匹配（整词/复数 + ALIASES 显式别名），已修复 rest->persistence、spa->space 类子串误中。
- 排除规则：QUARANTINED/REJECTED/UNAVAILABLE 不入候选；License 未知资源标记"仅供参考，不可直接采用为依赖"。
- 中文画像特征经 CJK_BRIDGE 查表转英文检索词（落地页->landing/redirect 等 17 组），不做翻译。

## 真实查询记录（真实库 resolver_queries，共 7 条）
| task_id | 候选 | 过滤 | 返回 | 耗时ms | 状态 |
|---|---|---|---|---|---|
| rtis-task | 81 | 81 | 0 | 5.65 | no_match |
| scenario-a | 2 | 0 | 2 | 0.92 | ok |
| scenario-b | 5 | 1 | 4 | 1.28 | ok |
| scenario-b2 | 5 | 1 | 4 | 1.47 | ok |
| rtis-task | 81 | 81 | 0 | 1.49 | no_match |
| rtis-task | 1 | 0 | 1 | 1.16 | ok |
| rtis-20261105-231342-2697 | 1 | 0 | 1 | 0.98 | ok |

## 场景回归结论（tests/run_p2_tests.py，真实库副本）
- 场景A 落地页/资产发现：awesome-asset-discovery 命中，无移动资源混入
- 场景B SPA/API：API Security Checklist 进入前 3，persistence 不再误中
- 场景C Android：android-security-awesome 前 2，全程零外呼
- 场景D 密码学：awesome-cryptography 前 3
- 场景E 无关任务（量子/steam）：空集返回，不虚构
- 场景F 重复控制：候选无重复、来源行数不变、fork 不合并
