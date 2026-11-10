# 更新说明 · 2026-11-11（RIP 2.0 Phase 5：Web Workbench 基础版）

> Web Workbench 基础版落地：五个基础页面与全局搜索接真实后端（Overview、Knowledge、Tool Center、Runs 与 Run 详情、Runtime，各自走本阶段新增的五个只读端点），Projects 最小版走既有 /api/tasks 同批交付。修掉两类隐患：egress 代理三处时序竞态（放行前探活、关闭断言有界轮询、事件先于响应落账）与迁移测试并行共用库副本的竞态。cargo/python/web 全门禁当日实测全绿。

| 项         | 值                                                                                                                                    |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| 阶段交付   | git `c0a4284`（02:31 CST 收口入库：egress 门禁修复、react-query 迁移、文档；前序实施快照 25d101e、2b65cf2、55e72ea；本行短哈希由 docs 索引回填提交落定） |
| 数据库迁移 | 无新增（`git status --short migrations/` 当日实测仅 dev-p13.db-journal 运行时产物清出一行；生产 platform*.db 零写入）                  |
| 自动化测试 | cargo test `--offline --workspace` 一次性实测 passed=273 failed=0 ignored=0（59 个测试二进制全 ok）；worker unittest Ran 125 OK skipped=11；selftest_phase5 PASS=31 FAIL=0；selftest_auth PASS=11 FAIL=0 |
| 五道门禁   | web 五门禁当日实测：typecheck 零错误；vite build 成功 17 个 JS chunk（8 个路由页独立 chunk + vendor-react/query/icons/echarts 分组，入口 20.06 kB，626ms）；vitest 11 文件 69 用例全过；test:visual:gate 30 用例全过（22.2s）；prettier format:check 通过 |
| 静态检查   | cargo clippy `--offline --workspace --all-targets -- -D warnings` 零告警（当日实测，Finished 无警告输出）                              |
| 验收结论   | 阶段验收通过（docs/phase5/PHASE5-ACCEPTANCE.md，2026-11-10 落笔；门禁 2026-11-11 复测全绿）                                            |

## 一、面向使用者的变化（页面清单与真实数据源）

1. **五个基础页面接真实 API**：Overview（/，KPI 卡可点击下钻）、Knowledge（q= 检索 + 防抖 + 详情侧栏）、Tool Center（目录 + lifecycle/governance 过滤 + 详情 Schema 与版本历史，试运行禁用占位）、Runs（列表 + 详情四 JSON 块 + artifact 预览入口）、Runtime（节点 kind/health/version/心跳 + 真实空态）；Projects 最小版（42 个存量任务 + by_state/by_type chips + ?task_id 高亮）同批交付。全局搜索为 ⌘K 面板（GlobalSearchPalette），源已切到 RIP 工作区数据模型。
2. **页面与数据源对照**（对账数字来自 docs/phase5/PHASE5-ACCEPTANCE.md 第二节，2026-11-10 起服对账）：Overview 走 /api/overview + /api/stats（46/42/81/700/165/20/3）；Knowledge 走 /api/knowledge 与 q= 检索（46 条，检索 3/3/0）；Tool Center 走 /api/tools（20 个登记工具，governance=ACTIVE 20）；Runs 走 /api/runs（3 条，221/208/927 ms）与详情；Runtime 走 /api/runtime（零行真实空态）；Projects 走既有 /api/tasks（42 条六态三类计数）。
3. **呈现与适配**：rip-dark/rip-light 双主题（首帧主题内联防闪烁）、1440x900 与 390x844 双视口、全路由 lazy 加载 + vendor 分组（当日 build 实测 17 个 JS chunk，8 个路由页独立 chunk，入口 chunk 无页面代码）。

## 二、API 面（五个新增只读端点）

