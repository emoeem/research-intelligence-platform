# Phase0 工具盘点：可登记为 RIP 工具的既有资产（TOOL-INVENTORY）

日期：2026-11-09
范围：`research-platform/workers/python/`（3 个 worker 与 runtime）、`research-platform/scripts/`、工作区根的 `tools/`、`toolkit/`，以及性质相同的可复用成熟脚本目录 `research-platform/integrations/rtis/` 与 `security-research-upgrade/eri/`（存在才盘）。`runs/` 下临时产物不在盘点范围。
依据：`docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md` §4.2 工具分类表、§4.3 来源与接入标准。
红线遵守：未在宿主机安装任何依赖（所有验证用仓库内 `.venv` 或系统已有解释器，`-I` 隔离模式）；未修改生产 RTIS 行为（RTIS 只做只读冒烟）；未写 `migrations/` 下任何 `platform*.db`。

结论先行：当前可作为 RIP Tool Arsenal 起步资产的共 12 组（详见 §7 总表），其中 7 组建议直接登记，3 组建议包装 Adapter 登记，2 组克隆物在许可证核实前不得登记；另有 6 个目标一次性脚本建议淘汰。§4.2 十三个工具族中，Document、Knowledge、Evidence、Data Processing、Utility 五个族已有候选，Web/HTTP、DNS/Domain、Network、Web Analysis、Source Code、Mobile/APK、Binary/Reverse、Browser Automation 八个族为空白（Network 仅有淘汰边缘的抓包导出脚本）。

---

## 1. research-platform/workers/python/（原生 Worker 族）

### 1.1 runtime/worker_runtime.py

| 字段 | 内容 |
|---|---|
| 路径 | `research-platform/workers/python/runtime/worker_runtime.py`（758 行） |
| 能力 | Worker Protocol v1 基类：JSON-RPC 2.0 over stdio，方法 `initialize`/`get_capabilities`/`execute`/`cancel`/`ping`（`worker_runtime.py:154`）；budget 强制（timeout 上限 600000 ms、输出上限 64 MiB，`worker_runtime.py:34-38`）；`idempotency_key` 结果缓存（`workers/.cache/`，指纹含 worker_version，`worker_runtime.py:529-594`）；协作式取消；超预算截断信封（`_send_fitted`，`worker_runtime.py:634`）；崩溃安全（handler 异常回 error 响应不退进程） |
| 语言与依赖 | Python 3，纯标准库（json/hashlib/threading/signal） |
| 来源 | RIP 2.0 自研，M3 交付，git 已跟踪 |
| 输入输出 | 输入：stdin 每行一个 JSON-RPC 请求；输出：stdout 协议行 + stderr 结构化日志（stdout 不承载日志，`worker_runtime.py:222-231`） |
| 运行方式 | 不独立运行，作为三个 worker 的基类（子类实现 `execute(ctx, task)` 并设 NAME/VERSION/CAPABILITIES） |
| 许可证 | 仓库内部资产，无独立 LICENSE 文件（随 RIP 仓库） |
| 复用建议 | 直接登记。这是 §4.5 统一工具调用协议的现成实现，Tool Engine 的 worker 接入层应直接复用它，而不是另造协议 |

### 1.2 parsing_worker.py（v0.1.0）

| 字段 | 内容 |
|---|---|
| 路径 | `research-platform/workers/python/parsing_worker.py`（309 行） |
| 能力 | 文档解析与结构化抽取：`md`（ATX + setext 标题）/`txt`/`html`（标准库 html.parser 剥标签，script/style 剔除，`parsing_worker.py:11`）三格式，产出 `sections[]`/`heading_tree[]`/`stats{}`（词数、CJK 字数、行号区间）；输入硬上限 10 MiB（`parsing_worker.py:28`）；md 保留行号，html 行号为 null |
| 语言与依赖 | Python 3，纯标准库 |
| 来源 | RIP 2.0 自研，M3 交付（`docs/M3_REPORT.md`），git 已跟踪 |
| 输入输出 | 输入：`execute.task` 内联 `text` 或文件 `path`（二选一）；输出：data 含 format/source(sha256)/sections/heading_tree/stats |
| 运行方式 | `echo '<jsonrpc>' | python3 workers/python/parsing_worker.py`（stdio 常驻服务） |
| 许可证 | 随 RIP 仓库 |
| 复用建议 | 直接登记。§4.2 归类：Document（页面/文档文本抽取） |

