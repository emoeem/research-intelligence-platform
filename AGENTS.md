# RIP: Research Intelligence Platform（智能体入口）

任何智能体（zcode / Claude Code / Codex / Cursor / 其它）加载本仓库，按此文件行动。
当前状态与里程碑全量记录见 `docs/SESSION-HANDOFF.md`；平台总览见 `README.md`。

## 进门三件事

1. **先挂红队知识库**（本仓库自带，无任何外部依赖）：

   ```bash
   cd integrations/rtis && ./bootstrap.sh    # 重建 knowledge.db（不入 git）
   ```

   然后要么把它挂成 MCP（`rtis_mcp.mjs`，见 `integrations/rtis/README.md`），
   要么直接用命令行：

   ```bash
   node integrations/rtis/rtis_kb.mjs <<< '{"cmd":"search","q":"目标特征词","limit":10}'
   python3 integrations/rtis/rt.py poc search --q <组件名>
   ```

   纪律：动任何目标前先查库；命中就取全文照做；打完回填（`rt.py poc add/use`）。

2. **接任务先看交接**：`docs/SESSION-HANDOFF.md` 是唯一权威状态源（M0-M8 全程记录）。
3. **数据分级**：`knowledge.db`、`platform*.db`、凭据类证据一律不入 git（.gitignore 已配）。

## 仓库地图

| 路径 | 内容 |
|---|---|
| `integrations/rtis/` | 自包含红队知识栈（引擎/CLI/编排/MCP/125 技能/POC 落盘），2026-11-09 起本仓库为唯一活体 |
| `crates/` + `workers/` | Rust 核心 + worker（cargo 走 rsproxy 镜像，离线 vendor 可构建） |
| `migrations/` | ETL（RTIS/ERI 源库到 platform.db），快照在 `sources-snapshot/` |
| `apps/` `models/` `schemas/` | 应用层 / 数据模型 / schema |
| `legacy/` | ERI P0-P2 与 architecture-2.0 的历史交付（只读） |
| `docs/` | 里程碑报告 + SESSION-HANDOFF.md |

## 与 dsh 的关系（2026-11-09 解耦）

红队知识栈已从 `~/.dsh/redteam` 整体迁入 `integrations/rtis/`（默认路径全部自包含）。
`~/.dsh/redteam` 冻结不再更新；全量备份 `~/backups/dsh-redteam-20261109-094341.tar.zst`。
临时回旧布局：`RTIS_HOME=~/.dsh/redteam`。
