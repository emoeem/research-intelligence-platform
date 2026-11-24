# Runtime 页面设计规格

> M1 设计冻结文档。生产实现必须遵循统一 Design System、AppShell、Navigation 与 Responsive 规范。
> 业务数据必须来自现有 API；本规格不授权新增 API 或使用 Mock 生产数据。

## 10. Runtime

展示 Runner/节点健康、心跳、能力、版本、执行槽位和错误。未知状态必须明确显示 unknown/stale，不渲染假绿灯。节点详情用 Inspector 展示配置和关联任务。
