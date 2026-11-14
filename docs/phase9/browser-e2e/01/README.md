# Phase 9 验收员独立浏览器复测（2026-11-14）

真实后端链路：SPA（web/dist）由验收侧反代 4180 提供，/api/** 转发真实 rp_server 7842
（RP_AUTH_TOKEN 开启，Bearer 经反代注入，主库 migrations/platform.db 只读 + aux fixture
运行库，真实 run 数据在场）。浏览器二进制：PW_CHROMIUM 纪律第 7 条，Playwright 用户缓存
chromium-1234（Chrome for Testing 151）显式注入 executablePath，零系统级安装。

| 文件 | 内容 |
| --- | --- |
| 01-mcp-status-card-accept.png | MCP 目录同步状态卡特写：对账正常徽章 + 目录条数 21（能力面 6 条固定）+ 最后对账时刻 |
| 02-tool-list-accept.png | 工具目录列表区：21 行（主库 20 + aux 增量 KB Keyword Search） |
| 03-tool-center-fullpage-accept.png | 工具中心整页（1440x900） |
| 04-tool-center-mobile-accept.png | 移动视口（390x844）单列降级 |

复测结论：状态卡真实取数渲染（catalog_count=21 与 /api/tools total=21 一致）、列表 aux
增量行在场、零 console error、移动视口无横向溢出。
