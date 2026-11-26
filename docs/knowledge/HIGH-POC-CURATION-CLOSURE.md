# High POC 证据审核收口

日期：2026-11-26

## 执行结果

本批从实时 RTIS 数据库识别并审核了 28 条未完成权威审核的 High 自有 POC。每条记录已进入 RIP-Owned overlay，原始 RTIS POC 正文、代码和生产库未修改。

| 统计项 | 数量 |
|---|---:|
| 审核完成 POC | 28 |
| K0 | 21 |
| K1 | 0 |
| K2 | 7 |
| K3 | 0 |
| K4 | 0 |
| Candidate | 21 |
| Researched | 7 |
| 源目录解析率 | 28/28 |
| Git Commit 解析率 | 28/28 |
| Evidence references | 57 |
| 缺失证据记录 | 21 |
| 完全重复记录 | 0 |
| 独立知识记录 | 24 |
| 关联族成员 | 4 |
| Overlay 关系总数 | 25 |
| Overlay条目总数 | 49 |

## 结构化入库

- 更新 `integrations/rtis/curation/k_meta_overlay.json`。
- 新增 28 条记录元数据和新增关系。
- 未修改 RTIS production `knowledge.db`。
- 无需迁移：现有 overlay、k_meta 投影和关系结构已足够。
- plugin-runtime 投影增加顶层可检索字段：`evidence_level`、`lifecycle`、`dedup_status`、`source_integrity`、`verification_result`、`independent_knowledge_status`、`missing_evidence`、`source_commit`、`evidence_refs`。

## 搜索能力

独立的只读 Overlay 搜索 CLI 可按以下维度检索：

- Evidence Level：K0/K1/K2/K3/K4
- Lifecycle：Candidate/Researched 等
- 漏洞分类：沿用 POC category
- 来源完整性：complete/partial/missing
- 缺失证据：missing_evidence 数组
- 验证结果：candidate_not_verified/historical_evidence_supported
- 独立状态与重复族状态
- 关联 POC：来源于 overlay relations 投影

只读搜索命令 `integrations/rtis/curation/search_overlay.py` 已实测：

- `--evidence-level K0`：返回当前 overlay 中 29 条 K0
- `--lifecycle Researched`：返回 19 条
- `--category unauthorized`：返回 7 条
- `--missing-evidence`：返回 21 条当前队列候选缺失记录
- `--relation-poc 19`：返回关联 POC 18

28 条队列记录均具备结构化搜索字段；已实测 Overlay 查询：K0=29、Researched=19、缺失证据=21、`unauthorized` 分类=7、POC 19 关联结果=1。

标准 `rp-search-index` 已增加可选 RTIS + RIP curation Overlay 只读数据源：仅索引 Overlay 明确列出的 49 条 POC 元数据，不将其伪装成已批准插件知识，也不读取 POC 正文、description、usage 或 `verified_note`。在临时索引库上完成 130/130 文档重建（原有资源 81 条 + Overlay POC 49 条）；其中 K0/Candidate 29、K1/Researched 9、K2/Researched 10、K3/Lab-validated 1。来源路径与 Commit 各 28 条，25 条 POC 带有 Overlay 关联关系。关键词 `zeai` 命中 3 条、`OpenAPI` 命中 3 条、`ThinkPHP` 命中 3 条、`gateway` 命中 2 条；K 级、Lifecycle、Severity、Category、CVE 均有独立过滤字段。索引 schema 升级到 v4，旧索引需 `--rebuild`。本轮只写入临时索引库，没有改写 `migrations/platform-search.db`。

## 测试与验证

最终验证：

- `python3 integrations/rtis/curation/validate_overlay.py`：通过，49 条 overlay、25 条关系；RTIS source DB 只读。
- RTIS source DB SHA-256 前后相同：`7b6083f8e1e84c9aff60a3d4f429f29cfc436ef1aafa7221d4d3cd945df641a7`。
- 两项 ignored 全量隔离摄取审计：通过，Nuclei 14,111 条（14,109 parsed、1 invalid YAML、1 body unavailable）；legacy 15,084 条；overlay 49 条、25 条关系。
- `cargo test -p search-engine --offline`：22 个单元测试 + 2 个索引恢复集成测试通过；新增 RTIS curation facet / relation 端到端测试。
- `b5_rtis_kit_directory_untouched`：通过；RTIS 生产资产仍受保护，只有 RIP 自有 `curation/` Overlay 路径从生产目录保护范围中排除。
- `mcp_subtask_roundtrip_and_caller_attribution`：通过；测试只隔离与 MCP 派发验收无关的可选 RTIS / curated-KB 上下文源，真实 MCP → rp-agent 子进程和 caller attribution 验证保留。
- `cargo fmt --all -- --check`：通过。
- `cargo clippy --offline --workspace --all-targets -- -D warnings`：通过。
- `cargo test --offline --workspace`：**1,421 passed、0 failed、6 ignored**（197 个 test targets，退出码 0）。迁移水位测试中既有的 31 断言已随 032 迁移更新为 32。

