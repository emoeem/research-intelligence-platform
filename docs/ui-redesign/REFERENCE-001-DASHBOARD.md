# Dashboard visual reference 001

> 状态：用户确认的 Dashboard 视觉方向，作为 M2-M7 的视觉验收基准之一。
> 日期：2026-11-24
> 来源：用户在 2026-11-24 会话上传的 Dashboard 截图，画面尺寸 1768 x 865。
> 适用：Dashboard、统一 AppShell、全站信息密度与视觉一致性。

## 一、参考图的结构与视觉要求

参考图表达的是一个全宽桌面研究情报工作台，不是普通营销式 Dashboard。它的视觉语言由以下结构组成：

| 区域 | 参考图要求 | 当前实现对齐点 |
| --- | --- | --- |
| Sidebar | 约 212px，品牌区、分组标题、图标与文字、固定底部个人区 | 当前桌面默认 212px；窄桌面按宽度折叠，允许用户手动展开 |
| Topbar | 约 56px，当前工作区面包屑、搜索和全局快捷操作 | 当前保留搜索、API/Runtime 状态、语言、主题与 Inspector；Runs 和 Runtime 从侧栏进入 |
| WorkspaceNav | 约 40px，只显示当前工作区页签，活动项使用低饱和强调底色 | 当前所有工作区均显示当前工作区页签；单页签隐藏重复的上下文说明 |
| Main | 全宽内容，标题和主要操作同一行，四列 KPI | Dashboard 采用全宽布局，四列 KPI 在宽屏显示 |
| Attention / Runtime | 两栏布局，左侧待处理队列更宽，右侧运行环境更窄 | 使用真实 reviews 与 runtime API |
| Recent activity | 跨全宽时间线，按时间显示真实运行事件 | 使用只读 runs API，链接到真实 Run 详情 |
| Activity Dock | 底部窄栏，默认收起，需要时展开后台任务和日志 | 当前底部 Activity Dock 默认收起 |

颜色和组件遵循 RIP Dark/Light Tokens。主要视觉特征是深色蓝黑背景、略亮一级的面板、细边框、低饱和青色主色、紧凑文字层级和清晰的状态色。不要加入纯黑底、霓虹渐变、重阴影或装饰性大图表。

## 二、真实数据映射

截图里的数字只作为视觉比例参考，不是生产数据。生产 Dashboard 继续使用现有只读数据接口：

| 区域 | 数据接口 | 字段或用途 |
| --- | --- | --- |
| Knowledge items | /api/overview | knowledge.total |
| Research tasks | /api/overview | tasks.total 与 tasks.by_state |
| Tool runs | /api/overview | runs.total 与 runs.by_status |
| Evidence records | /api/overview | evidence.total |
| Needs attention | /api/stats | reviews.by_queue.*.by_decision.PENDING |
| Runtime health | /api/runtime?limit=5 | 节点健康、Node ID 和 Heartbeat |
| Recent activity | /api/runs?limit=5 | Run 状态、创建时间、耗时与详情链接 |

不得将视觉参考中的 1,284、42、3,691、8,420 等数字写入生产页面或作为空态回退。

## 三、明确不照搬的内容

1. Last 7 days：现有 /api/overview 只提供累计只读计数，不接受时间窗口参数。没有后端支持前不显示一个看似有效但不筛数据的日期筛选器。
2. DESIGN PREVIEW / DEMO DATA：这属于参考图的演示环境标记。生产页面消费真实 API，不显示 Demo 标签；浏览器测试夹具必须与生产数据明确隔离。
3. 服务状态指示器：当前状态由 `/api/overview` 与 `/api/runtime?limit=5` 的真实响应组合推导，文案明确为「API 与 Runtime 可用」，不声称检查了所有本机进程。401 显示需要登录，API 错误显示不可用，Runtime 数据面不可用时显示降级。
4. 用户身份和管理员头像：继续使用应用已有的本地工作区信息，不硬编码截图中的人员与角色。

Topbar 保留搜索、服务状态、语言切换、主题和 Inspector。Runs 与 Runtime 的正式入口位于 Operations 侧栏；旧通知按钮没有真实通知数据源，因此移除空占位。Activity Dock 继续提供后台队列、运行记录、日志和输出。

## 四、浏览器验收尺寸

固定参考视口为 1768 x 865，另需覆盖 1440 x 900、1280 x 800、390 x 844。验收关注侧栏宽度、Topbar 与 WorkspaceNav 高度、KPI 列数、两栏比例、底部 Activity Dock、无横向溢出、Dark/Light 和三语切换。

实现截图存放于 screenshots/reference-dashboard-implemented-dark-1768x865.png。该截图由 Playwright 的固定 API fixtures 渲染，只用于布局对照，不代表生产数据快照。

## 五、剩余差异

- 服务状态只代表 Overview API 与 Runtime 数据面的可用性，不应解释为整台主机全部服务健康。
- 日期范围筛选依赖后端为 overview 计数定义时间字段和窗口语义，暂不实现。
- Activity Dock 的折叠状态与参考图一致，但摘要内容仍需基于真实队列、Runs 和 Runner 状态继续丰富。
- 当前 Figma 文件未完成像素级截图核对；本参考图是用户明确提供的实现方向，不代表已修改 Figma。
