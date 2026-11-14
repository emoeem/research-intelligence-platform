# 2.0 计划 §17 最终验收标准全绿自查（收尾闭环，2026-11-14）

> 口径：逐条对账 docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md §17 的 14 条判据；
> 每条给证据指针（阶段 RELEASE 与可复跑命令）。修复循环产物：本批同入库的
> strict JSON 修复（HANDOVER §七遗留项清账）与文档统一（README 与 M8_READINESS）。
> 实战校验三面与 ChatGPT 审阅步骤按 2026-11-14 用户裁决删除暂缓，不在本轮。

| # | §17 判据 | 结论 | 证据 |
| --- | --- | --- | --- |
| 1 | 可以作为独立研究平台启动和使用 | ✅ | fresh clone 独立部署演练（P10-4 报告第五节）：真 git clone 后 INSTALL.md 第二节全量离线门 644 passed exit 0 + 数据面初始化 + 冒烟；guides/INSTALL.md 部署双形态（前台与 systemd） |
| 2 | Web Workbench 能管理项目、知识、工具、插件、Agent、Workflow、Evidence 和 Runtime | ✅ | 十一工作区全实页（Phase 5 六页 + P7-1 五区壳层 + P7-7 AgentsPage + P8-4 WorkflowsPage 画布）；浏览器实测证据链 docs/phase5/、phase7/browser-e2e/、phase8/browser-e2e/、phase9/browser-e2e/、phase10/browser-e2e/；视觉门 71 用例当日复测全绿 |
| 3 | 知识库能够检索、追踪来源、治理质量和关联证据 | ✅ | search-engine 检索 + SK-1 到 SK-3 三源接入与治理双闸（RELEASE_2026-11-12_SK-SECURITY-SOURCES.md）+ rp-review 治理回路（fresh clone 演练场景 3 实测）+ citations 与 Evidence 关联（Phase 4/7） |
| 4 | Tool Registry 能发现工具、读取 Schema、管理版本和健康状态 | ✅ | tool-registry 版本化登记（validate.rs 九条）+ rp-tool validate/register/list + rp-tool-health 探针 + /api/tools 与 MCP 目录同源露出（Phase 9 状态卡）；本批新增三工具走完整路径实测（收尾武器库批） |
| 5 | Tool Engine 能把任务派发到专用 VM，并由 Podman 隔离执行 | ✅（VM 面按 ADR §3.1 挂起口径） | podman_host 容器隔离执行全链（Phase 2 起四镜像族 digest 台账，cap-drop ALL/keep-id/挂载白名单）+ 双 Runner 故障转移（P10-3 multi_runner_failover）；VM 面按 TOOL-RUNTIME-ADR §3.1 挂起（S1 到 S4 解除条件），podsman_host 为现役执行面，属计划内裁决非缺口 |
| 6 | AI 不需要在 CachyOS 宿主机安装工具或依赖 | ✅ | 宿主机零安装红线全程（npm 只进 web/、cargo 用户级、PW_CHROMIUM 用户缓存、cargo-deny 用户级）；selftest 宿主机快照 diff 核对；工具执行全容器内（network.mode=none / egress_allowlist） |
| 7 | 工具执行有统一 Run、Artifact、Evidence 和日志记录 | ✅ | tool_run/artifact/evidence 统一登记（Phase 1/7 同一登记实现）+ 六 ID trace 贯穿（P7-5，015）+ rp-admin backup/restore 面（P10-1） |
| 8 | 插件可以动态发现、安装、启用、禁用、升级和卸载 | ✅ | Phase 6 全回路真实 API 实测 + P10-2 信任级四档确认强度 + D10 自动更新策略 + 升级失败自动回滚与镜像 digest 回滚与回滚窗口清理三实演 + catalog 发布形态（manifests/plugins/catalog.json 本批入库） |
| 9 | 知识与工具说明按任务动态加载，控制 Token 消耗 | ✅ | Context Broker L0 到 L3 四级装配与预算降级（Phase 4，est_tokens 校准留痕）+ MCP 目录面按需 Schema（rip.get_tool_schema，计划 §11:658 口径） |
| 10 | Agent 可以结合知识库与工具库完成多步骤任务 | ✅ | Phase 7 Agent Engine（AgentStore + CAS 状态机 + 013 三类 Memory + 014 九类护栏 + capability_refs 校验）+ P8-3 Agent 节点执行器接入 Workflow（真实闭环 selftest_phase8 77/77） |
| 11 | Workflow 可以可视化编辑和运行 | ✅ | Phase 8 Workflow Studio 画布五区（拖拽/连线/保存/运行/暂停/恢复真实链路截图）+ 016-018 三迁移版本化/checkpoint/控制面 |
| 12 | 外部 AI 即使没有自己的工具，也可以通过 MCP 使用 RIP 的工具装备 | ✅ | Phase 9 rmcp 官方 SDK 迁移后全链（fresh clone 演练场景 2：initialize → tools/list 六条全含 inputSchema → run_tool SUCCEEDED 带 artifact/evidence 引用）+ EXTERNAL-AI-QUICKSTART.md 八步配方 + rp-mcp-parity 常驻 CI |
| 13 | Rust Core、Python Workers 和现有 RTIS 集成保持清晰边界 | ✅ | crate 边界（workspace 十六 crate 单一职责）+ worker 契约面（Worker Protocol v1 + 契约测试）+ RTIS 集成为只读接入（integrations/rtis 零改动红线，RTIS 生产行为不因平台变更） |
| 14 | 具备备份、恢复、升级、兼容性与故障诊断文档 | ✅ | docs/guides/ 四文档（INSTALL/BACKUP-RESTORE/MIGRATION/UPGRADE）+ CLI-REFERENCE.md + EXTERNAL-AI-QUICKSTART.md + ADDING-A-TOOL.md；fresh clone 演练以文档为脚本实跑验证（P10-4 报告第三节修正记录 5：文档口径正确性实测） |