### 1.3 embedding_worker.py（v0.2.0）

| 字段 | 内容 |
|---|---|
| 路径 | `research-platform/workers/python/embedding_worker.py`（369 行） |
| 能力 | 双后端文本向量化：`hashing-ngram-v1`（sha256 signed hashing trick，256 维，零依赖确定性，`embedding_worker.py:99-141`）；`bge-small-zh-v1.5`（ONNX Runtime CPU 推理，512 维，L2 归一化，`embedding_worker.py:144-213`）；批量上限 64 条（`embedding_worker.py:56`）；真实模型不可用回退 hash 并以 `backend="hash-fallback"` 标注，显式 dim 冲突时拒绝静默错维（`embedding_worker.py:310-344`） |
| 语言与依赖 | Python 3；hash 后端纯标准库；ONNX 后端依赖 numpy 2.5.4 / onnxruntime 1.29.0 / tokenizers 0.23.3（全部装在仓库内 `workers/python/.venv`，Python 3.14，未污染宿主机）；模型文件 `research-platform/models/bge-small-zh-v1.5/`（model.onnx / tokenizer.json / config.json，本次 ls 确认存在） |
| 来源 | RIP 2.0 自研，M3 + M8 尾批交付（`docs/M8_TAIL_EMBEDDING_REPORT.md`），git 已跟踪 |
| 输入输出 | 输入：`text` 单条或 `texts` 批量、`model`、`dim`、`ngram_range`；输出：vectors[{index,dim,nnz,l2_norm,vector}] + backend 标注 |
| 运行方式 | stdio 服务，同上；环境变量 `EMBEDDING_MODEL_DIR` / `EMBEDDING_BACKEND=hash`（`embedding_worker.py:68-69`） |
| 许可证 | 随 RIP 仓库（模型权重为 BAAI bge-small-zh-v1.5，上游许可待在 Tool Manifest 中补记，待验证） |
| 复用建议 | 直接登记。§4.2 归类：Knowledge（索引/向量化） |

### 1.4 evaluation_worker.py（v0.1.0）

| 字段 | 内容 |
|---|---|
| 路径 | `research-platform/workers/python/evaluation_worker.py`（238 行） |
| 能力 | 知识条目规则评估：信号抽取（source_count/cve/evidence_ref_count/content_chars，`evaluation_worker.py:90-118`）+ 外置规则集（reject_if/candidate_if/score_weights/score_thresholds，规则集校验 `evaluation_worker.py:56-87`），输出 candidate/reject/review 建议与命中原因；条目上限 500（`evaluation_worker.py:34`）；输出仅为建议，不改知识状态（M3 约束，`evaluation_worker.py:18`） |
| 语言与依赖 | Python 3，纯标准库 |
| 来源 | RIP 2.0 自研，M3 交付，git 已跟踪；默认规则集 `workers/python/rules/default_ruleset.json`（已读，version "1"） |
| 输入输出 | 输入：`entries[]` + 内联 `rules` 或 `rules_path`（缺省用 default_ruleset.json）；输出：results[{suggestion,score,matched_rules,reasons}] + summary |
| 运行方式 | stdio 服务，同上 |
| 许可证 | 随 RIP 仓库 |
| 复用建议 | 直接登记。§4.2 归类：Knowledge（质量检查） |

### 1.5 tests/（9 个文件）

路径 `research-platform/workers/python/tests/`：contract（协议一致性）/functional（功能）/model（真实模型）三层 + `run_all.py` 入口 + `_harness.py`。
本次实际执行：`cd research-platform/workers/python && ./.venv/bin/python tests/run_all.py`，输出末行为 `SUMMARY tests=125 failed=0 errors=0 failures=0 skipped=0 ok=True`。结论：worker 族 125 项测试全绿，具备登记条件。