1. 新增端点 /api/overview、/api/knowledge、/api/tools、/api/runs、/api/runtime（apps/server/rp_server.py:788 到 792 的 ROUTES 表），对应 ROADMAP P5-4 的本阶段落地形态；既有 4 端点（/api/stats /api/resources /api/reviews /api/tasks）零回归。
2. **RP_AUX_DB 只读 ATTACH 通道**（rp_server.py:129，`mode=ro`）：Phase 1 到 4 数据面（tool_run / artifact / runtime_node 等）尚未并入主库时经只读挂载读取；未配置时守卫降级为空态说明（selftest_phase5 含此对账行）。
3. 鉴权沿既有口径（Bearer constant-time 比较 + 失败节流）；只读 405 与路径穿越零回归由 selftest_auth PASS=11 实测覆盖。

## 三、复用与前端工程（focus-trap 等与 telegram-bot 同构、react-query 迁移）

1. **复用清单落地**（来源 docs/phase0/TELEGRAM-ADMIN-UI-REUSE 三级矩阵）：focus-trap（web/src/components/focus-trap.ts）、Dialogs、ui primitives、ErrorBoundary（按路由 re-key）、EChart 封装（components/echarts-install.ts + EChart.tsx）与 telegram-bot admin 同构解耦切点；workspaces 纯数据模型（web/src/workspaces.ts，零 import、图标字符串键、buildFeatureMap）沿 admin/src/workspaces.ts 模型改写为 RIP 两层信息架构（全局图标轨 + 工作区 tab 条）。
2. **react-query 迁移**（本发布提交内容）：web/src/lib/hooks.ts 手写 fetch/abort/reloadKey 整体换为 @tanstack/react-query useQuery 封装，对外形状 { data, error, loading, retry } 不变，页面三态消费方式不变。行为逐项对齐旧语义：staleTime 0（挂载即重取）、retry false（只有手动 retry）、聚焦/网络恢复不重取、keepPreviousData（旧数据留屏）、rip:identity 事件全量失效重取。迁移动因是 QueryClientProvider 此前空转（hooks.ts 注释原文：本 hook 是 src 内唯一 react-query 消费面）；App.tsx 的 QueryClient 默认值对齐旧语义并随挂载惰性创建（测试反复挂载互不串扰）。
3. **依赖纪律**：npm 依赖只进 web/node_modules，零全局安装（npm 全局基线 6 个不变）；版本锁定 Vite 8.3.7 / React 19.3.1 / TS 7.0.2 / Tailwind 4.3.5 / daisyUI 5.7.50（docs/phase5/PHASE5-PLAN.md 技术栈节）。`npm run test` 定为单次运行（vitest run），watch 拆到 test:watch（web/package.json）。

## 四、修复与根因

1. **egress 代理时序竞态三处**（本发布提交，crates/tool-engine）：
   - 放行容器前对代理端口做就绪探活（egress_proxy.rs 新增 wait_port_ready，podman_host.rs execute_container_via_engine 接线）：spawn 已同步 bind 但必须有界确认端口可达（1s 超时按 INFRA 准备失败落账），不放行容器撞未就绪代理。
   - shutdown 后端口关闭断言改有界轮询 wait_port_closed（2s），替代定长 sleep 100ms：内核侧 listen fd 收尾与新连接被拒的呈现有毫秒级漂移，高负载下定长窗口会撞上。
   - 403 拒绝路径事件先落账再回响应：客户端可见响应是「事件已可读」的同步锚点，实测反向排序在抢占下让测试读到空台账。
