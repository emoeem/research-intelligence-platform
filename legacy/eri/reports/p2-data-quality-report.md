# ERI P2 数据质量审核报告

生成：2026-11-05T15:31:07Z　三队列共 165 条审核记录，全部有决策与理由，零删除。

## D5 复核（68 候选）
| 决策 | 数量 |
|---|---|
| NEEDS_MORE_EVIDENCE | 63 |
| RELATED_KEEP_BOTH | 5 |

- FORK_KEEP_BOTH / SAME_OWNER_VARIANTS / RELATED_KEEP_BOTH / DISTINCT / NEEDS_MORE_EVIDENCE
- 规则复核后 63 条维持 NEEDS_MORE_EVIDENCE（名字近似但描述重叠不足，确需人工）
- 保留双方、保留历史；不自动合并（计划 §7.1）

## Skill 映射逐项审核（18 条）
| 终态 | 数量 |
|---|---|
| APPROVED_REFERENCE | 16 |
| COMPLEMENTARY | 1 |
| DUPLICATE_CAPABILITY | 0 |
| NEEDS_MORE_EVIDENCE | 1 |
| REJECTED | 0 |

- 审核方法：读 skill 文件真实正文（front matter + 前 2000 字符）与资源描述做词元比对
- 列表/索引类资源一律 APPROVED_REFERENCE（参考关系，不构成能力重复）
- tool:node 假阳性（P1 已修）未复现：本次 0 条 tool 映射
- 未修改任何 skill 文件（125 个全部原样）

## License 分层（79 仓库）
| 决策 | 数量 |
|---|---|
| OK_PERMISSIVE | 52 |
| RISK_NO_LICENSE | 15 |
| NEEDS_HUMAN_REVIEW | 8 |
| OK_REVIEW_SPDX | 2 |
| OK_COPYLEFT_NOTE | 2 |

- OK_PERMISSIVE 52 / OK_COPYLEFT_NOTE 2 / OK_REVIEW_SPDX 2：可直接参考，copyleft 采用前评估
- NEEDS_HUMAN_REVIEW 8（A/B 级无许可或未识别文件）：仅高价值项人工核读
- RISK_NO_LICENSE 15（低价值无许可）：默认不可作为依赖
- API null 语义 = "未检测到"，从未写成"确证无许可"
