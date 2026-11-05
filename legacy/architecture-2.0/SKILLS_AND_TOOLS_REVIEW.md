# 5. SKILLS_AND_TOOLS_REVIEW

日期：2026-11-05　对象：`~/.dsh/redteam/skills/`（125 文件）+ `rtis/data/registry/{tools.json,skills-index.json}` + zcode MCP/Agent 面。未修改任何文件。

## 5.1 Skills 实况（125 个，平均 6.4KB，最大 34.8KB）

| 维度 | 实测 | 问题 |
|---|---|---|
| front matter 字段覆盖 | name/description 125/125；whenToUse/role/enabled 119/125；generator 标记 19；tags 仅 6 | 6 个缺 role/enabled（无法被 role 路由）；tags 近乎不存在 |
| role 分布 | vuln-scan 72 (57.6%) / recon 20 / internal 14 / plan 7 / exploit 6 | 严重偏科：扫描类占过半，报告/规划类薄 |
| 命名聚类 | hunt-* 42 (33.6%) / framework-* 8 / miniprogram-* 6 | hunt 系为事实上的子家族，但无层级元数据表达 |
| 元数据结构 | 无输入/输出/权限级/依赖工具声明 | 与 rtis 路由只靠 role 单维 |
| 与知识库关联 | k_meta.archetypes/method_tags 挂知识侧；skill 侧无反向指针 | 双向关联靠检索期 join，无静态映射 |
| 重复度 | 未逐对去重；hunt-* 42 个内部疑似重叠 | 需要能力矩阵审计（M1 任务） |

## 5.2 Tools 实况（registry 20 项）

- 结构：`{name, kind, purpose, installed, version, readonly, external}`。有只读标记与安装状态，**缺**：输入输出 Schema、权限分级（T0/T1/T2）、执行日志、错误状态、超时策略。
- 实测缺陷：fscan 版本探测命令错误（返回"flag needs an argument"）；gogo `installed:false` 但 `~/.dsh/redteam/toolkit/gogo` 有二进制（P0 备忘：源码本地构建，registry 未更新）。
- 执行记录：无。工具调用史只散落在 assets.db 的 attack_step 与会话内，无法回答"这个工具最近一次跑是什么时候、结果如何"。

## 5.3 Agent Runtime 实况

- 载体：zcode harness（会话式）+ rtis_mcp.mjs 工具面 + RTIS 五角色稿件（recon/assess/vuln-scan/exploit/internal + plan）。
- 上下文管理：无显式机制（靠 harness 的会话/压缩）；RTIS 侧 handoff 13 字段是唯一结构化上下文传递。
- 错误处理：工具层无统一错误分类；角色失败靠 handoff status=failed/blocked 表达。
- 任务生命周期：task JSON 状态机（S0-S9）+ engagement 目录，跨会话可恢复；但"哪个 agent 在哪个任务上用了哪个工具得到哪个结论"三段无关联 ID。

## 5.4 对统一平台的输入

- **Skill 域实体**：现状是"Markdown 提示词 + 单维 role"。平台 Skill 应为 `{id, name, version, role[], input_schema, output_schema, permission_level, required_tools[], knowledge_refs[], body_ref}`，Markdown 正文降级为渲染产物。迁移脚本可从现有 front matter 自动抽取 119/125，6 个缺字段的人工补。
- **Tool 域实体**：registry 已有骨架，补 schema/权限级/执行日志三件套。T0/T1/T2 分级在 RTIS 架构文档已成文，落表即可。
- **Agent/AgentExecution**：handoff 13 字段 + task 状态机已是事实标准，域模型直接采纳；补全局 execution_id 打通"任务-角色-工具-证据"链。
- 125 个 skill 文件一条不改（本阶段红线）；能力矩阵与去重审计排入 M1（只读）。
