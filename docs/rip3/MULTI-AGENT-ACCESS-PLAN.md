# RIP3 多代理/多 AI 并发访问扩展规划：Multi-Agent Access（MA 轨）

> 文档类型：产品范围补充 / 架构规划 / 后续 AI 实施任务书
>
> 状态：设计提案，随批实施（用户 2026-11-21 明确需求）
>
> 日期：2026-11-21
>
> 适用仓库：`research-platform`
>
> 关联：`NATIVE-FIRST-HYBRID-RUNTIME-STRATEGY.md`（Runtime 面已就绪：RT-1 native_rust / RT-2 venv / RT-3 Policy Resolver 在跑）、`PHASE-ROADMAP.md`（Workflow/Agent Engine 既有编排面）

## 一、用户需求原文拆解（四形态）

1. **单人/单 AI 使用平台时，支持多个子代理并行工作**——同一任务内的 fan-out 提速。
2. **多个不同 AI 同时使用平台完成各自任务**——多租户并发：每个 AI 一个 caller 身份，任务互不串扰。
3. **一个任务由多个不同 AI 协作完成**——任务内分工（认领/移交/合并），平台做协调者。
4. **不同 AI 向平台供给知识**——知识库并发写入：来源署名、冲突治理、质量闸。

## 二、现状基线（2026-11-21 实测）

| 面 | 现状 | MA 缺口 |
|---|---|---|
| 鉴权 | rp_server 单一全局 Bearer Token（RP_AUTH_TOKEN / RP_AUTH_TOKEN_FILE），写端点强制 | 无 per-caller 身份、无租户隔离、无细粒度权限 |
| 子代理并行 | agent-engine `RunControlConfig.allow_parallel_subtasks=false`（低并发纪律，ROADMAP §8.5 风险 1），单轮多步骤串行 | 并行开关 + 预算化放开路径（不是裸开） |
| 工具并发 | podman_host FamilyLimiter（工具族并发，默认 1）；tool-engine 预算按 run | 无 per-caller 全局并发/速率配额 |
| 任务归属 | ResearchTask/Workflow 无 caller/owner 字段 | 多 AI 任务互斥与归属不可分 |
| 事件推送 | rp_server 无 SSE/WS（客户端轮询） | 多 AI 并发感知任务状态需推送或高效轮询 |
| 知识写入 | knowledge 面（PG V0004/V0005/V0006）单写者语义未验证并发冲突 | 并发供体署名 + 冲突治理 |
| 编排入口 | REST / MCP / CLI 三面同源 | 跨 AI 编排（外部 AI 经 MCP/REST 派发子任务）契约未定义 |

## 三、设计原则（对位既有红线）

1. **不建第二套 Agent/Workflow Runtime / 事实源**（PLAN §2.3 延续）：多 AI 并发复用 ResearchTask / Workflow / ToolRun / Artifact / Evidence / Trace 既有实体，只加 caller 维度。
2. **caller ≠ 用户账号**：caller 是"AI 代理身份"（人类操作者的多个 AI 助手各持独立 key），鉴权沿用既有 Bearer 面，扩展为多 key → caller 映射。
3. **低并发纪律升级为"预算化并发"**：不裸开 `allow_parallel_subtasks`；并行度由 per-caller 预算面（并发帽/速率帽/总预算）强制，Rust 后端执行（对位 WSW §7.3 口径）。
4. **知识供体署名与质量闸**：外部 AI 供知识必须走既有 RTIS/knowledge 治理面（写入即候选，审核后才生效），不建第二知识库；供体署名入来源字段。
5. **协作 = Workflow 节点角色**：多 AI 同一任务复用 Workflow Engine 节点语义（每节点绑定 caller 角色），不做自由写竞争——结构化协作优于共享文件竞争。

## 四、批次规划（MA-0 ~ MA-4）

