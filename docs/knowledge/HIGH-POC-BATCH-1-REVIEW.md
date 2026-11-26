# High POC Batch 1：接口与鉴权

## 队列与结论

| ID | 标题 | K级 | Lifecycle | 结论 |
|---:|---|---|---|---|
| 19 | Spring Cloud Gateway + springdoc 零认证接口契约暴露 | K0 | Candidate | NOTES 明确标记为 stub；脚本存在但没有原始 200/401/403 响应或下载的 OpenAPI 证据。 |
| 31 | H5 签名非空校验与跨路径会话复用 | K0 | Candidate | 脚本依赖未落盘的 tokens.tsv 与真实会话；没有成对请求/响应，保留为候选。 |
| 27 | 会员 token 访问代理端路由角色校验缺失 | K0 | Candidate | NOTES 要求角色反向验证和数据边界验证；脚本和说明不足以排除代理接口正常参数错误。 |
| 56 | 前置分发器路径遍历绕过 CDN | K0 | Candidate | 只有可执行探针，没有裸源站响应体、下载资源或 200 对照。 |
| 60 | BBIN 代理注册参数规则盲反推 | K0 | Candidate | 有脚本和说明，但缺少注册接口响应、阴性对照及成功创建或越权证据。 |
| 59655 | Spring Boot 微服务零鉴权与加密响应 oracle | K0 | Candidate | 只有探针模板，没有空 body、错误文案及加密响应的成对原始记录。 |
| 900042 | ThinkPHP send_code 短信绕过与 Cookie 回显 | K2 | Researched | `REPORT-傲众云-2026-11-04.md` 和 POC 脚本提供历史现场链；没有结构化 outcome_ref，故不提升为 Field-validated。 |
| 900048 | 短信签名由客户端密钥计算 | K2 | Researched | 原始报告、签名矩阵、请求头、响应和截图目录提供历史证据；仅保留在 Researched，不声称当前可利用。 |

## 关联核查

- 19 与既有 18 记录为同一 Springdoc/OpenAPI 暴露原理的两个实例：建立 `supports` 关系，不合并。
- 31 与 27 为跨角色/跨路径认证边界的独立实例：建立 `derived_from` 关系。
- 31 与 48 的关系只表示会话与 token 派生都涉及认证面，记录仍独立。
