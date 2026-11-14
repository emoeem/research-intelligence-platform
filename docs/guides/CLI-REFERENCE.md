# CLI 参考手册（rp-* 11 bin · Phase 9 P9-3 帮助面统一）

| 项       | 值 |
| ---      | --- |
| 状态     | 生效中（P9-3 批次交付；帮助文本以各 bin 实际输出为准，本手册逐条实测对账） |
| 日期     | 2026-11-14 |
| 适用版本 | Phase 9 P9-2 工作树及之后 |
| 相关     | docs/phase9/REST-API-CONTRACT.md（REST 面）、docs/phase9/PHASE9-DESIGN.md（MCP 面）、apps/server/selftest_phase9_rest.sh 第 11 段（帮助面统一口径机器断言）、crates/*/src/bin/*.rs（实现源） |

> rp-* 全族共 12 个 bin：11 个 CLI 入口（本手册逐 bin 覆盖）+ 1 个协议面 rp-mcp（stdio JSON-RPC，非 CLI，见文末注记）。本批次把 11 个 CLI 的帮助面统一为一套口径，并在此给出逐 bin 参考与可复制命令。

## 一、帮助面统一口径（11 bin 全量一致）

1. `--help` / `-h`（argv 任意位置）：用法全文走 **stdout，exit 0**。
2. 误用（无参数、未知子命令、缺必填旗标）：用法全文走 **stderr，exit 2**。
3. 用法骨架统一：首行 `usage: rp-<bin> <synopsis>`；其后 `subcommands:`（或 `commands:` / `flags:`）分节；`exit:` 节声明退出码语义；`help:` 节自指。
4. 机器断言：selftest_phase9_rest.sh 第 11 段实测 11 bin（--help exit 0 + stdout 首行 `usage: rp-<bin>`；无参误用 exit 2）。
5. 统一前差异（本批收口，如实记录）：rp-agent/rp-workflow/rp-context/rp-research/rp-tool/rp-plugin/rp-knowledge-fetch/rp-tool-health 此前不支持 `--help`（只有误用时打 usage exit 2）；rp-review/rp-search-index 此前无参与 `--help` 同路径（stdout exit 0），现无参归误用路径；rp-resolve 此前错误路径不带 usage，现 error + usage 同出。P9-2 发布注记记录的「rp-tool run --help 输出八行用法（exit 2）」按 DOC-STANDARD 快照纪律不回改，行为以本手册与本批 RELEASE 为准（现 exit 0）。

## 二、速览表

| bin | crate | 定位 | --db 缺省 | 构建目标 |
| --- | --- | --- | --- | --- |
| rp-agent | agent-engine | Agent 定义与运行 + 步骤/trace 轨迹 | migrations/dev-p7.db | -p agent-engine --bin rp-agent |
| rp-workflow | workflow-engine | Workflow 定义 CRUD + run 执行与控制 | migrations/dev-p8.db | -p workflow-engine --bin rp-workflow |
| rp-tool | tool-registry | 工具 validate/register/list/get + run/status/cancel（经 rp-mcp 通道） | 必填 | -p tool-registry --bin rp-tool |
| rp-plugin | plugin-runtime | 插件生命周期（install/enable/.../review） | 必填 | -p plugin-runtime --bin rp-plugin |
| rp-context | context-broker | Context 装配/展开/清单/缓存 | migrations/dev-p4.db | -p context-broker --bin rp-context |
| rp-resolve | resource-engine | 资源库关键词解析 | 必填 | -p resource-engine --bin rp-resolve |
| rp-search-index | search-engine | FTS5 索引构建与混合检索 | 派生索引库（--index-db） | -p search-engine --bin rp-search-index |
| rp-review | quality-engine | 治理评审队列（propose/decide/pending/counts） | 必填 | -p quality-engine --bin rp-review |
| rp-research | research-engine | 七能力契约化调用（mock 纯函数） | 无库（Schema 门 + 单行 JSON） | -p research-engine --bin rp-research |
| rp-knowledge-fetch | tool-engine | 知识文档抓取（egress 白名单约束） | 必填 | -p tool-engine --bin rp-knowledge-fetch |
| rp-tool-health | tool-engine | Worker Protocol v1 握手探测 | 无库（spawn worker） | -p tool-engine --bin rp-tool-health |

红线（全族一致）：生产 `migrations/platform*.db` 一律不写（ROADMAP §13）；rp-plugin 与 rp-workflow 显式拒绝 `--db` 指向 platform*.db（`production_db_refused`）。构建一律 `cargo build --offline`（vendor 配置按仓库根 CWD 发现）。

## 三、逐 bin 参考

### 3.1 rp-agent（Agent 定义与运行）

- 子命令：`run`（真实 AgentLoop + 工具派发）、`def-add` / `def-show`（定义与能力绑定）、`run-show` / `run-steps`（运行与步骤轨迹，P7-7）、`trace`（六 ID 全链汇聚，P7-5）。
- 例子（selftest_phase7_trace 同款链路）：
  ~~~bash
  cargo build --offline -q -p tool-registry --bin rp-tool -p agent-engine --bin rp-agent
  rp-tool register --manifest manifests/tools/rip.kb.keyword_search.json --db /tmp/dev.db
  rp-agent def-add --agent-id agent.demo --version 0.1.0 --name Demo --caps "rip.kb.keyword_search" --db /tmp/dev.db
  rp-agent run agent.demo --goal "调研目标" --db /tmp/dev.db
  rp-agent trace <run_id> --db /tmp/dev.db
  ~~~
- 数据面：agent_run / agent_run_step / 013 三记忆表 / 014 控制行（迁移由 CLI 落，临时库即用即建）。

### 3.2 rp-workflow（Workflow 定义与执行）

- 子命令：`validate` / `put` / `get` / `list` / `import-legacy` / `run` / `status` / `nodes` / `resume` / `retry` / `cancel`。
- exit 语义：ok=0；run 终态 FAILED / 静态校验拒绝 / 存储失败=1；装载失败 / 受理拒绝 / 用法错误=2。
- 例子（REST 写面同款定义契约 workflow/definition-v1）：
  ~~~bash
  rp-workflow put docs/phase8/samples/p8-tool-node.json --db /tmp/dev.db
  rp-workflow run wf-p8-tool --input '{"target":"example.com"}' --db /tmp/dev.db
  rp-workflow status <run_id> --db /tmp/dev.db
  ~~~
- REST 对应面：POST /api/workflows（put）、POST /api/workflow-runs（run/pause/resume/retry），同一路径语义（见 REST-API-CONTRACT §十）。

### 3.3 rp-tool（工具登记与执行通道）

- 子命令：`validate`（D-TC-02 静态校验）、`register`（登记入 --db）、`list` / `get`（目录读面）、`run` / `status` / `cancel`（P9-2 起执行通道）。
- run/status/cancel 实现形态是最小 MCP 客户端：spawn rp-mcp 做一次性 stdio JSON-RPC 交换（与外部 AI 同一执行通道，dispatch.rs 唯一实现）；rp-mcp 二进制缺失报 `mcp_bin_missing`，可用 `RP_MCP_BIN` 指路（缺省 target/debug/rp-mcp）。
- 例子：
  ~~~bash
  rp-tool validate --manifest manifests/tools/rip.doc.parse.json
  rp-tool register --manifest manifests/tools/rip.doc.parse.json --db rip-data/tool-runs.db
  rp-tool run --db rip-data/tool-runs.db --tool rip.doc.parse --input '{"path":"README.md"}'
  rp-tool status --db rip-data/tool-runs.db --run <run_id>
  ~~~
- 环境变量：`RP_MCP_BIN`（rp-mcp 路径）、令牌经环境注入子进程（RP_MCP_TOKEN 纪律，禁止匿名）。

### 3.4 rp-plugin（插件生命周期）

- 命令族：install / enable / disable / uninstall / activate / deactivate / load / mark-error / list / upgrade / update-check / ingest / sync / review（词表见帮助全文）。
- 特性：生产库拒绝（guard_db_path）；JSON 单行输出（stdout），错误 JSON 到 stderr。
- 例子：
  ~~~bash
  rp-plugin --db migrations/dev-p6.db --plugins-dir rip-data/plugins install <id> --package <dir>
  rp-plugin --db migrations/dev-p6.db list
  ~~~

### 3.5 rp-context（Context Broker）

- 子命令：`assemble`（按 task 装配）、`expand`（清单项展开）、`manifest-show`、`cache-stats`。
- 例子：
  ~~~bash
  rp-context assemble --task-id <task_id> --text "研究意图" --db migrations/dev-p4.db --migrations-dir migrations
  ~~~

### 3.6 rp-resolve（资源解析）

- 必填：`--db`、`--keywords`（逗号分隔）；`--migrate-dir` 可对内存库/新库落迁移（幂等）。
- 例子：`rp-resolve --db migrations/platform.db --keywords "agent,trace" --limit 5`
- 注意：platform.db 只读消费（`--writable` 才可写打开；缺省只读）。

### 3.7 rp-search-index（FTS5 索引）

- 模式：`build`（platform 库 + catalog 库 → 派生索引库，`--rebuild` 可重建）与 `query`（bm25 50% / quality 30% / keyword 覆盖 20%）。
- 例子：
  ~~~bash
  rp-search-index build --platform-db migrations/platform.db --catalog-db <catalog.db> --index-db /tmp/search-index.db --rebuild
  rp-search-index query --index-db /tmp/search-index.db --keywords "trace,agent" --limit 10
  ~~~

### 3.8 rp-review（治理评审）

- 子命令：`propose` / `decide` / `pending` / `counts`（quarantine 静态审查提案的双轴状态机面）。
- 例子：`rp-review pending --db migrations/dev-b34.db`

### 3.9 rp-research（研究能力契约化）

- 子命令：`list`（七能力词表）、`run <capability_id> --input '<json>'`（request-v1 校验 → 派发 → result-v1 自检，单行 JSON stdout）。
- exit 语义：SUCCEEDED=0，FAILED=1，用法/契约违规=2，内部不变量违规=3。
- 能力词表：research.planner / research.source-discovery / research.evidence-extraction / research.verification / research.synthesis / research.citation / research.evaluator（P7-6 mock 纯函数实现，真实接线归后续批次）。

### 3.10 rp-knowledge-fetch（知识文档抓取）

- 必填 `--db` 与 `--url`；egress 约束经 `--allow-domain` / `--allow-port`（缺省收紧），`--max-bytes` / `--max-redirects` 上限；`--no-require-approved` 显式豁免已核准来源校验。
- 例子：`rp-knowledge-fetch --db migrations/dev-p6.db --url https://example.com/ --allow-domain example.com`

### 3.11 rp-tool-health（Worker 握手探测）

- 命令：`ping --manifest <path> [--timeout-ms <n>]`：spawn worker 一次 Worker Protocol v1 initialize/ping 交换；ok=0，探测失败=1，用法错误=2。
- 例子：`rp-tool-health ping --manifest manifests/tools/rip.doc.parse.json`

## 四、rp-mcp 注记（协议面，非 CLI）

rp-mcp（crates/mcp-server）是平台 MCP 面（rip.* 前缀，stdio JSON-RPC 2.0 子集），消费方式是协议交换而非命令行旗标，故不入 11 bin CLI 统一口径：无 --help，启动即 stderr ready 横幅并等待 stdin 请求行。要点：`RP_MCP_TOKEN` 禁止匿名（未配置时全部请求 -32001 auth_required）；`RP_DB_PATH`/`RP_AUX_DB` 目录读面；`RP_RUN_DB` 运行库（缺省 rip-data/tool-runs.db）；外部 AI 完整走法见 docs/guides/EXTERNAL-AI-QUICKSTART.md。

## 五、对账与维护纪律

- 帮助文本改动必须重跑 selftest_phase9_rest.sh 第 11 段（11 bin exit/首行断言）与三道门禁（cargo test / clippy / workers unittest）；本手册章节与 bin 实际输出漂移即债，改 bin 先改本手册（同批同提交）。
- 新增 bin（后续批次）必须：实现 usage_text()/usage() 双路径（--help stdout exit 0，误用 stderr exit 2）、更新本手册速览表与逐 bin 节、更新 selftest BINS 数组。
