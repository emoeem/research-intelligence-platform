# ADR-004：HTTP Message Viewer 与 Editor

状态：M1 设计裁决候选。
日期：2026-11-24

## 决策

保留 MessageViews 作为消息预览的业务基础，逐步形成共享 MessageViewer。需要 Raw 请求编辑、语法高亮、行号、搜索和大文本处理时使用 CodeMirror 6。CodeMirror 按需加载，默认只读视图不必初始化完整编辑能力。

## 安全与保真要求

- 原始捕获内容、脱敏展示投影、编辑草稿和序列化发送内容必须区分。
- 不把脱敏后的展示内容称为原始 Wire bytes。
- 不在 DOM、日志或截图中暴露 Authorization/Cookie 等凭据。
- Body 继续按需读取并保留现有单响应 8MiB 上限和后端其他限制。
- 二进制内容走原字节/Hex，不用 UTF-8 替换结果覆盖原始内容。
- 语言包按页面和内容类型懒加载。
