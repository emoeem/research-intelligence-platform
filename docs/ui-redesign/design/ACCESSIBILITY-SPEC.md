# RIP UI 可访问性验收规格

## 一、键盘

- 所有操作可由键盘完成。
- Focus ring 清晰且符合主题对比度。
- Dialog 打开时焦点进入对话框，关闭后恢复到调用控件。
- Tabs 支持键盘导航和 aria-selected。
- Splitter 支持键盘调整。
- 表格行选择/排序按钮可聚焦并有名称。

## 二、语义与读屏

- IconButton 具备 aria-label。
- 数据表格有 caption 或等价 accessible name。
- Loading 区域使用 aria-live 适当播报，不重复朗读整个列表。
- 错误和权限反馈使用明确文字，不只展示图标。
- 当前导航项使用 aria-current。
- Tooltip 不作为唯一信息来源。

## 三、视觉

- WCAG AA 文本对比度。
- 状态同时使用颜色和文本/图标。
- 不用 10px 以下文字呈现关键数据。
- Dark / Light 下焦点、Hover、Selected、Disabled 状态均需检查。
- 支持 prefers-reduced-motion。

## 四、验收工具

沿用 @axe-core/playwright 和现有视觉 Playwright。严重/critical 问题必须清零；页面关键交互另有键盘测试。截图差异不得通过一次性更新整个基线来规避。