---

## 2. research-platform/scripts/（平台只读运维脚本族）

三个脚本共同特征：只读（sqlite `mode=ro` URI）、纯标准库、面向 `migrations/platform.db`。git 已跟踪。

### 2.1 rip_kb_search.py

| 字段 | 内容 |
|---|---|
| 路径 | `research-platform/scripts/rip_kb_search.py`（73 行） |
| 能力 | 对 platform.db 的 tool/skill/resource/knowledge/research_task 五表做关键词 LIKE 检索，分组输出（`rip_kb_search.py:16-40`） |
| 语言与依赖 | Python 3，标准库 sqlite3 |
| 输入输出 | 输入：`rip_kb_search.py <关键词> [每组条数]`；输出：终端分组文本 |
| 运行方式 | `python3 -I scripts/rip_kb_search.py kingdee 2` |
| 许可证 | 随 RIP 仓库 |
| 复用建议 | 直接登记（可再包一层结构化输出 Adapter）。§4.2 归类：Knowledge（检索） |

冒烟验证（本次实跑）：`python3 -I scripts/rip_kb_search.py kingdee 2` 输出研究任务 1 条命中（`rtis-20261103-003535-9050  KDSRC 金蝶 SRC 漏洞挖掘...`），末行 `共 1 条命中（每组最多 2 条）。只读查询。`

### 2.2 rip_inventory.py

路径 `research-platform/scripts/rip_inventory.py`（47 行）。能力：资源清单按质量等级/生命周期/信任级统计（只读，`rip_inventory.py:18` mode=ro）。运行：`python3 -I scripts/rip_inventory.py [A/B/C/D]`。复用建议：直接登记。归类：Knowledge（质量统计）/ Data Processing。
冒烟验证（本次实跑）：输出 `资源总量: 81`，按质量等级 `{A: 4, B: 28, C: 44, D: 5}`。

### 2.3 rip_etl_status.py

路径 `research-platform/scripts/rip_etl_status.py`（41 行）。能力：读取 `migrations/etl_*_last_run.json` + `PRAGMA quick_check`，报告数据管线健康（不触发重算，`rip_etl_status.py:4-5`）。运行：`python3 -I scripts/rip_etl_status.py`。复用建议：直接登记。归类：Utility（健康诊断）。
冒烟验证（本次实跑）：输出三个 last_run JSON 摘要（ETL 版本 1.1.1/2.0.0，fk_violations=0）与 `platform.db: quick_check=ok | resources=81 evidence=700 tasks=42`。

---

## 3. toolkit/（rtk：授权优先工作流工具，独立成包）

| 字段 | 内容 |
|---|---|
| 路径 | `toolkit/`（工作区根，独立目录，pyproject 包名 `rtk` v0.1.0） |
| 能力 | 四命令一脚手架（`toolkit/README.md` 命令表 + `toolkit/src/rtk/cli.py`）：`rtk auth check`（授权记录完整性与 scope 覆盖校验，auth.py 152 行）；`rtk ledger sync`（报告目录汇总台账生成 ledger.json 并更新 README 状态表，ledger.py 189 行）；`rtk report lint`（报告结构完整性 + 域名/IP/密钥/个人信息残留检查，lint.py 193 行）；`rtk mcp list/call`（最小 MCP stdio 客户端，mcp.py 152 行）；`rtk scaffold`（engagement 骨架创建，scaffold.py 173 行）。合计 9 模块 1187 行 + 4 个 pytest 测试文件 + examples/ |
| 语言与依赖 | Python >= 3.11，`dependencies = []`（pyproject.toml 原文，零第三方运行时依赖；dev 组 pytest/ruff 仅开发用） |
| 来源 | 自研独立项目，`pyproject.toml` 声明 `Homepage = "https://github.com/emoeem/redteam-toolkit"` |
| 输入输出 | 输入：engagement 目录（targets.toml/scope.md/reports/）；输出：ledger.json、README 状态表、lint 结果；`--check` 模式非零退出码可嵌 CI |
| 运行方式 | `pip install -e ".[dev]"` 后 `rtk <cmd>`；或免安装 `PYTHONPATH` 引用 src/（本次验证方式） |
| 许可证 | MIT（`toolkit/LICENSE` 全文已读，Copyright (c) 2026 rtk contributors） |
| 复用建议 | 直接登记（许可证、依赖、测试齐备，符合 §4.3 全部接入字段）。§4.2 归类：Evidence（ledger/report lint）+ Utility（mcp 客户端、scaffold、auth 治理） |

