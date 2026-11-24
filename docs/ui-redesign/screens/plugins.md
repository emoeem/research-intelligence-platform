# Plugins 页面设计规格

> M1 设计冻结文档。生产实现必须遵循统一 Design System、AppShell、Navigation 与 Responsive 规范。
> 业务数据必须来自现有 API；本规格不授权新增 API 或使用 Mock 生产数据。

## 6. Plugins

插件目录展示类别、来源、版本、信任级别、生命周期和治理状态；详情展示 Manifest、能力、权限、依赖、审计 trail。安装、升级、禁用、卸载必须沿用现有 TrustConfirmDialog 和服务端策略，不能因 UI 重构简化确认等级。
