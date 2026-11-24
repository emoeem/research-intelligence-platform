# M4–M8 自主推进状态与最终边界

日期：2026-11-24

## 阶段判定

| 阶段 | 状态 | 证据 / 结论 |
|---|---|---|
| M4 Projects / Research Assets | **CLOSED** | 两个领域均独立建模；031 迁移、Rust Store/CLI、HTTP API、关系管理、前端 CRUD/详情、三语言、端到端测试完成。`M4-REPORT.md`、`selftest_projects_assets.sh`。 |
| M5 Web Security 工作台 | **CLOSED** | History、Repeater、Proxy/CA、Fuzzer（复用 Rust FuzzSpec）、WebSocket、Decoder、Comparer、Passive Audit 路由与真实 API 已存在；Scope、Body 按需加载、脱敏与信任确认保持不变。 |
| M6 Agent/Workflow/Runs/Runtime | **CLOSED** | Agent Studio、Workflow Studio、Runs、Run Detail、Trace、Runtime 页面与真实 API 已存在；补强了 /api/runs 对 WorkflowRun / WorkflowRunNode 真实持久化记录的兼容投影，phase9 REST 自测恢复 33/33。 |
| M7 全站视觉/响应式收口 | **AUTOMATED PASS / HUMAN REVIEW OPEN** | 85/85 Playwright 通过；12 路由 × 3 语言 × 2 主题 × 5 视口 360/360 响应式 smoke；Projects/Assets 双主题截图矩阵；22 files / 155 Vitest。仍需人工审阅既有视觉基线、Figma 像素级对照及人工交互签收。 |
| M8 单一入口迁移 | **STAGED — NOT DEFAULT** | 新 SPA 并行入口 `http://127.0.0.1:7801/` 已运行；隔离 DB 副本；SPA 深链接、API 404 透传、缺失静态资源 404、Origin/Bearer 与静态安全头已验证。未切换默认根。旧 7800 服务当前未监听；部署反向代理/TLS 与实际回滚演练仍待确认。 |

## 最终自动化门禁（2026-11-24）

- `cargo test --offline -p verification-engine`：56 passed（45 unit + 2 project/assets integration + 9 integration）。
- `selftest_projects_assets.sh`：PASS；真实 HTTP CRUD/关联/工作区隔离/来源与安全验证；primary `migrations/platform.db` 保持 user_version=30。
- `selftest_phase9_rest.sh`：33 PASS / 0 FAIL；包括 API 文档与路由对账、Bearer、限频、审计、Trace 双头和生产库零写指纹。
- `selftest_spa_entry.py`：5/5 PASS；深链接 fallback、静态 404、API 404、Origin、CSP/缓存/安全头。
- Web：TypeScript PASS；Prettier PASS；Vitest **22 files / 155 tests PASS**；production build PASS。
- Playwright：**85/85 PASS**；其中视觉/axe 48 组合与其他专项覆盖，新增 12 路由 × 3 语言 × 2 主题 × 5 视口响应式 smoke 360/360 PASS。
- 本地生产构建性能 smoke：12 routes × desktop/mobile 共 24 次导航，24/24 HTTP 200，API/console errors 0，warm-cache loadEventEnd median 24 ms / p95 28 ms；非 Lighthouse/Core Web Vitals。详见 M7-PERFORMANCE-REPORT.md。
- `git diff --check`：最终复核 PASS（2026-11-24）。
- 仓库级 `cargo fmt --all -- --check` 曾发现仓库内既有其他 crate 的格式问题；本轮修改 crate 的 `cargo fmt -p verification-engine -- --check` 通过。没有对全仓执行格式化以免改动无关文件。

## M7 视觉标准与 Figma

用户决策：M7=reference、Figma=existing。四张用户提供的桌面深色参考图用于共同视觉语言（Tools & Agents、Report Studio、Research Investigation、Workflow Builder）。Projects/Assets 使用现有 RIP tokens 与产品域规格补足；它们没有在四张图中直接展示。

Figma canonical source：https://www.figma.com/design/aqB4Y2xjlTva50C6k2eSQn/RIP-%E2%80%94-Research-Intelligence-Platform?m=auto&t=Rqfew6Udowb4Zx9m-1。当前 Figma MCP Starter 调用额度已触顶，本轮 get_metadata 请求被额度限制拒绝，未能读取页面/Frame 节点；没有声称完成像素级 Figma 对照。用户提供的四张截图已作为本轮 reference。当前 85 项 Playwright 自动化门禁不等于人工视觉签收。下一轮在 Figma 调用额度可用时逐页核对，审阅现有基线差异，并完成 5 个 viewport 的全站人工检查。

## M8 staged 拓扑与安全

- `apps/server/stage_spa.sh` 在 `127.0.0.1:7801` 启动新 SPA；`web/dist` 为静态目录；`RP_SPA_FALLBACK=1` 只对 HTML 且无文件扩展名的页面导航生效。
- staging 数据库位于 git-ignored `rip-data/staging/platform.db`，由主库复制，首次 Project/Asset API 调用后独立迁移到 31；主库仍是 30。
- API 未知路径始终 JSON 404；缺失 JS/CSS 等静态资源始终 404。
- POST/PATCH/PUT 使用同源 Origin / Fetch Metadata guard；继续要求 Bearer 与原有白名单。
- `index.html` no-store；`assets/*` immutable；设置 CSP、X-Frame-Options、Referrer-Policy、Permissions-Policy、CORP 与 nosniff。
- 新入口当前可访问 `http://127.0.0.1:7801/`。旧 7800 目前未监听；本轮没有覆盖或禁用旧静态文件，也没有更改正式默认入口。

## 仍然需要人工/部署环境确认的边界

1. M7：Figma 文件像素级核对、变更基线人工签收、五视口完整人工检查和真实浏览器交互验收。
2. M8：旧 7800 入口在真实部署拓扑中的启动/回滚路径、反向代理与 TLS、安全头在边缘层的最终值、备份恢复演练。用户验收前不切换默认根。
3. 真实发现 Provider/外部 Feed 目前尚未自动填充 Research Assets；当前已交付的是手动导入与同一 Observation/Endpoint/Technology API 契约，Provider 适配可继续接入。
4. 本轮没有 commit/push/reset/clean/stash/checkout/rebase；原有工作区改动继续保留。
