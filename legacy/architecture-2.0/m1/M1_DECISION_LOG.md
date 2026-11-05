# M1 Decision Log（架构决策记录）

| 项 | 值 |
|---|---|
| 日期 | 2026-11-06 |
| 模板 | 计划书附录 B（`RESEARCH_INTELLIGENCE_PLATFORM_MASTER_PLAN.md:1281-1300`），附加 Recommendation 字段承载评审建议 |
| 状态说明 | 全部条目 **Status: Proposed**：批准权在项目负责人（[计划 §12.1:1083-1088]），本日志不构成批准 |
| 证据标签 | `[计划 §x]` 计划书原文；`[侦察:架构评审]` 工作流架构侦察快照；`[实测]` 本会话执行的命令与输出；`[建议]` 评审建议 |

---

## DEC-001

```text
ID: DEC-001
Title: Rust 分阶段落地与 vendored 构建回退
Status: Proposed
Date: 2026-11-06
Recommendation: Accepted with Conditions [建议]
```

**Context**：M0 决策（TECHNOLOGY_DECISION.md §9.1）为「Rust 用于核心域，M2 先做一个 crate：resource-engine 的同步器，跑通 vendored 构建」；网络证据为 static.crates.io 曾返回 200、crates.io API 超时、sparse index TLS EOF（[计划 §2.3:122-128]）；[计划 §13:1153] 明问「DEC-001 的 Rust vendored 构建回退是否已通过可执行验证」。[侦察:架构评审] 确认 M0 证据止于网络探测，vendored 构建从未实际执行过一次。

**Decision（草案，待批）**：采纳 Rust 核心 + Python AI Runtime 双栈路线；M2 以 resource-engine 同步器为首个 crate；vendored 离线 clean build 为 M2-A 准入硬条件；失败即按本决策内回退条款转 **Python 回退路线**（继续 Python + 进程隔离。注意：与 Bridge 评估的「方案 C：隔离 Adapter」为不同概念，后者见 M1_READINESS_REVIEW 步 8）。

**Alternatives**：(1) 全 Rust 重写（拒绝：违反 [计划 §1.2] 分阶段结论）；(2) 全 Python 不引入 Rust（拒绝：丢失 [计划 §7.1] 职责边界的性能与类型安全收益，且与 M0 决策不一致）；(3) 在线依赖解析（拒绝：网络证据不支持）。

**Evidence**：TECHNOLOGY_DECISION.md §9.1（「M2 先做一个 crate」原文）；[计划 §7.2:489-502] Rust 构建可靠性门槛；[侦察:架构评审] DEC-001 rationale 与 gaps 第 14 项；R-03 更新记录。

**Consequences**：M2-A 交付前必须补齐 toolchain 固定、lockfile、vendored 目录与 CI 复现（[计划 §7.2] 准入项）；M1 范围不受影响。

**Security Impact**：vendored 依赖纳入供应链审查（记录许可证与来源，[计划 §M2-A:691]）。

**Migration Impact**：无直接迁移影响；首个 crate 绑定 resource-engine 同步器。

**Rollback**：回退条款即决策组成部分：vendored clean build 失败 → 转 Python 回退路线（Python + 进程隔离），不阻塞整体路线。

**Owner**: 项目负责人 | **Review Date**: M1-A 决策冻结评审；M2-A 开工前复核。

---

## DEC-002

```text
ID: DEC-002
Title: 数据库选型：SQLite 持续，PG 触发条件写死为可证指标
Status: Proposed
Date: 2026-11-06
Recommendation: Accepted with Conditions [建议]
```

**Context**：M0 决策（TECHNOLOGY_DECISION.md §9.2）：「继续 SQLite……PG 触发条件写死为可证指标：并发写冲突率 >1%、单库 >5GB、或跨靶标联邦查询成为高频需求」，与 [计划 §7.3] 不提前引入原则一致。但「每域一库或单库多 schema」在决策中两可，而 IMPLEMENTATION_ROADMAP 已写死单库 platform.db（[侦察:架构评审] gaps 第 15 项）；现库无外键强制、无 schema_version（CURRENT_DATABASE_MODEL.md §6.2）。

**Decision（草案，待批）**：SQLite 持续，PG 触发条件按 M0 原文固化；物理布局由 M1-B 定案。

