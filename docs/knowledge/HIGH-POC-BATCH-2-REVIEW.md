# High POC Batch 2：加密配置与 Token

## 队列与结论

| ID | 标题 | K级 | Lifecycle | 结论 |
|---:|---|---|---|---|
| 35 | app_config.txt AES-256-CBC 离线解密 | K0 | Candidate | NOTES 明确为 stub；脚本存在但缺少密文、解密结果、服务端接受请求和阴性对照。 |
| 36 | miWen SECRET 派生 | K0 | Candidate | 派生脚本未证明服务器接受伪造签名；缺请求/响应及重放结果。 |
| 43 | BBIN entrance 双层加密通道 | K0 | Candidate | 算法脚本存在，但缺少原始密文、响应和服务端复现。 |
| 45 | AES-256-GCM bundle 密钥匿名解密远程配置 | K0 | Candidate | NOTES 为 stub；缺 bundle、密文和回读响应。 |
| 48 | H5 token AES/MD5 派生离线铸造 | K0 | Candidate | NOTES 为 stub；缺服务端接受伪造 token 的请求/响应。 |
| 66 | SPA encrypt_query 静态 key/IV 伪造 | K0 | Candidate | NOTES 为 stub；缺加密请求、解密响应和服务端接受判据。 |
| 83 | 小程序 wxapkg 静态 AES 密钥/PII 解密 | K0 | Candidate | NOTES 要求重加密逐字节复现，但缺对应原始密文和响应包。 |
| 900009 | pig4cloud OSS/Hall 双层密钥还原 | K0 | Candidate | 只有脚本实现；缺在线响应、服务端接受和两环境交叉证据。 |

## 关联核查

- 35、36、43、45、48、66、83、900009 建立为共享密码学原理的关联族，不合并为重复记录。
- 48 与 31 的认证关系保留为跨 POC 支持关系，不推断等价漏洞。
