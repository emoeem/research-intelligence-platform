# 13. FINAL_ARCHITECTURE（Architecture 2.0 / M0 终稿）

日期：2026-11-05　状态：**设计定稿待批准**（emoeem 审阅本目录 13 份报告后放行 M1）。技术方向（DEC-001..005）均为建议态，批准前不写一行实现代码。

## 13.1 一句话架构

**以 15 实体统一域模型为逻辑核心、SQLite 为事实源、FTS5+2-gram 为检索层、Python AI Workers 承担理解与执行、可选 Rust 同步器加速热路径、MCP/HTTP 双投影对外、本地 Web UI 收口**的研究情报平台；ERI 与 RTIS 降位为两个域模块，现有数据以"红线保真 + 可重建重灌"两种方式迁入。

## 13.2 架构总图

```
┌─ 对外投影 ──────────────────────────────┐
│  MCP（AI 宿主，现状工具面）  HTTP（UI/远程）│
└──────────────┬──────────────────────────┘
        ┌───────┴────────┐
        │  domain（15 实体 + 状态机 + 权限级）   │ ← 零依赖类型层
        └───────┬────────┘
   ┌────────┬───┴────────┬───────────┐
   │ resource-engine │ knowledge-engine │ tool-engine │ workflow-engine │ agent-engine │ quality-engine
   └────────┴────────────┴───────────┘
        │            │              │
   database(迁移/备份)  search-engine(可重建)  audit-engine(旁路事件流)
        │
   SQLite（事实源）+ 28 engagement 证据冷存分区（不合并）
```

## 13.3 核心决策索引

| DEC | 决策 | 回退条件 |
|---|---|---|
| 001 | Rust Core 渐进（M2 单 crate 验证起步），失败回退纯 Python | vendored 构建在本机不可持续 |
| 002 | 继续 SQLite，PG 触发条件成文（并发冲突>1% / >5GB / 联邦高频） | 触发条件实际发生 |
| 003 | FTS5+2-gram 近中期；向量 M4 试产（5k 条）；Tantivy 仅实测瓶颈后 | 向量试产命中率无提升 |
| 004 | 内部 JSON-RPC，对外 MCP+HTTP 双投影 | 无（结构兼容） |
| 005 | 本地 Web UI 先行，Tauri 留后 | 无 |

## 13.4 从现状到目标的对应（简）

- knowledge.db → knowledge-engine（28 知识+17 反知识红线迁移；14,111 模板索引归 Resource 重建）
- catalog.db → resource-engine + quality-engine（结构直迁，ID 不变）
- rtis.py/rt.py/rtis_mcp.mjs → workflow/agent/knowledge-engine + mcp 投影（接口 JSON 形状兼容）
- 28×assets.db → Evidence 冷存分区 + 全局 ID 桥（不合并）
- 125 skills → Skill 实体（Markdown 降级为渲染产物；正文零改动）

## 13.5 风险登记（审计实证）

| 风险 | 证据 | 缓解 |
|---|---|---|
| 本机网络限制拖垮 Rust 构建 | crates.io 索引超时、rustup TLS EOF（实测） | DEC-001 vendor 策略 + M2 只验证不铺开 + C 路线回退 |
| 单人+AI 维护双语言成本 | 全系统现 6,300 行 stdlib Python | M2 验证门槛从严；收益不成正比即回退 |
| 迁移期破坏活跃红队工作区 | engagement 每日在用 | 双轨 + runtime.mode 开关 + 每阶段演练回滚 |
| 域模型过度设计 | 15 实体 vs 3 人月现实 | M1 先落 8 个有现网数据的实体（Resource/Knowledge/Evidence/Tool/Skill/Agent/Workflow/Review），其余随阶段补 |
| AI 成本失控 | 现状零 LLM 基线 | Token 账本 + 预算闸（超限拒绝） |

## 13.6 下一步（等待授权）

M1：统一 Domain Model 落表 + ETL dry-run + verify.py。不切流、不改现网、不装依赖。授权后开工。
