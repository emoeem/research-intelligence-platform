# WSW 复用矩阵（02）：既有面 × 复用判定 × 迁移链实测

> 文档类型：审计基线 / 后续批次边界依据
>
> 日期：2026-11-18（全部结论为本日会话内实测：`ls` / `grep` / `sqlite3 PRAGMA user_version` / `podman ps -a` / `/dev/tcp` 探测；未轻信任何文档描述）
>
> 上游红线：`docs/rip3/WEB-SECURITY-WORKBENCH-PLAN.md` §2.3（八条边界，其中第 3/4/5/8 条直接约束本矩阵：不另建第二套 Agent / Workflow Runtime / Tool Registry / ResearchTask / Finding / Evidence 数据体系；漏洞事实以 RTIS 为唯一生产事实源；必须复用 Trace 六 ID、Artifact、Evidence、Tool Engine、Runtime Gateway 与既有审计体系）
>
> 判定口径：
> - **直接复用**——既有 API / 表 / 存储位按现状可达，WSW 零改动挂接；
> - **需 Adapter**——既有面可承载语义，但需一层薄适配（不改事实源与持久层归属）；
> - **需新增**——实测未发现对应面（标注「未发现」），新增物必须落在既有链（迁移 023+ / 新 crate 挂既有依赖方向），不得另起体系。

---

## 一、复用主矩阵

