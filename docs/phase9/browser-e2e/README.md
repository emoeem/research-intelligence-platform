# Phase 9 P9-4 浏览器实测证据（Tool Center MCP 目录同步状态卡，2026-11-14）

| 项     | 值                                                                                                   |
| ------ | ---------------------------------------------------------------------------------------------------- |
| 性质   | P9-4 批次浏览器实测截图（SESSION-HANDOFF §九硬性要求）：Tool Center 新增 MCP 目录同步状态卡的逐区实测 |
| 工具   | Playwright 真实 Chromium（用户缓存 chromium-1234，PW_CHROMIUM 显式注入，零系统级安装，纪律第 7 条机器无关口径） |
| 载体   | web/tests/mcp-status.spec.ts + web/playwright.mcp.config.ts（模式 B：RP_P9_E2E=1，P6-8 同款链路，SPA 由 visual-server 提供、/api/** route 转发真实 rp_server 并注入 Bearer） |
| 后端   | rp_server 127.0.0.1:7832（RP_AUTH_TOKEN 开启，RP_DB_PATH=migrations/platform.db 生产主库只读，RP_AUX_DB=fixture 运行库副本 /tmp/p94_runs.db，RP_RATE_LIMIT_RPM=0） |
| 数据面 | fixture 运行库 = rp-tool register rip.kb.keyword_search + rp-workflow put wf-p8-tool + REST workflow 通道与 MCP rip.run_tool 通道各真实执行一次（tool_run 3 行、evidence/artifact 2 行、trace 锚在场）；主库 = 20 工具 ETL 注册面 |
| 声明   | 页面导航、取数、截图均为真实 UI 操作；Bearer 经 route 转发注入（等价反向代理注入凭据的部署形态，P6-8 同款声明） |

## 截图清单（真实后端，本目录）

| 文件                                | 内容                                                                                     |
| ----------------------------------- | ---------------------------------------------------------------------------------------- |
| 01-mcp-status-card-real.png         | MCP 目录同步状态卡特写：对账正常徽章 + 目录条数 21（能力面 6 条固定）+ 最后对账 2026-11-14 12:50 + 对账 scope 说明行 |
| 02-tool-list-real.png               | 工具目录列表区（同页逐区）：21 行（主库 20 + aux 增量 1），lifecycle/governance/health/T 级徽章在位 |
| 03-tool-center-fullpage-real.png    | 工具中心整页（桌面 1440x900）：状态卡位于页头过滤之下、目录列表之上；列表尾行 KB Keyword Search（VALIDATED，aux 增量合并行） |
| 04-tool-center-mobile-real.png      | 移动视口（390x844）：状态卡与列表单列降级，无横向溢出                                     |

## route-mock 三态截图（mock/，离线模式 A，状态卡三态覆盖）

| 文件                        | 内容                                                                                     |
| --------------------------- | ---------------------------------------------------------------------------------------- |
| mock/mcp-status-ok.png      | 对账正常态（route-mock：目录条数 20 / 能力面 6 / 对账正常徽章）                           |
| mock/mcp-status-drift.png   | 对账漂移态：对账漂移 1 条徽章 + lifecycle_wordlist 漂移条目如实展示                        |
| mock/mcp-status-unavailable.png | 数据面不可读态：available=false 诚实空态说明（不造假状态）                             |

## 实测路径与结论

1. 真实后端 /tools 页：MCP 状态卡在位且对账正常（catalog_count=21 与 /api/tools total=21 一致，MCP tools/list 注册面 total 同值，三面对账由 crates/rp-mcp-parity CI 门禁常驻断言）。
2. 状态卡三态（正常 / 漂移 / 不可读）在离线模式 A 全部截图留证；真实后端模式走对账正常态。
3. 移动视口单列降级无横向溢出（视觉门禁 pages.spec.ts 55 用例同批全绿，/tools 页 marker「共 20 个工具」不变）。

复跑命令（仓库根）：

```bash
# 真实后端（fixture 准备与两通道数据面同款口径见 docs/releases/RELEASE_2026-11-14_PHASE9.md §二；本行数据面描述见上表）
RP_P9_E2E=1 RP_P9_BASE_URL=http://127.0.0.1:7832 RP_P9_TOKEN=<token> \
  PW_CHROMIUM=~/.cache/ms-playwright/chromium-1234/chrome-linux64/chrome \
  npx playwright test tests/mcp-status.spec.ts -c playwright.mcp.config.ts   # 在 web/ 下执行
# 离线三态
npx playwright test tests/mcp-status.spec.ts -c playwright.mcp.config.ts     # 在 web/ 下执行
```
