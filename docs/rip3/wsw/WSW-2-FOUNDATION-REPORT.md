# WSW-2 安全基础面批次报告
日期：2026-11-18；状态：本地实现与测试验收，非完整代理运行时。
## 本批交付
- 新增 crates/wsw-runtime：Loopback-only 配置验证、队列容量/头/body/TTL 参数边界、loopback peer 判定、Listener 状态机合法迁移、Intercept 队列终态迁移、队列容量预算、内存元数据队列（ID 校验、TTL 到期清除、终态解析、有限历史裁剪）。
- 不创建 socket、不转发网络请求、不进行 TLS 终结、不生成/安装/信任 CA、不修改系统代理设置。
- 暂未持久化，也未接入 UI/API、数据库或审计。
## 验收
9 项单元测试覆盖配置校验、生命周期迁移、loopback peer、容量控制、TTL 到期、队列终态与 ID 校验。后续批次仍需单独设计并评审持久化、网络层、授权 Scope、CA、性能、审计和集成；本报告不宣称 Proxy/MITM 完成。
