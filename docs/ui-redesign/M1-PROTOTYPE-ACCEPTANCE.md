# RIP UI Redesign M1 原型验收记录

> 状态：设计规格与浏览器原型验收通过，Figma 画板像素级核对待补，M1 尚未正式冻结。
> 日期：2026-11-24
> 基线：`docs/ui-redesign/M0-BASELINE.md`

## 一、交付范围

M1 已完成：
- 12 个工作区信息架构和既有路由保留规格。
- Design System、AppShell、Navigation、Component、Responsive、Accessibility 六份设计规格。
- 四项 ADR：单一 UI 入口、成熟依赖选择、导航与面板职责、HTTP Message Viewer/Editor。
- Dashboard、HTTP History、Repeater、Agent Studio 的浏览器原型。
- 其余工作区的页面信息架构与状态规格。
- Dark/Light、Desktop/Mobile 的原型截图。

原型位置：`docs/ui-redesign/prototype/index.html`。所有数据均为固定演示样例，操作只在当前页面内改变本地展示状态；不访问真实 API、不写数据库、不发送 HTTP 测试请求。

## 二、浏览器验收

浏览器：本机 Chromium 158.0.8127.36。
视口：Desktop 1440×960；Mobile 390×844。

| 检查项 | 结果 | 证据 |
|---|---|---|
| Dashboard 首屏布局 | PASS | `screenshots/dashboard-dark-desktop.png` |
| HTTP History 条件搜索 | PASS | 输入 `admin` 后仅显示 1 条记录 |
| History 选中行更新 Request 详情 | PASS | 选中 `/v1/admin` 后详情同步 |
| 状态徽标与行耗时一致 | PASS | 403 显示 danger、42 ms |
| Repeater 演示发送 | PASS | 显示本地模拟响应，并声明未向服务器发送请求 |
| Dark/Light 主题切换 | PASS | Agent Studio 双主题截图 |
| 390px 窄屏 | PASS | `document.body.scrollWidth=390`、`innerWidth=390`，无横向溢出 |
| 浏览器控制台与 pageerror | PASS | 0 errors |
| axe Dashboard | PASS | 0 violations |
| axe HTTP History（选中 403 记录） | PASS | 0 violations |

修复记录：
1. 补充筛选下拉的 `aria-label`，关闭 axe `select-name`。
2. 为 Sidebar 和 Activity summary 增加可区分的 landmark 名称，关闭 `landmark-unique` 与 `region`。
3. 错误徽标改为红色底纹、正文前景色和红色边框，避免原文字/背景对比度 4.06:1 低于 AA 4.5:1。
4. HTTP 状态徽标和耗时从当前所选行动态映射，避免 403 误显示绿色和默认耗时。

## 三、截图清单

可复现截图命令：

```bash
cd /media/emo/code/pensting/research-platform
node docs/ui-redesign/prototype/capture.mjs
```

脚本实测退出码 0，结果为 pageErrors=0、History 示例行数 5、关键词过滤匹配 3 条、390px 视口 document/body 宽度均为 390。生成十张标准化截图：

- `screenshots/m1-dashboard-dark-1440x900.png`
- `screenshots/m1-dashboard-dark-390x844.png`
- `screenshots/m1-history-dark-1440x900.png`
- `screenshots/m1-history-selected-dark-1440x900.png`
- `screenshots/m1-history-light-1440x900.png`
- `screenshots/m1-history-light-1280x800.png`
- `screenshots/m1-history-light-390x844.png`
- `screenshots/m1-repeater-response-dark-1440x900.png`
- `screenshots/m1-agent-studio-dark-1440x900.png`
- `screenshots/m1-agent-studio-light-1440x900.png`

另有独立验收截图：`dashboard-dark-desktop.png`、`history-dark-desktop.png`、`history-dark-mobile.png`、`history-light-mobile.png`、`repeater-dark-desktop.png`、`agents-dark-desktop.png`、`agents-light-desktop.png`。

## 四、未完成项与风险

1. **Figma 视觉复核未完成**：已读取 Figma 文件 3 个页面的元数据，但 Figma MCP Starter 计划截图调用额度已触顶，未能查看 Overview 画板的像素级内容。未修改 Figma，也不宣称与 Figma 像素一致。恢复额度后须补核。
2. 浏览器原型不是生产页面，也没有连接真实 API。页面结构和业务状态仍需在 M2-M7 按各业务契约实现。
3. 当前 prototype 页面以一份 HTML 内联样式与脚本构成，目的是先锁定信息架构和交互方向。进入生产实施时，组件必须迁入 React 并复用生产 RIP Token，不能将演示 HTML 直接部署。
4. 当前 12 个工作区内的某些页面在原型中只保留结构预览，不代表该功能全部交互已实现。

## 五、M1 门禁结论

M1 的工程规格、主要交互原型和本地浏览器验收已通过。因 Figma 画板视觉复核受外部计划额度限制，仍缺最后一项设计来源对照，因此 M1 状态为 **Ready for Design Review**，而不是已冻结。

M2 生产 AppShell 编码暂不开始。完成 Figma 复核或经用户接受现有代码 Token 作为本批唯一视觉基准后，再启动 M2。


## 六、本轮补充验收（2026-11-24）

在原有浏览器验收基础上补充了以下交互和截图：

- Activity Dock：点击底部入口可展开/收起；展开态展示 Runs、Logs、Output、Runner 页签和演示运行进度。
- Mobile Navigation：390px 视口可通过顶部菜单打开带完整文字和分组的 Sidebar 抽屉；点击遮罩或选中工作区后关闭。
- Mobile Inspector：在 History 选择请求后以侧边抽屉展示该请求上下文；可通过顶部 Inspector 按钮关闭。
- 移动端抽屉打开后无横向溢出；Sidebar 展开态文字、分组和账号区均可见。

新增截图：
- `screenshots/m1-activity-dock-dark-1440x900.png`
- `screenshots/m1-mobile-navigation-light-390x844.png`
- `screenshots/m1-inspector-mobile-light-390x844.png`

本轮再次运行 `node docs/ui-redesign/prototype/capture.mjs`，Chromium pageErrors=0；History 样例行数=5，关键词过滤匹配=3；390px 下 document/body scrollWidth 均为 390px。新增截图是设计原型的交互证明，不代表生产 UI 已实现。



## 七、M1 文档冻结补充（2026-11-24）

- 已补齐 `design/COLOR-TOKENS.md` 与 `design/TYPOGRAPHY.md`。
- 已将原 `screens/SCREEN-SPECS.md` 的 12 个工作区规格拆分为独立页面文件，保留总览文件作为索引。
- 已补齐 Visual、Functional、Accessibility、Performance 四份验收规范。
- 用户提供的 Dashboard 参考图作为本轮视觉方向的确认依据。Figma 像素级对照仍未完成，不得声称 Figma 画板已更新。
- M1 设计门禁：规格和隔离原型完成，状态为 **DESIGN FROZEN**。M2–M7 生产代码需要按这些规格逐阶段审计验收。
