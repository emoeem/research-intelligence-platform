# RIP 1.0 Agent 与 Workflow

> 状态：实现能力与验收边界。日期：2026-11-28。

## 一、Agent

Agent Runtime 包含版本化定义、能力引用、Context Broker、Tool Engine、护栏、暂停、取消、Artifact、Evidence 和 trace。

模型 Planner 支持 OpenAI-compatible endpoint，包括本地 mock 和 DeepSeek/Kimi 等 provider 接口形态。接口密钥只从指定环境变量读取。

真实 Provider 测试待本地安全注入凭据后执行；现有 mock 和集成测试不能替代真实 provider E2E。

## 二、Workflow

Workflow Engine 支持定义版本、状态持久化、Tool、Knowledge、Condition、Parallel、Agent 和 Human 节点。运行可查询、暂停、恢复、重试和取消。

运行恢复和 checkpoint 依赖持久化状态机和六 ID trace，不可依赖临时内存推断。

## 三、工具权限

Agent 在 Broker 装配能力范围内规划工具；Tool Engine 仍执行最终 Schema、workspace、权限、network 和资源检查。

缺少 Evidence 或权限时应 abort 或 abstain，不得继续生成确定性结论。

## 四、已验证能力

- Agent Loop 和 mock Planner 集成测试。
- Workflow 持久化、暂停恢复和人工介入。
- MCP/REST parity。
- run/artifact/evidence/trace 关联。
- 真实工具执行自验脚本。

## 五、剩余验收

- 真实外部 Provider 端到端。
- 授权靶机上的完整 Research、Vulnerability、POC 和 Nuclei 工作流。
- 模型结果引用来源快照与执行证据的长期一致性。
- 危险工具绕过权限的负例审计。
