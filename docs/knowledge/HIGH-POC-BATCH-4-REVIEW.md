# High POC Batch 4：隐私与身份认证

## 队列与结论

| ID | 标题 | K级 | Lifecycle | 结论 |
|---:|---|---|---|---|
| 900031 | Zeai 婚恋会员清晰照片匿名直取 | K2 | Researched | SUMMARY 和交付报告提供原图/打码图尺寸、拉普拉斯方差、匿名 HTTP 200 与重现命令；保留为历史证据。 |
| 900033 | Zeai 图形验证码挑战与答案同源及短信面 | K0 | Candidate | 真实脚本可自动读取挑战并提交答案，但缺原始挑战/校验响应、跨会话结果和授权短信送达证据；本实现保守为 K0/Candidate。 |
| 900034 | UEditor config/listimage/catchimage SSRF | K2 | Researched | QHINFO FINDINGS 和原始输出目录提供真实响应与端口判别证据；文件也保留在 POC。 |
| 900035 | Zeai 真人认证证件影像公网静态目录 | K2 | Researched | 报告和 SUMMARY 提供无 Cookie 直取、清洁测试及静态目录证据；脚本本身不增加未落盘真实证件数据。 |
| 40 | 多租户 SaaS schema oracle 与租户 token 取回 | K0 | Candidate | NOTES 和脚本要求负样本、跨租户交叉及真实租户数据验证；这些关键证据未找到。 |

## 关联核查

- 900031、900033、900035 建立同一 Zeai CMS 业务系统和安全边界关系，但每条独立保留。
- 900033 不继承 900031/900035 的证据结论；验证码结论未提升为 K2。
- 40 独立保留；它与其他 SSRF/租户隔离记录不合并。
