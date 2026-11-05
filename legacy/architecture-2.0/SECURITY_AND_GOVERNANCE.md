# 11. SECURITY_AND_GOVERNANCE

日期：2026-11-05　基础：现网纪律成文（项目 AGENTS.md、RTIS 权限模型 §六、ERI 执行权限 §18）+ 本次审计发现的执行缺口。

## 11.1 现状与缺口

| 域 | 现状（已实现） | 缺口（平台要补） |
|---|---|---|
| 授权 | engagement.yaml + SCOPE.md 是授权事实源；RTIS T0/T1/T2 分级成文 | 授权是文档不是对象：T 级检查靠提示词自觉，无结构化 AuthorizationRecord 强制闸 |
| 工具执行 | registry 有 readonly 标记；fscan/gogo 元数据失真 | 无执行日志、无输入输出 Schema、无统一权限闸（Execute 前置检查缺失） |
| AI 数据访问 | ERI 检索预算（Top5/20 上限）、Reader 缓存按需读取 | 无全局 Token Budget 账本；ai_usage_log 表存在但零接入 |
| Prompt 注入 | Reader 落盘不执行；技能纪律文案 | 无注入检测启发（README 指令模式扫描）；外部内容与系统提示词之间无硬隔离声明 |
| 凭据 | 无凭据入库（审计确认三库无 secret 字段）；Token 只进请求头 | 凭据管理无规范（env 注入约定未成文） |
| 敏感证据 | .gitignore 白名单（PII 证据留本机） | sensitivity 无结构化标记，依赖人的记忆 |

## 11.2 平台治理机制（设计，未实现）

1. **AuthorizationRecord 实体化**：`{engagement_id, authorized_assets[], allowed_activity{}, t_level_cap, expires_at}`，由 tool-engine/workflow-engine 在每次 Execute/Route 时强制校验；无记录 = 只允许 T0。
2. **Execute 权限闸**：`Execute(tool, input, ctx)` 内部检查 ctx.authorization 覆盖 tool.permission_level；T2 附带审批引用（Review 实体）。
3. **Token Budget 账本**：ai_usage_log 升级为全局账本（per task/agent 预算、告警阈值、超预算拒绝而非降级）。
4. **外部内容三不**：不执行、不自动安装、不进系统提示词；Reader/Adapter 输出一律标 `untrusted: true`，进 LLM 前过注入启发扫描（规则先行：命令模式/密钥模式/指令式祈使句标记）。
5. **Evidence sensitivity**：三级（public/internal/sensitive），sensitive 走独立存储路径 + 访问审计；继承现 gitignore 白名单语义。
6. **审计不可绕过**：audit-engine Emit 为所有写路径强制出口（库内触发器兜底），删除/归档动作必有前置 AuditEvent。
7. **红队特有约束保留**：未授权目标零 T1/T2（硬编码在 workflow-engine，不接受配置覆盖）；报告只写实际执行事实（产出模板字段绑定 Evidence 引用）。

## 11.3 威胁模型要点（平台自身为被攻击面）

- 外部资源（README/仓库）→ AI 上下文：注入防线如上 4。
- 上游列表投毒（收录恶意项目）：D5 + License + 隔离三闸已有，补"收录即隔离待审"默认态（现 QUARANTINED 机制推广到所有新 DISCOVERED 资源）。
- 本地模型调用面（MCP）：工具描述含"检索即执行"类强纪律文案，迁移时改为结构化 permission_context，避免把安全策略放在自然语言里。
- 供应链：Rust vendoring（DEC-001）+ 工具 provenance（ToolVersion）双保险；二进制替换必须新版本行 + 哈希。

## 11.4 与现网纪律的兼容声明

本报告不降低任何现行约束（engagement 事实源、600 权限敏感文件、证据最小保存、报告一致性原则全部原样继承）；平台化的唯一变化是**把靠自觉的纪律变成靠结构强制的纪律**。
