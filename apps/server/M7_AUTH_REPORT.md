# M7 鉴权与服务化报告

日期：2026-11-07　执行：主会话（自主工作流授权下）
范围口径：只写实际执行事实。验证基于生产库 `migrations/platform.db`（M1 全量批，evidence 700 行）。

## 一、交付物

| 文件 | 说明 |
|---|---|
| `apps/server/rp_server.py` v0.2.0-m7-auth | Bearer Token 鉴权（hmac constant-time 比较 + 失败 0.3s 节流 + 401 携带 WWW-Authenticate）；令牌来源 RP_AUTH_TOKEN > RP_AUTH_TOKEN_FILE > 默认令牌文件（600 权限校验，过宽告警）；无令牌配置时为本地开发模式并输出显著告警；默认库切换为生产 platform.db |
| `apps/server/static/login.html` | 登录页（深色主题一致），令牌校验通过后存 localStorage |
| `apps/server/static/app.js` | fetch 统一附加 Authorization；401 清除令牌并跳转登录页 |
| `apps/server/rp_service.sh` | systemd user 服务管理（gen-token/install/uninstall/start/stop/restart/status/logs），无 sudo，单元写入 ~/.config/systemd/user/ |
| `apps/server/selftest_auth.sh` | 鉴权自验脚本（11 项，可重复执行） |

安全设计保持：mode=ro + query_only 双保险、无写端点（POST 仍 405）、路径穿越防护、SQL 全参数化、兜底异常不泄露堆栈。

## 二、验证（实测）

### 自验脚本 11/11 PASS

无令牌 401、WWW-Authenticate 头在场、错误令牌 401、正确令牌 200、evidence API=库直查（700=700）、登录页与静态资源免令牌可访问、POST 仍 405、路径穿越仍 404、开发模式可用且启动输出显著告警。

### 真实部署与端到端（本机 systemd user 服务）

- `rp_service.sh gen-token`：生成 `.rp-workbench-token`（600，已入 .gitignore，不入库）
- `rp_service.sh install`：单元已 enable --now，`systemctl --user is-active` = active
- 端到端：无令牌 401；正确令牌 200 且数字与库直查一致（resources 81 / evidence 700 / tasks 42 / alias 1273，即 M1 全量批数据在线）；登录页 200；错误令牌 401
- 开机自启：WantedBy=default.target（用户登录后拉起；如需免登录自启需 enable-linger，属用户决策）

## 三、边界与遗留

1. 令牌为单用户静态令牌，无会话过期与轮换机制（轮换 = 删除令牌文件重新 gen-token + 浏览器重新登录）；多用户/过期策略属后续增强。
2. 未配置 HTTPS（绑定 127.0.0.1，令牌不出本机；如需对外暴露应先加 TLS 反代）。
3. tasks API 的空表提示文案（"待 M1 全量数据接入后显示"）已不触发（现 42 条），文案下次 UI 迭代顺带更新。
4. rate limit 仅 0.3s 节流，无失败锁定；对外暴露场景需前置网关。
