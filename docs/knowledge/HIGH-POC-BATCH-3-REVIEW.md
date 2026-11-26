# High POC Batch 3：重定向与第三方案例

## 队列与结论

| ID | 标题 | K级 | Lifecycle | 结论 |
|---:|---|---|---|---|
| 59648 | `_c` 参数 hex2bin 模板任意域跳转 | K0 | Candidate | HTTP 模板和浏览器步骤存在，但缺原始触发响应和跳转截图。 |
| 59649 | `_s` 参数零点击任意域跳转 | K0 | Candidate | 载荷脚本存在，缺浏览器响应和实际跳转证据。 |
| 59650 | `_s` 参数 hex2bin 模板族绕过 | K0 | Candidate | 模板族存在，缺请求/响应与浏览器跳转结果。 |
| 59651 | `X-Forwarded-For` 注释逃逸任意 JS 执行 | K0 | Candidate | HTTP 模板显示请求原语，但缺响应和浏览器执行证据。 |
| 900044 | Airbnb 支付 iframe origin 信任边界 | K2 | Researched | 原始报告包含完整请求、响应、Chromium 正负对照和独立复核记录；仍为历史证据，未生成结构化 outcome。 |
| 900046 | 静态资产服务端注入取证 | K2 | Researched | `EVENT-REPORT` 和 freeze 文件包含响应哈希、Range、双部署对比及本地复现；保留为历史事件证据，不转为字段级漏洞。 |
| 900048 | 短信签名可由客户端密钥计算 | K2 | Researched | 报告、请求响应头、签名矩阵和送达记录存在；受限授权发送证据不用于提升 lifecycle 到 Field-validated 或 Verified。 |

## 关联核查

- 59648 至 59651 为同一落地页站群重定向原理的不同实例或原语，建立方向性 `derived_from`/`supports` 关系，不删除原记录。
- 900044 为独立第三方支付 iframe 案例。
- 900046 为事件取证方法，不与 UEditor/静态注入记录错误合并。