**条件（Recommendation 附带）**：(1) M1-B 定案「单库 platform.db」并与路线图对齐（[建议]：单库，理由为写路径唯一、与 ROADMAP/M5 阶段授权一致、避免 [R-13] 双写分叉）；(2) 按 [计划 §7.1] 启用外键、事务与备份一致性检查；(3) schema_version 表与 user_version 机制进 Schema V1（[侦察:28库盘点] 证实可读 27 库 user_version 全 0，另 1 个 engagement 无 assets.db，R-01 教训）。

**Alternatives**：每域一库（保留意见：跨域事务与对账复杂度上升，且与已写死的 ROADMAP 矛盾）；PG 立即引入（拒绝：违反 [计划 §7.3]）。

**Evidence**：TECHNOLOGY_DECISION.md §9.2；CURRENT_DATABASE_MODEL.md §6.2；[计划 §7.3:503-516]；[侦察:架构评审] DEC-002 rationale；[侦察:28库盘点] 可读 27 库 user_version 全 0（1 个 engagement 无 assets.db）。

**Consequences**：单库上限 5GB 与冲突率 1% 成为 M5/M8 前必须监控的指标。

**Security Impact**：外键强制减少悬挂引用导致的证据链断裂。

**Migration Impact**：Schema V1 以单库 platform.db 为目标；28 库 ETL 映射不受选型影响。

**Rollback**：M1 阶段为隔离库，删除即回滚；PG 触发为单向升级条件，触发前不迁移。

**Owner**: 项目负责人 | **Review Date**: M1-A 决策冻结评审；M1-B 定案物理布局。

---

## DEC-003

```text
ID: DEC-003
Title: 检索选型：SQLite FTS5 + 2-gram 起步，向量暂缓专用基础设施
Status: Proposed
Date: 2026-11-06
Recommendation: Accepted [建议]
```

**Context**：M0 决策（TECHNOLOGY_DECISION.md §9.3）与 [计划 §7.1]「SQLite FTS5 作为初期全文检索」「向量检索暂缓专用基础设施」逐条吻合；2-gram 分词有 P1 实测背书（Recall 6/8，对照 legacy 提升，§9.3）[侦察:架构评审]。

**Decision（草案，待批）**：M1/M4 检索以 FTS5 + 2-gram 为基线；向量检索待 M4 以 5k 条试产、按命中率评估后决策；Tantivy 仅在实测检索瓶颈后考虑；升级与回退路径照 M0 决策文本执行。

**Alternatives**：立即引入专用向量库（拒绝：违反 [计划 §7.3] 与 R-10「每项基础设施提供必要性证据」）；pg_trgm/外置 ES（同上）。

**Evidence**：TECHNOLOGY_DECISION.md §9.3；[计划 §7.1:468-488]；[侦察:架构评审] DEC-003 rationale（accepted，无附加条件）。

**Consequences**：M4 需产出命中率评估报告作为向量检索立项证据。

**Security Impact**：FTS 索引属派生数据（[计划 §6.1]），不得承载权限语义，权限在权威数据层校验（R-14/R-15 关联）。

**Migration Impact**：索引可重建级，不进保真级对账主体。

**Rollback**：索引重建即可；2-gram 与 legacy 分词器保留开关。

**Owner**: 项目负责人 | **Review Date**: M1-A 决策冻结评审；M4 试产前复核。

---

## DEC-004

```text
ID: DEC-004
Title: 协议边界：内部版本化 JSON-RPC，对外 MCP+HTTP 双投影
Status: Proposed
Date: 2026-11-06
Recommendation: Accepted with Conditions [建议]
```

**Context**：M0 决策（TECHNOLOGY_DECISION.md §9.4）「内部 JSON-RPC，对外 MCP+HTTP 双投影」符合 [计划 §7.1]「版本化 JSON-RPC……不能把 MCP 当作所有内部调用的默认协议」；MODULE_BOUNDARIES §8.4 已承诺前三接口 JSON 形状字段级兼容。但 M2 交付物只写「JSON-RPC 协议 v0」未落「版本化」，且回退条件写「无（结构兼容）」，不可验证 [侦察:架构评审]。

**Decision（草案，待批）**：采纳双投影协议边界；M2 交付物补「版本化 JSON-RPC v0」契约 Schema 与版本演进策略。

