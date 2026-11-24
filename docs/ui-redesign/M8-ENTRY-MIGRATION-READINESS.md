# M8：单一 Web 入口迁移准备度

- 日期：2026-11-24
- 状态：STAGED — SPA 并行入口已验证；默认根切换仍 Blocked，等待用户验收与部署环境确认
- 目标正式 UI：web/ 下的 rip-web（React / Vite）
- 既有服务入口：apps/server/rp_server.py，默认监听 127.0.0.1:7800

## 当前入口与功能边界

### 旧静态 UI

apps/server/static/index.html 目前是只读 Beta Workbench，只有四个页签：

1. Overview：资源、知识、审核、任务等汇总。
2. Resources：资源关键词、等级和分页过滤。
3. Reviews：D5 / License / Mapping 审核队列过滤。
4. Tasks：研究任务状态分布与列表。

apps/server/static/app.js 通过 Fetch 调用既有 API，并从 localStorage 的 rpToken 读取 Bearer Token。401 时清除本地 Token 并跳转 /login.html。动态数据通过 DOM textContent 创建节点，不把响应数据直接拼成 HTML。

旧界面在页脚声明 API 仅 GET、绑定 127.0.0.1、无写端点。此声明只描述旧 UI 自身的请求方式，不是当前 rp_server 后端的完整 API 契约。

### 当前 rp_server

