# 检索相关性标注指南（M8 qrels · 2026-11-28）

任务单：`retrieval-qrel-task-2026-11-28.jsonl`（44 查询 / 291 条待评，预计 1.5 到 2 小时）。

## 你要做的事

对任务单每一行，把 `relevance` 从 `null` 改成 **0-3** 的整数，其余字段不动：

| 分值 | 判据 |
|---|---|
| **3** | 直接命中：该文档正是查询所问（CVE 精确匹配该 CVE；主题/产品/漏洞类型完全一致；工具/Skill 即所查对象） |
| **2** | 相关：主题一致但不完全命中（同产品的相邻漏洞、同类漏洞的其他实例、所查工具的强相关替代） |
| **1** | 沾边：仅共享某个泛化词（都涉及"扫描""web 安全"），对回答查询无实质帮助 |
| **0** | 无关 |

`notes` 可选，写一句话理由（0/1 分时建议写，便于分析误过滤/误召回）。

## 特殊行

- `rank=0` 且 `notes="NO_RESULT"`：该查询无命中，不用打分（评测器用它算 No-result Rate）。
- `include_candidates=true` 的查询（最后 2 条，sourcemap 类）：候选结果（governance_status=CANDIDATE）按同样标准打分；这条查询的本意就是找"未审核的 sourcemap 泄露检测模板"，所以 CANDIDATE 的 webpack-sourcemap 模板应打 3。
- 判断只看**文档对查询意图的价值**，不看治理状态本身（状态已被单独统计为 Candidate Leakage）。

## 无结果 / 弃权探针（category=no_result 的 2 条）

这两条查询故意荒谬。期望行为是**空结果或全 0 分**，这是"应该弃权"的测试，不是搜索失败。

## 完成后

把标注完的文件另存为 `retrieval-qrel-labeled-2026-11-28.jsonl`（保持 JSONL 格式），然后跑：

```bash
.venv-check/bin/python scripts/security-intel/evaluate_retrieval.py \
  --labeled docs/knowledge-governance/retrieval-qrel-labeled-2026-11-28.jsonl
```

评测器输出 Recall@5/10、Precision@5/10、MRR、nDCG@10、No-result Rate、Source Coverage、Candidate Leakage Rate（按文档 §M8 口径，relevant = relevance >= 2）。

## 边界

- 查询结果取自 Schema 8 派生索引（18,147 docs）的真实 API 返回，冻结于 2026-11-28；索引重建后需重新生成任务单再标注。
- 你的评分是唯一 Ground Truth 来源；AI 辅助评分不进 qrels。
