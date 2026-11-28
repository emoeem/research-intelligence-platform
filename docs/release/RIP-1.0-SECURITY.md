# RIP 1.0 安全文档

> 状态：安全边界与已知限制。日期：2026-11-28。

## 一、鉴权

REST 写接口和 MCP 管理面使用 Bearer/token 控制。当前个人工作区模式不代表多租户产品能力。跨工作区统一检索 API 保持 fail-closed。

写入需保留 Origin/Fetch Metadata 防护和现有审计；错误认证不得作为只读或写入模式的替代验证。

## 二、工具隔离

Tool Manifest 必须声明能力、网络、资源和 runtime 策略。Podman 路径使用只读根文件系统、keep-id、cap-drop ALL、挂载白名单和网络约束。

未受信任文件不得使用宿主机直接解析绕过容器。工具执行超时、取消、输出上限、trace 和 evidence 都必须落盘。

## 三、数据安全

- 外部资料一律视为数据，不得裸注入为系统指令。
- POC 正文和利用细节不得进入公开仓库或生产知识库。
- 来源、快照版本、Evidence hash 和审核状态必须可追溯。
- AI 结果不得覆盖人工 VERIFIED 或 APPROVED 状态。

## 四、来源与资产治理

APPROVED 表示来源或分类审查通过，不等于模板成功检测到漏洞或 POC 可复现。legacy verified 标志只能作为历史信息保留。

候选 Skill、Tool、Knowledge 关系必须保持 CANDIDATE 或 UNREVIEWED，直到人工审核。

## 五、安全验证

仓库安全扫描、cargo-deny、来源监测 SSRF、workspace 隔离、backup/recovery 和 repository governance 已有本地证据。

未完成项包括真实公网 DNS takeover、真实外部 LLM 凭据和授权靶机上的 PoC/Nuclei/Agent 端到端执行。不得伪造这些结果。