| 面 | 现状（实测） | 判定 | 证据（path:line） | WSW 用法一句话 |
|---|---|---|---|---|
| Tool Engine | `crates/tool-engine` 受理/状态机/信封三面 + 四执行 Adapter（podman_host / python_worker / structured_output / egress_proxy）+ document_fetch / image_ledger；提交落 `tool_run`（005 表） | 直接复用 | `crates/tool-engine/src/lib.rs:158`（submit）、`lib.rs:253`（INSERT tool_run）、`lib.rs:795`（insert_artifact）、`src/adapter/egress_proxy.rs:1-12`、表 `migrations/005_tool_contract.sql:34` | Fuzzer/Intruder/Scanner 的一切执行动作一律经 `Engine::submit` 落 tool_run，禁止旁路子进程直跑 |
| Runtime Gateway | Runner 探测/派发/故障转移（P10-3 多候选节点表）+ WAITING_FOR_RUNNER 语义（不伪装失败）+ `runtime_node` 健康落库（007） | 直接复用 | `crates/runtime-gateway/src/lib.rs:120`（impl Gateway）、`lib.rs:245`（dispatch）、`lib.rs:279`（probe_and_resume）、`lib.rs:307`（recover_waiting_runs）、`src/health_store.rs` | 批量任务的节点选择、健康判定与恢复走 Gateway，不自制执行调度器 |
| Workflow Engine | definition-v1 DSL（JSON Schema 编译期内嵌）+ 静态校验 + 版本存储（016，版本追加不可变）+ run 三表（017：workflow_run 六态 / workflow_run_node 五态 / workflow_checkpoint 只追加）+ 控制面（018）+ 执行器四类节点（Tool/Knowledge/Condition/Parallel）；agent 节点 fail-closed 拒绝；`rp-workflow` CLI 子命令 | 直接复用（红线面：第二 Workflow Runtime 禁止） | `crates/workflow-engine/src/lib.rs:9-24`（职责）、`lib.rs:26-32`（边界冻结与 legacy 只读）、`src/exec/mod.rs:120`（WorkflowExecutor）、`src/store.rs:122`（put）、`src/state.rs:57`（RunStore）、表 `migrations/017_workflow_run.sql:44,78,115` | Intruder 批量编排与扫描管线用 definition-v1 + workflow_run 持久化 + checkpoint 续跑，不另建 job runner |
| Domain | 纯类型/校验域层（agent/audit/kcard/context/document/knowledge/node/plugin/resource/tool/verification/workflow/ids/time/error），零 IO | 直接复用 | `crates/domain/src/lib.rs:29-43`（模块表）、`lib.rs:59`（六 ID 再导出）、`src/ids.rs:10-17` | 新增类型（如 WSW 会话/请求对 ID）挂 domain 模块，校验口径沿用 `CORRELATION_ID_MAX_LEN`（`ids.rs:31`） |
| Database（SQLite 面） | 唯一迁移执行器 `database::migrate`（`PRAGMA user_version` 水位幂等 + ALTER ADD COLUMN 行守卫）；文件链 001→022；`migrations/platform.db` 为 ERI 兼容词表库（user_version=1，实测无 tool_run/artifact/agent_run/workflow_run 表）；RIP schema 开发库水位 22 | 直接复用（红线面：第二数据库事实源禁止） | `crates/database/src/migrate.rs:115`（migrate）、`migrate.rs:71`（守卫）、`migrations/022_coverage_matrix.sql:56`（链头自钉 22）、`apps/server/rp_server.py:92,259`（mode=ro 只读） | WSW 流量/历史/会话新表走既有链增量迁移（023+）与同一执行器，禁止新 .db 文件当事实源 |
| Database（PG 情报核面） | `migrations_pg` V0001→V0006 六文件（refinery 每文件事务 + checksum），intel-db 承载 Document 域/检索/对象存储；verification-engine 在情报核零新表；rip-pg17 容器存在（Exited 135），5432 不可达 | 直接复用 | `crates/intel-db/src/lib.rs:4-18`（职责与红线：不做 RTIS 直写）、`lib.rs:135-138`（`embed_migrations!("../../migrations_pg")`）、`migrations_pg/V0006__search_hybrid.sql:4-6`（链头）、`crates/verification-engine/src/lib.rs:23`（零新表） | 文档情报/卡片向量面只走 intel-db + PG，不在 SQLite 重复建知识/向量表 |
| Artifact | `artifact` 表（005）+ 双登记写面（tool-engine `insert_artifact`、agent-engine `register_tool_result`）+ trace_id 单锚传播（从 run 行同链取）+ 落盘目录 `rip-data/artifacts`（rn-* 目录，实测在）+ PG 面 LocalFsStore（S3 预留） | 直接复用 | 表 `migrations/005_tool_contract.sql:78`；`crates/tool-engine/src/lib.rs:795,815`；`crates/agent-engine/src/evidence.rs:85,119`；trace 传播 `crates/tool-engine/src/lib.rs:802-809`；`rip-data/artifacts/`（实测 ls）；`crates/intel-db/src/object_store.rs:56` | 原始请求/响应/帧以 artifact 行登记（sha256 + uri + provenance），正文落既有 rip-data 目录，不发明新存储格式 |
| Evidence | `evidence` 表（001）+ `research_evidence`（021）+ 登记写面 agent-engine（一次登记双写 artifact+evidence）+ verification-engine finding-link 面 + legacy ETL `etl_evidence.py`（ERI 兼容库通道） | 直接复用 | `migrations/001_init.sql:250`；`migrations/021_research_evidence.sql:16`；`crates/agent-engine/src/evidence.rs:142`（INSERT evidence）；`crates/verification-engine/src/store.rs:863`（INSERT research_evidence）；`migrations/etl_evidence.py` | 漏洞事实落 RTIS（唯一事实源），平台侧结论经 evidence 行引用 artifact/tool_run/research_plan_item，不建第二 Finding 表 |
| Trace 六 ID | 六 ID 集中定义于 `domain::ids`（TraceId / TaskId / AgentRunId / ToolRunId / ArtifactId / EvidenceId；后两者为 RunId 别名）；015 迁移五表 trace_id 列 + 索引 + 只读时间线视图；生成在 agent-engine 受理面（请求传入或 `new_id("tr")`）；audit-engine `trace_by_trace_id` 与 rp-server `/api/traces` 查询面齐备 | 直接复用 | `crates/domain/src/ids.rs:10-17,240,430,436`；`migrations/015_trace_correlation.sql:53-57,81+`；生成 `crates/agent-engine/src/agent_loop.rs:318-325` + `crates/agent-engine/src/lib.rs:72`（new_id）；查询 `crates/audit-engine/src/lib.rs:152`、`apps/server/rp_server.py:1713,3095` | Proxy→History→Repeater→Fuzzer→RTIS 全链一个 trace_id 贯穿（请求可传入沿用同一锚），跨实体族回放走 015 视图与 /api/traces |
| RTIS Adapter | `integrations/rtis` 三层：`eri_adapter.py`（M5 统一 Resolve 网关，rp→legacy→error 三段式，9 字段契约）；`rtis.py`（`RTIS_RESOLVER_MODE` 默认 rp，M8 已切换，回滚=改回 legacy）；`rt.py`（红队知识库 CLI + stdio MCP：全局 knowledge.db poc 14,649 条 + 每靶标 engagements/<slug>/assets.db 资产/漏洞/链路，物理位置在 `~/.dsh/redteam/`，仓库外） | 直接复用（漏洞写入只走 rt.py / RTIS MCP 既有接口） | `integrations/rtis/eri_adapter.py:2-23,42-44`；`integrations/rtis/rtis.py:664,679`；`integrations/rtis/rt.py:1-30`（数据位置实测自述，2026-11-02 核对口径）；红线 `crates/intel-db/src/lib.rs:10`（不做 RTIS 直写） | Scanner 发现 → 经 rt.py / RTIS MCP 落 vuln 事实；平台面只存 evidence/artifact 引用，不直写也不复制 RTIS 库 |
| Plugin Registry | 静态链五渠道 source（archive/git/http_api/local_dir/mcp_server）+ manifest strict 解析 + validate/checksums/lock/trust_policy/catalog 发布；运行链 `plugin-runtime`（七态跃迁 + `plugin`/`plugin_capability` 两表唯一写面 009 + install/enable/disable/upgrade/uninstall 全经 audit-engine）；Web 写面经 rp-plugin 子进程 | 直接复用（WSW Plugin Center 挂同一链） | `crates/plugin-registry/src/lib.rs:4-14,23-29`；`crates/plugin-runtime/src/lib.rs:5-20`（职责与审计纪律）；Web 面 `apps/server/rp_server.py:32,3359-3378` | 规则包/扫描 Provider/解码器以插件形态进 catalog 与七态生命周期，不另造扩展体系 |
| Web API | `apps/server/rp_server.py`（stdlib http.server，3432 行）：22 条 `API_ROUTES` + 5 组动态路径（plugins/documents/research plans+items+evidence）；POST 白名单五族（文档导入 / workflows put / workflow-runs 控制面 / research plans 控制 / 插件 install+upgrade+manifest-inspect），写面一律经子进程（rp-doc / rp-workflow / rp-plugin）零直连 SQL；Bearer 鉴权四件套 + 全局限频 + JSONL 访问审计；GET 全量 mode=ro | 直接复用（WSW 后端入口按既有模式加白名单条目） | 路由 `apps/server/rp_server.py:3081-3104`；动态路径 `rp_server.py:3233-3261`；POST 白名单 `rp_server.py:3284-3380`；鉴权 `rp_server.py:3157-3168`；审计 `rp_server.py:3198-3214`；只读 `rp_server.py:259` | Proxy/History/Repeater/Fuzzer 后端挂 `/api/wsw-*` 新白名单 + 子进程模式（进程边界即数据边界），前端复用 React 壳层 |