验证（本次实跑）：
- 模块导入：`python3 -I -c "import sys; sys.path.insert(0,'toolkit/src'); import rtk.auth, rtk.ledger, rtk.lint, rtk.mcp, rtk.scaffold; ..."` 输出 `rtk 0.1.0 modules import OK`（宿主 Python 3.14.9）。
- 未验证项：pytest 测试套件（`tests/test_auth.py` 等 4 文件）未跑，宿主机无 pytest（`python3 -I -c "import pytest"` 报 ModuleNotFoundError），红线禁止安装，待验证。以临时 venv 跑套件是登记前的补充检查项。

---

## 4. tools/（混合目录：自研脚本 + 第三方克隆）

### 4.1 自研可复用脚本

| 路径 | 能力 | 语言与依赖 | 归类（§4.2） | 复用建议 |
|---|---|---|---|---|
| `tools/reqmcp.py` | 最小 stdio MCP 客户端，指向 `/usr/lib/reqable/mcp-server`（`reqmcp.py:9`；该二进制本次 ls 确认存在，7528512 字节），`list`/`call <tool> '<json>'` | Python 3，标准库 | Utility（抓包桥） | 包装 Adapter：server 路径参数化后与 rtk.mcp 能力重叠，二选一保留（倾向以 rtk.mcp 为基底吸收 reqable 用法） |
| `tools/yakitmcp.py` | Yakit MCP（HTTP+SSE transport）最小客户端，`list`/`call` | Python 3，标准库 | Utility | 包装 Adapter：同上，与 rtk.mcp 合并；端到端调用需 Yakit 在线，本次未实跑（待验证） |
| `tools/dump_flows.py` | 经 reqmcp.Client 导出 reqable 抓包 flow 列表（`dump_flows.py:5`） | Python 3，依赖 reqmcp.py | Network（流量导出） | 淘汰为独立工具，改为新 HTTP Adapter 的示例用法保留 |
| `tools/sync_ledger.py` | 汇总 `workflows/progress.json`、`src-programs/INDEX.md` 等数据源同步台账状态区块（`sync_ledger.py` 头注） | Python 3，标准库 | Data Processing | 包装 Adapter：职责与 rtk ledger sync 重叠，建议合并进 rtk 或改造成 RIP 原生 job；会写工作区文件，本次未实跑 |
| `tools/backup_external.sh` | 备份 `~/.dsh/redteam`（红队知识库/engagement/配置/报告）到 `archive/external-backup-<date>`（`backup_external.sh:3-5`） | Bash，tar/zstd | Utility | 直接登记为运维脚本，但注意 RTIS 已于 2026-11-09 迁入 `integrations/rtis/`（见 §5），备份对象语义需先更新（待验证源目录现状） |

### 4.2 目标一次性脚本（淘汰）

以下四个脚本硬编码单一目标 `https://release.chaopin100.cn`（`cp_admin_bypass.sh:3`、`cp_param_probe.py:4`、`cp_points_probe.py:6`、`cp_sweep.py:5`），属超拼严选单次 engagement 的探针产物，参数与路径全部写死，不可作为通用工具登记：
`tools/cp_admin_bypass.sh`（路径变体绕过探测）、`tools/cp_param_probe.py`（55 接口参数补打）、`tools/cp_points_probe.py`（积分/资金面无 token 对照，token 经 `CP_TOKEN` 环境变量）、`tools/cp_sweep.py`（未授权面只读扫描）。
复用建议：全部淘汰出工具库，正文中保留为 engagement 案例（方法论可入知识层，代码不入 Tool Arsenal）。

