# 10. MIGRATION_STRATEGY

日期：2026-11-05　原则：双轨并行、按数据风险分级迁移、每步可回滚、现网 ID 全保留。

## 10.1 迁移资产分级

| 级 | 资产 | 策略 |
|---|---|---|
| 红线级（逐条保真） | 28 条分层知识 + 17 条反知识 + 633 审计事件 + 81 条资源 + 全部 review 决策 + engagement 授权记录 | ETL 逐行校验（源/目标计数+字段哈希比对），迁移前后双读跑金测 |
| 结构级（随引擎走） | workflows.yaml / signal-catalog / tools.json / skills front matter | 格式转换脚本 + 前后行为对比（同输入同输出） |
| 可重建级 | 14,111 nuclei 模板索引、FTS 索引、缓存 | 不迁移，新索引层重灌（来源：nuclei-templates 仓库 + 现库导出兜底） |
| 冷存级 | 28 个 assets.db | 原地不动，Evidence 全局 ID 回填索引表；永不合并物理库 |

## 10.2 阶段迁移计划（对应 ROADMAP 里程碑）

- **M1（Domain Model 落表）**：新库 platform.db 建表（15 实体 + alias 列 + schema_version 表 + 迁移日志表）。只读映射脚本（ETL dry-run），产出差异报告，不切流。
- **M2（Rust Core 验证）**：resource-engine 同步器 crate，数据仍走现 catalog.db（Rust 读写 SQLite，schema 不变）。验证点：与 eri.py 并行跑 sync 幂等一致（同 commit 产生逐字节相同结果）。失败即触发 DEC-001 回退。
- **M3（Python AI Runtime）**：knowledge-engine + agent-engine Python 化收编 rt.py/rtis.py 逻辑；k_event 双写（旧表 + 新 AUD 事件），读旧写新。
- **M4（检索与治理）**：search-engine 重灌（FTS5 2-gram 先行）；quality-engine 收编 review 队列；四审核队列合并为 Review 实体单入口。
- **M5（RTIS 集成）**：gate 的 ERI subprocess 钩子替换为进程内 Resolve()；task JSON 状态机迁 platform.db（旧 JSON 只读保留一个过渡期）；rtis_mcp.mjs 工具面指向新接口（宿主无感）。
- **M6（Tool/Workflow 统一）**：tools.json → Tool/ToolVersion 表（补 schema/权限级/执行日志）；workflows.yaml → Workflow 表（YAML 保留为导入格式）。
- **M7（UI）**：只读仪表盘先行，写操作全部经 Review 队列。

## 10.3 回滚方案（每阶段一个）

1. 每阶段开工前 `backup()` 全部涉库（现有 sqlite backup API，实测 30.5ms@100k）。
2. 双轨期写操作：新库成功 → 旧库镜像写；新库失败 → 旧库照旧，新库标记 diverged 待人工。
3. 切换开关集中一处（配置文件 `runtime.mode=legacy|dual|platform`），回滚 = 改开关 + restart，不做数据反向迁移（旧轨从未停写）。
4. 红线级数据迁移附带金测集：P0 8 组 + P1 57 断言 + P2 场景 A-F 改写为对新接口的断言，任何回滚后必须复跑。

## 10.4 明确禁止

- 不做"停机大爆炸"迁移（现网是活跃红队工作区）。
- 不在迁移分支上顺手改行为：先等价迁移，后独立优化。
- 不动 125 个 skill 文件正文（结构化抽取是读取，不是改写）。

## 10.5 数据校验脚本约定（M1 交付）

`migrations/verify.py`：对每个红线表输出 `{源行数, 目标行数, 抽样字段哈希一致率, 差异清单}`；一致率 <100% 即失败退出。此脚本是每阶段验收的第一闸。