**条件（Recommendation 附带）**：(1) M2 交付物明确协议版本号与演进策略（新增字段默认向后兼容，破坏性变更升主版本）；(2) 回退条件从「无（结构兼容）」改为契约测试可验证（契约测试进 [计划 §M2-D] 对照验证）。

**Alternatives**：内部直接用 MCP（拒绝：[计划 §7.1] 明文反对）；内部 gRPC（拒绝：引入新基础设施，违反 R-10）。

**Evidence**：TECHNOLOGY_DECISION.md §9.4；MODULE_BOUNDARIES.md §8.4；[计划 §7.1]；[侦察:架构评审] DEC-004 rationale 与 gaps 第 3 项。

**Consequences**：契约 Schema 文件与契约测试进入 M2 交付清单。

**Security Impact**：授权校验在 JSON-RPC 方法层强制，MCP/HTTP 投影不得旁路（R-15 应对）。

**Migration Impact**：现有 ERI/RTIS 调用方经 legacy-adapters 接入，[计划 附录 A:1256-1261] 目录已预留。

**Rollback**：协议版本化后可并行挂载旧版本端点直至调用方迁移完成。

**Owner**: 项目负责人 | **Review Date**: M1-A 决策冻结评审；M2-C 交付前复核。

---

## DEC-005

```text
ID: DEC-005
Title: 交互形态：本地 Web UI（server 模块 + 静态前端）
Status: Proposed
Date: 2026-11-06
Recommendation: Accepted with Conditions [建议]
```

**Context**：M0 决策（TECHNOLOGY_DECISION.md §9.5）：「最薄：server 模块 + 静态前端，零新桌面依赖」，方向可取且可逆。但 [计划 §M7] 明文「GPUI、Tauri + React 或其他方案必须通过实际 PoC 后决策，不在没有验证前预设最终答案」，M0 仅纸面否决 GPUI、未做任何 PoC（[侦察:架构评审] gaps 第 3 项，K-06）。

**Decision（草案，待批）**：本地 Web UI 作为当前基线方向进入路线图，但不锁定为最终答案。

**条件（Recommendation 附带）**：(1) M7 开工前补一次最小 PoC 对比记录（至少覆盖 server+静态前端 与一个备选方案，含移动端可用性与可访问性评估）；(2) 保留 [计划 §M7] 移动端退出门槛；(3) 本条件不影响 M1。

**Alternatives**：GPUI（M0 纸面否决，缺 PoC 支撑）；Tauri + React（更重，未验证）；纯 CLI（不满足 [计划 §14.4] 用户工作台交付定义）。

**Evidence**：TECHNOLOGY_DECISION.md §9.5；[计划 §M7:895-934]；[侦察:架构评审] DEC-005 rationale 与 gaps 第 3 项。

**Consequences**：M7 预算中含一次 PoC 记录的成本；决策可被 PoC 推翻，UI 相关下游（M7 退出门槛）以 PoC 后版本为准。

**Security Impact**：本地绑定地址与鉴权要求进 M7 设计；UI 仅反映授权不承担安全边界（R-15）。

**Migration Impact**：无。

**Rollback**：PoC 结论若推翻 Web UI，M7 范围重议，此前无 UI 资产沉淀。

**Owner**: 项目负责人 | **Review Date**: M1-A 决策冻结评审；M7 开工前复核。

---

## DEC-006（新增草案）

```text
ID: DEC-006
Title: AuthorizationRecord 实体化：独立授权子域中的独立实体（第 16 实体）
Status: Proposed
Date: 2026-11-06
Recommendation: Accepted with Conditions [建议]
```

**Context**：[计划 §5.2:351] 明文要求 M1 完成正式决策：「AuthorizationRecord / 授权记录是否作为独立实体、还是作为独立授权子域中的实体……不得因为上表列出 15 个核心实体就忽略授权对象的实体化要求」。现状三份证据互不覆盖：

- M0 UNIFIED_DOMAIN_MODEL.md（§7.1-7.15）无此实体 [侦察:架构评审 entityGaps]。
- M0 SECURITY_AND_GOVERNANCE.md §11.2.1 仅有字段草案：`{engagement_id, authorized_assets[], allowed_activity{}, t_level_cap, expires_at}`。
- 现网实测：task JSON 已内嵌 authorization 块 `{status:'declared', authorized_assets[], excluded_assets[], allowed_actions[], approval_required[]}`（rtis-20261102-220853-7827.json 只读抽查）[侦察:架构评审]。
- 项目约定（`/home/emo/code/pensting/AGENTS.md`）：engagement.yaml + SCOPE.md 为授权状态事实源。

