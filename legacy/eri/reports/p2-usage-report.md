# ERI P2 Usage Tracker 报告

生成：2026-11-05T15:31:07Z

## 七态事件漏斗（真实库）
| 事件 | 次数 |
|---|---|
| OPENED | 4 |
| RETURNED | 12 |
| SEARCHED | 7 |

- SEARCHED 7 次（每次 resolver 查询 1 条），RETURNED 12 条（Top-K 逐资源 1 条），OPENED 4 次（真实 Reader 读取）
- SELECTED/APPLIED/SUCCESSFUL/REJECTED 为研究侧手动上报接口，当前 0 条（本阶段无真实研究任务消费资源）
- 比率字段语义：returned/searched 反映平均每次查询返回候选数（12/7=1.71），非百分比口径
- 漏斗按 task_id / resource_id / event_type 均可切片（funnel --task-id、by_resource）

## Token 成本
- Resolver：SQL+规则，0 token
- Reader HTTP 获取：0 token（未调用任何模型）
- 本阶段 ai_usage_log 写入 0 条；模型调用记账表就位，供后续 AI 审核队列使用