| 批次 | 名称 | 主要交付 | 硬验收 | 前置 |
|---|---|---|---|---|
| **MA-0** | 现状审计与规划定稿 | 本文档 + 差距矩阵实测（本文 §二）+ 各批任务书骨架 | 文档入库 + 与 WSW/DI/RT 轨无边界冲突声明 | 无（本文即交付） |
| **MA-1** | Caller 身份与多密钥鉴权 | SQLite 026（caller 表 + key 哈希 + 权限位）+ rp_server 鉴权层扩展（多 key → caller 映射，兼容既有单 Token）+ per-caller 审计行 + key 管理面（CLI） | 多 key 并发访问实测互不串扰 + 单 Token 兼容回归 + 未授权 401 | MA-0 |
| **MA-2** | 并发派发引擎 | agent-engine 并行子任务预算化放开（per-caller 并发帽/速率帽/总预算，Rust 强制）+ tool-engine per-caller 限流（FamilyLimiter 扩展）+ ResearchTask caller 归属字段（SQLite 027） | 单 caller 多子代理并行实测（N=3 并行提速对比）+ 超预算硬拒 + 双 caller 互不抢占 | MA-1 |
| **MA-3** | 协作协议与知识供体面 | Workflow 节点 caller 角色绑定 + 子任务认领/移交/合并语义（TaskClaim）+ 外部 AI 经 MCP/REST 派发子任务契约 + 知识供体署名与候选写入治理（PG V0007 knowledge 来源扩展） | 双 AI 协作同一 workflow 实测（节点分属两 caller）+ 外部 AI MCP 派发往返 + 并发知识写入冲突治理实测 | MA-2 |
| **MA-4** | 事件推送与跨 AI 端到端验收 | SSE/WS 事件推送面（多客户端订阅）+ 四形态端到端验收脚本 + 截图/实测入库 | 四形态各一条端到端实测（含多 AI 并发压测小规模 N=4）+ 事件推送实时性断言 | MA-3 |

## 五、界面与集成

- **不另建控制台**：多 AI 并发状态在既有 Web 工作台 Research/Runs 页扩展 caller 维度列与过滤（沿 docs/ui-ux 体系）。
- **CLI**：`rp-verify`/既有 CLI 增 caller 上下文参数；key 管理 CLI（生成/吊销/列表）。
- **MCP**：外部 AI 供知识/派任务走既有 MCP 面，caller 由 MCP 会话鉴权映射。

## 六、禁止事项（MA 轨专属）

1. 禁止 per-caller 数据分库/分 schema（租户隔离=行级 caller 字段 + 查询强制过滤，不是第二套库）。
2. 禁止裸开并行（一切并发经预算面）。
3. 禁止把 caller 概念塞进 Tool Registry 工具定义（caller 是运行时维度）。
4. 禁止多 AI 直接共享写同一 ResearchTask 行（协作走节点角色与 Claim，禁止自由写竞争）。
5. 禁止跳过既有 RTIS/knowledge 治理面直写知识库。

## 七、与既有轨的关系

- **RT-3 Policy Resolver**（在跑）：其 per-caller 并发帽是 MA-2 的输入面（Resolver 输出 constraints 扩 caller 维度），RT-3 设计已按纯函数留扩展位，无返工。
- **WSW 轨**：代理/History 已有 session 概念（wsw_session），MA-1 caller 维度与之正交，WSW-4 后续批顺带带 caller 过滤。
- **排期**：MA-0 本文即入库；MA-1 在 RT-3 收口后插入为最高优先（用户明确需求），其后 MA-2 → WSW-4 → MA-3 → WSW-5+ 与 MA-4 交替。

## 八、完成定义

MA 轨交付完成的判定：四形态各一条端到端实测通过（真实多进程/多 key 并发，非模拟）、per-caller 预算硬拒实测、双 AI 协作 workflow 节点实测、并发知识写入冲突治理实测、事件推送实时性实测——全部数字与命令原文入批次报告，禁模拟冒充。