**Decision（草案，待批）**：

1. AuthorizationRecord 定为**独立授权子域中的独立实体**，不计入 15 核心实体清单（满足 [计划 §5.2] 差异决策要求，不为数量一致强行合并）。
2. 主 ID：`AUTH-*` 全局 ID（不可复用，[计划 §5.4:382]）。
3. 状态机：`draft → active → expired / revoked`；状态跃迁有合法边与触发者（[计划 §5.3:371]）。
4. 变更控制：创建与变更走 Review；写与读走 AuditEvent。
5. 字段：合并现网块与 M0 草案，超集为 `{engagement_id, authorized_assets[], excluded_assets[], allowed_actions[], approval_required[], t_level_cap, expires_at, status}`；两来源字段的并集为最小充分集（M0 草案缺 excluded_assets/approval_required，现网块缺 t_level_cap/expires_at）。

**条件（Recommendation 附带）**：(1) **事实源边界**：engagement.yaml + SCOPE.md 保持授权文档事实源（项目 `/home/emo/code/pensting/AGENTS.md` 约定）；AuthorizationRecord 仅为其**结构化投影与强制闸**，不构成双事实源；投影生成器记录源文件哈希与生成时间，漂移可检测。批准签署时须明确接受该边界；若负责人要求反向同步回 yaml，则本条件改写并重新评审。

**Alternatives**：(1) 嵌入 ResearchTask 的 authorization JSON 字段（现状延续；拒绝：查询与强制闸能力弱，违反实体化要求）；(2) 独立授权子域多实体拆分（AuthGrant/AuthBoundary/AuthApproval 等；拒绝：M1 过度设计，违反 R-10）；(3) 仅保留 SECURITY_AND_GOVERNANCE 字段草案不实体化（拒绝：直接违反 [计划 §5.2:351]）。

**Evidence**：[计划 §5.2:351]；SECURITY_AND_GOVERNANCE.md §11.2.1；rtis-20261102-220853-7827.json（现网 authorization 块抽查）；/home/emo/code/pensting/AGENTS.md（授权文档事实源约定）；[侦察:架构评审] entityGaps 第 1 项。

**Consequences**：Schema V1 增加 authorization 子域表组（1 主表 + 关联表）；M1-C Source Mapping Matrix 新增 yaml/SCOPE → AuthorizationRecord 投影映射；28 个 engagement 各生成至少 1 条投影（其中 27 个有 assets.db 的 engagement 与 1 个无库 engagement 均有 yaml/SCOPE，见 [侦察:28库盘点]）。

**Security Impact**：授权闸成为强制层（t_level_cap 进 schema，超限操作被拒）；R-12 缓解设计输入；投影与 yaml 漂移检测为新增审计点。

**Migration Impact**：现网 task JSON 内嵌块作为历史投影导入（保真级）；不做反向写入 yaml。

**Rollback**：删除 authorization 表组与投影生成器即回滚；事实源 yaml/SCOPE 全程只读，无数据回滚需求。

**Owner**: 项目负责人 | **Review Date**: M1-A 决策冻结评审；M1-B Schema 设计时定稿字段。

---

## 汇总

| ID | 主题 | Status | Recommendation [建议] | 条件数 |
|---|---|---|---|---|
| DEC-001 | Rust 分阶段 + vendored 回退 | Proposed | Accepted with Conditions | 1 |
| DEC-002 | SQLite 持续 + PG 触发条件 | Proposed | Accepted with Conditions | 3 |
| DEC-003 | FTS5+2-gram、向量暂缓 | Proposed | Accepted | 0 |
| DEC-004 | 内部 JSON-RPC / 对外 MCP+HTTP | Proposed | Accepted with Conditions | 2 |
| DEC-005 | 本地 Web UI | Proposed | Accepted with Conditions | 3（M7 前生效） |
| DEC-006 | AuthorizationRecord 实体化（第 16 实体） | Proposed | Accepted with Conditions | 1（事实源边界） |

签署说明：项目负责人逐项批复 Accepted / Accepted with Conditions / Rejected（[计划 §M1-A:599]）；批准后本日志 Status 字段更新，Review Date 按各条记录滚动。
