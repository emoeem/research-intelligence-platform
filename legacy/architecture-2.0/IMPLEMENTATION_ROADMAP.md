# 12. IMPLEMENTATION_ROADMAP（M0-M7 重排后）

日期：2026-11-05　调整依据：M0 审计证据（依赖方向、网络现实、现网活性）。原列 M2 Rust 提前验证风险，故 M2 范围收窄为"单 crate 验证"而非全面移植；M4 检索与治理合并（共享 review/索引基础设施）。

| 阶段 | 名称 | 交付物 | 测试标准 | 数据迁移 | 回滚 |
|---|---|---|---|---|---|
| **M0（本次）** | 系统审计+架构设计 | 13 份报告 | 结论全部有实查证据 | 无 | 无（只读） |
| **M1** | 统一 Domain Model | platform.db schema（15 实体+alias+schema_version）；映射 ETL（dry-run）；`migrations/verify.py`；域类型定义 | verify 一致率 100%；ETL 差异报告零红线差异 | 不切流，仅生成 | 删 platform.db 即回滚 |
| **M2** | Rust Core 验证 | resource-engine 同步器 crate（vendored 构建）；Core↔Worker JSON-RPC 协议 v0 | 与 eri.py 双轨 sync 结果逐字节一致；离线可重复构建 | 双写 catalog.db | 停用 crate，eri.py 独走（DEC-001 回退条款） |
| **M3** | Python AI Runtime | knowledge-engine + agent-engine（收编 rt.py 写路径与 handoff）；Token 账本接入 | 金测：28 条知识+17 反知识字段哈希一致；gate 金集 Recall 不降 | k_event 双写 | runtime.mode=legacy |
| **M4** | 统一检索与数据治理 | search-engine（FTS5+2-gram 重灌）；四审核队列并入 Review 实体；质量引擎收编 | P1 57 断言等价改写全过；检索 gold set（20 条）Recall ≥ 现状 | FTS 重灌（可重建级） | 索引重建即回滚 |
| **M5** | RTIS/Agent 集成 | gate→进程内 Resolve()；task 状态机迁 DB；MCP 工具面切新接口 | P2 场景 A-F 全过 + rtis.py 既有自测过；宿主工具形状不变 | task JSON 只读保留，新写 DB | 钩子切回 subprocess 版（保留备份） |
| **M6** | Tool/Workflow 统一 | Tool/ToolVersion 表（schema/权限级/执行日志）；Workflow 表（YAML 导入器） | registry 20 工具逐项迁移校验；路由同输入同输出 | tools.json/workflows.yaml 转换 | YAML/JSON 仍为事实源可切回 |
| **M7** | UI 工作台 | 本地 Web UI：实体视图/关系链/审核队列/任务时间线/成本看板 | 只读先验收；写操作全部经 Review | 无新迁移 | 不部署即无影响 |

## 依赖图

```
M1 ──→ M2 ──→ M4 ──→ M5 ──→ M7
 │            ↑
 ├──→ M3 ─────┘
M6 ←─ M1（可与 M2-M5 并行，不依赖 Rust）
```

关键路径：M1 → M3 → M4 → M5。M2 是验证性支线（失败不阻塞主路径，触发 C 路线回退）。M6 独立可并行。

## 每阶段通用验收门

1. `migrations/verify.py` 红线一致率 100%。
2. 三套既有测试等价改写全绿。
3. `runtime.mode` 切回 legacy 后现网行为零变化（演练一次）。
4. 新增 Token 消耗如实入账本并出具成本小节。

## 明确不做（贯穿）

不重建知识库内容、不批量改 skill 正文、不自动装依赖、未授权不触碰外部目标、不 push。