## 修复循环产物（本轮）

1. **strict JSON 重复键拒绝（HANDOVER §七遗留项清账）**：`tool_registry::strict_json`
   新模块（serde 数据模型递归 + 每层键集合查重，零新增依赖），tool manifest JSON 通道
   （rp-tool parse_manifest，egress_allowlist 配置所在面）与 plugin manifest JSON 通道
   （plugin-registry parse_document）双接线；单测锁定（tool-registry 5 用例 + plugin-registry
   json_duplicate_key_rejected 含嵌套 network 块形态）；存量全量 manifest 回归零拒绝
   （26 份工具 Manifest：manifests/tools 18 + 三镜像族 8 + 三插件 manifest 全过）。
2. **README 与 M8_READINESS 状态统一（HANDOVER §七遗留项清账）**：README 升到 2.0 现状口径
   （Phase 0 到 10 全交付 + 架构图补全 2.0 crate 面）；M8_READINESS 加历史快照注记
   （第 5 条限制被 M7 鉴权与 2.0 WebUI 关闭、第 4 条已关闭留痕、1/2/3 条仍为现状如实声明）。
3. **已知遗留台账更新**：rtis.py 自测复测现值 10 过 7 败（原台账 4 败口径有漂移，RTIS 侧
   非平台引入，见下节）；视觉回归 Playwright 基线已建（P10-4 47 张）从遗留清单勾销。

## 已知遗留（非阻塞，如实登记）

- **rtis.py 自测 10 过 7 败（2026-11-14 复测）**：原台账为 4 败（T2/T4/T11/T13）；本轮复测
  17 项中 7 项不过（含 T1 站型识别、T3 路由等）。属 RTIS 侧（integrations/rtis/）测试与
  环境数据面（知识库状态、engagement 源数据在位性）问题，非平台引入（平台五门禁不含该
  自测；SK-SECURITY-SOURCES 批次的 RTIS 15155 对账三批后代码态复跑逐数一致不受影响）。
  处置：留 RTIS 侧清账批次，接手方按 integrations/rtis/ 自查路径推进，不在平台收尾循环内。
- **license/maintenance 轴数据源未接**（M8_READINESS 限制 3 的存留部分）：RTIS 知识轴的
  数据源接线项，同属 RTIS 侧清账候选。
- **M8_READINESS 限制 1/2**（evidence 抽样 4/26、research_task 核心字段映射）：环境面限制
  （26 个活动源库本机缺失，P10-1 报告同口径声明），ETL 全量批路径与文档已具备。
- **CI 无浏览器面**：视觉门与基线断言本地执行（P10-4 已知取舍 1，config 注释口径）。
- **服务端插件来源拉取完整链路**：需真实上游可达（P10-2 已知限制 1）。

## 结论

§17 十四条判据全部有实证对账（✅），其中判据 5 的 VM 面为 ADR §3.1 冻结裁决的挂起口径
（非缺口）。修复循环产物两项入库（strict JSON + 文档统一），已知遗留逐项如实登记且均
不在平台五门禁面上。收尾闭环按修订版口径（无外部 AI 询问环节）完成本轮，实战校验三面
待用户启动。
