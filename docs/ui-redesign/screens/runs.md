# Runs 页面设计规格

> M1 设计冻结文档。生产实现必须遵循统一 Design System、AppShell、Navigation 与 Responsive 规范。
> 业务数据必须来自现有 API；本规格不授权新增 API 或使用 Mock 生产数据。

## 9. Runs

默认列表：状态、工具/Agent/Workflow 类型、开始时间、耗时、Caller、Trace；状态过滤和分页。详情分为 Summary、Timeline/Steps、Logs、Artifacts、Inputs/Outputs、Trace links。敏感输出在详情中遵循权限和脱敏规则。
