# Phase 0 交付物 1：RIP 当前架构盘点（CURRENT-ARCHITECTURE-INVENTORY）

日期：2026-11-09
性质：只读盘点，本次未修改任何源文件、未向任何数据库写入。
对应计划条目：`docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md` §16 第 1 条（模块、API、数据库、Workers、RTIS、Web 和 MCP 清单）。
处置基准：同计划 §2.1「RIP 当前状态」表格，本文逐行对齐并补充审计细节。

证据口径：以下所有结论均来自本会话实际读取的文件（文中给出 `路径:行号`）或实际执行的命令（文中给出命令原文与输出）。未能亲自验证的项明确标注「待验证」。

---

## 1. 仓库总览

独立 git 仓库：`research-platform/`（分支 main，remote origin = github.com/emoeem/research-intelligence-platform，`git remote -v` 实测确认）。HEAD = `022a717`（rtis kit 迁入提交）。工作区当前唯一未跟踪文件是本计划的来源文档 `docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md`（`git status --short` 实测输出 `?? docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md`）。

顶层结构（`find -maxdepth 2` 实测）：

| 路径 | 内容 |
|---|---|
| `crates/` | 6 个 Rust crate + workspace 根 `Cargo.toml`（Cargo.toml:3-11 实测成员列表，另含 `tests` 载具 crate） |
| `workers/python/` | 3 个 Python worker + 共享 runtime + 契约测试 |
| `apps/server/` | M7 只读 Web 工作台（stdlib http.server + Token 鉴权） |
| `integrations/rtis/` | RTIS 红队知识栈 kit + ERI adapter（生产 Resolve 接入层） |
| `migrations/` | Schema V1-V4 SQL + 5 个 ETL/对账脚本 + 隔离清单 + 快照 |
| `scripts/` | 3 个只读维护脚本 |
| `models/` | bge-small-zh-v1.5 ONNX 模型（gitignore，实测目录含 config.json / model.onnx / model.onnx_data / tokenizer.json 四文件） |
| `schemas/worker-protocol/` | Worker Protocol v1 契约（README.md + v1.json） |
| `.github/workflows/ci.yml` | CI（Rust 测试 + clippy + worker 测试） |
| `Containerfile`（根） | Rust Core 多阶段容器构建；另有 `apps/server/Containerfile` |
| `tests/` | `rp-eri-parity` 对照测试载具 crate（workspace 第 7 成员） |
| `vendor/` + `.cargo/config.toml` | 离线 vendored 依赖（43MB 级，rsproxy 拉取后 vendor，config.toml:1-4 指定 vendored-sources） |
| `probe-cargo/` | DEC-001 构建准入验证工件（PROBE-RESULT.md 留结论） |
| `legacy/` | ERI P0-P2 与 architecture-2.0 报告的 rsync 归档（README.md:13） |

---

## 2. Rust crates（6 个，逐一盘点）

workspace 根 `Cargo.toml:1-36`：resolver 2，workspace 版本 0.2.0，edition 2021，rust-version 1.99。依赖锁定最少集：rusqlite 0.32（bundled，免系统 sqlite）+ serde + serde_json（Cargo.toml:19-23，注释注明 DEC-001 / PROBE-RESULT.md 约定）。release profile 显式 `strip = "debuginfo"`（Cargo.toml:33-35）。

依赖方向实测（各 crate Cargo.toml 的 `[dependencies]`）：domain 零依赖；database 仅依赖 rusqlite；audit-engine 依赖 database + domain；resource-engine 依赖 database + domain；search-engine 依赖 database + domain + resource-engine（仅复用 tokenizer 数据表，见 search-engine/src/lib.rs:19-21 依赖声明）；quality-engine 依赖 database + domain + serde + serde_json。全部 crate 头部带 `#![forbid(unsafe_code)]` 与 `#![deny(missing_docs)]`。

### 2.1 crates/domain

