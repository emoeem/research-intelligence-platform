# RIP 1.0 检索评测报告

> 状态：已验收范围和评测边界。日期：2026-11-28。

## 一、检索实现

统一检索保留源 identity、canonical key、vulnerability ID、来源记录、版本、quality、生命周期和治理状态。派生索引可重建，默认检索候选和源边界必须可解释。

## 二、已运行的评测

- 100 查询 exact-CVE baseline：100/100 命中、Recall@10、MRR 和首相关位级测试通过。
- 该评测只证明结构化 ID 检索和确定性排序，不是 human qrel 语义评测。
- 14 个查询、140 个跨源候选已建立 batch 和 AI advisory predictions。
- 140 个候选已由用户按样本检查并接受。

## 三、人类标注事实

持久化状态为：

- Label Studio tasks：140
- AI predictions：140
- human annotations：0
- annotation drafts：0
- CSV human_grade：0
- numerical relevance metrics：null

因此不得宣称 NDCG、MRR、MAP、Precision、Recall 或人工语义排序指标已通过。

## 四、可用结论

可以证明以下事实：

- exact-CVE 结构检索可重复命中。
- 跨源 exact identity 可以合并并保留来源记录。
- 审计未发现可以安全自动 merge/delete 的 exact or metadata duplicate。
- sampled candidate review 已被用户接受。

## 五、后续评测

后续可使用小规模用户批准 judged sample 或独立 qrel batch。标注流程应保存 reviewer、reason、版本和 conflicts，不可将 AI grade 预填为人工 grade。