---

## 二、迁移链现状（实测，不轻信文档）

### 2.1 SQLite 链（`migrations/`）

- **文件链**：`001_init.sql` → `022_coverage_matrix.sql`，共 22 个增量文件（实测 `ls`）。**链头 = 022**，文件尾自钉 `PRAGMA user_version = 22`（`migrations/022_coverage_matrix.sql:56`）。022 内容：research_plan_item 七维 `dimension` 列 + `research_hypothesis` 表（P5-8 覆盖矩阵与自适应研究）。
- **执行器**：`database::migrate` 唯一入口，按 `PRAGMA user_version` 水位幂等 + 015 起 ALTER ADD COLUMN 行守卫（`crates/database/src/migrate.rs:115,71`）。
- **实测库水位**（`python3 sqlite3 PRAGMA user_version`，2026-11-18，mode=ro）：

  | 库 | user_version | 说明 |
  |---|---|---|
  | dev-b1 / dev-b2 / dev-b34-b3 / dev-b34-b4 / dev-b5 / dev-p13 / dev-p4 / dev-p71 | **22**（全链到位） | RIP schema 开发库（tool_run/artifact/agent_run/workflow_run 族所在面） |
  | dev-p7 | 12 | 历史相位冻结副本 |
  | dev-p8 | 18 | 历史相位冻结副本 |
  | dev-b34 | 5 | 历史相位冻结副本 |
  | **platform.db / platform-dryrun.db / platform-fullbatch.db** | **1** | **ERI 兼容词表库**：实测表清单仅 `audit_event/entity_alias/etl_run_log/evidence/knowledge/research_task/resource/resource_source/review/schema_version/skill/tool/tool_execution/tool_version/workflow/workflow_registry_meta`——**无** tool_run/artifact/agent_run/workflow_run；水位语义为 001 自钉 + `schema_version` 表自管（`001_init.sql:38`），非 migrate.rs 水位。rp_server 默认库即此（`rp_server.py:92`），Phase 1-4 数据面经 `RP_AUX_DB` 只读 ATTACH（`rp_server.py:98,280`） |
  | platform-governance.db / platform-search.db | 0 | 衍生快照/对照库 |

- **纪律**（022 文件头原文，`022_coverage_matrix.sql:17-19`）：增量迁移只在开发库副本与测试内存库执行；`migrations/platform*.db` 生产文件不写。

### 2.2 PG 链（`migrations_pg/`）

- **文件链**：`V0001__extensions.sql` → `V0006__search_hybrid.sql`，共 6 文件（实测 `ls | wc -l` = 6）。**链头 = V0006**（ParadeDB pg_search BM25 索引，`V0006__search_hybrid.sql:4-6`）。V0001 扩展装载，V0002 起 Document 域。
- **执行器**：refinery `embed_migrations!("../../migrations_pg")`，每文件事务 + checksum（`crates/intel-db/src/lib.rs:135-138`）；消费方 = intel-db（Document/检索/对象存储）+ verification-engine（情报核**零新表**，`crates/verification-engine/src/lib.rs:23`）。
- **库内水位：未实测**。实测依据：`podman ps -a` 显示容器 `rip-pg17` 存在但 **Exited (135)（27 小时前）**；`/dev/tcp/127.0.0.1/5432` 探测**连接被拒**；数据卷 `rip-data/pg17` 存在（权限 700，容器属主 uid 100998）。本审计会话不启动生产容器（避免运行时副作用），refinery schema history 表水位留待下次 PG 启动时核对——**文件链头 V0006 为实测，库内水位为未验证项**。