- 证据：`crates/domain/src/lib.rs:1-25`（crate 头注与 re-export）、`crates/domain/src/resource.rs:177-238`、`crates/domain/src/ids.rs:10-42`、`crates/domain/src/audit.rs:7-29`。规模 662 行（cat src/*.rs | wc -l 实测）。
- 职责：统一域模型纯类型库。Resource 实体（resource.rs:184-231，24 字段，注释说明 platform 库 resource 表 40+ 列，本批只收编 resolve 与实体身份所需字段）+ 双轴状态枚举 LifecycleStatus 七态 / GovernanceStatus 四态（resource.rs:12-119，K-08 双轴正交口径）+ QualityGrade A/B/C/D（resource.rs:136-175）+ ResourceId 新类型（ids.rs:15-31，`gh:owner/repo` / `url:hash16` 前缀校验）+ AuditEvent 纯数据类型（audit.rs:7-29，builder 风格 + validate）+ DomainError + UTC ISO8601 时间工具（lib.rs:21 注释）。
- 公开接口（lib.rs:23-25 re-export 实测）：`DomainError`、`ResourceId`（try_new / as_str / into_inner，TryFrom String 与 &str）、`Resource`、`LifecycleStatus`、`GovernanceStatus`、`QualityGrade`；子模块 `audit / error / ids / resource / time`。
- 处置结论：保留（§2.1 原文「保留，作为统一领域模型基础」）。审计补充：domain 当前只收编了 15 实体中的 Resource 与 AuditEvent 两个（domain/src/lib.rs:6-7 自述「本批收编」），Knowledge / ResearchTask / Evidence / Tool / Skill / Workflow 等 2.0 需要的实体尚未进入 Rust 域模型，仍只存在于 migrations schema 与 platform.db 数据面。2.0 扩展插件（Plugin）、工具运行（ToolRun）、Agent 等实体时，domain 是唯一合法落点（MODULE_BOUNDARIES §8.1 domain 行「自身零依赖」约束不变）。

### 2.2 crates/database

- 证据：`crates/database/src/lib.rs:1-19`、`crates/database/src/pool.rs:13-53`（DbPath / PoolConfig）、`pool.rs:163-191`（with_conn / with_tx）、`crates/database/src/migrate.rs:32-80`（discover_migrations / migrate）。
- 职责：SQLite 基础设施。同步连接池（Mutex + Condvar，签出 PooledConn，Drop 归还，pool.rs:219-229；内存库强制单连接 pool.rs:89-91）；只读 / 可写打开（pool.rs:231-249，统一 `PRAGMA foreign_keys=ON`，pool.rs:247）；迁移执行器：扫描 `NNN_name.sql`，按 `PRAGMA user_version` 幂等递增，重复版本号报错（migrate.rs:32-64）。
- 公开接口（lib.rs:14-19）：`DbError / DbResult`、`discover_migrations / migrate / AppliedMigration / Migration`、`DbPath / Pool / PoolConfig / PooledConn`、`with_conn / with_tx`；并重导出 rusqlite 使下游 crate 不直接声明 rusqlite（lib.rs:18-19）。
- 处置结论：保留并演进（§2.1 原文「保留并演进」）。审计补充：MODULE_BOUNDARIES §8.1 database 行声明的 `backup()/restore()` 尚未实现（lib.rs 公开接口里没有，实测 grep 确认），2.0 的备份恢复能力（§17 验收标准第 14 条）应落在本 crate。迁移执行器与 migrations/ 目录的 `schema_version` 表登记机制并存（002_tasks.sql:13-16 明确 user_version 与 schema_version 表双轨是 dry-run 期口径），2.0 演进时要收敛为单源真相。

### 2.3 crates/audit-engine

- 证据：`crates/audit-engine/src/lib.rs:1-33`（职责与旁路纪律声明、BUFFER_CAP=1024）、`lib.rs:45-100`（emit / emit_best_effort / flush / buffered）、`lib.rs:102-116`（trace）。
- 职责：统一审计事件流，写 `audit_event` 表。`Emit(AuditEvent)` 为各模块唯一审计出口；`Trace(correlation_id)` 按 event_id 升序回放「任务-查询-工具-证据」链（lib.rs:102-109）。旁路纪律（MODULE_BOUNDARIES §8.2.4）：写失败不反压业务，进程内有界缓冲（1024 条，丢最旧），下次调用自动重试（lib.rs:52-95）；非法事件直接丢弃不缓冲（lib.rs:56-60）。跨进程持久队列为后续里程碑（lib.rs:8-9 自述）。
- 公开接口：`AuditEngine`（new / emit / emit_best_effort / flush / buffered / trace）、`StoredAuditEvent`、`BUFFER_CAP`。
- 处置结论：保留并扩展运行记录/证据事件（§2.1 原文）。审计补充：2.0 计划「所有工具执行产生标准化 Run、Artifact、Evidence 和日志」（计划 §3 架构原则）要求 audit_event 成为 Tool Run 链路的关联骨干，当前 correlation_id 已具备该能力但 emit 只覆盖 Rust 侧调用方；Python worker 与 rtis 编排层目前各自写 JSONL 审计（integrations/rtis/rtis.py:167-168 分工注释、eri_adapter.py:203-211 的 adapter_audit.jsonl），未接 audit-engine。这是 2.0 需要收敛的旁路分叉。

### 2.4 crates/resource-engine

- 证据：`crates/resource-engine/src/lib.rs:1-30`（职责与 re-export）、`src/repository.rs:11`（trait）、`src/sqlite.rs:22-28`（SqliteResourceRepository）、`src/model.rs:11-138`（ResolveQuery / ResolveFilters / ResourceHit / RetrievalStats / CoveredRef / CapabilityGap / ResolveResult / WeightsConfig）、`src/bin/rp-resolve.rs:1-36`（CLI）。
- 职责：Resource Repository + Resolve 查询垂直切片。只读 `resource` 表，治理轴 QUARANTINED / REJECTED / ARCHIVED 对 resolve 不可见（lib.rs:8-9 数据边界）。Resolve 输出 JSON 与 eri_p2.resolve 字段级兼容（lib.rs:5-6，兼容承诺 MODULE_BOUNDARIES §8.4）。相关性权重裁定为 keyword 60% + quality 40%（lib.rs:11-12，P2 六轴权重待 quality / search 引擎落地后迁移）。
- 公开接口（lib.rs:24-30）：`ResourceRepository` trait（get / count / resolve）、`SqliteResourceRepository::new(pool)`、`ResolveQuery / ResolveFilters / ResolveResult / ResourceHit / RetrievalStats / CoveredRef / CapabilityGap / EngineError`、tokenizer 公开函数（tokenizer.rs:25/43/68，expand_keywords / tokenize / keyword_hits，ALIASES 表被 search-engine 复用）。CLI 二进制 `rp-resolve`（rp-resolve.rs:4-6 用法：--db、--migrate-dir、--keywords、--limit、--task-id、--writable，退出码 0/2）。
- 处置结论：保留（§2.1 原文「Resource / Resolver 保留」）。审计补充：本 crate 是 M8 切流的生产路径（eri_adapter.py:136-150 经 rp-resolve 二进制调用，RTIS_RP_DB 默认 migrations/platform.db，eri_adapter.py:35-36）。§8.4 声明的 `Enrich(job)` 与 `resource.changed` 事件未实现（公开接口实测无），2.0 的资源同步 / 元数据富化要在此扩展。权重两轴 0.6+0.4 是已记录的平台简化（shadow_compare.py:11-13 差异归因清单第一项）。

### 2.5 crates/search-engine

- 证据：`crates/search-engine/src/lib.rs:1-37`（crate 头注，含数据面可重建声明）、`src/index.rs:26-29`（INDEX_SCHEMA_VERSION=1 与 INDEX_SCHEMA）、`index.rs:72-214`（SearchIndex::ensure_schema / rebuild / build / doc_count / meta）、`src/query.rs:19-66`（SearchEngine / build_match_query / search）、`src/bin/rp-search-index.rs:3-6`（CLI 用法）。
- 职责：FTS / 混合检索。数据面是独立派生索引库（全部表 `search_index_` 前缀，可整库删除后从 platform 库 + production catalog 库重灌恢复，lib.rs:6-11），从 `resource` + `resource_source` 与 catalog 库 `github_metadata`（topics / language / license / maintenance）建索引。混合排序：FTS bm25 50% + quality_score 归一 30% + keyword 覆盖 20%（lib.rs:12-14）。分词：ASCII 沿用 eri 口径并复用 resource-engine 的 ALIASES 单一同步源；中文 2-gram 预分词入库 + CJK_BRIDGE 查表查询扩展（lib.rs:15-19）。
- 公开接口（lib.rs:32-37）：`SearchIndex`（ensure_schema / rebuild / build / doc_count / meta）、`INDEX_SCHEMA_VERSION`、`SearchEngine::new(index_pool) + search(&SearchQuery) -> Hits`、`SearchQuery / SearchFilters / SearchHit / Hits / IndexDoc / BuildReport / WeightsSnapshot / SearchError`。CLI 二进制 `rp-search-index`（build / query 两个子命令，rp-search-index.rs:4-7）。
- 处置结论：保留，扩展跨插件检索（§2.1 原文）。审计补充：向量检索轴尚未进入本 crate（§8.1 search-engine 行写明 FTS/向量/混合，当前只有 FTS+启发式混合；bge 向量在 Python embedding worker 侧，二者未接通）。2.0 的「知识层跨插件检索」要求本 crate 索引源从 platform + catalog 两库扩展到插件自带数据面。

### 2.6 crates/quality-engine

- 证据：`crates/quality-engine/src/lib.rs:1-36`（归属裁定与职责声明）、`src/state.rs:18-63`（LIFECYCLE_TRANSITIONS / GOVERNANCE_TRANSITIONS 与四个纯函数）、`src/governor.rs:73-171`（Governor：current_lifecycle / apply_lifecycle / apply_governance / update_resource）、`src/review.rs:131-260`（ReviewStore：open / propose / propose_all / decide / pending / get / counts）、`src/bin/rp-review.rs:3-9`（CLI 用法）。
- 职责：数据治理引擎。生命周期状态机（DISCOVERED→FETCHED→PARSED→CANDIDATE→VERIFIED→INDEXED / STALE）与治理轴（ACTIVE / QUARANTINED / REJECTED / ARCHIVED）双轴正交，非法跃迁报错（state.rs 常量表）；VERIFIED 守卫：任何属性更新路径对 VERIFIED 行返回 NeedReview，AI / 自动管道不可覆盖人工验证结果（lib.rs:24-26）；Review 候选队列从 `migrations/quarantine/*.jsonl` 读 D5 隔离清单，propose 幂等入队（decision=PENDING），decide 只写 review 表不自动改资源实体（lib.rs:27-30，rp-review.rs:8 口径一致）。治理写面在自持库 platform-governance.db（gitignore）或测试内存库（lib.rs:32-33）。
- 公开接口（lib.rs:29-35）：`Governor / ResourceUpdate`、`ReviewStore / Proposal / ProposeOutcome / ReviewRow / QuarantineProposeReport / proposals_from_quarantine_dir`、`lifecycle_transition_allowed / lifecycle_allowed_targets / governance_transition_allowed / governance_allowed_targets`、`GovernanceError`。CLI 二进制 `rp-review`（propose / decide / pending / counts 四子命令）。
- 处置结论：保留（§2.1 原文）。审计补充：§8.4 稳定接口 `Propose(Review) / Decide(review_id, decision)` 已按承诺交付；`Score(id)` 接口未实现（六轴评分明细仍在 platform 库 quality_* 列，由 ETL 迁入，本 crate 未重建评分逻辑）。2.0 的知识治理（quality gate、反知识）应复用本 crate 的状态机与队列骨架，但要把治理对象从 Resource 扩到 Knowledge / Tool / Plugin。

### 2.7 tests（rp-eri-parity，workspace 第 7 成员，非 §2.1 六 crate 但属盘点范围）

- 证据：`tests/Cargo.toml:1-16`（package 名 rp-eri-parity，description 写明「M2-B/M4 ERI 行为对照测试载具」）、`tests/src/lib.rs:1-27`（KEYWORD_GROUPS 三组 / LIMIT=5 / REQUIRED_HIT_FIELDS 八字段）、`tests/tests/` 下 4 个集成测试文件（eri_parity.rs / m2_integration.rs / m4_governance.rs / m4_search_parity.rs，find 实测）。
- 职责：同一批关键词跑 eri_p2.py（子进程，生产库副本）与 Rust resolve / search，断言核心命中集合重叠与字段兼容。
- 处置结论：保留。2.0 改动 resolve / search 输出形状时，本载具是兼容承诺（MODULE_BOUNDARIES §8.4「迁移期双轨」）的回归闸门。

---

## 3. Python Workers（workers/python）

- 证据：`workers/python/parsing_worker.py:1-19`、`workers/python/embedding_worker.py:1-45`、`workers/python/evaluation_worker.py:1-30`、`workers/python/runtime/worker_runtime.py:1-36`。三个 worker 共 913 行，runtime 757 行（wc -l 实测）。
- 职责（Worker Protocol v1 之上，契约见 schemas/worker-protocol/README.md:9-18，JSON Schema draft 2020-12 见 v1.json）：
  - 共享 runtime（worker_runtime.py）：JSON-RPC 2.0 over stdio，一行一请求；5 个方法 initialize / get_capabilities / execute / cancel / ping；budget.timeout_ms 用 threading.Timer 强制、max_output_bytes 截断信封（worker_runtime.py:31-33 常量）；idempotency_key 结果缓存到 workers/.cache/<sha256>.json 原子写（worker_runtime.py:34-37）；崩溃安全（handler 异常回 error 不退出）；协作式取消（worker_runtime.py:5-17 头注）。
  - parsing_worker（M3）：md / txt / html 解析，输出 sections / heading_tree / stats，输入硬上限 10 MiB（parsing_worker.py:17），html 用 stdlib html.parser 剥标签（parsing_worker.py:12-13）。
  - embedding_worker（M3/M8）：双后端。`hashing-ngram-v1`（默认，sha256 signed hashing trick，256 维，零依赖）与 `bge-small-zh-v1.5`（ONNX Runtime CPU，512 维，模型从 models/bge-small-zh-v1.5/ 或 EMBEDDING_MODEL_DIR 加载）；真实模型不可用自动回退 hash 并在响应 backend="hash-fallback" 标注（embedding_worker.py:24-36）；维度显式协商不静默截断（embedding_worker.py:38-41）。
  - evaluation_worker（M3）：规则集评估（reject_if / candidate_if / score_thresholds 三层判定，evaluation_worker.py:16-19），输出 candidate / reject / review 建议，不直接改写知识状态（evaluation_worker.py:21），默认规则集 rules/default_ruleset.json，条目上限 500（evaluation_worker.py:26-27）。
- 测试：tests/ 下 7 文件，功能测试 50 个 test 方法（grep -c "def test" 实测：embedding_functional 10 + embedding_model 16 + evaluation_functional 13 + parsing_functional 11），契约测试经 `_harness.py` 的 ContractMixin 共享断言（_harness.py:226 class ContractMixin 实测）。CI 跑 `python -m unittest discover -s tests`（ci.yml 实测）。
- 处置结论：保留，不做全量 Rust 重写（§2.1 原文）。审计补充：Worker Protocol v1（5 方法 + budget + 幂等 + 取消）已经是 2.0 Tool Contract 草案（计划 §16 第 6 条）最接近的现有协议基础；2.0 的 Tool Runner 对 worker 的调度可直接复用该协议，缺口在「宿主侧派发器」：对 apps/、integrations/、migrations/、scripts/ 全目录 grep 三个 worker 名与 worker_runtime（排除 __pycache__）命中 0 行（本会话实测），即仓库内除 workers/ 自身与其测试外没有任何调用方，当前 worker 只能被测试 harness 驱动，无常驻宿主把 execute 请求发进去。

---

## 4. apps/server（Web 工作台与鉴权）

- 证据：`apps/server/rp_server.py:1-21`（模块 docstring）、`rp_server.py:45-70`（令牌加载优先级与权限检查）、`rp_server.py:337-342`（API_ROUTES）、`rp_server.py:379-390`（require_auth：Bearer + hmac.compare_digest 常时比较 + 0.3s 失败节流）、`rp_server.py:392-405`（静态路径 realpath 越界防护）、`rp_service.sh:14-48`（gen-token / systemd user unit 安装）、`selftest_auth.sh:1-25`（鉴权自验 11 项）、`apps/server/Containerfile:1-23`（python:3.12-slim 容器形态）。
- 职责：M7 只读 Web 工作台。单文件 http.server（ThreadingHTTPServer），stdlib-only：
  - 4 个只读 JSON API（rp_server.py:337-342）：`/api/stats`（resource / knowledge / review / research_task / evidence / entity_alias / tool / skill 分组计数，rp_server.py:143-209）、`/api/resources`、`/api/reviews`、`/api/tasks`。全部 sqlite3 URI mode=ro 直查生产库 platform.db（rp_server.py:6-7），无任何写端点；未知 /api/* 返回 404 no_route（rp_server.py:427-428），POST / PUT / DELETE / PATCH 一律 405 readonly 拒绝（rp_server.py:446-452）。不存在 /api/healthz 端点（grep rp_server.py 无命中）；selftest_auth.sh:20 的启动等待循环只是尽力探测 /api/healthz 并回落 `/`。
  - 鉴权：配置 RP_AUTH_TOKEN 或 RP_AUTH_TOKEN_FILE（默认仓库根 .rp-workbench-token，权限应 600，rp_server.py:19-20）后所有 /api/* 强制 Bearer；未配置令牌为本地开发模式（鉴权关闭，启动显著告警，rp_server.py:10-11）。令牌文件权限过宽会告警（rp_server.py:53-57）。
  - 默认绑定 127.0.0.1:7800（rp_server.py:43-44），容器内 RP_HOST=0.0.0.0 覆盖（apps/server/Containerfile:16-17）。
  - 静态页：static/（index.html「只读工作台 Beta」+ login.html + app.js 472 行 + style.css）。app.js 只消费 4 个 API 渲染 Overview / Resources / Reviews / Tasks（app.js:141/264/322/364 fetch 实测），登录页与 Bearer 头注入逻辑在 app.js:50 附近（fetch 带 headers）。
  - 部署：`rp_service.sh {gen-token|install|start|stop|restart|status|logs}`，systemd 用户服务 rp-workbench.service，unit 内固定 RP_DB_PATH=migrations/platform.db 与 RP_AUTH_TOKEN_FILE（rp_service.sh:36-37）；install 前强制校验令牌文件 600（rp_service.sh:26）。
- 处置结论：逐步演进为 API / Web 服务边界（§2.1 原文）。审计补充：当前 4 个 API 全部直连 SQL（handler 内嵌查询，rp_server.py:143-209），没有经过任何 Rust engine，与 2.0「Application API 是唯一业务入口，Web / MCP / CLI 不各自实现业务逻辑」（计划 §3 架构原则）相悖，这是 2.0 最大的架构债之一；演进路径应是 handler 逐个切换到 engine 接口（resolve / search / review 队列）而不是继续加 SQL。只读约束、Token 鉴权、常时比较、节流、路径越界防护四项安全实现可整体保留。计划 §2.1 末段对 app.js 的定位（「M7/M8 轻量工作台，不应误认为完整研究工作台」）与实测一致。

---

## 5. integrations/rtis（RTIS 接入层）

- 证据：`integrations/rtis/README.md:1-60`（kit 清单表与纪律）、`eri_adapter.py:1-23`（契约与环境变量）、`eri_adapter.py:214-255`（bridge_gate a→b→c 链与审计）、`rtis.py:653-679`（_eri_external_review 与 RTIS_RESOLVER_MODE 默认 rp）、`shadow_compare.py:1-15`（M8 影子对照材料）、`backups/`（rtis.py.pre-m5-20261106-033940.bak，ls 实测）、`tests/`（test_eri_adapter.py + test_rtis_regression.py，find 实测）。
- 职责：两层。
  - kit 层（2026-11-09 从 ~/.dsh/redteam 解耦迁入，README.md:3-5）：检索引擎 rtis-bridge.js（node 22+ node:sqlite，中文 2-gram + 组覆盖率门禁 + 证据等级 K0-K4）+ 调用器 rtis_kb.mjs + 知识库 CLI 与唯一写入实现 rt.py（FTS5，零第三方依赖）+ 编排层 rtis.py（任务状态机 / 站型画像 / 知识门禁 / 路由 / 交接 / AAR，需 pyyaml）+ MCP server rtis_mcp.mjs（stdio JSON-RPC 2.0，工具面 9 个：redteam_poc_search / poc_get / poc_add / poc_use / poc_update / skill_list / skill_get / rtis_task / rtis_knowledge，README.md:44-47）+ skills/ 125 个技能 md + pocs/ 正文目录 + knowledge.db（14,717 POC，README.md:14；rtis.py:6 记 14664 条 poc，两处口径有差，knowledge.db 本体不入 git，bootstrap.sh 从备份重建）。纪律：写入只走 rt.py，k_event 是 append-only 遥测，knowledge.db 的 poc / k_meta / hit_count / schema 绝不修改（rtis.py:22）。
  - 接入层（M5/M8）：eri_adapter.py 是 ERI 统一 Resolve 网关，契约与 eri_p2.py bridge-gate 完全一致（eri_adapter.py:4-5）；解析顺序 auto = (a) rp-resolve CLI → (b) eri_p2.py bridge-gate 回落 → (c) status=error 信封，绝不抛异常（eri_adapter.py:7-12）；每路径最多 2 次尝试、子进程 5s 超时（eri_adapter.py:23）；输出含 9 字段 external_resource_review + adapter 信封（resolver_used / attempts / elapsed_ms），审计写 adapter_audit.jsonl 可 off（eri_adapter.py:20-21, 203-211, 249-255）。生产 rtis.py 在 RTIS_RESOLVER_MODE=rp 时前置调用本 adapter（eri_adapter.py:5），M8 已把默认值从 legacy 切为 rp（rtis.py:664 行内注释「M8 切换：默认 rp（回滚=改回 legacy 或还原 integrations/rtis/backups）」），RP_DB_DEFAULT 指向 migrations/platform.db（eri_adapter.py:35-36 行内注释「M8 切换（2026-11-06）」）。
- 处置结论：保留兼容与回滚路径（§2.1 原文）。审计补充（对齐 2.0 红线「不修改生产 RTIS 行为」）：
  - 切换开关三态 rp / legacy / off 与 backups/ 备份是既有回滚机制，2.0 动 rtis.py 时必须沿用同一机制。
  - MODULE_BOUNDARIES §8.3 对 rtis 组件的去向判定（rt.py → knowledge-engine、rtis.py task/profile/gate → workflow-engine + audit-engine、rtis_mcp.mjs → integrations/mcp）是 2.0 演进的既有裁定，2.0 的 MCP 章节可直接以 rtis_mcp.mjs 为起点。
  - 知识库本体（knowledge.db）与 skills/pocs 目录是内容资产不是代码，2.0 知识库章节（计划 §8）应把它当作知识源之一接入 Context Broker，而不是重写。
  - 口径差：README.md:14 写 14,717 POC，rtis.py:6 头注写 14664 条。两处均非本会话实测库内计数（knowledge.db 不入 git 且本会话未跑查询），实际条数待验证。

---

## 6. migrations（schema 版本与 ETL 纪律）

- 证据：`migrations/001_init.sql:1-30`（Schema V1 范围裁定与 K-07/K-08/K-09/K-10/K-16/K-17 修正）、`002_tasks.sql:1-16`（research_task + 版本策略说明）、`003_tools_workflow.sql:1-28`（workflow / workflow_registry_meta / tool_execution / etl_run_log）、`004_fullbatch.sql:1-27`（research_task 大 JSON 三列 + evidence 全量）、`verify.py:1-45`（11 类对账维度）、`etl_evidence.py:1-30`（全量批版本 v2.0.0 语义）、`etl_m6.py:1-28`（M6 工具健康实测）、`id_audit.py`（366 行）+ `id_audit_result.json`（结论落盘）。
- schema 版本：4 个 SQL 文件。V1（001）建 6 实体（Resource / Knowledge / Evidence / Tool / Skill / Review）+ 3 基础表（entity_alias / audit_event / schema_version）+ 2 子表（resource_source / tool_version），每表含 created_at / updated_at / schema_version，主键沿用现网稳定 ID 不生成新平台 ID（001_init.sql:23-26）；V2（002）research_task；V3（003）workflow / tool_execution / etl_run_log + tool 列扩展；V4（004）research_task 大 JSON 段三列。版本登记双轨：PRAGMA user_version 锚定 V1 基线（001 幂等重申），schema_version 表登记 V2-V4（002_tasks.sql:13-16 明确这是 dry-run 期口径，正式 user_version 递增随 M2 Rust migrate 统一，实测 crates/database/src/migrate.rs 已提供该机制）。
- ETL 纪律（实测自脚本头注与 verify.py 校验维度）：
  - 快照对快照：源快照锁定 migrations/sources-snapshot/（四库冻结副本 + tools.json + 42 task JSON + manifest.sha256 七项，M8_READINESS.md §一第 4 项），活源 drift 由 registry 自愈豁免覆盖（README.md:47-49）。
  - 隔离不丢弃：单行映射失败逐行写 quarantine/*.jsonl，不迁移不静默丢弃（004_fullbatch.sql:14-18，etl_evidence.py:18-20）；异构库只登记隔离不猜测转换（etl_evidence.py:24-26）。
  - 幂等：全部 SQL 用 IF NOT EXISTS / INSERT OR IGNORE（001_init.sql:29-30）；verify 第 7/9 维校验重复执行 no-op（verify.py:36-42）。
  - 对账红线：源行数 = 目标行数 + 隔离行数逐实体逐表（不允许总量对抵）、ID 100% 覆盖、alias 100% 登记、字段抽样哈希、一致率 100% 才能切流与推送（README.md:47-49；verify.py:24-45）。
  - ID 审计：id_audit.py 产出 id_audit_result.json，结论「现网 ID 可直接沿用」（verify.py:43-45 第 10 维）。
- 库文件与入库状态（`git ls-files migrations/` + `git check-ignore -v` 实测，migrations 共 82 个跟踪文件）：platform.db（生产库，gitignore 且未入库，3.7MB，ls 实测）、platform-dryrun.db（对照库，已入库，1.1MB）、platform-fullbatch.db（gitignore 未入库，SESSION-HANDOFF 记录其含凭据类证据已移出版库）、platform-governance.db 与 platform-search.db（已在 .gitignore 第 14-15 行，但 git ls-files 实测仍在版本库中，属「先提交后忽略」遗留，2.0 整理时应裁决是否移出）。sources-snapshot/ 已入库：catalog / knowledge 两库及 wal/shm、platform-dryrun 与 platform-search 两个 snapshot.db、manifest.sha256、tools.json、42 个 task JSON（git ls-files 计数实测 42）。隔离清单 4 个 jsonl 已入库（assets / d5_candidates / evidence / research_task）。运行证据：etl_*_last_run.json、etl_*_run_log.jsonl、rtis_tasks_stats.json、source_manifest.json、M1 四份报告。
- 处置结论：保留迁移纪律（§2.1 原文）。审计补充：2.0 新增 Tool / Plugin / Run / Artifact / Agent / Workflow 等表时必须延续同一纪律（快照、隔离、幂等、100% 对账）；platform.db 是生产库，红线「不写 migrations/ 下的 platform*.db」在 2.0 全程生效；schema_version 与 user_version 双轨待收敛（见 2.2 database 审计补充）。

---

## 7. scripts（维护脚本）

- 证据：`scripts/rip_etl_status.py:1-8`（ETL 最近运行状态 + platform.db PRAGMA quick_check，只读）、`scripts/rip_inventory.py:1-9`（资源清单按质量等级 / 生命周期 / trust 统计，mode=ro）、`scripts/rip_kb_search.py:1-7`（tool / skill / resource / knowledge / research_task 五类 LIKE 检索，永不写库）。
- 职责：三个只读查询脚本，面向 platform.db 的健康与检索口径，commit b3c4d65 注明「供 toolbox-hub Recipe 经 MCP 调用」。
- 处置结论：纳入工具登记，不视为完整工具平台（§2.1 原文）。审计补充：2.0 的 Tool Registry 落地时，这三个脚本应作为第一批登记对象拿到 manifest（身份 / Schema / 版本 / 运行记录），它们只读 platform.db 的特性使它们天然满足 2.0 的权限分级（T0 只读）。

---

## 8. models（模型资产）

- 证据：`models/bge-small-zh-v1.5/` 四文件（config.json / model.onnx / model.onnx_data / tokenizer.json，ls 实测）；加载方 embedding_worker.py:31-33（「模型文件与 tokenizer.json 从 models/bge-small-zh-v1.5/（或环境变量 EMBEDDING_MODEL_DIR 指向的目录）加载」）；目录在 .gitignore（`models/` 行，gitignore 实测）。
- 职责：bge-small-zh-v1.5 ONNX（BAAI，BERT hidden=512），94.8MB，ModelScope 下载 + sha256 校验（README.md:41-42；SESSION-HANDOFF.md 尾巴进展段）。
- 处置结论：保留。§2.1 表未单列 models，但按「workers/python 保留，不做全量 Rust 重写」与 M8 已交付能力（embedding 真实模型后端）推论保留是唯一一致结论。2.0 的向量检索轴（search-engine 缺口，见 2.5）应继续消费该模型目录约定（EMBEDDING_MODEL_DIR 可重定向）。

---

## 9. CI 与 Containerfile

### 9.1 CI（.github/workflows/ci.yml）

- 证据：文件全文实测（66 行，wc -l 实测）。触发 push / PR 到 main；permissions contents: read；concurrency 取消进行中。
- 两个 job：
  - rust：dtolnay/rust-toolchain@stable + clippy 组件，actions/cache 缓存 cargo（key 含 Cargo.lock 哈希），`cargo test --offline --workspace` + `cargo clippy --offline --workspace --all-targets -- -D warnings`。
  - python-workers：setup-python 3.12，`python -m unittest discover -s tests -v`（working-directory workers/python）。
- 明确排除：migrations/verify.py 不在 CI 内跑（ci.yml 头注第 3-4 行：对账对象含生产衍生快照不入库，对账在本地执行）。
- 处置结论：保留，扩展。2.0 新增的 worker / engine / registry 改动继续进同一质量门；verify.py 保持本地对账纪律不变；若 2.0 引入容器化 Tool Runner 的端到端测试，需评估 CI runner 上 podman 的可用性（待验证：GitHub runner 无 podman 预装，需自行安装，本会话未验证该路径）。

### 9.2 Containerfile（两份）

- 根 `Containerfile`：Rust Core（M2）多阶段构建。builder = debian:bookworm-slim + rustup（rsproxy.cn）自建 1.99.1 工具链（直连 rust:1.99 镜像大层传输停滞的网络规避，Containerfile:6-13）；依赖全部 vendor 进镜像，构建期 cargo 全程 --offline（Containerfile:15-17）；质量门 = fmt check + clippy -D warnings + 全量测试（排除 rp-eri-parity 的 eri_parity 对照，其需 python3 + eri catalog，容器内只跑 m2_integration，Containerfile:36-41 注释）；runtime 层只带 rp-resolve 二进制，ENTRYPOINT 即冒烟 resolve（Containerfile:43-47）。
- `apps/server/Containerfile`：python:3.12-slim，stdlib-only，COPY rp_server.py + static/，RP_HOST=0.0.0.0，数据库以只读卷挂载（apps/server/Containerfile:4-8, 16-19）。
- 处置结论：保留，不等于最终工具运行架构（§2.1 原文）。审计补充：这两份 Containerfile 只覆盖「平台自身的构建与部署」，2.0 的工具执行面是「专用 VM + Podman 隔离运行用户工具」（计划 §5 / §16 第 3 条 TOOL-RUNTIME-ADR），是全新的运行拓扑；现有两份容器实践（离线 vendored 构建、只读卷挂库、0.0.0.0 仅容器内）可直接作为 ADR 的既有约束输入。

---

## 10. 周边工件（盘点附记）

- `probe-cargo/`：DEC-001 构建准入实验工件，PROBE-RESULT.md 记录 rsproxy 在线 9.57s / vendored 离线 8.73s 构建全通，R-03 降级为已缓解（PROBE-RESULT.md:1-30 实测）。处置：保留（历史验证证据）。
- `legacy/`：eri P0-P2 源码（eri.py / eri_p1.py / eri_p2.py + db-snapshot.sql）与 architecture-2.0 十四份报告的 rsync 归档；与外层 `security-research-upgrade/architecture-2.0/` 的 MODULE_BOUNDARIES.md 实测 diff 一致（diff -q 输出「两份一致」）。处置：保留（只读归档）；注意 Cargo.toml:25 与多个 crate 头注引用的模块边界文档路径写作 `security-research-upgrade/architecture-2.0/MODULE_BOUNDARIES.md`（仓库外相对路径），仓库内对应物是 `legacy/architecture-2.0/MODULE_BOUNDARIES.md`，两份内容当前一致，但引用路径脆弱，2.0 文档整理时应修正为仓库内路径。
- `schemas/worker-protocol/`：v1 契约（README + JSON Schema draft 2020-12）。处置：保留，作为 2.0 TOOL-CONTRACT-DRAFT 的输入（见第 3 节审计补充）。
- `vendor/` + `.cargo/config.toml`：离线构建基线（source.crates-io replace-with vendored-sources）。处置：保留（DEC-001 约束延续）。
- `.rp-workbench-token`：工作台令牌文件（gitignore，600）。敏感文件，本文不记录内容。
- `.zcode/`、`.sock.lock`、`__pycache__/`：会话运行产物，非架构组件。

---

## 11. 处置汇总表（对齐计划 §2.1，含审计补充）

| 路径 | 当前职责（实测） | §2.1 处置 | 审计补充 |
|---|---|---|---|
| crates/domain | 域模型纯类型库（Resource + AuditEvent 已收编） | 保留 | 仅 2/15 实体入域；2.0 新实体唯一落点 |
| crates/database | 连接池 + 迁移执行器 | 保留并演进 | backup/restore 未实现；user_version 与 schema_version 双轨待收敛 |
| crates/audit-engine | Emit / Trace + 旁路缓冲 | 保留并扩展 | Python 与 rtis 侧审计仍是 JSONL 分叉，待收敛进统一事件流 |
| crates/resource-engine | Repository + Resolve（rp-resolve） | 保留 | 生产切流路径；Enrich/事件接口未实现；权重 0.6+0.4 简化已归因 |
| crates/search-engine | FTS5 派生索引 + 混合检索（rp-search-index） | 保留，扩展跨插件检索 | 向量轴未接入；索引源仅 platform + catalog 两库 |
| crates/quality-engine | 双轴状态机 + Review 队列 + VERIFIED 守卫（rp-review） | 保留 | Score(id) 未实现；治理对象待从 Resource 扩到 Knowledge / Tool / Plugin |
| workers/python | 3 worker + runtime + 契约测试（50 功能用例） | 保留，不做全量 Rust 重写 | 协议 v1 可作 Tool Contract 雏形；缺常驻宿主派发器 |
| apps/server | 只读工作台：4 API + Token 鉴权 + systemd 服务 | 逐步演进为 API / Web 服务边界 | handler 直连 SQL 与「API 唯一入口」原则相悖，是最大架构债 |
| integrations/rtis | RTIS kit（检索/CLI/编排/MCP）+ eri_adapter 接入层 | 保留兼容与回滚路径 | RTIS_RESOLVER_MODE 三态 + backups 即回滚机制；生产行为红线生效 |
| migrations | Schema V1-V4 + ETL + verify 100% 对账纪律 | 保留迁移纪律 | 快照/隔离/幂等纪律是 2.0 新表的前置模板；platform.db 红线生效 |
| scripts | 3 个只读查询脚本 | 纳入工具登记 | Tool Registry 第一批登记对象 |
| Containerfile（含 apps/server/） | 平台自身容器构建/部署 | 保留 | 不等于 2.0 工具运行架构（VM + Podman 另立 ADR） |
| .github/workflows/ci.yml | Rust + worker 测试质量门 | 保留，扩展 | verify.py 留在本地对账，纪律不变 |
| models/ | bge-small-zh-v1.5 ONNX | 保留（§2.1 未单列，按 worker 保留推论） | EMBEDDING_MODEL_DIR 可重定向约定延续 |
| tests/（rp-eri-parity） | ERI 行为对照载具 | 保留 | 兼容承诺的回归闸门 |

## 12. 与 2.0 目标的差距清单（盘点直接产出，供后续 Phase 0 文档引用）

1. Application API 缺位：Web 直连 SQL、MCP 直连 rtis kit、CLI（rt.py / rp-* / scripts）各自为政，无统一业务入口（计划 §3 原则第 1 条）。
2. knowledge-engine / tool-engine / workflow-engine / agent-engine 四个 §8.1 规划模块在 crates/ 中尚不存在；workflow 与 tool 的数据面已在 platform.db（003_tools_workflow.sql 的 workflow / tool / tool_execution / etl_run_log 表）但无 Rust 读写面。
3. 工具执行面为零：tool_execution 表 0 行空结构（003_tools_workflow.sql:16-17），VM + Podman Runner、Tool Registry、Run/Artifact/Evidence 标准记录全部待建（计划 §16 第 2/3/6 条）。
4. Worker 宿主派发器缺失：协议与 worker 齐备，无常驻进程把任务发进 worker。
5. 审计分叉：Rust audit-engine 与 JSONL 审计（adapter_audit.jsonl、rtis 任务审计）并存。
6. 版本登记双轨：PRAGMA user_version 与 schema_version 表并存（002_tasks.sql:13-16 已自我声明）。
7. domain 实体覆盖 2/15：Knowledge / ResearchTask / Evidence / Tool / Skill / Workflow 等仍在数据面，未进类型面。

以上第 1 至 7 条均有本文字级证据，无推测项。