### 4.3 第三方克隆（许可证未核，暂缓登记）

| 路径 | 现状（本次 ls/head 实证） | 复用建议 |
|---|---|---|
| `tools/TPscan/` | ThinkPHP 漏洞检测 Python3 工具（README 原文「一键ThinkPHP漏洞检测,基于Python3」），含 TPscan.py 与 plugins/；目录内无 LICENSE 文件（find 实证） | 待验证：核对上游许可证与版本 commit 后，按 §4.3 接入标准（源码检查 + 最小功能验证 + 输出格式验证）决定 Adapter 登记；Web Analysis 族目前空白，它是唯一候选 |
| `tools/thinkphp_gui_tools/` | JDK8 + javafx 图形化综合利用工具克隆，目录仅剩 README、3 张图片、php.php，jar 缺失，克隆不完整 | 淘汰（不可运行；第三方 GUI 也不符合无头容器运行形态） |
| `tools/thinkphp-RCE-POC-Collection/` | 仅一个 README.md，空壳克隆 | 淘汰 |

---

## 5. research-platform/integrations/rtis/（生产 RTIS kit，Adapter 登记不改本体）

| 字段 | 内容 |
|---|---|
| 路径 | `research-platform/integrations/rtis/`，git commit `022a717`（rtis: 红队知识栈从 ~/.dsh/redteam 解耦迁入） |
| 组成（本次 wc -l 实证） | `rtis-bridge.js`（531 行，node:sqlite，Node 22+，检索引擎：中文 2-gram 分词 + 组覆盖率门禁 + K0-K4 证据等级排序）；`rtis_kb.mjs`（107 行，引擎 stdin/stdout 调用器，search/get/related/archetype/stats/event）；`rt.py`（749 行，知识库 CLI 与唯一写入实现，FTS5，零第三方依赖，检索路径 `mode=ro`，`rt.py:557`）；`rtis.py`（1656 行，编排层：任务状态机/站型画像/知识门禁/路由/交接/AAR，依赖 pyyaml）；`rtis_mcp.mjs`（510 行，stdio MCP server，9 个 redteam_* 工具面）；`eri_adapter.py`（281 行，ERI 外部资源复查 adapter）；`shadow_compare.py`（178 行，新旧检索影子对比）；`skills/`（125 个红队技能 md）；`pocs/`（POC 正文落盘）；`knowledge.db`（本次 ls 实证 28299264 字节，README 注明不入 git，`bootstrap.sh` 重建） |
| 语言与依赖 | JavaScript（Node 22+，node:sqlite 标准模块）+ Python（rt.py 零依赖；rtis.py 需 pyyaml，本次 `python3 -I -c "import yaml"` 实证宿主已有 pyyaml 6.0.3） |
| 来源 | 自研 kit，2026-11-09 从 `~/.dsh/redteam` 解耦迁入，原目录冻结 |
| 输入输出 | rtis_kb.mjs：stdin JSON `{"cmd":"search","q":...}` 到 stdout JSON；rt.py：CLI 子命令（poc search/get/add/use/update）；rtis_mcp.mjs：stdio JSON-RPC 2.0 |
| 运行方式 | 见 `integrations/rtis/README.md`：`python3 -I rt.py poc search --q kingdee`；`echo '{"cmd":"search",...}' | node rtis_kb.mjs`；MCP 宿主挂 `rtis_mcp.mjs` |
| 许可证 | 自研内部工具，目录内无 LICENSE 文件（find 实证），随 RIP 仓库 |
| 复用建议 | 包装 Adapter 登记。红线：不修改生产 RTIS 行为，登记方式是 Tool Manifest 指向其现有 CLI/stdin 入口（`rt.py`、`rtis_kb.mjs`），Tool Engine 只做进程编排与输出结构化，不动本体代码。§4.2 归类：Knowledge（检索/实体关联/事件） |

