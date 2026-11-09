# RTIS Kit: 自包含红队知识栈（任何智能体即插即用）

2026-11-09 从 `~/.dsh/redteam` 解耦迁入。此后 **RIP 仓库就是唯一活体**：
`~/.dsh/redteam` 原样冻结不动，全量备份在 `~/backups/dsh-redteam-20261109-094341.tar.zst`
（sha256 `998f3b6d...c0406`，权限 600）。

## 里面有什么

| 文件/目录 | 作用 |
|---|---|
| `rtis-bridge.js` | 检索引擎本体（中文 2-gram + 组覆盖率门禁 + 证据等级 K0-K4），node 22+（`node:sqlite`） |
| `rtis_kb.mjs` | 引擎的 stdin/stdout 调用器（search / get / related / archetype / stats / event） |
| `rt.py` | 知识库 CLI + 写入唯一实现（poc search/get/add/use/update，FTS5，零第三方依赖） |
| `rtis.py` | 编排层（任务状态机 / 站型画像 / 知识门禁 / 路由 / 交接 / AAR），需 pyyaml |
| `rtis_mcp.mjs` | MCP server（stdio JSON-RPC 2.0），任何 MCP 宿主可挂 |
| `skills/` | 125 个红队技能 .md（带 YAML frontmatter） |
| `pocs/` | POC 正文落盘目录（kit 版 `poc.path` 已迁到本目录前缀） |
| `knowledge.db` | 知识库（14,717 POC + 反知识/关系/站型/事件表）。**不入 git**，用 `bootstrap.sh` 重建 |

## 任何智能体 30 秒上手

```bash
cd integrations/rtis && ./bootstrap.sh                       # 首次：重建 knowledge.db
echo '{"cmd":"search","q":"泛微 任意文件上传","limit":5}' | node rtis_kb.mjs
python3 rt.py poc search --q kingdee                          # CLI 检索
python3 rtis.py task new --objective "..." --target example.com   # 走任务状态机+知识门禁
```

## 挂成 MCP（zcode / Claude Code / 任何 stdio MCP 宿主）

```json
{ "mcpServers": { "rtis": {
    "command": "node",
    "args": ["/绝对路径/research-platform/integrations/rtis/rtis_mcp.mjs"],
    "cwd":   "/绝对路径/research-platform/integrations/rtis"
} } }
```

工具面：`redteam_poc_search` / `redteam_poc_get` / `redteam_poc_add` / `redteam_poc_use` /
`redteam_poc_update` / `redteam_skill_list` / `redteam_skill_get` / `rtis_task` / `rtis_knowledge`。

## 环境变量（全部可选，默认即自包含）

| 变量 | 默认 | 说明 |
|---|---|---|
| `RTIS_HOME` / `RT_HOME` / `REDTEAM_HOME` | 脚本所在目录 | 根目录（DB/skills/pocs 都在根下） |
| `RTIS_DB` / `RTIS_KB` | `<根>/knowledge.db` | 知识库路径 |
| `RTIS_BRIDGE` | `<根>/rtis-bridge.js` | 引擎文件 |
| `RTIS_SKILLS` / `RTIS_ENGAGEMENTS` | `<根>/skills`、`<根>/engagements` | 技能库 / 靶标目录 |
| `RTIS_RESOLVER_MODE` | `rp` | ERI 外部资源复查：rp=本仓库 adapter，off=关 |
| `RTIS_STORE_CORE` | `<根>/store-core.js`（缺省不存在） | dsh 角色模板；不设则 sync-roles 安静降级 |

要临时指回旧 dsh 布局：`RTIS_HOME=~/.dsh/redteam`（mcp 层）或
`RTIS_ROOT=~/.dsh/redteam/rtis RTIS_KB=~/.dsh/redteam/knowledge.db`（编排层）。

## 纪律（继承自 dsh 时代，继续生效）

- 动手前先 `redteam_poc_search`（或 `rt.py poc search`）；命中即取全文照做。
- 写入只走 `rt.py`（或 MCP 工具转调）；`k_event` 是 append-only 遥测。
- 任何 key/凭据不进仓库：FOFA_KEY 等一律环境变量；`knowledge.db` 不入 git。
- `engagements/<slug>/assets.db` 是每靶标事实库（运行时生成，gitignore）。

## 本次迁移做了什么（相对 dsh 原版的差异）

1. 四个运行时脚本的默认路径从 `~/.dsh/...` 改为脚本所在目录（自包含；环境变量仍可覆盖）。
2. kit 版 `knowledge.db` 中 133 条 `poc.path` 绝对前缀从 `~/.dsh/redteam/pocs/` 改写为本目录 `pocs/`；
   其余约 14k 条为系统 nuclei 模板路径（`/usr/share/nuclei-templates`，与 dsh 无关）。
3. `rtis.py` 的 ERI resolver 默认指向本目录 `eri_adapter.py`（M8 后默认 rp 模式）。
4. `sync-roles` 的 store-core.js 模板缺省不存在则安静降级；需要时用 `RTIS_STORE_CORE` 指回。