2. **egress 外网瞬断重试**（tests/podman_host_egress.rs）：实测根因是宿主出站经透明代理面（本机 dae0 eBPF 代理接口在位），上游 edge 偶发在 CONNECT 隧道建立后瞬断，容器内 urlopen 落 SSL: UNEXPECTED_EOF_WHILE_READING；属外网瞬态而非 allowlist/代理语义失败。仅当错误命中该特征时换 run_id 重跑（3 次尝试 + 2s 退避，给透明代理节点健康切换留收敛窗口），其余失败原样断言不掩盖真实回归。当日实录：全量首轮 egress 两用例撞瞬断失败，90 秒后隔离重跑 5/5 全绿，随后全 workspace 一次性 273 全绿，与「重跑即绿且断言本身真实打外网」的判定一致。
3. **迁移测试并行竞态**（2026-11-11 补记，PHASE5-ACCEPTANCE 第五节第 0 条）：三个迁移测试文件（migrations_context_broker / migrations_tool_contract / migrations_agent_data_plane）并行共用 dev 库副本，reset_dev_copy 会删掉对方打开中的库文件（SQLITE_READONLY 1032 / disk I/O 5898）；统一加进程内 static Mutex 串行化，路径契约不变，修复后全 workspace 连续全绿（当日 273 全绿含此修复）。
4. **三族工具镜像被外部清理的恢复**：rip-tool-base/smoke、rip-tool-webresearch、rip-tool-coreutils 被外部清理，按 Containerfile 自带规程重拉同 digest 基础镜像（python:3.12-slim@4840d514）重建三镜像，新 digest 三处同步（tool-manifest.json/yaml 双份、e2e 测试常量、containerfiles/README.md 台账三行）；drill3 与 core_utils/egress/web_research 演练全部恢复通过（前序快照 55e72ea 入库）。

## 五、已知取舍

1. **像素级视觉回归待引入**：Playwright 视觉矩阵已本地实测（30 用例，行为断言口径：零 console.error / 零 pageerror、无横向溢出、每页关键内容标记可见），像素级快照 diff 未启用；基线只能由 CI 字体环境生成（ROADMAP §6.5 风险 3，telegram-bot 提交 a1796e8 教训），待 CI 接线后在统一字体环境生成基线。
2. **axe color-contrast 13 节点已知项**（当日视觉门实测同口径）：/projects x2、/knowledge x2、/tools x3、/runs x3、/runtime x3，总览 0；根因是主题对比度为设计目标值，调色收口排后续批次（serious/critical 级非色彩违规 0 处，门禁断言项）。
3. **egress 重试是对外网瞬态的让步**：3 次 + 2s 退避只对命中 UNEXPECTED_EOF_WHILE_READING 特征的失败生效；透明代理面健康切换的收敛窗口依赖外网质量，极端持续抖动下 3 次仍可能不够（当日首轮即撞上，等待约 90 秒后重跑即绿）。
4. **三项待验证原样有效**（PHASE5-ACCEPTANCE 第五节）：Runtime 实时心跳（Gateway 常驻编排未落地，当前展示登记节点快照）、artifact 预览真实数据回归（dev 库无 artifact 行）、npm 依赖许可审计未做逐包审计。
5. **api-server crate 顺延**（PHASE5-PLAN 偏差登记 1）：本阶段以 apps/server 只读增量过渡，rp_server.py 直连 SQL 不再扩大的约束保持，任何新页面未新增直连 SQL 端点。

## 六、回滚

- 本发布提交整体回滚：`git revert <本发布提交哈希>`（回填后见元信息表阶段交付行）；web/ 与 docs/phase5/ 为新增目录，删除即完全回滚。
- apps/server 变更（五端点 + RP_AUX_DB 通道 + selftest 脚本）均为只读增量，可按 git 历史单文件还原；migrations 零新增零变更（dev-p13.db-journal 删除为运行时产物清出，无库结构影响）。
- cargo 侧修复（探活/时序/测试重试）可按 crates/tool-engine 三个文件还原，行为回退到 Phase 3 交付面；前端依赖回滚还原 web/package.json 与 package-lock.json 后 `npm ci` 重建 node_modules，无全局残留。

## 七、文档索引

- 阶段说明、批次判据与偏差登记：docs/phase5/PHASE5-PLAN.md
- 阶段验收（门禁实测、视觉矩阵、坏消息清单）：docs/phase5/PHASE5-ACCEPTANCE.md
- 前序基线：docs/releases/RELEASE_2026-11-10_PHASE4.md
