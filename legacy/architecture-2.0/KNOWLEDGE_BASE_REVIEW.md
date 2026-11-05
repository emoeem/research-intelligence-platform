# 4. KNOWLEDGE_BASE_REVIEW

日期：2026-11-05　对象：`~/.dsh/redteam/knowledge.db`（11 表，WAL）+ `knowledge-sources/` + `rtis/rtis_kb.mjs`。

## 4.1 实况计数（当日实查）

| 表 | 行数 | 说明 |
|---|---|---|
| poc | 14,717 | 主表；UNIQUE(code)；FTS5 伴生 |
| poc 来源分布 | nuclei 14,111 (95.9%) / web 451 / self 139 / manual 9 / kb 7 | 模板索引为主体 |
| poc verified | 1 仅 130 (0.88%)，verified_note 随行 | 验证率极低 |
| k_meta | 28 | 自有知识分层：Rule 12 / Method 9 / Case 7 |
| k_meta 证据级 | K2 5 / K3 23 | K0/K1/K4 未用 |
| k_meta lifecycle | Field-validated 20 / Lab-validated 3 / Researched 5 | |
| k_event | 633 | 检索/选择/应用事件流 |
| k_relation | 34 | 8 型关系边（supports/contradicts/requires/…） |
| k_negative | 17 | 反知识（不能得出什么/误报案例/停止条件） |
| k_proposal / k_version | 1 / 15 | 候选与版本链 |

## 4.2 治理机制评估

| 机制 | 状态 | 说明 |
|---|---|---|
| 知识生命周期（计划 §5 的 10 态） | 部分 | 实际只有 k_meta.lifecycle 3 个取值在用（Field-validated/Lab-validated/Researched）；DISCOVERED/FETCHED/PARSED/CANDIDATE/STALE/CONFLICTED/QUARANTINED/REJECTED 未落地 |
| 证据分级 K0-K4 | 部分 | 仅 28 条自有知识有级；14,689 条模板索引无级 |
| 版本 | 部分 | k_version 15 条（升级迁移产生）；日常更新不产生版本 |
| 去重 | 已实现 | code slug UNIQUE + dedup_key |
| 反知识 | 已实现（小规模） | 17 条；检索排序参与降权，机制好但覆盖薄 |
| 检索 | 已实现 | 2-gram + 组覆盖率门禁 + bm25 + K 级加权（bridge）；FTS5+LIKE 兜底（rt.py legacy） |
| 写入纪律 | 已实现 | rt.py 唯一写者；MCP 写路径转调 |
| AI 提议-人审采纳 | 已实现 | k_proposal → 人工 → poc add；无自动覆盖 VERIFIED 的路径（符合平台原则） |

## 4.3 结构性问题

1. **96% 的行是无治理的模板索引**：nuclei 模板以 poc 行存在，与自有知识同表，字段语义错位（kind=template 混 poc/exp）。统一域模型必须拆分：模板索引是"检索素材"不是"知识"。
2. **验证率 0.88%**：verified 字段可信，但 13 条已验证知识的验证证据（verified_note）无结构（自由文本），无法机读复验。
3. **知识与证据断裂**：http_evidence 在各 assets.db 里，k_meta 的 K 级声明无法回指证据行。跨库无外键。
4. **k_negative 是差异化资产**：反知识（已知误判、停止条件）在通用工具里没有对应物，是平台应作为头等公民保留的实体。

## 4.4 对统一平台的输入

- Knowledge 实体 = 现有 k_meta 化的 poc（自有知识部分），带 10 态生命周期目标态。
- KnowledgeVersion ← k_version；Review ← proposal 审批记录；AuditEvent ← k_event（633 条已有事件类型可归并）。
- 模板索引（14,111 条 nuclei）在新模型中归入 Resource（type=template_index）而非 Knowledge：它们有上游源（nuclei-templates 仓库）、有版本、可重建，性质是外部资源缓存。
- 迁移红线：现有 28 条分层知识与 17 条反知识逐条迁移并保留全部字段；14,111 条模板索引可由 nuclei-templates 仓库重建，允许降级为可重建索引。
