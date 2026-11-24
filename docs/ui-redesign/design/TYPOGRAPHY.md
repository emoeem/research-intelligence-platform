# RIP Typography Specification

> M1 冻结规格。沿用生产 CSS 字体变量，不引入第二套字体加载机制。

## Font roles

- UI text: `--font-sans`，优先使用操作系统 UI 字体栈；中文按系统字体回退。
- Code/data: `--font-mono`，用于 HTTP、URL、Header、JSON、Trace ID、命令、日志和时间戳。
- Icons: Lucide React；图标不使用 Emoji 代替。

## Type scale

| Role | Size | Weight | Typical use |
|---|---:|---:|---|
| Page title | 20–24px | 600–700 | 页面主标题 |
| Section title | 14–16px | 600–700 | 面板/分区标题 |
| Body | 13–14px | 400–500 | 正文、表单说明 |
| Data table | 12–13px | 400–600 | 表格行和关键数据 |
| Metadata | 11–12px | 400–500 | 时间、来源、次要状态 |
| Editor | 12–13px | 400 | 等宽编辑内容 |

关键数据和可操作控件不得低于 12px。11px 元数据仅用于非关键辅助信息，并且必须满足对比度标准。

## Density and line-height

- 使用 4px 基础网格；字号和行高不应挤压点击目标。
- 表格密度可高于普通内容页，但必须保持行间辨识度、选中态和键盘焦点。
- 长 URL、ID、Header 和路径使用换行或横向滚动策略，不得撑破容器。
- 多语言文案可能比英文更长，按钮、导航和表头必须允许合理伸缩。

## Verification

在 Desktop XL、Desktop、Compact Desktop、Tablet、Mobile 五种视口检查标题截断、表格密度、按钮换行和多语言溢出。截图测试固定浏览器、字体与视口；系统字体不可用时不得假设所有机器像素完全一致。
