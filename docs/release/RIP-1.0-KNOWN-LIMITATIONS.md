# RIP 1.0 已知限制

> 状态：显式限制清单。日期：2026-11-28。

## 一、检索评测

M4 batch 001 没有完整 human qrel。NDCG、MRR、MAP、Precision 和 Recall 等人工相关性指标不得声称已通过。

## 二、用户界面

M7 按参考和产品级验收关闭，不包含像素级 Figma parity、逐张历史截图签收、真实设备或 Lighthouse/Core Web Vitals。

M8 仍是 staged SPA；7800 默认入口未切换，7801 未作为生产入口承诺。

## 三、Agent 和模型

LLM Planner 的本地 mock、请求形状、超时和 Tool Engine 集成已验证，但真实外部 Provider 凭据和执行仍待验收。

## 四、武器库验证

Nuclei template 结构审查、来源批准或 RTIS static presence 不等于引擎验证、目标验证或执行验证。

POC 未经当前人工证据和本地 sha256 不得标记为完整有效或已修复验证。

## 五、部署与运维

生产反向代理、TLS、外部网络拓扑和真实部署回滚未由本仓库直接验证。

首次部署仍需独立确认数据库、备份、密钥、浏览器入口和宿主隔离能力。

## 六、数据和权限

项目当前按 personal 单工作区验收，不声明多租户 RBAC 已完成。

统一检索只对默认工作区可靠；跨工作区访问是 fail-closed，不是完整授权产品能力。

## 七、文档结论

上述限制不是开发任务省略理由。只有真实证据补齐后，才可更新本文件并重新声明 production-ready。
