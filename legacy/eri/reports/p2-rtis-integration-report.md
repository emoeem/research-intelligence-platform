# ERI P2 RTIS 集成报告

生成：2026-11-05T15:31:07Z

## 集成点
`~/.dsh/redteam/rtis/rtis.py` gate() 尾部（知识门禁完成后、返回前）调用 `_eri_external_review(task)`：
- subprocess 调 `eri_p2.py bridge-gate`（-I 隔离模式，30s 超时）
- 开关 `RTIS_ERI=off` 可关闭；ERI 缺失/异常一律静默跳过，不影响知识门禁
- 调用顺序符合计划 §3.1：Task -> Profile -> 内部知识检索（gate 本体）-> ERI Resolver
- 未硬编码 Awesome-Hacking；resolver 只查 resource catalog 表

## 端到端实测（task rtis-20261105-231342-2697）
- objective "白牌落地页跳转链路研究" + profile 维度 cdn-fronted
- gate 返回 external_resource_review：status=ok，candidate_count=1（hacker101，redirect 命中）
- 结果写入 task JSON 的 external_resource_review 字段与 gate_rec 摘要
- 候选少是目录真实规模（81 条）所致，不凑满（计划 §2.3）

## 输出结构（计划 §3.2 全字段）
status / query_id / candidate_count / recommended_resources / rejected_resources /
already_covered_by_internal / capability_gaps / detailed_research_required / reason

## 能力重叠判断
- already_covered_by_internal：资源已映射到任务相关 skill 时列出（skill_mappings 驱动）
- capability_gaps：高相关但无任何 skill 映射的资源（P1 映射驱动，18 条候选池）
- recommended/rejected：按 grade+license 二分；license 未知一律 rejected 并给原因

## 回滚
备份 db/backups/rtis.py.pre-p2-20261105-230907.bak；补丁为 gate 尾部一处调用 + 一个辅助函数 + tempfile import。