验证（本次实跑，全部只读）：
- `python3 -I rt.py poc search --q kingdee --limit 2`：返回 2 条（id 900008 K3Cloud 错误码判读、id 15630 nuclei-kingdee-eas-directory-traversal）。
- `echo '{"cmd":"search","q":"kingdee","limit":2}' | node rtis_kb.mjs`（宿主 node v24.22.2）：返回 `{"ok":true,...,"items":[...]}`，items 2 条含 score 与 rtis 元数据。
- 未验证项：`rtis_mcp.mjs` 未以 MCP 宿主实挂验证（引擎层已通，协议层待验证）；`rtis.py` 编排层未跑状态机（涉及任务写入，属生产行为，本次不动）。

---

## 6. ERI（外部资源目录）：活跃副本、legacy 快照、adapter 三处边界

本次 `diff -q security-research-upgrade/eri/eri.py research-platform/legacy/eri/eri.py` 无输出：两文件逐字节一致，`legacy/eri/` 是活跃副本的快照，不单独登记。

| 字段 | 内容 |
|---|---|
| 路径 | `security-research-upgrade/eri/`（活跃 sidecar）：`eri.py`（749 行）+ `eri_p1.py` + `eri_p2.py` + `tests/`（run_tests.py / run_p1_tests.py / run_p2_tests.py + fixtures） |
| 能力 | External Resource Intelligence P0：Awesome-Hacking 资源目录入库（81 条，与 rip_inventory 的 resources=81 对账一致）、增量同步、去重、来源追踪、软删除（UNLISTED）、元数据层检索；独立 `db/catalog.db`，每条资源必有 source_id/section/commit/行号（`README.md` 定位节） |
| 语言与依赖 | Python 3，标准库（测试套件头注：全程离线不访问网络，临时数据库） |
| 来源 | 自研，P0 已交付（README 原文「P0 已交付」） |
| 输入输出 | CLI 子命令：sync/search/get/stats/report/dedup-report/parse |
| 运行方式 | `python3 -I eri.py <subcommand>` |
| 许可证 | 自研，无独立 LICENSE 文件；上游 Awesome-Hacking 列表为 CC0-1.0（README 注明子项目许可核查属 P1） |
| 复用建议 | 包装 Adapter 登记（入口即 `integrations/rtis/eri_adapter.py`，M8 已把它接为 RTIS 的资源复查通道）。§4.2 归类：Knowledge（来源核验）/ Data Processing |

验证（本次实跑）：`cd security-research-upgrade/eri && python3 -I tests/run_tests.py`，输出末行 `ALL PASS`（含解析/规范化/去重/幂等/增量/软删除/检索/report 全组 ok）。

---

## 7. 总表与 §4.2 归类映射

| 编号 | 资产 | 路径 | §4.2 工具族 | 复用建议 |
|---|---|---|---|---|
| T01 | Worker Protocol v1 runtime | workers/python/runtime/worker_runtime.py | （协议底座，支撑全部族） | 直接登记 |
| T02 | Parsing Worker v0.1.0 | workers/python/parsing_worker.py | Document | 直接登记 |
| T03 | Embedding Worker v0.2.0 | workers/python/embedding_worker.py | Knowledge | 直接登记 |
| T04 | Evaluation Worker v0.1.0 | workers/python/evaluation_worker.py | Knowledge | 直接登记 |
| T05 | KB 关键词检索 | scripts/rip_kb_search.py | Knowledge | 直接登记 |
| T06 | 资源清单统计 | scripts/rip_inventory.py | Knowledge / Data Processing | 直接登记 |
| T07 | ETL 健康检查 | scripts/rip_etl_status.py | Utility | 直接登记 |
| T08 | rtk 四命令 + scaffold | toolkit/ | Evidence + Utility | 直接登记 |
| T09 | RTIS 知识栈 kit | integrations/rtis/ | Knowledge | 包装 Adapter（不改本体） |
| T10 | ERI 资源目录 sidecar | security-research-upgrade/eri/ | Knowledge / Data Processing | 包装 Adapter（经 eri_adapter.py） |
| T11 | reqmcp/yakitmcp/dump_flows | tools/reqmcp.py 等 3 个 | Utility / Network | 包装 Adapter（与 rtk.mcp 合并） |
| T12 | TPscan | tools/TPscan/ | Web Analysis | 待验证许可证后 Adapter，否则淘汰 |

