# P7-1 浏览器实测证据（WebUI 五区壳层，2026-11-12）

| 项   | 值                                                                                                          |
| ---- | ----------------------------------------------------------------------------------------------------------- |
| 性质 | P7-1 批次（WebUI 壳层升级）的浏览器实测截图：顶部栏五要素、中央多标签（固定/关闭/深链接）、Inspector 与 Activity 折叠 |
| 工具 | browser-use 真实浏览器（ZCode In-app Browser，1440x900）+ 离线静态服务器 web/scripts/visual-server.mjs（4177 端口，服务 dist，无 API） |
| 声明 | 静态服务器不提供后端，页面数据面显示「加载失败」属预期（无 rp_server 实例）；本批验证对象是壳层布局与交互，页面导航、点击、开关均为真实 UI 操作，断言值取自 DOM 探针（aria-current、标签计数、URL、data-testid） |

## 截图清单

| 文件                                    | 内容                                                                                       |
| --------------------------------------- | ------------------------------------------------------------------------------------------ |
| 01-overview-five-zones.png              | 总览页五区全貌：顶部栏（切换器 1 · Dashboard、搜索 ⌘K、运行任务、Runner、通知）+ 左侧图标轨 11 工作区 + 中央 11 个固定 tab（编号 1.1 到 11.1）+ 右侧 Inspector（当前对象/关联知识工具/运行配置）+ 底部 Activity 五页签 |
| 02-deeplink-nmap-session-tab.png        | 深链接 /tools/nmap 直达：中央区追加可关闭会话 tab「nmap」，Inspector 当前对象标注「对象 工具 nmap / 路径 /tools/nmap」 |
| 03-close-tab-fallback-tools.png         | 点击「关闭标签 nmap」后：URL 回退 /tools，固定 tab「5.1 工具中心」aria-current=page，会话 tab 消失 |
| 04-inspector-collapsed.png              | 顶部栏 Inspector 开关点击后：右侧面板消失，中央内容占满（aria-expanded=false，complementary 计数 0） |
| 05-activity-collapsed-inspector-reopened.png | Activity 面板折叠生效（region 计数 0），Inspector 重新展开（complementary 计数 1）        |
| 06-activity-tool-runs-tab.png           | Activity 页签切换到 Tool Runs：tabpanel 计数 1，页签高亮迁移                                |
| 07-switcher-to-runs.png                 | 顶部栏切换器选「9 · Runs」：URL 跳 /runs，固定 tab「9.1 运行记录」激活，Activity 停在 Tool Runs 页签 |
| 08-notification-popover.png             | 通知按钮弹出气泡：「暂无通知 / 通知中心随后续批次接入」（Esc 与点击外部关闭，jsdom 用例覆盖） |
| 09-agents-placeholder.png               | Agents 占位工作区：标题 Agent Studio、徽标「工作区 7 · 占位」、data-testid=fills-in 文本为 P7-7、Inspector 标注 7 · Agents 与编号 7.1 |

## 实测路径与结论

1. 顶部栏五要素在场且可用：切换器（11 个 option，selectOption 跳转）、搜索（⌘K 面板，jsdom 用例覆盖）、运行任务（/runs 链接）、Runner（/runtime 链接）、通知（气泡开合）。
2. 中央多标签：11 个固定 tab 全程在场且带稳定编号；深链接 /tools/nmap 登记会话 tab 并在 Inspector 标注对象；关闭后按「所属工作区着陆页」回退。
3. Inspector 与 Activity 折叠开关生效，aria-expanded 状态与 DOM 显隐一致；Activity 五页签（任务队列/Tool Runs/日志/输出/状态）可切换。
4. 已知工具侧备注：IAB 后端对标签条内小尺寸按钮的 Playwright locator click 会超时（可操作性管线卡死），改用 dom_cua 节点路径点击（真实鼠标事件）完成；同一交互在 vitest（fireEvent）与真实浏览器两条路径均验证通过，非应用缺陷。