## 未解决问题

- 21 条 K0 记录仍缺原始请求/响应或独立复现，不能提升等级。部分记录属于历史状态或修复后状态，修复、清理、目标下线或失效接口已使原始现场不可重复；这类缺失不得被推断为证据完整。
- 对修复状态必须保留历史证据等级与独立的当前 Lifecycle；缺少明确修复复验证据时不得填写 `Deprecated` 或 `Rejected`。
- 900033 的短信发送结论未提升为 K2；缺挑战/校验原始响应和授权短信证据。
- 没有对任何公网目标重测。
- 未执行敏感信息审查、凭据扫描或脱敏审查。
- 标准 FTS 仅纳入 Overlay 明确列出的 49 条 POC；其余未进入 Overlay 的 POC 不会自动进入搜索结果，也不会自动获得可信度。若后续扩大覆盖范围，必须先完成独立证据审核并更新 Overlay。

## 后续交付：Knowledge 工作台检索接入（2026-11-26）

已将 curated POC 搜索接入 Web：Knowledge 页面新增「自有 POC」Tab；服务端新增受 Bearer 保护的 `GET /api/curated-pocs/search`，按需重建 `rip-data/derived/curated-poc-search.db` 独立派生索引。支持关键词、K0–K4、Lifecycle、Severity、Category、CVE 筛选，并展示来源路径、Commit 与关联 POC ID。端点只读取 Overlay 列出的 POC 元数据，不读取正文/`verified_note`；K 级与 Lifecycle 保持独立。

验收：`selftest_poc_search.sh` 真实 HTTP 回归通过；`selftest_phase9_rest.sh` 33 PASS / 0 FAIL；Web 22 个测试文件 / 156 个测试通过；`npm run build` 与 `npm run typecheck` 通过。详见 `docs/knowledge/POC-WEB-SEARCH-INTEGRATION-2026-11-26.md`。

## 全量 Overlay 治理与搜索闭环补强（2026-11-27）

已将 Overlay 治理覆盖从原 High 专项队列扩展到全部 49 条记录：四个治理轴（来源完整性、核验结论、去重状态、独立知识状态）完整率 49/49；33 条存在明确证据缺口，共 94 个缺失证据项，均有 `review_gate`。8 条工具/方法参考标记为 `not_applicable`，避免混淆目标漏洞验证。K 级与 Lifecycle 分布不变。

来源覆盖：42/49 有本地来源路径，39/49 有可解析 Git Commit，9/49 记录上游来源 URL。7 条缺少本地路径，均为上游工具/方法引用；10 条缺少 Commit（7 条无本地路径 + ID 900061–900063 文件无 Git 历史）。ID 16 的源脚本已恢复，迁移提交只用于来源追溯，不是测试执行 Commit。没有虚构来源。

关系治理：25 条关系端点全部解析，新增 2 个显式外部案例组节点；去重状态为 44 条独立、5 条关联族，独立知识判定为 45 条独立、4 条族成员。Overlay 校验器现检查 source_path 边界与存在性、Git Commit 解析、Evidence Ref（含工作区外的 `../runs` 路径）、治理枚举、missing_evidence→review_gate 约束以及关系端点解析。

搜索索引 schema 升级为 v6，新增上游来源 URL、来源完整性、核验结论、去重状态、独立知识状态、缺失证据、Evidence Ref、审核门槛与外部案例关系字段；新增对应筛选器。Knowledge Web 工作台默认展示待补证队列，并新增 Overlay 汇总 API。真实 HTTP 回归覆盖 21 条缺少来源完整性候选、33 条总体证据缺口、上游 URL、外部案例组、`not_applicable` 和 Bearer 鉴权。

验证结果：`validate_overlay.py` 通过；`cargo test -p search-engine --offline` 22 单测 + 2 集成测试通过；`selftest_poc_search.sh` 全部通过；Web typecheck 和专项测试通过。源数据库和 Overlay 在 HTTP E2E 测试前后 SHA-256 不变，索引只写入临时目录。详细实现见 `POC-WEB-SEARCH-INTEGRATION-2026-11-26.md` 的 2026-11-27 更新段。

## 证据路径恢复核对（2026-11-26）

新增逐条矩阵：[`POC-EVIDENCE-RECOVERY-MATRIX-2026-11-26.md`](POC-EVIDENCE-RECOVERY-MATRIX-2026-11-26.md)。33 条证据缺口中 33 条本地 source_path 存在；ID 16 的源脚本已从同目录唯一的 poc-actuator.sh 恢复，但仍缺 raw_request_response 与 impact_boundary。25 条具有 Evidence Ref，51/51 个现有引用路径可解析；8 条没有 Evidence Ref。没有将脚本/说明误判为原始请求响应或独立复现，也没有提升任何 K 级。
