# 3. RTIS_ARCHITECTURE_REVIEW

日期：2026-11-05　对象：`~/.dsh/redteam/rtis/rtis.py`（1,633 行）、`scripts/rt.py`（1,171 行）、`scripts/rtis_mcp.mjs`（506 行）、registry 4 文件、28 个 engagement。

## 3.1 运行时架构实况

```
zcode/AI (MCP 客户端)
  └─ rtis_mcp.mjs (stdio JSON-RPC, TOOLS: poc_search/poc_get/knowledge/task/skill/…)
       ├─ 检索 → rtis-bridge.js（动态定位最新副本；当前缺失，降级为 rt.py legacy）
       └─ 写入 → subprocess rt.py（唯一写者）
rtis.py（CLI 编排）
  ├─ task 状态机：CREATED→…→S0-S9 主链 + SIDE_STATES，transition 有跳段审计
  ├─ profile：scan（signal-catalog 正则→维度候选+置信度）/ set / explain
  ├─ gate：build_queries（任务特征）→ kb_search → sufficiency 三档 → ERI 钩子(P2)
  ├─ route：workflows.yaml 触发规则 → selected/excluded_workflows + 理由
  ├─ handoff：13 字段角色交接单 + check
  └─ aar：复盘 → knowledge_missed → k_proposal 候选
```

## 3.2 各层评估

| 层 | 实现质量 | 证据 | 缺口 |
|---|---|---|---|
| 任务状态机 | 高 | 状态转移白名单 + skipped 审计 + tasks/*.json 持久化 | 状态存 JSON 文件，无 DB；并发任务无全局视图 |
| 站型画像 | 高 | signal-catalog v2（信号→维度候选，冲突衰减公式成文）；8 维 schema | 画像质量依赖信号目录维护，无信号回归集 |
| 知识门禁 | 高 | gate 三档 sufficiency + per_query 记录 + ERI 钩子 | kb 检索在 zcode 侧降级（桥缺失）；门禁断言靠文字约定而非证据 id |
| 路由 | 中 | workflows.yaml 触发规则结构化，excluded 带理由 | 触发规则覆盖度未量化（WF 数量、命中率无统计） |
| 执行层 | 中 | 角色 R1-R8 稿件 + handoff 13 字段 + engagement 内 attack_step 全记录 | 执行记录在 assets.db 与 task JSON 两处，无关联 |
| 复盘 | 高 | aar() + k_proposal + 回归测试概念（run_tests） | proposal→正式库的采纳链路人工，无自动化门 |
| 权限控制 | 中 | 授权分级 T0/T2 设计成文；eris/rtis 层无强制执行（靠提示词纪律） | 无 AuthorizationRecord 结构化对象，全靠会话自觉 |

## 3.3 关键发现

1. **设计-实现差距最小的模块**：rtis.py 基本把 `RTIS_System_Architecture.md` 的 L0/L1/L3/L6 落地了（这是 11-02 升级工程的成果）。L2 检索的桥文件断裂是当前最大断点。
2. **Engagement 数据孤岛**：28 个 assets.db（每个 15+ 表含 FTS）结构同源、无 schema 版本、跨靶标不可查。历史案例检索（"有没有遇过类似站"）只能靠 k_relation，不能查执行明细。
3. **rtis_mcp.mjs 的纪律文案即接口契约**：工具描述里写"检索即执行，不向用户确认"，这是把安全边界放在提示词里。统一平台必须把它变成结构化授权对象（见 SECURITY_AND_GOVERNANCE.md）。
4. **gate 的 ERI 钩子（P2 加的）是目前唯一的跨系统集成点**，subprocess + 静默降级，M5 应升级为库内调用 + 错误上报。

## 3.4 对统一平台的输入

- Workflow/WorkflowExecution/ResearchTask/AgentExecution 四实体在 rtis.py 有直接对应物（workflows.yaml / task.steps / task / handoff+attack_step），域建模以现状为起点做规范化。
- 状态机、信号目录、触发路由三个机制是平台的研究方法论内核，迁移优先级高于任何存储重构。
