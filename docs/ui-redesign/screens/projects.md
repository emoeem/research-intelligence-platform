# Projects 页面设计规格

> M1 设计冻结文档。生产实现必须遵循统一 Design System、AppShell、Navigation 与 Responsive 规范。
> 业务数据必须来自现有 API；本规格不授权新增 API 或使用 Mock 生产数据。

## 2. Projects

目标：管理研究项目和项目任务树。页面头部包含项目搜索、筛选、创建入口（仅 API 已支持才出现）；主区为表格/卡片切换；行显示名称、状态、目标、任务计数、最近更新时间；详情采用 Inspector 或独立详情页。不能把 Project 与 Workspace 混为一谈。
