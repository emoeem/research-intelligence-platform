# RIP Color Tokens

> M1 冻结规格。生产唯一来源：`web/src/styles/token.css` 与 `web/src/theme.ts`。本文定义语义角色，不另建颜色体系。

## Semantic roles

| Token role | Existing source | Use |
|---|---|---|
| App background | `--color-base-200` | 页面底层与工作区背景 |
| Surface | `--color-base-100` | Sidebar、Topbar、面板 |
| Raised / inset surface | `--color-base-300`, `--surface-2` | 内嵌区域、悬浮、选中行 |
| Primary text | `--color-base-content` | 标题、正文、主要数据 |
| Muted text | `--muted` | 时间、辅助信息、说明 |
| Primary action | `--color-primary` | 主操作、当前导航、焦点 |
| Secondary accent | `--color-secondary` | 次级强调 |
| Success | `--color-success` | 成功、健康、完成 |
| Warning | `--color-warning` | 等待、风险提示 |
| Danger | `--color-error` | 错误、拒绝、破坏性操作 |
| Info | `--color-info` | 中性提示 |
| Border | `--edge`, `--edge-soft` | 面板、表格、控件分隔 |
| Sidebar | `--sidebar` | 全局导航底色 |

## Rules

- Dark 与 Light 必须映射相同语义角色，不允许按页面单独换色。
- 页面和组件不得硬编码 HEX/OKLCH 颜色；透明叠色仅限组件 Token 中已有的状态表达。
- 状态必须同时具备文字、图标或 accessible name，不可只靠颜色。
- 正文和小字号辅助信息按 WCAG AA 检查；正常文本对比度至少 4.5:1，大文本至少 3:1。
- Focus ring 必须在两个主题下可见，不能仅使用浏览器默认且低对比的描边。
- 图表颜色从主题语义色派生，图例、轴标签、Tooltip 同步主题。
- 第三方编辑器、Popover、Dialog、Tooltip 和菜单必须继承主题 Token。

## Change control

变更基础 Token 必须先更新本文件与 ADR，再运行全页面双主题截图、axe、图表和编辑器主题测试。禁止为单页视觉修复创建新的颜色变量。
