# 双 MCP 边界：kit 层 rtis_mcp.mjs 与 Phase 9 平台 MCP 面（P6-6）

| 项     | 值                                                                                              |
| ------ | ----------------------------------------------------------------------------------------------- |
| 状态   | 生效中（P6-6 批次内落稿；Phase 9 实施时随 P9-1 复核）                                            |
| 日期   | 2026-11-11                                                                                      |
| 基准   | ROADMAP §7.5.4（双 MCP 并存混淆风险）、§10.3 P9-1、PLUGIN-CONTRACT §3.4、TOOL-INVENTORY §5、盘点 §5 |
| 相关   | docs/phase6/PHASE6-PLAN.md、docs/phase0/PHASE-ROADMAP.md、schemas/plugin-manifest/examples/project.rtis-mcp.yaml |

## 一、两个 MCP server 是什么

| 维度          | kit 层：rtis_mcp.mjs（P6-6 接入对象）                                         | 平台层：rp-mcp（Phase 9，P9-1 交付）                                    |
| ------------- | ------------------------------------------------------------------------------ | ------------------------------------------------------------------------ |
| 定位          | RTIS kit 自带的 MCP server，让任何 MCP 宿主直接用 RTIS 知识库引擎              | RIP 平台自身的 MCP server，适配 Application API                          |
| 代码归属      | integrations/rtis/rtis_mcp.mjs（kit 资产，动态 import rtis-bridge.js）         | crates/mcp-server（P9-1 新建，代码独立，以 rtis_mcp 形态为参考不复制）   |
| 工具命名空间  | redteam_* 前缀（redteam_poc_search 等）+ rtis_knowledge / rtis_task            | rip.* 前缀（计划 §11:640 到 656：17 个 tool 与 7 个 resource）           |
| 数据面        | 直连 kit knowledge.db（RTIS 引擎自持）                                         | platform 数据面（经 Application API，不直连库）                          |
| 鉴权          | 无（stdio 本机进程，无网络面）                                                 | rp_server 鉴权基准（Bearer + 常时比较 + 节流），MCP 握手鉴权补齐         |
| 协议          | stdio NDJSON JSON-RPC 2.0（MCP 2024-11-05）                                    | stdio JSON-RPC 2.0（P9-1 定稿；目录分页 + 按需发现，计划 §11:658）       |
| 生命周期      | RTIS kit 自持；bootstrap.sh 重建 knowledge.db；RTIS_RESOLVER_MODE 三态回滚不动 | 平台发布节奏（P10）                                                      |

## 二、P6-6 的接入方式（本批落地）

rtis_mcp.mjs 以 **project_endpoint capability** 接入（不是被重写、不是被合并）：

1. `project.rtis-mcp` 插件（schemas/plugin-manifest/examples/project.rtis-mcp.yaml）：kind=project，source=local_dir integrations/rtis，capability type=project_endpoint。
2. install 握手阶段对 rtis_mcp.mjs 走 initialize → notifications/initialized → tools/list（stdio，CWD=安装副本），实测 10 个工具（redteam_poc_search/redteam_poc_get/rtis_knowledge/redteam_kb_stats/redteam_skill_list/redteam_skill_get/redteam_poc_add/redteam_poc_use/redteam_poc_update/rtis_task），快照 sha256 进 plugin.lock.negotiation。
3. 能力映射：每个 MCP 工具映射为 **mcp_client runtime 的 Tool 行**落 Tool Registry（id = `rtis.mcp.{tool_name}`，provenance origin=internal-kit，permission_level=T1），tool-registry query 可查。
4. kit 本体零改动：RTIS 红线全程生效（rtis_mcp.mjs 与 knowledge.db 哈希不变，P6-5 §四同款核查）。

## 三、边界规则（防混淆的硬约束）

1. **命名空间隔离**：kit 工具在 Tool Registry 中以 `rtis.mcp.*` 前缀落行（映射副本），平台工具以 `rip.*` 前缀；两个前缀永不混用。外部 AI 经 Phase 9 平台 MCP 面看到的目录只有 rip.* 工具；kit redteam_* 工具面只经 rtis_mcp.mjs 原生通道（或经 mcp_client runtime 的 Tool 行间接调用）。
2. **不是平台面的一部分**：rtis_mcp.mjs 不进 Phase 9 的 rp-mcp 目录、不承担 rip.* 语义；它是 project_endpoint 接入对象（外部完整项目接入点，PLUGIN-CONTRACT §3.4），其生命周期跟随 project.rtis-mcp 插件（disable 即从匹配面撤出，kit 本体不动）。
3. **代码独立**：平台 rp-mcp 以 rtis_mcp.mjs 为形态参考（stdio JSON-RPC、目录分页、按需发现），代码不复制、不 import。
4. **数据面隔离**：kit MCP 直连 kit knowledge.db（RTIS 引擎自持）；平台 MCP 只走 Application API。跨面知识流动只经 P6-5 摄取管道（knowledge_ingest 表，plugin_id 溯源）。
5. **鉴权差异**：kit MCP 无鉴权（stdio 本机）；平台 MCP 继承 rp_server 鉴权基准并补 MCP 握手鉴权（P9-1 验收项）。两者暴露面不同等级，不得互相替代。

## 四、验证口径（本批实测）

- install project.rtis-mcp exit 0（真实 rtis_mcp.mjs 握手，10 工具快照进 lock）。
- tool-registry 只读对账：13 行（3 个 tool.rip.* worker 工具 T0 + 10 个 rtis.mcp.* mcp_client 映射 T1），tool_version 13 行。
- 测试面：project_endpoint_handshake.rs（fixture stdio 握手 3 项）与 tool_plugin_registration.rs（5 项）。

## 五、移交

- P9-1 实施时复核本文件：rp-mcp 目录定稿后，在此追加「平台面最终词表」一节并对照 kit 前缀隔离再次确认。
- kit MCP 的调用链压测与目录分页（外部 AI 视角）属 P9-2 提前通道。