apps/server/rp_server.py 同时承担静态资源分发与 /api/*。现有代码包含 GET、受保护的 POST/PUT 写接口，例如：

- 插件 install / upgrade / enable / disable 等状态机操作。
- Workflow 定义保存和 Workflow Run 创建、暂停、恢复、重试。
- 文档导入。
- WSW Listener、CA、Fuzz Job 等端点。
- Repeater 请求更新端点。

这些写接口按既有后端规则进行 Bearer 鉴权；鉴权关闭时明确拒绝部分写操作。M8 不得把它们从服务中移除，也不得将只读旧 UI 的页脚说明外推为后端无写接口。

### rip-web

当前 rip-web 已有完整工作区路由：Overview、Projects、Research 子页、Knowledge、Documents、Tools、Plugins、Agents、Workflows、Runs、Traces、Runtime、Settings、Web Security（HTTP History、Repeater、Passive Audit、WebSocket Frames、Intruder / Fuzzer、Decoder、Comparer）。

其 UI 能力还包括工作区子导航、全局搜索、Inspector、Activity Dock、三语资源和响应式布局。它不是旧静态页的简单换肤，涉及更多现有 API 与受保护写操作，因此需要按页面、操作和权限逐项核对。

## 已知差异与阻断项

| 项目 | 旧静态 UI | rip-web | M8 当前结论 |
|---|---|---|---|
| 功能面 | 4 个只读页签 | 全工作区、详情与安全工作台 | 不能做单纯入口替换；需建立页面级 parity 清单 |
| API 服务 | 同一 7800 进程提供 | 使用现有 /api/* | 必须保留 rp_server 进程与 API 路径 |
| Bearer | rpToken，401 跳登录页 | 浏览器需有效授权上下文；未认证测试浏览器会返回 401 | 已确认未认证浏览器不能代表真实授权验收；需以正式授权会话验证读写权限 |
| 写操作 | 旧 UI 自身只发 GET | 部分页面执行受保护的 POST / PUT 操作 | 逐个验证确认步骤、Bearer、CSRF/Origin、审计、失败回滚 |
| 深链接 | 静态文件解析，不等同 SPA fallback | React Router 深层路径 | 7800 当前静态解析器没有已证明的 SPA history fallback；部署配置需先解决 |
| 回滚 | 旧 static 目录仍在 | 尚未切为 7800 根入口 | 必须有已演练的单步回滚路径 |
| 多语言 | 主要简体中文 | zh-CN / en-US / ja-JP | 不要求旧静态 UI 同期迁移；rip-web 的全路由语言验收需通过 |

## 切换前强制门禁

- [ ] 为旧 UI 四个页签建立逐项行为清单，包含过滤、排序、空态、错误态和深链接。
- [ ] 为 rip-web 每个工作区建立 API 清单，标出 GET、POST、PUT、受保护操作和审计要求。
- [ ] 使用有效 Bearer 会话测试 401、403、过期 Token、读接口、写接口和确认弹窗；不能使用鉴权关闭模式替代。
- [ ] 验证 Origin / CSRF、防止跨站写入的现有保护不被部署代理绕过。
- [ ] 决定部署拓扑：推荐保留 7800 为 API，单独将 rip-web 构建产物作为 SPA 入口；或在同一服务中增加明确的 SPA fallback。不得把任意未知 /api/* 路径回退到 index.html。
- [ ] 验证所有深链接直接刷新、浏览器前进/后退、登录跳转回原始路径。
- [ ] 验证静态资源路径、缓存策略、CSP、MIME、X-Content-Type-Options 与本机绑定范围。
- [ ] 做只读数据库快照 / 备份、发布前后 API smoke test、错误率观察和回滚演练。
- [ ] 保留旧静态 UI 与原始静态资源至少一个回滚窗口；未经用户明确授权不删除、不关闭。
- [ ] 只有在所有门禁通过并由用户验收后，才变更默认根入口。

## 建议迁移顺序

1. 先让 rip-web 在独立入口上完成部署级验证，API 仍指向 7800。
2. 增加对 SPA 深链接的显式 fallback，但 /api/* 与真实静态文件必须优先匹配；未命中的 API 继续返回 JSON 404。
3. 在真实 Bearer 环境里跑旧四页与新页面读写 parity；重点覆盖 Reviews、Resources、Tasks 以及 rip-web 的写操作。
4. 同时发布旧静态路径与 rip-web 路径，记录新 UI 的问题和 API 指标。
5. 用户验收后再切换默认入口。回滚只需还原入口路由，不迁移数据库、不改 API 端点。
6. 观察期结束后再决定是否保留旧静态 UI 的可访问路径。

## 当前可确认的状态

- 旧静态 UI 的四个页签与 Token 行为已从源码核对。
- rp_server 的静态解析器与多类写接口已从源码核对。
- rip-web 的前端工作区路由与三语资源已存在。
- 未在真实授权浏览器中完成所有写操作端到端验收。
- 未完成 7800 根入口的 SPA fallback、功能 parity 和回滚演练。
- 因此 M8 不应标为完成，也不应在当前阶段停用旧静态 UI 或修改默认 7800 行为。


## 2026-11-24 staged implementation update

### Implemented in this repository

- Added apps/server/stage_spa.sh: separate staged SPA process on 127.0.0.1:7801, static root web/dist, isolated SQLite clone rip-data/staging/platform.db. The script refuses the primary migrations/platform.db path and refuses to start without a configured Bearer token.
- RP_SPA_FALLBACK=1 enables HTML-only, extension-less deep-link fallback. Existing static files are served first; missing files with extensions remain 404; unknown /api/* routes remain JSON 404 and are never rewritten to index.html.
- Static response policy: index.html no-store; built assets immutable one year; other static files one hour. CSP, nosniff, X-Frame-Options, Referrer-Policy, Permissions-Policy and CORP are set.
- POST/PATCH/PUT share an Origin / Fetch Metadata guard. Cross-site fetch metadata and mismatched Origin are rejected before write handlers; Bearer and existing write allowlists remain in force.
- apps/server/selftest_spa_entry.py passes 5/5, covering deep links, missing assets, API 404 passthrough, Origin mismatch and same-origin pass-through.
- Live port 7801 smoke test: /projects/prj-deep-link 200; /api/projects and /api/research/assets return authenticated JSON; unknown API 404; missing asset 404; mismatched Origin 403; same-origin request reaches route handling; stage DB version 31; primary DB version remains 30.
- The SPA stage is available at http://127.0.0.1:7801/ and is not the default root.

### Remaining promotion blockers

- Port 7800 was not listening during this validation. The legacy static source tree remains intact, but an active legacy process was not available for a live rollback rehearsal.
- The actual reverse proxy / TLS termination / external access topology is not present in this repository context; security headers and cache behavior must be checked after that layer is known.
- The Figma file could not be read for frame metadata during this run; visual acceptance still requires the user's Figma-based review.
- Therefore M8 is STAGED — NOT DEFAULT. Do not switch the 7800 root or remove the old static UI until the user has inspected 7801 and deployment-level rollback is rehearsed.
