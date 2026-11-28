# SESSION HANDOFF：跨会话交接文档（RIP 主线）

更新：2026-11-28　用途：新会话接续执行的唯一状态源。配合自动加载的记忆索引（MEMORY.md）使用。

## Quality remediation status（2026-11-23 续批）

M0-M5 quality remediation has implementation and local verification evidence in
`docs/quality-audit/`. Final report status is DONE for executable local items.
Key additions: PostgreSQL real integration/concurrency/fault/backup gates,
Source Monitor pinned-IP SSRF implementation, workspace predicates, repository
security scan, browser CI gate, SQLite smoke, `/api/metrics`, and synchronized
mobile visual baselines. No commit or push was created in this remediation batch.

## RIP3 最新状态（2026-11-23）

RIP3 Phase 7 与 Phase 8 已完成实现、阶段验收、Git 提交和推送。阶段实现提交为 `0229bf01f1cc133e997f5bd644221c88da95de18`；当前 `main` 与 `origin/main` 已同步，文档状态以本文件最新提交为准。GitHub Actions [Run #180](https://github.com/emoeem/research-intelligence-platform/actions/runs/45141019769) 已完成，4/4 Jobs success。

- Phase 7：新增 PostgreSQL V0009/V0010 与 Rust `intel-db::maintenance`，完成 Report IR Schema 校验、Finding→候选 KnowledgeCard、六 ID Evidence 溯源、候选 promote/reuse/reject 人工审核、workspace 隔离、来源 ETag/Last-Modified/SHA-256 监测、失效链接检查、RTIS POC 只读新鲜度检查、质量巡检和维护报告。RTIS 生产库未写入、POC 正文未复制。
- Phase 8：PostgreSQL 17.11 与 SQLite 3.54.1 完成 100k 文档 / 1M Chunk / 4k 运行事件对照；导入、索引、检索、内存、存储、并发、迁移、重建、备份恢复均有原始报告。决策：运营核继续 SQLite，不切库。
- 阶段文档：`docs/rip3/PHASE-7-KNOWLEDGE-FEEDBACK.md`、`docs/rip3/PHASE-8-SCALE-TESTING.md`、`docs/rip3/RIP3-ARCHITECTURE-UPGRADE-PLAN.md`。
- 本地验收：`apps/server/selftest_p7p8.sh` 通过；intel-db 单元 32、PostgreSQL 集成 4、Python Worker 单测 4、来源监测 PG smoke、RTIS 临时只读 fixture smoke、PG/SQLite 100k/1M 基准与 Clippy 均通过。全 Workspace 串行回归 1,396 passed / 0 failed / 3 ignored；全 Workspace Clippy 通过。
- 远程 CI #180：Web 构建与 Vitest、Python worker、Rust workspace 测试与 Clippy、REST/MCP 协议一致性四个 Job 全部 success。**覆盖边界**：CI 未配置 PostgreSQL service / `RP_TEST_PG_DSN`，所以 `phase7_maintenance` 的四个 PostgreSQL 集成测试在 CI 中会因 DSN 缺失提前返回；本地专用 PostgreSQL 容器实跑 4 项通过。Phase 7 PG/RTIS smoke 与 Phase 8 规模基准仍以本地隔离环境验收为准，不能将其描述为 CI 覆盖。
- 生产 `platform*.db`、RTIS `knowledge.db` 未修改。专用测试 PostgreSQL 容器 `rip-p7p8-pg` 保持运行，供后续复验；它不是生产数据库。

## 一、授权口径（用户三轮授权，已生效）

1. 项目内全部决策自主推进，无需逐步请示（2026-11-06）。
2. 目标 = 完成主计划书 /home/emo/Downloads/RESEARCH_INTELLIGENCE_PLATFORM_MASTER_PLAN.md 的全部内容（M1→M8）。
3. M8 切流为条件授权：四条硬检查全绿才切（对账 100% / 影子运行差异已解释 / 回滚演练成功 / 切换前快照完备）。
4. 每完成一个阶段目标 → commit + push 到回滚仓库（外发授权已给）。
5. 仍不动：engagements 靶标数据、runs/、knowledge.db 生产库本体（只读）、攻击证据。

## 二、回滚仓库（已建立）

- https://github.com/emoeem/research-intelligence-platform （private，gh 认证可用）
- 本地仓库根：/media/emo/code/pensting/research-platform/（main 分支；当前设备挂载路径）
- 结构：probe-cargo/（DEC-001 验证工件）、migrations/（M1 产出）、legacy/eri + legacy/architecture-2.0（rsync 自 security-research-upgrade/，排除 db/cache/tarball）、db-snapshot.sql（catalog 快照）
- 首次基线：a87d82b（F0/M0 基线）
- 阶段推送节奏：rsync 同步 legacy → git add -A → commit（中文，注明阶段与验证结果）→ push origin main

## 三、RIP 当前进度

- **授权补充（2026-11-06）**：可用任何技能/资源；容器隔离（docker 29.9.2 active，本机可用）；完成后清理临时内容；**质量目标 = 专业级可商业化**（这改变 M2+ 的工程标准：clippy 零警告、CI 容器可复现构建、测试金字塔、语义版本、文档与许可证完备）。
- 容器策略（用户指定 podman）：M2 起 Rust 构建与测试跑在 podman 容器（6.1.4 rootless，已验证 alpine 秒级运行；Containerfile 固化工具链+vendored 依赖）；Python Workers 同理；宿主机只留源码与产物。清理纪律：临时容器/镜像用完即删（podman rm -f / rmi），只保留 Containerfile 与 CI 配置。不用 docker。

- F0/M0：完成（13 份报告在 security-research-upgrade/architecture-2.0/）。
- M1 就绪评审：完成，判定 ready。产物在 architecture-2.0/m1/（READINESS_REVIEW / DECISION_LOG / DATA_BASELINE_SNAPSHOT / IMPLEMENTATION_PLAN）。15 条 M0 矛盾修正意见在评审报告里，实施必须遵守。
- M1 第一批实施：**后台 agent 进行中**（2026-11-06 01:1x 启动），产出 migrations/001_init.sql + etl_dryrun.py + verify.py + platform-dryrun.db + M1_DRYRUN_REPORT.md，agent 会自行推送。验收点：verify 一致率 100% 红线、幂等两次执行、只读源库。
- rtis-bridge.js：已修复（复制备份到 security-research-upgrade/rtis/tools/，MCP 检索实测恢复 ok:true）。R-02 关闭。
- 关键实测数：资源 81；D5=68（63 待人工+5 相关保留）；28 库=26 兼容/2 隔离；kb verified=130（0.88%，"13"是 M0 笔误）。
- DEC-001（Rust）：验证通过。本机 cargo 走 rsproxy.cn 镜像，在线 9.6s / vendored 离线 8.7s 构建全通（见 research-platform/PROBE-RESULT.md）。M2-A 前置已满足。
- **M8 切换已执行（2026-11-06，eff2918）**：RTIS_RESOLVER_MODE 默认 rp + platform.db 生产库 + 双路拆词修复。Flutterfox 授权更新：后续（含 Phase 7/8 移动端）不受 Rust 约束，能用非 Rust 方案完成即可。
- 下一步序列：M1 第一批 → 第二批 → M2-M8 全部完成（见 git log a87d82b..eff2918）。
- **RIP 主线收口（2026-11-06 23:5x 复核）**：M8 切换后重跑 verify.py，PASS 一致率 100% exit 0。
- **尾巴进展（2026-11-07 凌晨）**：CI 流水线已建（7d84c89 + gitignore vendor 修复 08f178d，Actions 全绿：Rust 69 测试+clippy 零警告+worker 109 测试）。M1 全量批已交付（f1de6ae + 评审修复 6da1d5e）：evidence 26 兼容库全量 700 行、research_task 大 JSON 126/126、verify 扩展 203 项 100% PASS、幂等 no-op、源库零写入；platform.db = fullbatch 生产批；M8_READINESS 限制 1/2 关闭。注意：platform-fullbatch.db 含凭据类证据，已移出版库（gitignore），评审时抓到。
- **剩余尾巴**：license 轴数据源（低优先）。真实 embedding 已收口（ae705b3 + 守卫评审修复 2ffd635）：bge-small-zh-v1.5 ONNX（ModelScope，94.8MB，sha256 校验），512 维显式协商，回退语义 backend 字段，125 测试双口径验证；**至此"彻底部署上线"四缺口（M1 全量批 / CI / M7 鉴权服务化 / 真实 embedding）全部关闭**，工作台本机常驻 active 且生产数据在线。M7 细节：08846d7（鉴权 11 项自验 + systemd user 服务）+ e5abdf1（harness flake 根治）。
- **RIP 2.0 程序启动（2026-11-09）**：计划书 docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md（架构规划稿，基于本机仓库只读检查，未动源码与生产库）。Phase 0 只读审计完成，八份交付物在 docs/phase0/：CURRENT-ARCHITECTURE-INVENTORY、TOOL-INVENTORY、TOOL-RUNTIME-ADR、TELEGRAM-ADMIN-UI-REUSE、PLUGIN-CONTRACT-DRAFT、TOOL-CONTRACT-DRAFT、CONTEXT-BROKER-DESIGN、PHASE-ROADMAP。后续按 docs/phase0/PHASE-ROADMAP.md 的依赖冻结顺序推进（Phase 1 Tool Contract 与 Registry 起步，详见该文件依赖图与阶段边界）。
- **Phase 1 完成（2026-11-09），验收通过**：`cargo test -p tool-registry -p tool-engine -p domain -p database --offline` 实测 105 测试全绿（0 失败）。关键落地物：Tool Contract schemas（schemas/tool-contract/request-v1、result-v1 + manifest v1）、tool-registry crate（manifest 校验/注册/哈希/目录）+ tool-engine crate（envelope、python worker 适配、mock runner）、migrations 005_tool_contract + 006_agent_data_plane、manifests/tools/ 首批 28 份工具清单（json/yaml 双份）、worker 脚本适配（scripts/rip_cli_worker.py、rtis_kb_worker.py）、vendor 离线依赖随 Cargo.lock 重生成、boundary 裁决文档 docs/phase1/T1-T2-BOUNDARY-RULING.md。
- **Phase 2 完成（2026-11-10，podman_host 起步拓扑），验收通过**：本会话三道门禁实测 cargo test `--offline --workspace` 194 全绿（0 失败 0 忽略）、clippy `-D warnings` 零告警、worker unittest 125 全绿（跳过 11）。关键落地物：podman_host 执行通道（crates/tool-engine/src/adapter/podman_host.rs，挂载白名单 + keep-id + cap-drop ALL + digest 锁定）、runtime-gateway crate（心跳/健康落库/WAITING_FOR_RUNNER/submit_key 幂等重放）、migrations/007_runtime_node.sql、containerfiles/rip-tool-base 冒烟镜像族、五项故障演练自动化（recovery_drill 3/3）；VM 面按 ADR §3.1 挂起（S1 到 S4，解除条件见 PHASE2-DESIGN §3.2），更新记录 docs/releases/RELEASE_2026-11-10_PHASE2.md。
- **Phase 3 完成（2026-11-10），验收通过**：本会话三道门禁实测 cargo test `--offline --workspace` 207 全绿（Phase 2 基线 194 + 13 新用例）、clippy `-D warnings` 零告警、worker unittest 125 全绿（跳过 11）。关键落地物：首批 Tool Packs 两包六工具（rip.http.inspect + rip.core 五工具，容器化登记与 Gateway 全链端到端，Artifact sha256 双向对账）、egress 白名单代理（network.mode=egress_allowlist 真实生效，跨 host 重定向拒）、pack-plan v1 Schema + 五份 planned 存根（三包 + TPscan + Browser Automation，解锁条件随存根）、TOOL-CONTRACT digest 占位全部回填（V-10 关闭）、零新增迁移；更新记录 docs/releases/RELEASE_2026-11-10_PHASE3.md。
- **Phase 4 完成（2026-11-10），验收通过**：本会话三道门禁实测 cargo test `--offline --workspace` 273 全绿（0 失败 0 忽略）、clippy `-D warnings` 零告警、worker unittest 125 全绿（跳过 11）。关键落地物：crates/context-broker（L0 到 L3 四级装配、七分项预算与降级驱逐、citations 引用集、内容寻址缓存 + 七类失效触发器、六源 ContextSource 端口）、migrations/008_context_broker.sql 三表、确定性意图面（rulepacks/rip-rules 版本化配置 + classify 纯函数）、rp-context 验收 CLI 四子命令、est_tokens 校准留痕（中位数 0.9333）；验收三条（不默认塞入/可解释选择/Artifact 引用）全部有测试落证；更新记录 docs/releases/RELEASE_2026-11-10_PHASE4.md，实施差异全量见 docs/phase4/PHASE4-IMPLEMENTATION-DIFF.md。
- **Phase 5 完成（2026-11-11），验收通过**：Web Workbench 基础版五页面接真实 API（Overview/Knowledge/Tool Center/Runs/Runtime 五页走本阶段新增五个只读端点 + Projects 最小版走既有 /api/tasks，全局搜索 ⌘K 接真实后端）；门禁当日复测全绿（cargo test 273 全绿、clippy 零告警、worker unittest 125 全绿跳过 11、web 五门禁 typecheck/build 17 chunk/vitest 69 用例/视觉门 30 用例/prettier 全过、selftest 31+11 零 FAIL）。关键落地物：web/ 独立 npm 工程（React 19 + Vite 8 + Tailwind 4 + daisyUI 5，六路由 lazy + vendor 分组 + 双主题双视口）、apps/server 五端点 + RP_AUX_DB 只读 ATTACH 通道、focus-trap 等基础件与 telegram-bot admin 同构、useApi 迁移 @tanstack/react-query（对外形状不变）、egress 代理三处时序竞态修复与外网瞬断重试；更新记录 docs/releases/RELEASE_2026-11-11_PHASE5.md，阶段验收 docs/phase5/PHASE5-ACCEPTANCE.md。
- **Phase 6 完成（2026-11-12 收口入库），验收通过**：Plugin Runtime 与 Plugin Center 全交付（P6-1 到 P6-9 含武器库扩展路径）：插件生命周期全回路经真实 API 实测（install/enable/disable/upgrade 0.1.0→0.2.0/ingest/uninstall）、两插件接入（knowledge.rtis-redteam + tool.rip.python-workers + project.rtis-mcp 三 capability 形态）、Plugin Center 网页四级信息（四分类目录 + 三级加载 + 审计 trail + 写操作）、/api/plugins 读写端点经 rp-plugin 子进程零直连 SQL 写；门禁当日复测全绿（cargo test 379 全绿、clippy 零告警、worker unittest 125 全绿跳过 11、web 五门禁 tsc/build 19 chunk/vitest 73 用例/视觉门 30 用例/prettier 全过、selftest 31+49 零 FAIL）；浏览器实测三面证据（docs/phase6/browser-e2e/ 独立验收 7 截图 + selftest 内嵌真实后端 E2E 4 截图 + mock 视觉矩阵 8 passed）；收口当日修复三处（workers 守卫 untracked 误报、selftest_phase6 夹具残留误报、prettier 六文件）；更新记录 docs/releases/RELEASE_2026-11-12_PHASE6.md，阶段验收 docs/phase6/PHASE6-ACCEPTANCE.md。
- **SK 安全知识源接入 SK-1 到 SK-3 完成（2026-11-12 收口入库）**：SK-1 通用 Knowledge Catalog/Document 摄取接线（`9058c93`，代码半成品基线 `fec5a3f` 未推翻；011 迁移 + 检索第三源/context-broker PluginSource/Evidence 固化接线，RTIS 15155 对账复跑收口）→ SK-2 三源知识来源插件化（`f77e7b5`，Awesome-Hacking 85 / SecSkills 102 / tongui 75 共 262 条目录元数据按固定 Commit 440486f/1867eb6/45a12a7 快照接入，CC0-1.0/Apache-2.0/UNLICENSED，APPROVED 双闸进检索面 262→160 禁用撤出实验，tongui 正文本体零落库）→ SK-3 目录抽取与静态审查四件工具（`ff9e93e`，catalog.extract/extension.audit/document.fetch/source.sync + 012 抓取审计表 + 六类恶意 fixture 拒绝矩阵，unittest 211 = 128+37+46）→ 收尾入库批（SK 扩展入库员：V-9 podman 缺席门禁修复 crates/tool-engine/tests/podman_host_egress.rs:287/:311 使 podman_host_egress 4 项转绿 + skverify 独立复测截图四张 docs/phase6/browser-e2e/skverify-01 到 04 + 三道门禁全量复跑全绿（cargo test --offline --workspace 430 passed/0 failed、clippy -D warnings 零告警、unittest 211 OK skip 11）+ RTIS 15155 对账三批后代码态复跑逐数一致（首跑 14717+125+313/bytes 6286781，二跑 skipped 15155 changed 0，knowledge.db sha256 f8580feb 前后一致）；总账 RELEASE docs/releases/RELEASE_2026-11-12_SK-SECURITY-SOURCES.md；生产 platform*.db 与 RTIS 生产文件全程零写）。
- **Phase 7 完成（2026-11-13 阶段收口入库；commit 与 push 短哈希待回填，按仓库回填惯例以独立 docs 批次补记，先例 05ed5ee/f02d9b3）**：七批 P7-1 到 P7-7 全交付（编排运行 dwfrun-954913b0-9e59-48cd-a2af-1202026edf95，2026-11-12 起跑批次 1 到 4 半成品 WIP 备份 `7effba9` 已推送，跨机续作收口批次 5 到 7；运行已结束，无需再 Resume）。批次链：P7-1 WebUI 壳层升级（五区布局 + 十一工作区 + 多标签/Inspector/Activity 数据无关组件，既有 10 页路由不变迁移，截图 docs/phase7/browser-e2e/01/）→ P7-2 Agent Engine 最小执行闭环（AgentStore + CAS 状态机 + capability_refs 引用校验 + rp-agent CLI 定稿，RELEASE_2026-11-12_PHASE7-P7-2.md）→ P7-3 三类 Memory 与 Context Memory 雏形（013 三表 + 生命周期闸门 + FK 反向锚定，评审 docs/phase7/AGENT-MEMORY-REVIEW.md，RELEASE_2026-11-13_PHASE7-P7-3.md）→ P7-4 运行控制九类护栏与 AgentRun 持久化轨迹（014 两表，RELEASE_2026-11-13_PHASE7-P7-4.md）→ P7-5 trace 统一关联与跨栈查询（015 + 六 ID 贯穿三栈 + /traces 链路页，commit `2bd35a8` 已推送，RELEASE_2026-11-13_PHASE7-P7-5.md）→ P7-6 研究能力契约化（crates/research-engine 七能力 mock 纯函数 + 14 份 Schema + rp-research CLI，commit `2d3fda8`，RELEASE_2026-11-13_PHASE7-P7-6.md）→ P7-7 Agent Studio 页与轨迹读取面（/api/agents 与 /api/agent-runs 只读端点 + rp-agent run-steps + AgentsPage 实页三区 + 六 ID 跳转，commit `4f79e2b`，RELEASE_2026-11-13_PHASE7-P7-7.md）。阶段收口：总账 docs/releases/RELEASE_2026-11-13_PHASE7.md（批次汇总 + 五门禁数字 + trace 关联说明 + 截图证据链 + 已知取舍 + 回滚方式）；阶段级五门禁入库当日复测全绿（cargo test --offline --workspace 510 passed 0 failed 94 目标、clippy -D warnings 0 error、worker unittest 211 OK 跳 11、web typecheck/build/vitest 100 用例/prettier/视觉门 55 用例（PW_CHROMIUM 用户缓存 Chrome for Testing 151，零系统级安装）全过、selftest auth 11/11 + phase7_trace 33/33 + phase7_agents 50/50，生产库零写核对一致）；独立验收 browser-e2e/03/ 8 截图（PhaseAcceptance，独立于批次自验不复用批次数据）。已知取舍与 Phase 8 尾洞（研究能力真实接线 / rp-agent with_memory / /traces 深链切换器「总览」预置表现）见总账第五节。
- **Phase 8 完成（2026-11-14 阶段收口入库；commit 与 push 短哈希待回填，按仓库回填惯例以独立 docs 批次补记，先例 05ed5ee/f02d9b3）**：四批 P8-1 到 P8-4 全交付（P8-1 到 P8-3 代码面随编排层 WIP 备份 `7707d13` 入库，2026-11-14 00:07 配额停跑纪律收口点；P8-4 web 终态与阶段总账随本收口提交入库）。批次链：P8-1 Workflow DSL 与定义版本化（definition-v1 + draft 2020-12 Schema 门 + 图静态校验纯函数 + 016 版本化存储 + rp-workflow CLI 五子命令 + 存量 003 表 13 行 legacy-method-reference 只读参考导入，RELEASE_2026-11-13_PHASE8-P8-1.md）→ P8-2 执行引擎与非智能节点 + 运行状态持久化（017 三表 workflow_run/workflow_run_node/workflow_checkpoint + Tool/Knowledge/Condition/Parallel 四执行器 + CLI run/status/nodes/resume/retry/cancel + selftest 46 断言，RELEASE_2026-11-13_PHASE8-P8-2.md）→ P8-3 控制面与智能节点 + 六 ID 贯穿 + API 写面（018 控制表与关联列 + Agent 节点执行器（agent-engine AgentLoop）+ human 人工介入第 8 词值 + 协作暂停/取消传播 + CLI pause + rp_server 读写面（POST 一律 rp-workflow 子进程零直连 SQL 写）+ /api/traces 双向 + selftest 76 断言，RELEASE_2026-11-13_PHASE8-P8-3.md）→ P8-4 Workflow Studio 画布页五区壳层（@xyflow/react 12.12.1 exact + vendor-xyflow 独立分组 + NodePalette/FlowCanvas/NodeInspector/RunPanel/VersionList 五区实页 + 画布模型纯函数 + 保存 POST /api/workflows put + 运行控制接 P8-3 API（两处轮询缺陷实测修复）+ Bearer localStorage 最小桥 + playwright.config 机器无关浏览器探测，RELEASE_2026-11-13_PHASE8-P8-4.md）。阶段收口：总账 docs/releases/RELEASE_2026-11-14_PHASE8.md（批次汇总 + 五门禁数字入库当日复测 + 画布与 checkpoint 恢复说明 + 截图证据链 + 已知取舍 + 回滚方式）；阶段级五门禁入库当日复测全绿（cargo test --offline --workspace 582 passed 0 failed 84 目标、clippy -D warnings 0 error、worker unittest 211 OK 跳 11、web typecheck/build/vitest 111 用例（基线 100 + workflows-smoke 11）/prettier/视觉门 59 用例（PW_CHROMIUM 用户缓存 Chrome for Testing 151.0.7922.34，三级探测机器无关）全过、selftest_phase8_workflow.sh 76/76，生产 platform*.db 零写指纹一致）；浏览器实测证据链 docs/phase8/browser-e2e/（01 P8-4 批次自验六步：拖拽/连线/保存/运行停等/暂停/恢复，真实 tool python_worker + agent-engine 闭环；02 验收员独立复测五截图：空白画布建图保存运行回看闭环，独立于批次不复用数据）。已知取舍见总账第五节（面板数据转换/证据保存分类待 DSL 扩词、画布坐标不入 definition、API run/resume/retry 异步承载、收尾审阅全工作区 browser-use 复验待执行）。
- **Phase 9 完成（2026-11-14 阶段收口入库；commit 与 push 短哈希待回填，按仓库回填惯例以独立 docs 批次补记，先例 05ed5ee/f02d9b3）**：四批 P9-1 到 P9-4 全交付（ROADMAP §10.3 的 P9-5 Resources/Prompts 按批次编排并入 P9-4）。批次链：P9-1 MCP 目录发现面（crates/mcp-server bin rp-mcp stdio JSON-RPC 2024-11-05 子集：握手鉴权禁止匿名 + tools/list 分页 + 按需 Schema，与 Tool Center 同源，RELEASE_2026-11-14_PHASE9-P9-1.md）→ P9-2 rip.run_tool 端到端执行通道（tool-engine 唯一派发链 + Artifact/Evidence P7-2 同一登记 + rp-tool run/status/cancel 经 MCP 通道 + 运行库 RP_RUN_DB 隔离，RELEASE_2026-11-14_PHASE9-P9-2.md）→ P9-3 REST 契约化收口与 CLI 帮助面（trace 透出 + 全局限频 + 019 审计 + 契约文档机器对账 + 11 bin --help 统一，RELEASE_2026-11-14_PHASE9-P9-3.md）→ P9-4 双通道一致性载具（crates/rp-mcp-parity 五业务操作 REST/MCP 同断言常驻 CI parity job，ROADMAP §10.5 风险 1 收口）+ rip:// 七 resource 与 prompt 三条剧本（rip://runs/<run_id> 与 rip://evidence/<id> 外部 AI 实读验证）+ MCP 通道审计链自锚定（trace_id = run_id，015/018 同惯例）+ /api/mcp-catalog 与 Tool Center MCP 目录同步状态卡（web 四态卡 + 真实后端浏览器实测 4 截图 docs/phase9/browser-e2e/，RELEASE_2026-11-14_PHASE9.md 总账）。门禁：cargo test --workspace 608 passed 0 failed（603 + 载具 5）、clippy 零告警、unittest 211 OK 跳 11、web 五门禁全绿（vitest 115 = 111 + mcp-smoke 4、视觉门 59、prettier 全过）、selftest rest 33/33 + mcp 23/23 + auth 11/11，生产 platform*.db 零写指纹一致。
- **Phase 9 追记（2026-11-14 成熟库审计与 rmcp 迁移决策）**：外部审计（全文 docs/phase9/MCP_MATURE_LIBRARY_AUDIT_2026-11-14.md，公开仓库 main 只读审阅 + MCP 官方 SDK 核对）确认 MCP 手写协议层不可长期保留：P0 tools/list 条目缺标准必需 inputSchema 且 mcp_protocol.rs 有测试断言其不得出现（固化错误行为）、P1 initialize 直接回显客户端 protocolVersion 无协商、P1 自定义协议层承担协议兼容与安全边界维护职责。处置（用户定案「按照这个方案」）：rmcp 迁移作为**阻断性兼容性整改批次**安排在 Phase 10 前置（编排填充体的前置批执行，审计 §5 十项任务：最终树复核、rmcp 选型记录、增量 vendor、rp-mcp 改 rmcp server + stdio transport、目录投影补必需 inputSchema、保留 Tool Center 同源与单一派发、RP_MCP_TOKEN 安全边界重定义、真实客户端端到端、conformance 测试、离线验收）；迁移未落地前 Phase 10 不开批。成熟库优先硬约束（审计 §6）已并入 AGENTS.md。另：收尾闭环的 ChatGPT 全面审阅步骤经用户指示删除暂缓（闭环改为 2.0 计划 §17 全绿自查修复循环 + 知识库补齐 + 实战校验三面，全程无外部 AI 询问环节）。
- **Phase 9 追记（2026-11-14 rmcp 迁移整改批次已实施）**：审计 §5 十项任务在 main 工作树落地（选型与台账 docs/phase9/RMCP-MIGRATION.md）：rmcp 3.10.0（crates.io 发布版，=3.10.0 钉定，default-features=false 仅 server+transport-io）经 rsproxy 受控联网增量 vendor（Cargo.lock 纯增量 43 外部 crate，既有零升级，vendor 114->164 目录，.cargo/config.toml 已还原 vendored 口径），OSV 版本级实查锁定版本零安全通告；crates/mcp-server 手写协议层退役（lib.rs 重写 + 新增 service.rs ServerHandler 接入层 + bin 重写），P0 tools/list 标准 Tool 对象（每条必需 name+inputSchema，注册面 Schema 列直读，宽松 object 回退）、P1 版本协商（2024-11-05 至 2025-11-25 列表内协商回显，列表外回落 2025-11-25）、RP_MCP_TOKEN 重定义为宿主注入即授权边界（缺失/空白 exit 2 fail-closed，_meta.rp.auth 退役）；catalog/dispatch/resources/prompts 四业务模块零重写，tool-engine 单一派发与 EvidenceRegistrar 复用不变；rp-tool 客户端补 initialize 握手（rmcp 状态机要求）。实测：cargo test --offline -p mcp-server 18 passed（mcp_protocol 11 + run_tool_e2e 7）、rp-mcp-parity 5/5、selftest_phase9_mcp.sh 26/26（SELFTEST_PORT=7832，7800 被占）、真实 Node 客户端 e2e 10 步全过（docs/phase9/rmcp-client-e2e/ 留证）、生产 platform*.db 零写指纹一致。Phase 10 开批前提（迁移落地）已满足，待整批 cargo clean 干净态全量离线验收与提交。（收口注记：干净态全量离线验收由 Phase 10 阶段收口的 fresh clone 演练承载，真 git clone 于收口提交态复跑 cargo test --offline --workspace 644 passed 0 failed exit 0。）
- **Phase 10 完成（2026-11-14 阶段收口入库；commit 与 push 短哈希按仓库回填惯例补记，先例 05ed5ee/f02d9b3）**：rmcp 前置批入库（8adbf1c）后四批 P10-1 到 P10-4 全交付，原编排运行中断后的批次工作树由接手会话按 §二.3 纪律 WIP 备份（159f1b2）并完成收口复测与批次报告补写。批次链：P10-1 可靠性基座（rp-admin backup/verify-archive/restore/reten-archive 四子命令 + 恢复产物 sha 与 manifest 逐字节一致 + 恢复后 FTS rebuild 81/81 对账 + 019 保留归档实演 archived=2 deleted=2）→ P10-2 插件生态收口（trust_policy 四档确认强度 + D10 仅 official 自动提示 + catalog 目录发布/校验/按目录安装 + 升级失败自动回滚与镜像 digest 回滚与回滚窗口清理三实演 + TrustConfirmDialog 真实浏览器四档 4 passed 五截图）→ P10-3 运行与质量门（双 Runner 故障转移 4 用例 + T0/T1/T2 越界拒绝与 secrets 零泄露矩阵 7 用例进 CI + autoresearch 性能基准闭环 assemble_ms 2.399 / dispatch_ms 0.474 中位数基线）→ P10-4 发布与合规收口（cargo-deny 0.20.2 licenses ok + 164/164 Rust 第三方与 144 npm 实装包许可证清单 + axe 对比度清零转阻断（双主题 color-contrast 阻断，PHASE5 风险 5 缺口清零）+ 统一字体视觉基线 47 张 + INSTALL/BACKUP-RESTORE/MIGRATION/UPGRADE 四文档 + fresh clone 独立部署演练 §17 五场景全过）。阶段级门禁入库当日复测全绿（cargo test 644 passed 0 failed 116 目标、clippy 零告警、unittest 211 OK 跳 11、web build/vitest 120/prettier 全过、视觉门 71 passed 双主题 axe 阻断口径、cargo-deny licenses ok、selftest backup 16/16 + plugin_eco 52/52 + release 29/29 + auth 11/11、生产库五库指纹零写一致）；fresh clone 演练含全量离线门 644 passed 与备份恢复 roundtrip（恢复产物 sha = manifest db_sha256 逐字节一致）。阶段总账 docs/releases/RELEASE_2026-11-14_PHASE10.md；docs/README.md Phase 10 行登记。复测期并发负载 flake（cargo 1 例 + vitest 3 例）隔离/静场复跑全过，详见 P10-4 报告第三节。
- **收尾批次完成（2026-11-14 队列第 7 项，修订版闭环）**：知识库补齐（manifests/plugins/catalog.json 发布产物入库：catalog-publish 3 entries + content_sha256，verify 一致）；武器库补齐（source-analysis pack 冻结存根转正：containerfiles/rip-tool-source/ 三工具 deps-manifest/static-check/code-search（T1，network=none，纯标准库）+ 镜像族建成 digest sha256:8251ba32 台账三处同步 + rp-tool validate/注册//api/tools 露出实测 + src_analysis_e2e.rs 3 passed（V-9 digest 守卫）+ test_src_tools.py 21 用例 + 容器内三 op 冒烟）；§17 十四条判据全绿自查（docs/phase10/PLAN-17-SELFCHECK.md，判据 5 VM 面为 ADR §3.1 冻结挂起口径）；修复循环两项入库（tool_registry::strict_json 重复键拒绝：tool 与 plugin manifest 双 JSON 通道接线 + 单测锁定 + 26 份存量 manifest 回归零误拒；README 升 2.0 口径 + M8_READINESS 历史快照注记）；遗留台账更新（rtis.py 自测复测现值 10 过 7 败如实登记为 RTIS 侧清账候选；视觉基线遗留勾销）。队列 1 到 7 项全部完成，项目完成自查收口；实战校验三面待用户启动（§十四 暂缓口径）。

## 四、Flutterfox/Voicefox（第二项目，Phase 0-8 完成，v0.9.0 已发布）

- **RIP3 启动与 Phase 1 完成（2026-11-14 到 11-15）**：R2 生效条件满足后用户三项裁决（PG 绞杀者 ADR-0001 / 成熟库优先 / 许可证只记录能力优先）；Phase 0 审计冻结批（afe6f5b：00-ARCHITECTURE-AUDIT + 四冻结物 + ADR-0001/0002 + R2 修订记录，用户评审通过）→ Phase 1 文档域与存储（c4c76a9 + 41bf3359 CI success + 15ff4063 + 本收口提交）：rip-pg17 镜像族（三扩展 + jiebacfg 分词）、intel-db crate（refinery PG-0001/0002 + Document 域存取 + LocalFs CAS + rp-doc CLI）、REST 通道（/api/documents import/list/detail）、/documents 文档中心页（vitest 124）、备份双轨演练（pg_dump 四表对账 + rp-admin 16/16）、浏览器实测 7/7 三截图入库。CI #63 failure（vendor 嵌套 .gitignore 挡 add）修复留档 P1-2 报告 E2 节。阶段总账 docs/releases/RELEASE_2026-11-15_RIP3-PHASE1.md；Phase 2（rip-parser 容器族）待开批。
- 计划书：/home/emo/Downloads/flutterfox-ui-redesign-plan-v2.md；工程：/home/emo/code/voicefox-gui（main，远程 flutterfox/main，最新 a0cfbf4）。
- **Phase 0-8 全部完成并推送**：Phase 7（c2c72fa）MobileShell/平板三态/手势全屏播放，批次 0 补齐桌面冻结门禁截图（桌面逐像素 RMSE=0 零回归）；Phase 8（525f3d2）Rust 转发型 PlayerBackend + FRB 反向通道 + jniLibs + just_audio/audio_service。主会话验收：analyze 0 issue、Dart 189 + Rust 813 测试全过。报告：docs/PHASE7-REPORT.md、docs/PHASE8-REPORT.md。
- **Release v0.9.0 已发布**（github.com/emoeem/flutterfox/releases/tag/v0.9.0）：Flutter 四平台产物（Linux/Android 全链路验证 + Windows/macOS CI 实跑通过）+ TUI 五平台资产同挂一个 Release。CI 全绿（fmt/clippy 零警告门禁修复 7e33e74 + flutter test 门禁增补）；release-flutter 管线落地（a0cfbf4）。踩坑记录：.cargo/config.toml 本机硬编码 NDK 路径已去本地化（本机配置外置 ~/.cargo）、Flutter 3.47 移除 --no-codesign、runner 镜像变量不进 env 上下文。
- 剩余：真机验证（通知栏/锁屏/耳机键/蓝牙路由，需真机）、iOS 批次（无工具链未启动）、Windows/macOS 产物尝试矩阵转正。
- Flutter 3.47.9；桌面实跑正常（FFI/mpv OK）。
- **toolbox-hub 联动（2026-11-07）**：`~/.config/toolbox-hub/tools/rip-*` 四个脚本（verify 对账红线 / status 部署健康 / ci 双仓状态 / service 服务控制）经 MCP 网关可被 AI 调用；授权在 `~/.config/toolbox-hub/mcp.toml`（rip-verify/status/ci 在 allow，network 仅这三个 id 可用）。

## 五、环境事实

- 会话工作目录曾在 pensting 与 security-research-upgrade 间切换；workflow/agent 的 world cwd = 启动时 cwd。用绝对路径最稳。
- GitHub：gh 认证 emoeem（repo+workflow scope）。git user：emoeem / 3353745917@qq.com。
- 无 sudo 权限（密码不可用）；无 xdotool/wmctrl；Wayland+X11 并存；截图用 grim。
- 破折号纪律：全局 hook 拦 em/en dash，文档一律用逗号/冒号替代。

## 六、新会话开场动作（建议）

1. 读本文件 + 记忆索引。
2. RIP 主线已收口（第三节）；RIP 2.0 已启动（2026-11-09），Phase 0 完成，按 docs/phase0/PHASE-ROADMAP.md 推进；Flutterfox 桌面重构已完成（第四节），剩余项均在各节列明。
3. 未决决策（用户定）：RIP 尾巴（CI/鉴权/evidence 迁移等）与 Flutterfox Phase 7/8（移动端）的优先级；实机截图需桌面空闲。

## 七、自主循环授权（2026-11-09 深夜，用户睡前）

- 额度窗口：GLM-5.3-Flash 额度约 2026-11-10 09:00 重置，重置前持续自主开工，不浪费剩余额度。
- 活体运行：RIP 2.0 主工作流 run dwfrun-9a0ff747-5233-4615-8333-e8c2e8e88d30（由 dwfrun-5bd5996b 经 GUI 改设置换入：subagent 模型 account:bigmodel-start-plan/GLM-5.3-Flash$max，并发上限 19，已完成步骤全部走缓存）。尾洞照常填充，不 amend 不 restart。
- 循环规则：主工作流完成后按 FinalReport 加 PHASE-ROADMAP 待验证清单评估缺口，串行开新工作流（同时只跑一个，避免同仓库 git 竞争）；每个工作流收尾必须独立验收加 commit+push 本仓库；额度耗尽或无可做项才停。
- 候选清账项：rtis.py 自测 4 失败（T2/T4/T11/T13）、M8_READINESS 已知限制（evidence 抽样 4/26、research_task 核心字段映射）、license 轴数据源、安全加固（鉴权、Artifact 路径、容器边界）、性能基准、运维文档。

## 八、收尾后审阅与武器库扩展（2026-11-11 用户追加）

- 项目完成（Phase 5 到 10）后：另开「全面审阅与优化」工作流，审阅整个已完成项目：对照 2.0 计划 §17 最终验收标准逐条核验、架构与计划一致性、五道门禁健康度、安全姿态（鉴权、容器边界、路径穿越面）、文档完备度（DOC-STANDARD 合规）；产出优化批次逐批实施并入库（每批 commit+push）。
- 武器库扩展路径（用户硬性要求）：平台必须让「添加新工具」成为一条可复制路径。Phase 6 实施时交付 docs/guides/ADDING-A-TOOL.md 扩展指南与脚手架命令（新增工具 = 写 Tool Manifest + 放入对应 pack 或 python worker + 登记注册 + 健康检查一条线）；Phase 6 验收用「实际添加一个新工具并端到端露出（Registry 到 Tool Center 到 MCP 目录）」作为测试；审阅工作流复验该路径。
- **武器库扩展路径交付注记（2026-11-12 收口实测）**：指南 docs/guides/ADDING-A-TOOL.md + 脚手架 `python3 -I scripts/rip_new_tool.py --id <tool_id> --name <名称> --family <family> --description <描述> --db <开发副本>` 一条线（D-TC-02 校验 + Manifest/worker 生成 + rp-tool validate/register + rp-tool-health ping + 露出核验）；端到端实测以样例工具 rip.http.tls_inspect（TLS Inspector v0.1.1，stdlib ssl）跑通：validate ok=true、ping ok=true（Worker Protocol v1 握手）、注册进 aux 开发副本后经 /api/tools 增量合并在 Tool Center 露出（工具中心 21 个工具，独立验收 browser-use 截图 docs/phase6/browser-e2e/06/07）；指南红线四条（禁写 platform*.db、禁手写 SQL、command 不经 shell、network.mode=none 缺省）。MCP 目录同源露出归 Phase 9（ROADMAP §十）。

## 九、浏览器实测硬性要求（2026-11-11 用户追加）

- 凡浏览器端开发（Web Workbench、Workflow Studio 画布等 UI 阶段），必须用真实浏览器亲自逐页逐功能点查看，不以代码检查和构建通过替代实测。
- 工具链：browser-use 插件（skills: browser-use:control-browser 驱动真实浏览器、browser-use:web-gui-tester GUI 测试流程）；autoresearch 插件（性能与优化实验：init_experiment/run_experiment/log_experiment 计量闭环）用于 Phase 10 性能基准与收尾审阅。
- 执行点：Phase 5 入库前补一轮浏览器 E2E（起服后逐页对账、全局搜索、三态、移动视口，留截图证据）；Phase 8 画布开发全程浏览器实测；Phase 10 用 autoresearch 跑性能基准；收尾审阅工作流对全部页面再走一轮浏览器实测。

## 十、外部审阅采纳清单（2026-11-11，用户转交 ChatGPT 公开仓库审阅 + Yuxi 架构参考）

仓库已公开是用户决定。卫生纪律不变：无凭据、无生产库、无内部敏感信息入库（已有 gitignore 与 DOC-STANDARD 约束）。

审阅建议按剩余阶段分配：
- Phase 7（Agent Engine）新增验收：统一 trace 关联（trace_id/task_id/agent_run_id/tool_run_id/artifact_id/evidence_id 贯穿并可查询）；研究能力以输入输出契约加测试交付（Research Planner/Source Discovery/Evidence Extraction/Verification/Synthesis/Citation/Evaluator），不只是 prompt；Context Broker 增 Context Memory 雏形（可追溯研究成果，与聊天记忆分离）。
- Phase 8（Workflow Studio）新增验收：持久化状态、checkpoint、恢复、重试、超时、并行、人工介入节点；参考 LangGraph 状态机与 Temporal 持久化执行的设计（不替换 Rust 自研方向）。
- Phase 9 新增：外部接口暴露 correlation id；MCP 工具目录含 Tool Center 同源。
- Phase 10 新增：database backup/restore API 实现（当前 crate 未实现）、FTS 重建校验、数据保留归档、依赖许可证逐包清单（参照 admin/DEPENDENCIES.md 口径）、axe 无障碍对比度修复（前审 13 节点）、视觉基线统一字体后建立。
- 收尾审阅工作流（§八）扩充审计维度：容器退出残留清理、文件系统越界、代理绕过与 DNS 边界、并发资源耗尽、Artifact 路径穿越、worker 崩溃恢复、Gateway 重启幂等（工具执行安全清单）；README/ARCHITECTURE/PROJECT-STATUS 三件套文档重组；八个参考项目深研（DeerFlow/Haystack/RAGFlow/GPT Researcher/LangGraph/Dify/Temporal/GraphRAG，吸收设计不搬代码）；Yuxi 参考点（文档解析管线插件链、SSE 事件流、per-thread sandbox、API Key 访问）。
- Phase 5 遗留项澄清：真实后端浏览器 E2E 已于 6217ca8 前实测通过（浏览器实测员逐页对账），审阅所见 PHASE5-ACCEPTANCE 为其前置版本。

## 十一、WebUI 工作台目标形态（2026-11-11 用户拍板采纳）

硬性要求：Web Workbench 从管理面板升级为 VS Code 式工作平台，按 2.0 计划 §10.2 布局加外部审阅 §6 工作区清单执行：
1. 布局五区：顶部栏（工作区切换、全局搜索/命令面板、运行任务、Runner 状态、通知）、左侧主导航、中央多标签工作区（固定/关闭/最近访问/深链接）、右侧 Inspector（当前对象属性、关联知识/工具、运行配置，可折叠）、底部 Activity 面板（任务队列、Tool Runs、日志、输出、状态）。
2. 十一个工作区：Dashboard、Projects、Research、Knowledge、Tools、Plugins、Agents、Workflows、Runs、Runtime、Settings；每区按 2.0 计划 §10.3 与 §10.4 的页面设计落地。
3. 概念分离（数据模型与 UI 同步）：Workspace 是工作界面与上下文的组织方式；Project 是研究对象；Agent 是执行主体；Workflow 是执行过程。四者不得混用实体。
4. 接入点：Phase 7 首批做壳层升级（多标签 + Inspector + Activity + workspaces 纯数据模型，参照 telegram-bot workspaces.ts 模式），Agent Studio 落新壳；Plugin Center 在 Phase 7 壳层升级批次中迁移；Phase 8 Workflow Studio 画布直接落新壳；收尾审阅工作流用真实浏览器对全部工作区复验（browser-use）。**落地注记（2026-11-13 Phase 7 收口）**：壳层升级已交付（P7-1：五区布局 + 十一工作区，既有 10 页路由不变迁移，截图 browser-e2e/01/）；Agent Studio 实页已落新壳（P7-7 三区：定义与能力绑定对账 / 运行与步骤轨迹回放 / 三 Memory 视图，截图 browser-e2e/02/ 与 03/）；Plugin Center 已随既有页路由不变迁移进新壳（Plugins 工作区，无新增写面）；Phase 8 Workflow Studio 画布已落新壳（P8-4 五区：面板/画布/Inspector/RunPanel/版本列表，@xyflow/react 12.12.1 独立 vendor-xyflow 分组，保存/运行/暂停/恢复接 P8-3 API，截图 browser-e2e/03/（phase8/browser-e2e/01/））；收尾审阅的全工作区 browser-use 复验仍待执行。

## 十二、ChatGPT 外部审阅批次（2026-11-11 用户要求，项目完成后执行）

收尾审阅工作流新增批次「ChatGPT 外部审阅」：用 browser-use 驱动真实浏览器打开网页版 ChatGPT（复用浏览器里用户已登录会话；未登录则停下请用户登录一次），提交仓库地址 github.com/emoeem/research-intelligence-platform（公开仓库）与提问（找 bug 与可优化点，可附 PHASE-ROADMAP 与阶段 RELEASE 清单供其定位），等待其浏览分析并回收回答；整理为建议清单逐条 triage（与 §十采纳映射同口径）后并入优化批次实施。前提：仓库保持公开（用户已决定）；敏感信息纪律照旧。

ChatGPT 外部审阅链路已于 2026-11-11 实测通过（用户登录后全自动跑通）：browser-use 打开 chatgpt.com 复用登录会话（账号 eme333emoemo 免费版）→ fill 提问后 Enter 不触发发送，必须点击「发送」按钮 → 轮询判断方式：按钮「停止」消失即生成完成（本次含仓库浏览约 70 秒）→ 用对话 region innerText 回收全文。ChatGPT 免费版可自主浏览公开 GitHub 仓库，实测正确读出 README、SESSION-HANDOFF、M8_READINESS 与 releases，产出含依据文件的发现清单。正式审阅批次直接复用本配方；本次测试会话「仓库快速审阅」保留在账号内可续问。

## 十三、安全知识源接入计划采纳（2026-11-11 用户提供）

docs/RIP-SECURITY-KNOWLEDGE-SOURCES-PLAN.md（commit 68e0f51）为 P6-9/10/11 的执行规格：通用 Knowledge Catalog/Document 摄取（补 CONTENT_KINDS 白名单与 PluginSource 空实现，先 ADR 后迁移）→ 三源插件（Awesome-Hacking CC0 目录项、SecSkills Apache-2.0 目录项加 skill_candidate、tongui 课程无 LICENSE 只索引元数据按需读）→ 目录抽取与静态审查工具（rip.knowledge.catalog.extract、rip.security.extension.audit、T1 的 document.fetch 与 source.sync）。接线方式：活体工作流到达「Phase 7 起逐阶段实施」尾洞时，先执行 Phase 6 扩展（P6-9 到 P6-11，按该计划 §8 批次与 §10 验收清单）再进 Phase 7；实施必须复用既有 Registry/Runtime/Search/Context/Egress，不建平行系统，RTIS 旧链 15,155 条对账不回归。§9 每周更新监控在 P6-10 落地后以定时自动化实现（周报含无变化来源）。

## 十四、实战校验闭环（2026-11-12 用户要求，项目完成后的最终循环）

目标：Phase 5-10 与 P6-9/10/11 全部完成后，RIP 必须达到「立马投入生产」状态，以实战使用校验，循环直到无重大问题。

循环步骤（每轮产出问题清单，问 ChatGPT 后优化修复，修复入库后进下一轮）：
1. ChatGPT 全面审阅（§十二配方）：仓库地址 + ROADMAP + RELEASE 清单，找 bug 与优化点，回答 triage 分配。
2. GitHub 知识库与武器库补齐：知识侧提交三个来源插件的 Manifest、catalog_entry 元数据与 checksums（许可红线不变：课程原文不批量镜像，28MB knowledge.db 仍不入库）；武器侧提交 Tool Pack Containerfile、Manifest 与首批安全工具（静态分析、代码审计、依赖检查类为 T0/T1 先行），确保 ADDING-A-TOOL 指南指引下武器库开箱可用。
3. 实战校验三面（每面跑 RIP 端到端：建研究任务 → Context Broker 检索 → Tool Engine 派发 → Evidence 固化 → 报告生成，记录效果与不足）：
   a. 本地靶场：podman 容器化靶场（vulhub/DVWA/pikachu 类，本机 rootless Podman 即执行面），验证 Web 研究工具链（HTTP 检查、目录发现、指纹、payload 助手）真实可用；
   b. 在线靶场：公开授权的练习平台（HackTheBox/TryHackMe 类或 vulhub 云实例），验证远程目标场景下的知识检索与工具编排；
   c. 公益 SRC：补天等已注册平台的授权项目，按 AGENTS.md 纪律（低并发、间隔不小于 1s、只打授权范围、发现按平台规则提交）真实挖掘，验证平台对真实资产研究全流程的支撑度。
4. 每轮后：问题清单 + 效果评估再问 ChatGPT，按其建议与本机实测修订优化批次，逐批实施入库（commit+push），直到 §17 生产就绪清单全绿。
红线：实战校验不改变既有安全边界（容器隔离、出口白名单、T0/T1/T2 权限闸照常生效）；SRC 目标严格限授权范围，漏洞细节仅提交平台不进公开仓库。

## 十五、跨项目交接入口（2026-11-12）

新 ZCode 项目接手全部剩余工作的唯一入口：docs/HANDOVER-RIP2-CONTINUATION.md（含接手清单、剩余队列、贯穿纪律、运行时事实、完成后闭环、已知遗留）；活体工作流脚本快照在 docs/handoff/workflow-script-RIP2-2026-11-12.dwf.ts。原会话工作流 dwfrun-5f804a20 若仍在跑，新项目按该文档第二节三选一接手。



## 十六、RIP Unified Workbench UI 重构交接（2026-11-24）

本轮工作目录：`/media/emo/code/pensting/research-platform`。UI 计划总入口为 `docs/ui-redesign/README.md`。

- M0 已收口：现场审计和复测结果见 `docs/ui-redesign/M0-CLOSEOUT.md`。当前工作树有大量既有未提交改动，禁止 reset/clean/stash/checkout/rebase/commit/push；42 张 Playwright 基线 PNG 曾由先前 snapshot-update 进程改写，71/71 自动化通过但人工视觉签收仍待 M7。
- M1 设计规格冻结：Token、Typography、12 个独立工作区页面规格、四份 acceptance 文档与隔离 prototype 均已落盘。Figma 像素核对未完成，不得声称已更新 Figma。
- M2 仍在实施：Topbar Runs/Runtime 快捷入口已补，Notifications 因无后端 API 保持禁用；Inspector 对象上下文、Inspector/Activity 可调整分栏、Activity 实时 Runs/Logs/Output 仍未完成。见 `docs/ui-redesign/M2-REPORT.md`。
- 当前最近一次验证：20 个测试文件 / 135 tests 全通过；typecheck、format、build 通过；Playwright 71/71 通过；axe 覆盖项无 serious/critical；`git diff --check` 通过。
- 下一步严格留在 M2：完成 Inspector 的真实对象上下文与相关实体、统一可调整面板（鼠标/键盘/持久化）、Activity 接入现有真实运行数据并补齐视觉/交互测试。M2 验收通过前不进入 M3。


### M2 continued progress (2026-11-24)

- Activity Tool Runs is now connected to `/api/runs?limit=12` (5s refresh); Activity Status is connected to `/api/runtime?limit=20` (10s refresh). Both use real backend responses, explicit error/empty states and route links.
- Inspector enriches selected `/runs/:run_id` with real Run status/tool/duration/error/task fields. Other object types still need context resolution.
- Added zh-CN/en-US/ja-JP translations and a shell integration test for live Run/Runtime Activity data.
- Latest gates: typecheck + format pass; 20 test files / 136 tests pass; production build pass (2425 modules); Playwright visual gate 71/71 pass. A transient WSW test failure in one parallel run passed on isolated rerun and full-suite rerun.
- Still M2 blockers: Logs/Output lack separate live backend streams; Inspector for non-Run objects incomplete; Inspector/Activity resizable keyboard-accessible persisted splitters not done; Notifications stay disabled; manual 5 viewport × 8 state review and human baseline sign-off remain pending. See `docs/ui-redesign/M2-REPORT.md`.


### M2 最终收口（2026-11-24）

- M2 前端实现完成。Inspector 已支持 Run、Tool、Plugin、Task、Knowledge 真实对象上下文；Activity 的 Tool Runs、Run 失败/生命周期事件与错误载荷、Run Artifact、Runtime 节点状态均接现有 API。
- Inspector/Activity 使用 react-resizable-panels，支持鼠标拖动、键盘方向键、尺寸约束与 localStorage 持久化；1280px 以下 Inspector 采用抽屉，不挤压主工作区。
- zh-CN/en-US/ja-JP 翻译齐全。新增 M2 Playwright 专项：5 视口（360/768/1024/1280/1600）+ 8 个 Activity 数据状态。
- 最新全量门禁：20 files / 138 unit tests；typecheck、Prettier、build 通过；Playwright visual/axe 73/73 通过。M2 专项 5 视口/8 状态通过。
- 后端边界：当前无 Notifications 查询/确认 API，按钮保持禁用；当前无 stdout/stderr 流式 API，Logs 显示真实 Run 生命周期/失败事件与 error_json，不伪装成终端流；Output 只展示 API 返回 Artifact。
- 详细记录：docs/ui-redesign/M2-REPORT.md。下一阶段按 MASTER-PLAN 的 M3 进入，不回头重做 M2；M7 人工截图签收与 Figma 像素复核仍保留。


## UI Redesign M3 最终收口（2026-11-24）

- M3 CLOSED。详细报告：`docs/ui-redesign/M3-REPORT.md`；执行记录：`docs/ui-redesign/M3-EXECUTION-PLAN.md`。
- 最终门禁：Vitest 21 files / 152 tests；typecheck PASS；Prettier PASS；production build PASS（2439 modules）；Playwright visual/axe 78/78 PASS；git diff --check PASS。
- 10,000 行 Trace VirtualTable 浏览器验证通过，DOM 窗口少于 40 行。M3 专项 5 个 Playwright 场景全部通过（列宽鼠标/键盘、五视口、三语×双主题 axe、分栏折叠/响应式）。
- 真实数据边界：Research Tasks 当前只对已加载数据做本地过滤/排序；WebSocket Frames 使用 API cursor；History Hex 优先原始 body bytes，缺失时只显示 UTF-8 文本投影；Run Logs 只展示后端真实生命周期/错误事件，不冒充 stdout/stderr 流。
- 无 M3 阶段新增/升级生产依赖；工作树中 package.json/package-lock 的其他既有差异保持不动。
- M7 人工截图签收与 Figma 像素复核仍未执行，按总计划后置。
- 下一阶段允许进入 M4，仍需按 MASTER-PLAN 顺序执行；不要直接跳 M5。


## M4–M8 自主推进最终记录（2026-11-24）

- M4 UI CLOSED：Dashboard/Research/Knowledge/Tools/Plugins 使用既有 API；Projects 不再把 `/api/tasks` 冒充 Project，明确 `/api/projects` 缺失并链接 Research Tasks；Research Assets 诚实显示后端未提供 `/api/research/assets`。详细见 `docs/ui-redesign/M4-REPORT.md`。
- M5 CLOSED：History、Repeater、Proxy/CA、Fuzzer、WebSocket、Decoder、Comparer、Passive Audit，保留 Rust FuzzSpec、Scope、Body 按需加载、脱敏和 TrustConfirm。
- M6 CLOSED：Agent、Workflow、Runs、Trace、Runtime 与 Activity Dock 使用现有执行/API 契约。
- M7 OPEN：全量 Playwright visual/axe 78/78 通过，Vitest 21 files / 149 tests，typecheck、Prettier、production build、git diff --check 通过；但全路由三语言/全部状态/五视口、42 张既有变更基线人工审阅、Figma 像素核对和性能人工记录尚未完成。不能宣称 M7 最终视觉签收。
- M8 BLOCKED：未切换 7800 默认入口、未停用旧静态 UI。当前 rp_server 未提供 SPA fallback，缺真实 Bearer/Origin/CSRF parity 与回滚演练；按 MASTER-PLAN 需用户验收后才可切换。
- Projects 视觉基线：经实际图像检查后仅更新 `projects-rip-dark-mobile.png` 和 `projects-rip-light-mobile.png`；Projects 桌面基线未改。全量 78 项复验通过。
- 未执行 commit/push/reset/clean/stash/checkout/rebase；保留原有工作树更改。


## RIP M4–M8 follow-up completion (2026-11-24)

User decisions: Projects=independent; Research Assets=independent; M7=reference; M8=staged; Figma=existing. Four desktop dark-theme Figma screenshots were supplied and registered as the visual reference.

- M4 CLOSED: migration 031, Rust Store/CLI, Projects and Research Assets REST APIs, workspace-scoped details/updates, Project↔Task/Asset relations, Asset Endpoint/append-only Observation/evidence-referenced Technology Profile, frontend management/details and zh-CN/en-US/ja-JP. The main platform.db remains user_version 30.
- M5/M6 remain CLOSED as documented.
- M7: Playwright 85/85, including 12 routes × 3 locales × 2 themes × 5 viewport widths responsive smoke (360/360); Vitest 22 files / 155 tests, typecheck/Prettier/build pass. Local production-build smoke covered 12 routes at desktop/mobile (24 navigations), 24/24 success, no API/console errors, warm-cache load median 24 ms / p95 28 ms. M7 is CLOSED for the user-approved reference-based scope; Figma pixel parity is not required. Real-device profiling remains non-blocking.
- M8: staged SPA is live at http://127.0.0.1:7801/ with an isolated DB clone and CSP/Origin/Bearer/SPA fallback/API 404 tests. The old 7800 service was not listening; default root remains unchanged and M8 is STAGED — NOT DEFAULT.
- Gates: verification-engine 56 tests; selftest_projects_assets.sh PASS; selftest_phase9_rest.sh 33 PASS / 0 FAIL; selftest_spa_entry.py 5/5; web 155 tests; Playwright 85/85, including the 360-combination locale/theme/five-viewport route sweep.
- No Git commit/push/reset/clean/stash/checkout/rebase was performed. Existing worktree changes were preserved.


## Security knowledge continuation（2026-11-26）

当前活体目录仍为 `/media/emo/code/pensting/research-platform`，分支 `main`。本节覆盖 2026-11-26 最新的 KEV / Nuclei / PoC Index 接入与插件运行时修复；旧阶段记录保留为历史事实。

### 数据切换与 CI

- 已完成持久化开发知识数据面切换：`migrations/dev-p6.db` schema 32，KSE 18,102；Search v6 位于 `migrations/platform-search.db`，3,708 documents / 3,708 FTS rows。
- 数据分布：CISA KEV 1,805 APPROVED；Nuclei 552 APPROVED、13,559 DISCOVERED；PoC Index 1,296 APPROVED。Nuclei 的 552 条有明确 `user:cli` 审核事件，不代表模板运行成功或目标已验证。
- 当前 dev-p6 SHA-256：`0c1091dd3ade02cb6ab13e1fb2a8743db80302ac61606a7db0ce70221e1e19ac`；search index hash：`2da952cad715e179ee2626b79b55784ab014897e270d18c65c05d1587dd43d54`。
- 插件路径修复已由 commit `b9db4453` 推送；GitHub Actions run `45804184849` 已 8/8 jobs success。当前后续 POC source recovery / fixture 更新尚未提交，将单独触发下一轮 CI。

### Plugin install-path repair

- 发现 CISA KEV 0.1.2、Nuclei 0.1.1、PoC Index 0.1.0 的 active plugin rows 指向已删除的 `/tmp/rip-cisa-kev.qfBoNo/...` 安装目录。KSE 数据和搜索当时仍可用，但后续插件 ingest/sync 会因找不到 package directory 失败。
- 新增受控 `rp-plugin repair-path` 命令、`PluginRuntime::repair_install_path`、`PluginStore::repair_installed_path` 和生命周期集成测试。仅允许 active REGISTERED/ENABLED 插件；要求旧路径不存在、Manifest 与数据库登记完全一致、staged package 通过静态校验；重新生成 checksums/plugin.lock；只更新 installed_dir/lock_sha256，不改变状态、active pointer、capabilities、KSE rows 或 review 状态。
- 三个插件已修复到：
  - `rip-data/plugins/installed/knowledge.cisa-kev/0.1.2`
  - `rip-data/plugins/installed/knowledge.nuclei-templates/0.1.1`
  - `rip-data/plugins/installed/knowledge.pocindex-kev/0.1.0`
- 修复前备份：`/home/emo/backups/research-platform/20261126-182822-dev-p6.db.pre-plugin-path-repair.bak`；SHA-256 `6a5c05fcd9ca92b54d0da187ea5e05b4447f12cf602428f84a5047894fea24a7`；权限 0600。
- 三插件 ingest dry-run：1,805 / 14,111 / 1,296 条全部 skipped，changed=0；KSE 18,102 和 Nuclei review distribution 未变；Search v6 文件 hash 未变。全量检查七个 active knowledge plugin 安装目录均存在；失败 Manifest 修复路径也验证会清理本次 staging 目录。
- 本地验证：完整 `cargo test --offline --workspace` 通过；最终 `cargo test --offline -p plugin-runtime` 85 passed / 0 failed / 3 ignored；全工作区 `cargo clippy --offline --workspace --all-targets -- -D warnings`、Python package tests、Knowledge Review selftest、POC Search selftest 均通过；`cargo check --offline -p plugin-runtime --bin rp-plugin` 通过；CLI 在 disposable clone 上三插件 repair-path 均成功。
- 全工作区测试发现并修正 MCP 子任务测试夹具环境变量错误：Agent 使用 `RIP_PLUGIN_DB`，测试此前只设置 MCP catalog 的 `RP_PLUGIN_DB`，导致意外读取本机 18,102 条知识记录并超出 8k 测试预算；现已明确指向不存在的隔离路径，完整 workspace 测试复绿。
- 完整报告：`docs/knowledge/PLUGIN-INSTALL-PATH-REPAIR-2026-11-26.md`。

### POC 证据审核后续队列

- Overlay 当前 49 entries、25 resolved relations；Evidence level 分布 K0 29 / K1 9 / K2 10 / K3 1 / K4 0。
- High POC queue 28；证据缺口 33 records / 94 items；source_path 33/33、source_commit 30/33；51 个 Evidence Ref 路径可解析；8 条记录没有 Evidence Ref ID（4, 12, 16, 18, 20, 29, 72, 900047）。
- 旧 POC 源文件只做过静态语法检查，不能等同于 runtime verification。不得执行外部 exploit、不得对真实目标发请求；缺少原始证据的记录继续保持待补。
- 下一批继续按 evidence recovery matrix 找已有原始材料，能验证来源/完整性/时间线的先补引用；无法恢复的明确标记 missing，不推断 fixed 或 vulnerable。ID 900043 已有 2026-11-11 session-binding fix 记录，仍需单独的当前回归证据。
- 2026-11-26 补充检查 ID 12、20 的候选 sibling runs 目录：../runs/jxnu/pan 有 prefix_probe.txt，../runs/aozoneyun-admin/pwd-oracle 有历史 header/body 与 phpsessid.txt；两者均无 POC verified_note 路径引用或 k_event 关联，ID 4/12/18/20/29/72/900047 的 source_commit 也都只是 2026-11-09 RTIS 迁移提交。未读取响应正文/会话文件，未将候选目录错误挂到 Evidence Ref。详见 docs/knowledge/POC-EVIDENCE-RECOVERY-MATRIX-2026-11-26.md。
- ID 16 源文件已恢复：原 RTIS DB 路径 basename 是 poc.sh，当前同目录唯一脚本为 poc-actuator.sh，已记录 source_path 与迁移提交 source_commit；移除 missing_evidence 中 source_artifact，剩 raw_request_response / impact_boundary 两项。K0/Candidate 不变，Evidence Ref 仍为空。
- 扩展清点 ../runs 的目录名/文件名后，未找到能唯一关联 ID 4、18、29、72、900047 的新运行产物；ID 12、20 的相似目录仍缺少 verified_note/k_event 关联。未读取响应正文、脚本正文或会话材料，未新增 Evidence Ref。
- 本轮 POC 检索自测已更新：21 条 source_integrity=missing 队列；ID 16 通过 Nacos 查询专项断言验证 source_path/source_commit 可追溯、Evidence Ref 仍空、K0/Candidate 不变、仅剩 raw_request_response / impact_boundary。Web 全量 Vitest 22/22 files、157/157 tests 通过；TypeScript noEmit、npm run build、Knowledge 页专项 8/8 通过。
- POC source recovery commit 8c7d6877 已推送；对应 CI run 45807868916 已 8/8 jobs success（Rust workspace tests/clippy、Browser visual/accessibility 均通过）。两个历史 High POC 汇总文档已同步 ID 16 与全量来源覆盖统计。
- 用户未跟踪目录保持原样，不得触碰：`scripts/security-intel/data/`、`scripts/security-intel/download_trickest_cve_pages.sh`、`test-results/`。

### 工作树纪律

- 插件路径修复 commit `b9db4453` 已提交推送且 CI 8/8 通过。POC source recovery commit `8c7d6877` 已提交推送且 CI run `45807868916` 8/8 通过。ID 16 来源恢复、Overlay、POC selftest、Web summary fixture 与证据矩阵文档均已纳入该提交；本轮剩余仅为本次更新的历史统计文档与 handoff。
- 不得 reset/clean/stash/checkout/rebase；不删除数据库备份；不改 `migrations/platform.db` 或原始 `integrations/rtis/knowledge.db`。


### 2026-11-26 continuation: Awesome-Hacking version accounting and Trickest source snapshot

- Read-only audit of `migrations/dev-p6.db` found `knowledge.awesome-hacking` has four retained plugin versions (0.1.0–0.1.3). Their KSE row totals are 85/84/84/84. Only 0.1.3 has `is_active=1`; it contains 84 unique directory entries: 6 APPROVED, 75 DISCOVERED, 3 REJECTED. The previous aggregate 24/301/12 counted historical version rows, not unique resources.
- Fixed `apps/server/rp_server.py::knowledge_source_stats()` to group by `(plugin_id, plugin_version)` and map version-scoped stats in plugin list/detail. No historical KSE rows or review states were modified. Added `apps/server/test_knowledge_source_stats.py`; test and Python compile pass. `apps/server/selftest_knowledge_review.sh` passes.
- Retrieved all 30 Trickest/CVE top-30 candidate metadata pages from pinned commit `0330ab928da94d1210b84fbe537bb328073f6114`; retried the single missing CVE-2017-10271 page successfully. Verified upstream MIT LICENSE. SHA-256 manifest is `docs/knowledge/TRICKEST-CVE-TOP30-SOURCE-SNAPSHOT-2026-11-26.md`; raw pages remain under ignored `target/`. No PoC executed and no target contacted.
- Current CI for commit `0d83a685` was still running its Rust clippy job at last check; other seven jobs had succeeded. Recheck before declaring CI green.
- Worktree modifications are intentionally local and uncommitted. Preserve existing untracked `scripts/security-intel/data/`, `scripts/security-intel/download_trickest_cve_pages.sh`, and `test-results/`; do not reset/clean/stash/checkout/rebase.
- Validation complete: version-scope regression PASS; `py_compile` PASS; `apps/server/selftest_knowledge_review.sh` PASS; isolated real-server `/api/plugins` integration PASS for all four versions (0.1.0 = 6/76/3, 0.1.1–0.1.3 = 6/75/3). `git diff --check` PASS. The CI for prior pushed commit `0d83a685` completed SUCCESS (8/8 jobs); it does not include the current uncommitted API fix. Do not change plugin approval states or delete old version rows.


## Security knowledge governance continuation — M0/M1/M6

- M0 read-only audit is complete; report: `docs/knowledge-governance/M0-BASELINE-AND-GAP-REPORT.md`.
- M1 unified entity/relation model is implemented and locally verified. ADR and closeout: `docs/knowledge-governance/M1-UNIFIED-ENTITY-RELATION-ADR.md` and `docs/knowledge-governance/M1-CLOSEOUT-AND-REMAINING-ISSUES.md`.
- V0011 was exercised only on disposable PostgreSQL databases/schemas. The PostgreSQL regression suites passed 8/8 serially on a fresh database; `cargo test --offline -p domain` passed 82 tests; scoped Clippy passed with `-D warnings`.
- A custom-format V10 database backup was made, V0011 was applied, and restoring the backup to a clean disposable database returned the schema to version 10 with the pre-migration relation intact and no V0011 registry. The temporary backup was not retained.
- M6 source/evidence governance is complete for the persistence layer; decision and closeout: `docs/knowledge-governance/M6-SOURCE-EVIDENCE-GOVERNANCE-DECISION.md` and `docs/knowledge-governance/M6-CLOSEOUT-AND-REMAINING-ISSUES.md`.
- PostgreSQL V0012 and SQLite 033 were tested on disposable databases only. PostgreSQL governance tests passed 8/8; KSE review/ingest tests passed 14/14; scoped Clippy passed. The SQLite backup/restore drill restored V032 with the original KSE row still `DISCOVERED` and no review-log table.
- No production or persistent development databases were modified. No POC, Nuclei template or target request was executed.
- M2 is complete for static ingestion, provenance and source-presence lifecycle. The closeout and remaining issues are in `docs/knowledge-governance/M2-CLOSEOUT-AND-REMAINING-ISSUES.md`; the original pre-implementation baseline remains in `docs/knowledge-governance/M2-BASELINE-AND-GAP-REPORT.md`.
- M2 added raw-template SHA-256, structured matcher/extractor projection, independent syntax/shape/effectiveness states, full 40-hex source pin checks, exact clean-Git-checkout verification, transactional missing/reappearance reconciliation and Search exclusion for upstream-missing rows. Detection validation stays `NOT_TESTED`; the local Arch package revision suffix matches only the first eight pin characters and its non-Git tree remains unverified.
- M2 validation: related Rust tests PASS (three manual static audits explicitly ignored by default and run individually); scoped Clippy PASS; package-builder Python tests 4/4 PASS; focused Web knowledge smoke tests 8/8 PASS; disposable full KSE projection PASS; `git diff --check` PASS. No CI result is claimed.
- Historical Nuclei closeout documents now distinguish Migration 032's original M2 scope from M6's later SQLite 033 review-log migration; current SQLite migration head is 033, tested only on disposable copies.
- **Next in required order: M3 POC evidence, remediation and retest governance.** M0, M1, M6 and M2 are complete. Continue preserving the existing modified and untracked worktree; no commit/push or destructive cleanup.


## M3 closeout — POC evidence, validity, remediation and retest governance (2026-11-26)

- M3 implementation and local acceptance are complete. Closeout: `docs/knowledge-governance/M3-CLOSEOUT-AND-REMAINING-ISSUES.md`; read-only baseline: `docs/knowledge-governance/M3-BASELINE-AND-GAP-REPORT.md`.
- Overlay adds explicit per-POC assessment defaults (evidence completeness, artifact validity, remediation and license), strict state/review/evidence guards, local artifact SHA-256 verification and workspace path containment. HTTPS URLs remain provenance refs only; remote content cannot be labelled byte-hashed until materialized locally.
- Review decisions APPROVED/REJECTED both require reviewer, reason and reviewed_at. Positive claims require evidence, timestamps and reviewer; complete evidence requires a gap-free checklist and hashed local artifacts.
- Retest history reuses existing `research_plan_item`, `research_plan_event` and `research_evidence`; no migration or duplicate history table. Added `PlanStore::list_poc_retest_history` and `rp-verify poc-history --poc-id`. It returns five-state transitions and evidence hashes/metadata, never evidence payloads.
- Validation: Python assessment tests 11/11 PASS; Overlay validator PASS (49 entries, 2 external entities, 25 relations, RTIS source DB read-only); Rust verification-engine tests 45 unit + 2 provider + 10 store PASS; scoped Clippy with `-D warnings` PASS; disposable SQLite CLI history smoke PASS including NOT_TESTED→INCONCLUSIVE transitions, run/trace metadata and non-positive-ID rejection; Python compile and `git diff --check` PASS.
- Data boundary: new per-POC assessment metadata is 0/49. This is deliberate: no historical narrative, legacy verified bit or source presence was promoted to a current validity/remediation claim. The 49 curated entries retain their prior evidence levels, lifecycle, review gates and evidence refs. Existing 33 evidence-gap records / 94 missing items remain visible.
- No PoC execution, target request, production DB write, or persistent dev DB migration occurred. No CI result is claimed for this uncommitted worktree.
- **Next required phase: M4 search relevance and cross-source deduplication.** M5 governance UI follows M4; M7 full quality gates and final handoff remain outstanding.
- Preserve all existing modified/untracked files. No commit/push/reset/clean/stash/checkout/rebase.


## M4 baseline — search relevance and cross-source deduplication (2026-11-26)

- M4 read-only baseline: `docs/knowledge-governance/M4-BASELINE-AND-GAP-REPORT.md`.
- Search index v6 is healthy: 3,708 documents / 3,708 FTS rows (3,107 catalog_entry, 552 nuclei_template, 49 rtis_poc; all ACTIVE).
- Critical identity finding: do not deduplicate by `canonical_url` or title. 1,296 PoC Index entries share a corpus homepage URL; 552 Nuclei entries share a plugin corpus URL. 15 repeated normalized title groups are Nuclei templates with different CVE/template paths. These are not confirmed duplicates.
- Current FTS search is BM25 + quality + keyword coverage with CJK 2-gram, filters and explanations; no generic entity identity/hash/duplicate-group projection and no cross-source dedup grouping. `rp-search-unified` merges by `(store,id)`, so cross-store same-entity hits remain separate; no general unified search API route was found.
- ⌘K global search loads at most 200 tools/knowledge/tasks from separate APIs and does client-side token matching; Knowledge POC tab searches only curated POC records through its dedicated endpoint.
- Existing PostgreSQL KnowledgeCard exact-hash and MinHash duplicate workflows produce reviewable candidate edges; M1 typed entity/source refs and evidence-backed relation reviews should be reused. No auto-merge or delete.
- M4 implementation order: typed canonical identity/provenance projection → unified search relevance and API → conservative cross-source duplicate candidate generation → 100+ judged-query benchmark. M5 UI follows M4; M7 remains final full quality gates.
- Audit was read-only; no source DB writes or POC/template execution. Preserve the current uncommitted worktree; no commit/push/reset/clean/stash/checkout/rebase.
- Reused the existing `rp-kc bench` / `crates/intel-db/src/bench.rs` evaluation harness: current qrel has 10 category/prefix-generated queries over 141 PG KnowledgeCards; it is not a human-judged cross-source benchmark. Historical PG-only hybrid Recall@10 0.5812 / P@10 0.4700 / MRR 0.9167 / NDCG@10 0.8212 (see `docs/rip3/batches/P4-4-BENCHMARK.md`).
- Added `scripts/security-intel/evaluate_exact_cve_search.py` and saved `test-results/m4-exact-cve-search-baseline-2026-11-26.json`. Read-only 100-query exact-CVE regression against SQLite FTS: 100/100 queries; Recall@10 1.0; MRR 1.0; first relevant rank 1 in 100/100; all structured relevant template IDs in top 100; mean precision among returned hits 0.99; repeat order deterministic. Two queries include an additional same-product/different-CVE template and require human relevance judgement. This is same-index self-consistency, not human-labelled semantic benchmark.


## M4-A implementation — typed identity and exact-ID search projection (2026-11-26)

- M4-A complete; M4 overall in progress. Search engine schema v7 adds `identity_json` and `SearchIdentity` (typed entity/canonical key, source system/record/version, content/metadata hashes, vulnerability IDs). No source DB or PostgreSQL migration.
- Mappings: Platform resources → `platform/<resource_id>`; KSE → plugin/item/version + source Commit/path/content and metadata hashes; Nuclei → `detection_template` canonical key from source template ID + CVE refs; PoC Index → `vulnerability` canonical key `cve:<ID>`; curated RTIS → `poc` canonical key `rtis:poc:<id>`. PoC Index risk records are not labelled exploit POCs; RTIS POC bodies are not read or hashed.
- Added exact `canonical_key` and `vulnerability_id` filters to `rp-search-index` and `rp-search-unified --sqlite-filters`.
- Real temporary v7 rebuild from read-only dev-p6 + RTIS source: 3,708/3,708 docs. Identity coverage 3,708; canonical entities 1,897; vulnerability IDs 1,849; source content/metadata hashes 3,659 each.
- Dedup audit: 0 exact content-hash groups, 0 metadata-hash groups, 0 duplicate typed canonical-key groups; 15 repeated-title groups (Nuclei different paths/CVEs) and 2 corpus URL collisions. No automatic merge/delete. Audit scripts: `scripts/security-intel/audit_search_dedup.py`, `scripts/security-intel/evaluate_exact_cve_search.py`; reports under `test-results/m4-*-2026-11-26.json`.
- v7 exact-CVE self-consistency: 100/100 structured CVE queries; Recall@10=1.0, MRR=1.0, first relevant rank=1 in 100/100, all structured relevant templates within top100, mean precision over returned hits=0.99, deterministic first 10. Not a human-judged semantic benchmark.
- Validation: search-engine 24 unit + 2 rebuild integration; unified-search 2 library + 1 CLI parser = 29 tests PASS; cargo check PASS; scoped Clippy `-D warnings` PASS; rustfmt touched-file check PASS; `git diff --check` PASS.
- Tracked `migrations/platform-search.db` remains schema v6 and unchanged; querying it with the v7 engine correctly reports rebuild required. M4-B must move unified runtime/API to a v7 derived index lifecycle, not overwrite the tracked DB.
- Next: M4-B unified authenticated/workspace-scoped search API and cross-store typed-entity merge; M4-C conservative near-duplicate candidates; M4-D 100+ human-reviewed cross-source qrel. M5 after M4; M7 final gates still outstanding.


## 2026-11-26 governance phases update

- M3 POC assessment/retest governance: local acceptance passed (11 Python tests; overlay validator; verification-engine 45 unit + 2 provider + 10 store tests; scoped Clippy/rustfmt/diff checks). Curated 49 POCs remain explicitly unassessed; no historical POC was executed.
- M4: schema-v7 typed identity projection and unified API/merge code are implemented and covered by local unit/API tests. Current PostgreSQL service is offline and REST caller-to-workspace ACL mapping is not available; API rejects non-default workspaces. Cross-source M1 relation roundtrip and human-reviewed 100+ qrel remain open. No automatic merge/delete occurred.
- M5: added `/api/curated-pocs/governance` and Knowledge → POC Governance UI. Read-only projection shows M3 states and retest history metadata/hashes only. zh-CN/en-US/ja-JP strings added. Frontend typecheck/build and 22-file/158-test suite passed.
- M7: local quality gates passed: `cargo build --workspace --offline`, `cargo test --workspace --offline` (exit 0; 3 manual Nuclei corpus tests + 1 Chromium big-page test explicitly ignored), workspace Clippy `-D warnings`, `cargo fmt --all -- --check`, Python/server tests and both server selftests, frontend typecheck/build and 22 files/158 tests, `git diff --check`. Fixed stale migration-tail 032 assertions to 033 and narrowed the B2 protected-script check to its actual three protected scripts plus two explicitly audited M4 security-intel files.
- Git boundary: worktree changes are uncommitted and preserved. No push/commit/reset/clean/stash/checkout/rebase performed. GitHub Actions has not been triggered for this worktree.
- M4 remains partial, not accepted: local PostgreSQL is inactive, current REST identity has no workspace ACL mapping (API fails closed to `default`), no live dual-store parity/relation roundtrip, and the 100+ cross-source relevance qrel still needs human judgement. Exact-CVE 100-query benchmark is a self-consistency test only.

- M5 final refinement: POC detail now includes incoming/outgoing relation edges from the overlay (25 total), confidence, notes, external entity titles, and evidence references. The existing source-review UI/API was fixed to pass plugin version and a required audit reason through to the `rp-plugin review` CLI. Backend review API tests 3/3; source review selftest passes; full web suite remains 22 files / 158 tests after final relation UI change.


## M4 acceptance continuation (2026-11-27)

- User decision: workspace mode `personal` (default workspace only), PostgreSQL `podman`, qrel review `batch`. Do not add multi-tenant RBAC; the unified search API remains fail-closed to `default`.
- Started isolated PostgreSQL 17 container `rip-m4-personal-pg` on loopback `127.0.0.1:55432`, DB `rip_m4_acceptance`, named volume `rip-m4-personal-pgdata`; migrations through V0012 and `vector`/`pg_search`/`pg_jieba` extensions. No system DB or source DB changes.
- Fixed separate plugin DB indexing: `rp-search-index` has optional `--plugin-db`; server index freshness checks and rebuild command include `PLUGIN_DB_PATH`. Live v7 derived index: 3,789 docs / 3,789 FTS rows.
- Fixed Podman local digest-pinned image resolution: verify the actual local manifest digest and run the immutable image ID. Rebuilt `rip-embed`; synchronized JSON/YAML manifest digest and `containerfiles/README.md`.
- Live unified search smoke: SQL injection and CVE-2021-44228 both returned PostgreSQL + SQLite channels. PG used isolated acceptance-only fixtures; not production data.
- Created 140-pair cross-source qrel review batch (14 queries × top-10) and LLM advisory suggestions. `human_grade` remains blank for all 140; evaluator correctly stays pending. See `docs/knowledge-governance/M4-QREL-REVIEW-BATCH-001.md` and `test-results/m4-cross-source-qrel-review-batch-001.csv`.
- Fixed `document_domain` integration test migration bootstrap so fresh PostgreSQL test runs do not depend on test execution order.
- M4 remains PARTIAL until user confirms 140 human grades. M4-C remains audit-only; no automatic duplicate merge/delete. No commit/push/reset/clean/stash/checkout/rebase.


## M4 human qrel workflow update (2026-11-27)

- Argilla 2.8.0 SDK API was inspected and installed in an isolated exploratory venv. Upstream README explicitly states feature development has stopped (only bug fixes/security patches); therefore Argilla was not deployed.
- Chosen annotation UI: Label Studio 1.23.2 installed from PyPI in an isolated Python 3.13.15 uv venv because official Docker Hub image pulls repeatedly failed with EOF. It runs as enabled systemd user service rip-m4-label-studio, bound only to 127.0.0.1:8062, with data under ~/.local/share/rip-m4-label-studio/data. SSRF protection is enabled; startup version checks are disabled. A temporary API token was deleted after provisioning; legacy token auth is disabled.
- Created build_m4_label_studio_batch.py, import_m4_label_studio_annotations.py, compare_m4_ranking_runs.py, requirements-m4-evaluation.txt, and test_m4_qrel_tools.py.
- Generated Label Studio task bundle: 140 JSON tasks, 14 queries, source distribution 102 KEV / 29 Nuclei / 8 RTIS / 1 platform. AI suggestions are Label Studio predictions only; no human annotation is prefilled.
- Replaced the evaluator with ir-measures 0.4.3 and added ranx 0.3.21 as an offline run-comparison/RRF experiment tool. Evaluator blocks incomplete qrels; importer rejects conflicts and preserves AI suggestions. Tool tests: 4/4 pass.
- Review instructions: docs/knowledge-governance/M4-QREL-REVIEW-BATCH-001.md. Human review remains pending; no metric based on AI suggestions is valid.


- Label Studio project ID 1 was created and populated. API readback confirmed 140 tasks, 140 advisory predictions, and zero human annotations. First task Q01-R01 has one prediction (grade 3) and no human annotation.
- Local reviewer login is rip-m4-reviewer@example.com. Password is in ~/.local/share/rip-m4-label-studio/credentials.env (mode 0600; contains username/password only). Open http://127.0.0.1:8062 on CachyOS and review the preloaded project. The review UI is ready; M4 qrels remain pending until the user completes the human labels.
- Added repository launcher scripts/security-intel/start_m4_label_studio.sh and pinned review UI requirements. Systemd user unit now calls the repository launcher.

- Live M4 cross-store merge smoke now verified using an isolated PG acceptance fixture linked to exact approved CISA KEV source tuple. One merged result preserved pg-intel + sqlite-knowledge source records; both channels available, no SQLite error. Rust unit tests 6/6 pass including approved exact match, version mismatch, unreviewed guard, and source preservation.


## 2026-11-27 M4 sampled user acceptance decision

- User explicitly accepted the 140-pair M4 cross-source search batch after sample inspection and waived the cumbersome requirement to submit all 140 individual Label Studio annotations.
- Recorded in docs/knowledge-governance/M4-USER-ACCEPTANCE-2026-11-27.md; M4 qrel and baseline docs now distinguish sampled acceptance from exhaustive relevance labels.
- Label Studio annotations/drafts remain 0; CSV human grades remain 0/140; evaluator remains pending with metrics null. No AI prediction was converted into a human label.
- Podman PostgreSQL acceptance container rip-m4-personal-pg is running and accepts connections on 127.0.0.1:55432. The system PostgreSQL service on 5432 is separate and intentionally inactive.


## 2026-11-27 final validation and closeout

- Project closeout report: docs/PROJECT-CLOSEOUT-2026-11-27.md.
- Rust workspace: format PASS; 1,442 passed / 0 failed / 6 ignored across 198 test binaries; Clippy -D warnings PASS; offline build PASS.
- PostgreSQL disposable DB gate: Phase 7 7/7 twice; source monitor PASS; backup/recovery PASS. Temporary role/database removed.
- Python: workers 237 passed / 11 skipped; server 12/12; security builders 7/7; POC assessment 11/11; M4 qrel tools 5/5.
- Web: 22 files / 158 Vitest; build/typecheck/Prettier PASS; Playwright 85/85; responsive matrix 360/360.
- Security: repository scan PASS; cargo-deny full check PASS (duplicate dependency warnings); SQLite benchmark 1,000 docs / 10,000 chunks, backup/restore verified.
- M4 batch 001 accepted by user sample review with individual labels waived. Zero saved human labels and null relevance metrics remain factual and unchanged.
- M7 closed for reference-based scope after the user's UI acceptance. M8 remains staged per user decision.

## 2026-11-28 documentation and configuration correction

- Added the RIP 1.0 release document set under `docs/release/` and indexed it in `docs/README.md`.
- Corrected the formal start path: `apps/server/rp_service.sh` and `apps/server/dev_up.sh` now share `apps/server/platform_env.sh` for `RP_PLATFORM_PG_DSN`.
- The shared DSN file is `rip-data/dev/rip-platform-dsn.env` by default and must be mode 600. It is excluded from source control and must not be written into logs or documentation.
- The systemd unit uses `EnvironmentFile=-<DSN_FILE>`, so PostgreSQL batch-review features receive the same configuration as development startup.
- Missing DSN remains fail-closed: batch-review APIs return `503 batch_review_unconfigured`.
- Updated the release operations and disaster recovery reports with PostgreSQL `rip_platform` backup, restore, verification and rollback boundaries.
- Fixed the DSN loader exit-code capture and `rp_service.sh install` to require a persistent 600-permission DSN file. Direct shell-only DSN is not inherited by systemd.
- Completed a 2026-11-28 isolated drill: real user systemd service on 7815 accepted the DSN and `/api/review-batches` returned 200; custom dump/pg_restore to a separate template0 database restored 2/2 rows. Drill unit, databases and temp files were removed.

## 2026-11-28 production gate freeze

- DSN and PostgreSQL recovery code/docs are frozen; no further edits or reinterpretive rewrites are allowed in that area.
- Added `docs/release/RIP-1.0-PRODUCTION-GATE-STATUS.md` as the remaining production acceptance gate table.
- Current snapshot: 7800 live, 7801 closed, formal rp-workbench.service absent, provider secrets unset, M4 has 140 rows and zero human grades.
- Next gates: formal PostgreSQL backup policy, M8 TLS/rollback promotion, real Provider E2E, authorized POC/Nuclei execution and production rollback evidence.

## 2026-11-28 knowledge ingestion architecture review

- Added `docs/knowledge-governance/KNOWLEDGE-INGESTION-FEEDBACK-ARCHITECTURE-1.0.md`.
- The document proposes one unified Raw/Candidate/Published pipeline for AI
  findings and external document, repository, vulnerability, POC, Nuclei,
  tool and Skill ingestion.
- Repository audit confirms reuse of Document Domain/IR, Document Router,
  Finding feedback, Evidence provenance, KnowledgeCard, entity relations,
  source assessment, batch review and workflow/search infrastructure.
- The missing core is a unified Ingestion Service and API/CLI/Agent Tool,
  document chunk evidence mapping, structured extraction, dedup/version
  diff, risk-aware review UI and publish/index refresh.
- Docling, Unstructured and MarkItDown are comparison candidates only.
  Existing native/document-router capability remains the first implementation
  choice; no parser dependency is installed or selected by this document.
- Project status remains scoped closeout: M4 has zero human grades and null relevance metrics; M8 is staged; real Provider E2E and production reverse-proxy/TLS/rollback remain open.

## 2026-11-28 Knowledge Ingestion P1 vertical slice

- Implemented PostgreSQL migration `migrations_pg/V0015__knowledge_ingestion.sql`
  and typed protocol/state service in `crates/intel-db/src/ingestion.rs`.
- The initial adapter accepts an already stored Research Report IR and delegates
  Finding/Evidence processing to `maintenance::ingest_report`; resulting
  candidates remain pending human review and are never auto-published.
- Added `rp-ingest report/status/retry` and authenticated REST submit/status/
  retry routes. API is fail-closed to `default` workspace until caller-workspace
  ACL mapping exists. CLI calls retain existing workspace isolation checks.
- Idempotency is scoped to `(workspace_id, idempotency_key)`. Reusing a key for
  a different request is rejected. Failed jobs can be retried; stale PROCESSING
  jobs can be reclaimed after the 30-minute lease.
- Updated the REST contract, CLI reference, architecture status and REST route
  selftest. Validation: 8/8 Phase 7 PostgreSQL integration tests (including the
  new ingestion replay/workspace test) passed in a disposable database; 34/34
  intel-db library tests passed; Clippy passed with `-D warnings`; ingestion API
  tests 6/6 and unified-search workspace regression tests 7/7 passed; the full
  apps/server Python suite passed 19/19; REST/CLI selftest passed 33/33 and
  production SQLite fingerprints were unchanged.
- This is not completion of the full ingestion architecture: Document/URL/
  repository input, Chunk-to-Evidence mapping, extraction orchestration,
  dedup/version diff, Agent Tool and publish/index refresh remain unimplemented.
- DSN loader, systemd start/recovery code and the PostgreSQL recovery module
  remain frozen and were not part of this change.

## Knowledge Ingestion document adapter continuation

- Added additive PostgreSQL migration `V0016__document_knowledge_ingestion.sql`:
  document-revision source references, immutable block evidence anchors,
  identifier candidates, occurrence offsets and links to the existing review
  ledger. Raw source bytes remain in Document Domain/CAS.
- Added `rp-ingest document` and REST support for
  `source.kind=document_revision`. It requires an existing revision in the
  requested workspace with parse_state `APPROVED` or `REVIEW_PENDING`; parsing
  continues to use `rp-doc parse` and the existing parser/provider pipeline.
- Added immutable block SHA-256 multiset diffs against the immediately prior
  revision (added/removed/unchanged counts). A repeated identifier in a changed
  block is a `VERSION_CHANGE_REVIEW` candidate linked to its prior candidate;
  unchanged blocks and cross-document exact matches remain duplicate signals.
  No candidate is merged or overwritten.
- Extraction is intentionally narrow and non-authoritative: it recognizes
  exact CVE/CWE identifier mentions, stores parser/source/block/page/offset
  provenance, detects exact candidate/entity duplicates and creates review
  batch items. It does not infer a vulnerability or weakness claim.
- Connected the `knowledge_ingestion` review domain to batch apply/undo.
  APPROVED/apply records a human-reviewed mention (`REVIEWED`) only; it does not
  publish a KnowledgeCard or modify authoritative vulnerability facts or
  search indexes. Undo restores the prior candidate status.
- Added REST contract/API tests and a PostgreSQL integration test covering
  parsed revision acceptance, page/block/offset provenance, repeated mentions,
  cross-ingestion exact duplicates, historical revision diff/change candidates,
  unparsed-source rejection, prompt-injection risk priority, review apply and
  undo. `cargo test --offline -p intel-db --test parser_pipeline --test
  phase7_maintenance -- --test-threads=1` passed 14/14 with
  `RP_REQUIRE_PG_TESTS=1` against a disposable database on `rip-pg17`; its
  temporary database and temporary superuser role were dropped afterward.
- Additional validation: intel-db library tests 36/36, Clippy
  (`-p intel-db --all-targets -- -D warnings`) clean, apps/server Python tests
  22/22, Knowledge Ingestion API tests 9/9, REST/CLI selftest 33/33,
  `git diff --check` clean, and production SQLite fingerprints unchanged.
- Extended the pinned-parser end-to-end test to submit its committed revision
  into ingestion and assert the resulting job envelope. This environment lacks
  the digest-pinned `rip-parser` image, so that one gated test reports SKIP here;
  no image was installed. Its remaining V-9 execution is still required.
- Remaining: additional document format/parser sample validation, broad
  structured extraction, semantic similarity, full risk/
  quality review UI, authoritative knowledge promotion/index refresh, and
  Agent/MCP Tool + Workflow integration. Do not mark the ingestion architecture
  complete until these are implemented and separately verified.
- DSN loader, systemd start/recovery code and PostgreSQL disaster-recovery
  module remain frozen.

## 2026-11-28 ingestion evidence read surface

- Added workspace- and ingestion-scoped candidate detail retrieval to
  `rp-ingest candidate` and authenticated
  `GET /api/knowledge-ingestions/{ingestion_id}/candidates/{candidate_id}`.
  Returned evidence is bounded to 100 occurrences and 240 characters per
  snippet; source text is explicitly marked untrusted. Wrong-workspace,
  wrong-ingestion and invalid IDs fail closed.
- Added integration assertions for repeated evidence offsets, bounded source
  snippets and ingestion scoping; Knowledge Ingestion API contract tests pass
  11/11. Isolated PostgreSQL suites pass 14/14; Clippy passes with
  `-D warnings`; REST/CLI selftest passes 33/33 and production SQLite
  fingerprints are unchanged.
- Added three MCP capability tools for Knowledge Ingestion:
  `rip.submit_knowledge_ingestion`, `rip.get_knowledge_ingestion_status`, and
  `rip.get_knowledge_ingestion_candidate`. All fail closed without a resolved
  caller, fix workspace to `default`, derive requester from caller identity, and
  invoke the configured `rp-ingest` child (`RP_INGEST_BIN`, default
  `target/debug/rp-ingest`). MCP callers cannot provide a workspace override or
  database DSN. The candidate tool reuses the bounded, untrusted evidence output.
- Verification for this MCP increment: `cargo test --offline -p mcp-server`,
  `cargo clippy --offline -p mcp-server --all-targets -- -D warnings`,
  `cargo test --offline -p rp-mcp-parity`, and
  `SELFTEST_PORT=27864 bash apps/server/selftest_phase9_mcp.sh` pass. The MCP
  self-test reports PASS=26/FAIL=0 and confirms unchanged production SQLite
  fingerprints.
- Remaining blockers: pinned `rip-parser` is absent locally, so real parser-format
  gates remain blocked. Candidate vectorization and review-only similarity
  suggestions are implemented through the pinned `rip-embed` worker, but a
  human-labeled Duplicate/Related/Distinct dataset with independent
  calibration/test splits is still required before quality acceptance.
  Additional structured extractors, factual publication, and approved internal
  Agent/Workflow service credentials also remain incomplete. These MCP tools
  serve authorized MCP callers only; they do not constitute internal
  Agent/Workflow integration. Do not pass database credentials through
  ToolCallRequest, native_rust manifests or arbitrary workers.
- DSN/systemd and PostgreSQL recovery modules remain frozen.

## 2026-11-28 Bounded Nuclei Metadata Extraction

- Added PostgreSQL migration V0017 to admit `nuclei_template` review candidates.
  The Document Revision adapter now recognizes bounded YAML blocks and records
  only the source template ID, `info.name`, and allowlisted severity as
  `nuclei-template-metadata.v1`. IDs are used exactly as supplied for identity;
  title or corpus URL is not used for deduplication.
- Extraction is static and capped at 64 KiB per block. Request/payload and other
  template content is not persisted or executed. Extracted metadata is marked
  untrusted, linked to the template ID's block offset, and remains review-only.
  If the same ID occurs with differing metadata in one ingestion, the review
  payload flags the conflict and preserves up to eight metadata variants.
- Candidate detail returns the schema version, untrusted metadata, conflict
  signal, and existing bounded evidence. CVE/CWE mentions remain a separate
  non-publishable candidate type.
- Matching blocks with invalid YAML/metadata, unsupported severity, or a size
  above 64 KiB now produce bounded diagnostics in the ingestion result (up to
  100 entries plus a truncation count); source text is not copied into errors.
- Verification: intel-db library tests 39/39, Clippy with `-D warnings`, and
  `parser_pipeline` 6/6 against a disposable PostgreSQL container using the
  locally verified `rip-pg17` image. The container used a temporary writable
  layer and was removed after the test. The digest-pinned `rip-parser` image
  remains absent, so real parser-format sample validation is still blocked; the
  synthetic IR fixture is not parser acceptance.
- Remaining: additional structured asset types, semantic similarity,
  authoritative factual publication/index refresh, broader risk/quality review,
  and approved internal Agent/Workflow service credentials and correlation.
- DSN/systemd and PostgreSQL recovery modules remain frozen.

## 2026-11-28 Parser and Embedding Acceptance Update

- Runtime inventory: the `rip-parser` manifest pins
  `sha256:9a59eef52bf80aeb2562d9ca32cada455946c775fe92a0b0fb044a527dd8bf7a`,
  but that image is absent from the local Podman store; no approved OCI archive
  was found in the checked repository/runtime/backup paths. No pull, rebuild, or
  replacement was attempted. Real parser acceptance remains blocked.
- The pinned `rip-embed` image is present at
  `sha256:bca7d0b4343f35afe199a781dbf3794c1d1afdf5924b12105d55dc0fb2291788`.
  Against disposable PostgreSQL, the actual `EmbedWorkerProvider` / Tool
  Engine / `podman_host` path generated and persisted three 512-dimensional
  vectors as `bge-small-zh-v1.5`, worker `0.2.0`. One card-only smoke pair
  measured cosine 0.8358 (paraphrase) / 0.4949 (unrelated) at threshold 0.80.
  This is runtime validation, not candidate semantic-dedup acceptance or a
  calibrated benchmark.
- `gov_semantic` now uses a unique system temporary directory instead of the
  pre-existing `rip-data/parse/test-ops.db` and `test-ws`; the test owns and
  removes only that temporary directory.
- Verification: `gov_semantic` 1/1 against the pinned embed image and disposable
  PostgreSQL; intel-db library tests 40/40; Clippy `-D warnings`, rustfmt and
  `git diff --check` pass. The previous `parser_pipeline` 6/6 is PostgreSQL
  integration coverage over synthetic IR, not real parser acceptance.
- P1 still lacks a curated 40+ real-sample corpus and parser execution.
  Candidate-level Duplicate/Related/Distinct labels, metrics, authoritative
  factual publication and the Agent/Workflow return path are also incomplete.
- Frozen DSN/systemd and PostgreSQL disaster-recovery modules were not changed.

## 2026-11-28 Candidate Semantic Suggestion Implementation

- Added PostgreSQL V0018 candidate vector and similarity suggestion tables for
  static Nuclei metadata only. `rp-ingest semantic-scan` reuses the pinned
  `EmbedWorkerProvider` / Tool Engine / `podman_host` route, enforces workspace
  isolation and explicit threshold/top-K bounds, and exposes suggestions as
  review-only in candidate details. It never merges, approves, publishes, or
  deletes candidates.
- Verified against a newly created disposable PostgreSQL container using the
  pinned `rip-pg17` digest and pinned `rip-embed` digest:
  `parser_pipeline` 6/6 passed, with the parser container test explicitly SKIP
  because the pinned parser image is absent. Its candidate fixture generated
  two vectors, persisted 512-dimension/model/worker metadata, stored a
  review-only similarity suggestion, and passed an idempotent rescan. The
  candidate test also injected a missing embed manifest and a suggestion-table
  constraint failure: no partial vectors were written, failed suggestion
  persistence preserved the prior result, and both retries passed.
- Re-ran `gov_semantic` 1/1 against the pinned embedding image; the card-only
  smoke remains separate from candidate-quality acceptance. `intel-db` library
  tests 41/41, `cargo clippy --offline -p intel-db --all-targets -- -D warnings`,
  package rustfmt check, `git diff --check`, and `rp-ingest --help` passed.
- The candidate smoke threshold is not calibrated. No human
  Duplicate/Related/Distinct labels, independent splits, metrics, real parser
  corpus, factual publication/index refresh, parser-process exit recovery, or
  PostgreSQL connection interruption evidence exists. These remain release
  blockers; synthetic IR integration is not parser acceptance.
- The disposable PostgreSQL test container and test-owned temporary worker
  directory were removed. DSN/systemd and PostgreSQL recovery modules remain
  frozen and unchanged.

## 2026-11-28 final regression verification

- `cargo test --offline --workspace` passed after the M4 projection-count and
  SK3 network-guard test corrections: 1,485 passed, 0 failed, 6 explicitly
  ignored (two opt-in latency benchmarks, three installed-corpus audits, one
  Chromium-sensitive browser-capture test). Formatting checks for changed Rust
  packages and `git diff --check` pass.
- `cd web && npm run test:visual:gate` passed the production build and all 85
  Playwright visual/accessibility tests. The Web `format:check` still reports
  differences in unchanged `pnpm-lock.yaml` and
  `tests/m3-components.test.tsx`; not modified as unrelated.
- These checks do not clear ingestion acceptance blockers: the exact pinned
  parser image and 40+ real Golden Corpus are unavailable; human
  Duplicate/Related/Distinct labels and independent metrics are absent; factual
  publish/index refresh and parser/PostgreSQL interruption drills remain
  unverified. Do not mark production-ready or push until those gates are
  satisfied. DSN/systemd and PostgreSQL recovery modules remain frozen.

## 2026-11-28 P0 closure / P1 start

- P0 is complete as a project-state audit and freeze phase. `Production Ready =
  false` remains unchanged.
- P0 deliverables: `PRODUCTION-BASELINE.md`,
  `PRODUCTION-BLOCKERS.md`, `PRODUCTION-TEST-MATRIX.md`,
  `PRODUCTION-DEPENDENCY-MATRIX.md`, and `PRODUCTION-DATA-INVENTORY.md`.
- P1 started with Batch A (core execution capability) and Batch B (data
  protection). Batch C and Batch D remain gated.
- Batch B SQLite roundtrip passed 16/16, including backup, controlled damage,
  restore, FTS rebuild, retention archive and production fingerprint checks.
- Batch A remains blocked: exact pinned `rip-parser`
  `sha256:9a59eef52bf80aeb2562d9ca32cada455946c775fe92a0b0fb044a527dd8bf7a`
  is absent.
- P1 retry found and fixed a MCP Knowledge Ingestion compile defect:
  `RunChannel` now exposes and dispatches `rip.submit_knowledge_ingestion`,
  `rip.get_knowledge_ingestion_status`, and
  `rip.get_knowledge_ingestion_candidate` through the existing `rp-ingest`
  process boundary. Targeted MCP tests and workspace Clippy pass.
- P1 batch 1 host parser implementation: `rp-doc parse` now defaults to
  `HostParserProvider`, reusing Document Router, Tool Engine and Python
  parsing worker. `--container` selects the Podman fallback explicitly.
- Added real host smoke tests for Markdown/TXT IR, PDF, DOCX, PPTX and XLSX.
  PDF/Office/text/OCR passed after refreshing the Tesseract 5.5.5 SHA-256 pin.
- P1 batch 2 retrieval quality: the application evaluator is available in
  `apps/server/rip_retrieval_evaluator.py` and rejects
  unlabeled/invalid/noisy ranked rows, duplicate source-document identities,
  and AI-only provenance. The 301-row v3 set is blocked at
  `annotator=ai-assist`; no human MRR/NDCG metrics are claimed. Added metrics
  JSON snapshot discipline and application-level dual-mode regression tests.
- P1 batch 3 PostgreSQL recovery: the existing `rip_pg_recovery_gate.py`
  remains unchanged as a protected script; independent restore is in
  `apps/server/rip_pg_independent_restore.py`, which restores a table dump into
  `template0`, verifies 2/2 rows and cleans source/restore objects.
  Isolated local gate passed `PG_INDEPENDENT_RESTORE=PASS`; formal backup
  owner/frequency/offsite/RTO/RPO sign-off remains open.
- P1 final continuation: added host OCR SHA-256 pin refresh, real-corpus plan/
  execute automation, app-layer AI/human retrieval evaluator, independent
  PostgreSQL restore app script, and a host-backed Workflow restart recovery
  drill. The workflow drill passed: run `wfr-*` persisted, was read after a new
  CLI process, and returned `succeeded` with evidence and tool-run trace.
- Scope correction: a 40+ corpus suite, human Ground Truth, external DeepSeek
  credentials, public HTTPS/TLS, and formal offsite backup/RTO/RPO are not
  startup gates for the local automated knowledge pipeline. They remain optional
  scale/deployment additions.
- Host-first parser verification passed for PDF text, Tesseract OCR,
  Pandoc Markdown and LibreOffice DOCX/PPTX/XLSX conversion. The exact
  parser container is no longer a default P1 dependency.
- MCP knowledge closure implemented: `rip.search_knowledge`,
  `rip.get_knowledge_detail`, `rip.list_knowledge_review_queue`,
  `rip.submit_knowledge_candidate`, and `rip.review_knowledge_candidate`
  are wired. Workspace is fixed to `default`; AI callers cannot override it;
  MCP review excludes `approve`, and publication remains under the existing
  human/service review gate.
- MCP/REST capability parity passes 5/5. Candidate-envelope enforcement,
  evidence minimum, source hash and AI-approval rejection are covered by
  MCP ingestion dispatch tests.
- Authorized real-target review on `002237.xyz` found valid TLS, restrictive
  security headers and no low-risk exposure finding in the bounded pass.
- Publication replay closed PB-010/PB-012 (2026-11-28): the full
  candidate-to-publication chain is regression-locked at two levels.
  Library: `crates/intel-db/tests/publication_replay.rs` (2 live-PG tests,
  `RP_TEST_PG_DSN` gated) proves donor submits land `CANDIDATE` only,
  approve-before-score / hard-flag / rejected-content gates block unsafe
  publication, and after human approval the default search returns the card
  with no separate index rebuild step (bm25 index follows row updates).
  Binary: `apps/server/selftest_knowledge_publish.sh` 18/18 PASS on an
  isolated `rip-pg17` container, real `rp-mcp` stdio JSON-RPC, caller resolved
  through `rp-admin caller-add`/`key-gen`, production `migrations/platform*.db`
  fingerprints unchanged. MCP `approve` stays rejected ("MCP never approves");
  the human/service approval face is `rp-kc score` + `rp-kc review
  --decision approve`.
- Fix found by the replay: rmcp can dispatch parallel `tools/call` requests,
  so two `rp-kc` processes raced refinery first-migration on a fresh database
  (`error asserting migrations table`); `intel_db::run_migrations` now
  serializes with a session `pg_advisory_lock` released before the connection
  returns to the pool. intel-db suite 87 passed / 0 failed, clippy clean;
  live replay 2/2. Spec: `docs/knowledge-governance/MCP-KNOWLEDGE-CANDIDATE-SPEC.md`
  (Publication replay section).
- Remaining production-blocker register closed 2026-11-28 (PB-001/003/004/005/
  006/007/008/009; PB-002 stays post-launch by design). All work on isolated
  disposable environments; production `migrations/platform*.db` and RTIS
  `knowledge.db` untouched; no commit/push (worktree discipline unchanged).
  - PB-001: 41 real documents (8 formats, zero personal files) through
    `scripts/rip_golden_corpus_build.py` (new) + `rip_golden_corpus_ingest.py
    --execute` on isolated rip-pg17, 41/41 pass; re-parse idempotent and CAS
    dedup proven (44 docs / 40 distinct sha). Three parser defects fixed:
    run-DB manifest re-register raced VersionComparable (now register-if-absent),
    txt route wrote `document_type=txt` outside the IR enum (now maps to
    `other` like detect), empty-text documents emitted a fabricated empty
    block (now skipped, overall=poor drives REVIEW_PENDING). 1 MiB stdout cap
    boundary documented (multi-MB PDFs excluded by corpus builder).
    Report: docs/knowledge-governance/GOLDEN-CORPUS-ACCEPTANCE-2026-11-28.md.
  - PB-003: ground-truth characterization acceptance
    `apps/server/selftest_characterization_acceptance.sh` 9/9 (12 cards,
    3 exact / 1 near / 4 distinct; D2 SAME_CONTENT exact; D4 NEAR_DUPLICATE
    at threshold 0.85 with GT pair cosine 0.8825 logged in relation evidence;
    distinct zero false-merge; no auto-merge asserted). Matrix artifact:
    docs/knowledge-governance/CHARACTERIZATION-MATRIX-2026-11-28.json.
  - PB-004: dual-track enforcement added to
    `scripts/security-intel/evaluate_retrieval.py` (--label-mode; AI rows
    exit-3 rejected from human-GT track); selftest_qrel_discipline.sh 9/9;
    v3 301 AI rows evaluated in ai-assisted track only (mrr 0.9286) with
    artifact docs/knowledge-governance/retrieval-eval-v3-ai-assisted-2026-11-28.json.
    Human labeling remains the user's queued task per annotation guide.
  - PB-005: docs/operations/OFFSITE-BACKUP-POLICY.md (RPO/RTO per store,
    daily chain, retention, quarterly restore drill) +
    selftest_offsite_backup.sh 9/9 (backup → verify-archive → rsync offsite
    + sha256 → restore reconciliation → reten-archive; prod fingerprints clean).
  - PB-006: selftest_llm_readiness.sh 10/10 (five BLOCKED/READY boundary
    states, secret never echoed). Live provider call stays conditional on
    real RIP_LLM_* credentials.
  - PB-007: containerfiles/https-cutover/ (Caddyfile + nginx sample),
    docs/operations/HTTPS-CUTOVER-RUNBOOK.md; production probe hardened
    (RP_PRODUCTION_CACERT / RP_PROBE_TOKEN / RP_PROBE_PORT; fixed
    host-with-port openssl connect bug); selftest_https_entry.sh 10/10 on a
    disposable DB clone with local TLS-terminating proxy harness
    (apps/server/https_proxy_probe.py, drill-only).
  - PB-008: selftest_poc_execution_drill.sh 7/7 — isolated self-owned
    loopback container targets, nuclei v3.11.1 (rate-limited, no OAST),
    positive hit=1 / negative hit=0, raw curl double-check, containers
    removed after run. Template verified and backfilled to RTIS KB
    (id 900065, verified=true, used_on logged). Evidence:
    docs/phase-pb008/POC-EXECUTION-DRILL-2026-11-28.md. Destructive POC
    execution remains explicitly out of scope.
  - PB-009: selftest_recovery_drills.sh 18/18 — parser SIGKILL mid-parse
    (health alive, replay APPROVED), live container restart mid-ingest (all
    requests conclude, conditional replay fills gaps, CAS zero-duplicate),
    parse --timeout guard concludes, full replay idempotent.
    GNU find -size unit rounding pitfall recorded in the script header.
  - Register-side red line: the B2 protected-scripts test
    (crates/tool-engine/tests/first_batch_b2.rs) now explicitly registers this
    batch's audited scripts/ modification (evaluate_retrieval.py, PB-004
    dual-track enforcement); scripts/rip_golden_corpus_build.py enters as an
    untracked new file (allowed by the ?? rule). Frozen paths
    (scripts/security-intel/data/, download_trickest_cve_pages.sh,
    test-results/) remain untouched; production fingerprints unchanged
    (platform-search 2da952ca…, dev-p6 0c1091dd…).
