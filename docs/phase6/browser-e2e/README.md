# Phase 6 浏览器实测证据（独立验收员，2026-11-12）

| 项     | 值                                                                                                   |
| ------ | ---------------------------------------------------------------------------------------------------- |
| 性质   | Phase 6 独立验收的浏览器实测截图（SESSION-HANDOFF §九硬性要求），与批次实施的 web/qa/plugins 截图相互独立 |
| 工具   | browser-use 真实浏览器（ZCode In-app Browser，1280x720）+ vite dev（/api 代理到 rp_server 实例）        |
| 后端   | rp_server 127.0.0.1:7821（RP_AUTH_TOKEN 开启，RIP_PLUGIN_DB=/tmp/rp-p6-verify/zerowrite-dev.db 开发副本，RP_AUX_DB=/tmp/rp-p6-verify/e2e-tool.db，RP_PLUGIN_BIN=target/debug/rp-plugin） |
| 声明   | SPA 登录面尚未建立（web/src/lib/api.ts:20 注释，P5-4 定会话模型），页面侧为 /api 请求注入 Bearer 的测试垫片（等价反向代理注入凭据的部署形态）；页面导航、点击、状态刷新均为真实 UI 操作 |

## 截图清单

| 文件                                | 内容                                                                                     |
| ----------------------------------- | ---------------------------------------------------------------------------------------- |
| 01-plugin-detail-L1-knowledge.png   | 插件详情 L1 目录级：仅元数据卡片（版本/许可证/信任级/来源/lock sha256），capability 清单为 0 并提示「L2 起展示」，无 docs 正文 |
| 02-plugin-detail-L3-knowledge.png   | L3 调用级：完整 Manifest 与 docs，兼容性表（rip_core、manifest_schema），capability 清单 2 项 |
| 03-plugin-enabled-after-click.png   | 页面点击「启用」后：状态徽章 DISABLED 变 ENABLED，按钮组变为禁用/卸载/摄取，更新时间刷新，审计事件新增 plugin.enable（走真实 API 到 rp-plugin 子进程） |
| 04-plugin-disabled-after-click.png  | 页面点击「禁用」后：状态回到 DISABLED，审计事件序列 plugin.disable/enable/disable/enable 完整 |
| 05-plugins-list-categories.png      | 插件中心目录四分类（已安装 3 / 已禁用 1），状态徽章 + trust 徽章（local/remote）+ 类型（知识/工具/项目）+ capabilities 计数 |
| 06-tool-center-tls-inspect.png      | 工具中心共 21 个工具（主库 20 + aux 增量 1），新工具 rip.http.tls_inspect 在列表中           |
| 07-tool-center-tls-inspect-card.png | TLS Inspector 卡片（rip.http.tls_inspect · tool · 2026-11-12 01:20），列表内滚动中途帧       |

## 实测路径与结论

1. 未带凭据访问 /plugins：页面诚实呈现「登录状态已过期」空态（AUTH_EXPIRED_EVENT 生效）。
2. 注入 Bearer 垫片后：目录、详情、三级加载、启停按钮全部可用；启用/禁用各一次经真实 API 生效并刷新状态与审计 trail。
3. Tool Center：脚手架新增工具 rip.http.tls_inspect（经 rp_new_tool.py 一条线注册到 aux 开发库）在页面露出。
4. DOM 探针佐证：详情页 innerText 含 docs/usage.md 与 Manifest；工具中心 innerText 含「TLS Inspector / rip.http.tls_inspect · tool · 2026-11-12 01:20」；总数共 21 个工具。

## SK-2 批次截图（2026-11-12，三源知识来源接入）

| 文件 | 内容 |
| --- | --- |
| sk2-01-plugins-list-source-badges.png | 插件中心三卡片：许可证徽章（CC0-1.0 / Apache-2.0 / UNLICENSED）、快照短 Commit（440486f / 1867eb6 / 45a12a7）、来源条数与 APPROVED 计数（85 / 102 / 75） |
| sk2-02-plugin-detail-commit-homepage-review.png | tongui 详情 L2：来源主页链接、快照 Commit 全 40 位、内容类型与许可策略（catalog_entry · metadata_only）、审核状态（APPROVED: 75 + 最近摄取）、审计事件含 plugin.knowledge.ingest |
| sk2-03-knowledge-sources-view.png | 知识库页「知识来源」视图：共 262 条，条目带 APPROVED / 目录条目 / CC0-1.0 / 仅目录元数据 / 短 Commit 徽章与分类行 |
| sk2-04-knowledge-sources-tongui-filter.png | 按 tongui 过滤：75 条，UNLICENSED + 仅元数据与哈希 + 45a12a7，分类「渗透测试全套（74 篇，13 章）」，blob URL 可点跳回原始来源 |

实测环境：browser-use 真实浏览器 1280x800，vite dev（/api 代理）到 rp_server 127.0.0.1:7823（RP_PLUGIN_DB=/tmp/sk2.db 开发库，本地无鉴权模式）；页面导航与过滤均为真实 UI 操作（视图切换按钮经页面脚本触发点击，IAB 高层 click 对该 SPA 连续超时的替代路径，见 SK2-SOURCES-NOTES.md §八）。方法与对账记录：docs/phase6/SK2-SOURCES-NOTES.md。
