# Settings 页面设计规格

> M1 设计冻结文档。生产实现必须遵循统一 Design System、AppShell、Navigation 与 Responsive 规范。
> 业务数据必须来自现有 API；本规格不授权新增 API 或使用 Mock 生产数据。

## 11. Settings

设置侧栏分类：General、Appearance、Workspace、Security、API/Connection、Runtime、Storage、Advanced。按表单分组；每组有保存状态、验证错误和重置入口。安全设置需要确认，不将系统当前状态伪装成可编辑配置。