### 2.3 红线反查（第二套 X = 未发现核查）

- **第二套数据库事实源**：未发现。SQLite 唯一执行器 `database::migrate`；PG 唯一执行器 intel-db refinery；`legacy/` 为 ERI P0-P2 rsync 归档非运行面（`README.md` 架构节）；`migrations/*.db` 中 platform 族为 ERI 兼容库（同一兼容事实源的两代，经 `verify.py` 对账），不是第二 schema。
- **第二套 Workflow Runtime**：未发现。唯一 `workflow-engine`（016/017/018 持久层 + executor）；003 存量 13 行 legacy 定义仅只读参考导入、永不进执行器（`crates/workflow-engine/src/lib.rs:11,31`）。
- **第二套 Tool Registry**：未发现。唯一 `tool-registry` crate（tool-engine 依赖 `tool_registry::Registry`，`crates/tool-engine/src/lib.rs:30`）；`agent-engine/src/evidence.rs:207` 的 `INSERT INTO tool` 位于 `#[cfg(test)]`（`evidence.rs:171` 起）测试种子，非生产写面。

### 2.4 WSW 关键缺口（实测未发现项）

| 缺口 | 实测结论 | 最近接既有面 | 判定 |
|---|---|---|---|
| MITM / 拦截代理引擎 | 未发现（`grep mitm` crates 零命中） | `crates/tool-engine/src/adapter/egress_proxy.rs`（P3-2 CONNECT 白名单代理：run 生命周期、127.0.0.1 ephemeral 监听、pasta 端口转发、拒绝落 `ProxyEvent::Rejected` 结构化事件） | **需新增**（新 crate），但白名单/事件结构/派发链复用 |
| HTTP History / 流量记录存储 | 未发现（无对应表与 crate） | artifact + evidence 关联面 | **需新增**迁移（SQLite 023+），行面挂 artifact/evidence/trace_id |
| WebSocket 客户端/帧引擎 | 未发现（crates 无 websocket 代码） | POC 前全仓零 HTTP/WS crate（00 §1.1 实测；`crates/mcp-server/Cargo.toml` [dependencies] 复核仅 serde/serde_json/serde_yaml_ng/rmcp/tokio/database/domain/tool-registry/tool-engine/runtime-gateway/agent-engine，无 hyper） | **需新增**（hyper/h2/tokio-tungstenite 系随 WSW-2 hudsucker 组合面引入，ADR-0004 D1） |
| TLS 拦截证书面 | 未发现 | — | **需新增** |
| Fuzzer / Intruder payload 引擎 | 未发现 | 编排面可直接走 workflow-engine Tool/Parallel 节点 + tool-engine 派发 | 编排**直接复用**；payload 引擎**需新增** |

---

## 三、实测命令清单（复现）

```bash
# 1. 迁移文件链头（SQLite/PG）
ls -1 migrations/ | grep -E '^[0-9]{3}_.*\.sql$' | tail -1        # → 022_coverage_matrix.sql
ls -1 migrations_pg/ | tail -1                                     # → V0006__search_hybrid.sql
grep -n "PRAGMA user_version" migrations/022_coverage_matrix.sql   # → 56: PRAGMA user_version = 22;

# 2. 实测库水位（mode=ro）
python3 -c "
import sqlite3, glob
for db in sorted(glob.glob('migrations/*.db')):
    print(db, sqlite3.connect('file:'+db+'?mode=ro', uri=True).execute('PRAGMA user_version').fetchone()[0])"

# 3. platform.db 是否含 RIP schema 表（应为空 → ERI 兼容库）
python3 -c "
import sqlite3
c = sqlite3.connect('file:migrations/platform.db?mode=ro', uri=True)
print(sorted(r[0] for r in c.execute(\"select name from sqlite_master where type='table'\")))"

# 4. PG 可达性与容器状态
timeout 3 bash -c 'echo > /dev/tcp/127.0.0.1/5432'                 # → 连接被拒（本日实测）
podman ps -a --format '{{.Names}} {{.Status}}' | grep pg           # → rip-pg17 Exited (135)

# 5. 未发现项反查
grep -rln "mitm" crates --include='*.rs' | grep -v target          # → 空
grep -rln "websocket" crates --include='*.rs' | grep -v target     # → 空
```

预期回显见本文各表括注；若与本文不符，以回显为准并更新本矩阵。
