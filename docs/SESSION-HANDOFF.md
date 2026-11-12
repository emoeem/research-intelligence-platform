# SESSION HANDOFF：跨会话交接文档（RIP 主线）

更新：2026-11-09　用途：新会话接续执行的唯一状态源。配合自动加载的记忆索引（MEMORY.md）使用。

## 一、授权口径（用户三轮授权，已生效）

1. 项目内全部决策自主推进，无需逐步请示（2026-11-06）。
2. 目标 = 完成主计划书 /home/emo/Downloads/RESEARCH_INTELLIGENCE_PLATFORM_MASTER_PLAN.md 的全部内容（M1→M8）。
3. M8 切流为条件授权：四条硬检查全绿才切（对账 100% / 影子运行差异已解释 / 回滚演练成功 / 切换前快照完备）。
4. 每完成一个阶段目标 → commit + push 到回滚仓库（外发授权已给）。
5. 仍不动：engagements 靶标数据、runs/、knowledge.db 生产库本体（只读）、攻击证据。

## 二、回滚仓库（已建立）

- https://github.com/emoeem/research-intelligence-platform （private，gh 认证可用）
- 本地仓库根：/home/emo/code/pensting/research-platform/（main 分支）
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
- **Phase 7 进行中（2026-11-13 用户暂停点，7 批规划已完成 4 批半成品）**：活体运行 dwfrun-954913b0-9e59-48cd-a2af-1202026edf95（重建脚本 docs/handoff/workflow-script-RIP2-2026-11-12-RESUME.dwf.ts，Phase 5/6 已完成段为核对性说明）。运行历史：2026-11-12 03:20 起跑，当日因 start-plan 1005 配额停一次，续跑（已完成步骤缓存重放）后 2026-11-13 用户手动停。已做批次（半成品，未过阶段级门禁/独立验收/入库，WIP 备份 `7effba9` 已推送）：P7-1 WebUI 壳层升级（五区布局 + 十一工作区 + TabArea/InspectorPanel/ActivityPanel 数据无关通用组件，既有 10 页路由不变迁移，browser-use 真实浏览器实测 9 截图 docs/phase7/browser-e2e/01/；caveat：本机无 /usr/sbin/chromium，视觉门禁用 PW_CHROMIUM 指向宿主 Chrome for Testing 153.0.8010.47，后续批次沿用该环境变量）；P7-2 Agent Engine 最小执行闭环（AgentStore + 状态机 CAS + capability_refs 引用校验 + 计划 §9 八步 + rp-agent CLI 定稿，ROADMAP §8.4 基线命令 exit 0，run_id arn-14fa2ac2020f7c28c6ff26f4 七表对账一致，RELEASE_2026-11-12_PHASE7-P7-2.md）；P7-3 三类 Memory 与 Context Memory 雏形（013 迁移三表 + Evidence 快照三方对账 + FK 反向锚定，评审 docs/phase7/AGENT-MEMORY-REVIEW.md，RELEASE_2026-11-13_PHASE7-P7-3.md）；P7-4 运行控制九类护栏与 AgentRun 持久化轨迹（014 两表，workspace 481 passed 0 failed，RELEASE_2026-11-13_PHASE7-P7-4.md）。P7-5 trace 统一关联与跨栈查询（六 ID 贯穿 Rust/Python/Web）实施中被停；P7-6、P7-7（含 Agent Studio 页）未开工。恢复方式：ResumeWorkflowRun dwfrun-954913b0-9e59-48cd-a2af-1202026edf95，已完成批次走缓存不重跑。

## 四、Flutterfox/Voicefox（第二项目，Phase 0-8 完成，v0.9.0 已发布）

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
4. 接入点：Phase 7 首批做壳层升级（多标签 + Inspector + Activity + workspaces 纯数据模型，参照 telegram-bot workspaces.ts 模式），Agent Studio 落新壳；Plugin Center 在 Phase 7 壳层升级批次中迁移；Phase 8 Workflow Studio 画布直接落新壳；收尾审阅工作流用真实浏览器对全部工作区复验（browser-use）。

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
