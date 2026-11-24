# Workflow Studio 页面设计规格

> M1 设计冻结文档。生产实现必须遵循统一 Design System、AppShell、Navigation 与 Responsive 规范。
> 业务数据必须来自现有 API；本规格不授权新增 API 或使用 Mock 生产数据。

## 8. Workflow Studio

保留 @xyflow/react 现有画布。页面由 Node Palette、Canvas、Node Inspector、Run Panel、Version List 组成。画布最大化；Inspector 和 Run Panel 可折叠调整。运行、暂停、恢复、重试、取消必须显示服务端真实状态及 checkpoint。移动端提供只读/简化导航。