淘汰清单：`tools/cp_admin_bypass.sh`、`tools/cp_param_probe.py`、`tools/cp_points_probe.py`、`tools/cp_sweep.py`（目标一次性）、`tools/thinkphp_gui_tools/`（克隆不完整）、`tools/thinkphp-RCE-POC-Collection/`（空壳）、`research-platform/legacy/eri/`（快照不重复登记）、`tools/sync_ledger.py`（并入后原文件退役）。

工具族覆盖缺口（后续阶段按 §4.1 原则「优先复用成熟开源工具，通过 Adapter 纳入」补齐）：Web/HTTP、DNS/Domain、Network（T11 仅流量导出，无主动探测）、Web Analysis（仅 T12 待定）、Source Code、Mobile/APK、Binary/Reverse、Browser Automation 八族无可用候选。

---

## 8. 验证记录（本次实际执行）

| 命令 | 结果 |
|---|---|
| `cd research-platform/workers/python && ./.venv/bin/python tests/run_all.py` | `Ran 125 tests in 9.268s`，`SUMMARY tests=125 failed=0 ... ok=True` |
| `cd research-platform && python3 -I scripts/rip_etl_status.py` | 三份 last_run 摘要 + `platform.db: quick_check=ok | resources=81 evidence=700 tasks=42` |
| `python3 -I scripts/rip_kb_search.py kingdee 2` | 研究任务 1 条命中，只读 |
| `python3 -I scripts/rip_inventory.py` | `资源总量: 81`，等级 A4/B28/C44/D5 |
| `python3 -I -c "...import rtk.auth, rtk.ledger, rtk.lint, rtk.mcp, rtk.scaffold..."`（显式 sys.path） | `rtk 0.1.0 modules import OK` |
| `cd research-platform/integrations/rtis && python3 -I rt.py poc search --q kingdee --limit 2` | 2 条命中，只读 |
| `echo '{"cmd":"search","q":"kingdee","limit":2}' | node rtis_kb.mjs` | `ok:true`，items 2 条（node v24.22.2） |
| `cd security-research-upgrade/eri && python3 -I tests/run_tests.py` | `ALL PASS` |
| `diff -q security-research-upgrade/eri/eri.py research-platform/legacy/eri/eri.py` | 无差异（快照一致） |
| `python3 -I -c "import yaml"` | pyyaml 6.0.3 可用（rtis.py 依赖满足） |
| `ls /usr/lib/reqable/mcp-server` | 存在（7528512 字节，reqmcp.py 依赖满足） |
| `find tools/TPscan tools/thinkphp-RCE-POC-Collection tools/thinkphp_gui_tools -iname "LICENSE*"` | 无结果（许可证未见） |
| `python3 -I -c "import pytest"` | ModuleNotFoundError（toolkit dev 套件因此未跑） |

## 9. 未验证与遗留（待验证标注汇总）

1. toolkit pytest 套件未跑：宿主无 pytest，红线禁止安装；登记 T08 前用临时 venv 补跑。
2. `rtis_mcp.mjs` 未以 MCP 宿主实挂（引擎调用器已验证）；`rtis.py` 编排层状态机未跑（避免触发生产行为）。
3. `tools/reqmcp.py`、`tools/yakitmcp.py` 端到端调用未跑：需外部 GUI 服务端在线。
4. `tools/backup_external.sh`、`tools/sync_ledger.py` 未跑：均会写文件，非只读。
5. `tools/TPscan/` 上游许可证、版本 commit、功能验证均未做：登记 T12 的前置条件。
6. bge-small-zh-v1.5 模型权重的上游许可证未核对（登记 T03 的 Manifest 字段需补）。
7. `tools/thinkphp_gui_tools/php.php` 内容未审：第三方代码，如考虑复用必须先按 §4.3 做源码检查（当前建议为淘汰，此项随之关闭）。
