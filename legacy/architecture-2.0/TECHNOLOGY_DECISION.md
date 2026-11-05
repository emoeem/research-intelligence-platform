# 9. TECHNOLOGY_DECISION

日期：2026-11-05　方法：真实代码规模 + 本机网络实测 + 维护者现实（单人 + AI 协作）三点定案。**本报告是建议，最终架构批准权在 emoeem。**

## 9.1 语言路线对比

| 维度 | A. Rust Core + Python AI Workers（倾向案） | B. 全 Rust | C. 继续 Python |
|---|---|---|---|
| 现有资产复用 | ~6,300 行 Python 引擎逻辑需移植核心部分；AI 面（MCP/提示词）天然 Python | 同左 + AI 面也要 Rust 生态拼（pyo3 之外选择少） | 100% 复用 |
| 性能需求 | 检索/解析/同步是 CPU 密集，Rust 收益真实（100k LIKE 全扫 455ms 这类点可提一个量级） | 同 A | 已够用（100k 基本查询达标） |
| 本机网络现实 | cargo 索引（git/sparse）实测间歇 TLS EOF，static.crates.io 稳定；需 vendor/镜像策略才能可持续构建 | 同 A，且全 Rust 依赖面更大、风险更高 | 零新依赖（stdlib 已证明可走很远） |
| 单人维护成本 | 双语言 + FFI 边界 | 单语言但学习/迭代慢 | 最低 |
| AI Workers 隔离 | Python Worker 崩溃不伤 Core；内存安全的热路径用 Rust | 理想但过度 | 无隔离 |

**建议：A 路线，但分阶段验证（M2 先做一个 crate：resource-engine 的同步器，跑通 vendored 构建 + 与 Python 侧协议，再决定是否扩大 Rust 面）。若 vendored 构建在本机不可持续，回退 C（继续 Python + 进程隔离），这不是失败而是按证据决策。**

明确反对 B（全 Rust）：AI/提示词/rapid 迭代层在 Rust 里成本畸高，且本机网络对全量 crates 拉取不友好。

## 9.2 数据库

| 维度 | SQLite | PostgreSQL |
|---|---|---|
| 现状 | 全系统在用（WAL、单写者纪律成熟、100k 实测达标） | 未引入 |
| 并发 | 单写多读（现状纪律已适配） | 真并发写（当前没有这个需求：全部写路径都是单 Agent 批处理） |
| 运维 | 零（文件 + backup API 30.5ms@100k） | 需要服务进程、备份策略 |
| 28 库孤岛问题 | 不因换库解决（靠全局 ID/审计流解决） | 同左 |

**建议：继续 SQLite（每域一库或单库多 schema），PG 触发条件写死为可证指标：并发写冲突率 >1%、单库 >5GB、或跨靶标联邦查询成为高频需求。与平台计划"不提前引入"一致。**

## 9.3 搜索

| 选项 | 判断 |
|---|---|
| FTS5 | 已在用（knowledge.db poc_fts）；中文靠 2-gram 预处理（bridge 已实现）够用到百万级模板索引 |
| Tantivy | Rust 侧自然选择，性能强；但引入即要求 M2 先行 + vendor；**M4 再评估** |
| 混合检索 | BM25 + 向量 rerank 是终态；向量层当前零基础，先补 embedding 管线再谈 |
| 中文分词 | 2-gram 方案已被 P1 实测（Recall 6/8→对照 legacy 提升），保留；jieba 级分词为增强项非前提 |
| 向量 | sqlite-vec / LanceDB（轻）优于独立向量库（违背"不提前引入"）；增量 embedding 按 content_hash 触发（ERI summary_cache 同策略） |

**建议：近中期 FTS5 + 2-gram + 结构化过滤（已达标）；向量层作为 M4 的独立里程碑（先 5k 条自有知识 + A 级资源试产，以检索命中率评估）。Tantivy 只在 M2 落地且 FTS5 出现实测瓶颈后考虑。**

## 9.4 通信

| 选项 | 判断 |
|---|---|
| JSON-RPC (stdio) | rtis_mcp.mjs 现状，零改造可用；进程间（Core↔Workers）首选，天然隔离 |
| MCP | 对 AI 宿主的既有接口，保留为**投影层**（工具面不变，宿主无感迁移） |
| HTTP API | UI/远程场景需要；server 模块（M7 前）提供，内部仍走同一 domain 接口 |

**建议：内部 JSON-RPC（Core↔Worker），对外 MCP（AI 宿主）+ HTTP（UI）双投影，一套 domain 接口三个传输。**

## 9.5 UI

| 选项 | 判断 |
|---|---|
| GPUI | Rust 原生但生态年轻、Windows/Linux 支持不均衡、单人学习成本高；否 |
| Tauri + React | 桌面壳 + Web 前端，Rust 后端可复用 Core；打包/更新走网络（本机网络对 Tauri 更新源也是风险） |
| Web UI（本地 server + 浏览器） | 最薄：server 模块 + 静态前端，零新桌面依赖；审计需求（实体/关系/队列/时间线）全是表单+表格+图，Web 形态天然 |

**建议：本地 Web UI（server 渲染 + 轻前端），M7 交付；Tauri 壳留作 M7 之后的可选项。**

## 9.6 决策记录格式（后续沿用）

每项决策：`DEC-<n>: 结论 | 备选 | 关键证据 | 回退条件`。本报告即 DEC-001..005 的载体；M2 开工前若 vendored 构建验证失败，触发 DEC-001 回退条款（C 路线），其余决策不受影响。
