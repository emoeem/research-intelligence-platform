# Phase 5 阶段说明：Web Workbench 基础版

| 项     | 值                                                                                                       |
| ------ | -------------------------------------------------------------------------------------------------------- |
| 状态   | 生效中（P5-1 起随批次滚动更新）                                                                           |
| 日期   | 2026-11-10（首次制定，P5-1 批次内落稿）                                                                    |
| 基准   | 计划 §14 Phase 5（796 到 806 行）、计划 §10.6/§10.7、ROADMAP §6（351 到 401 行）与 §1.2 车道纪律、TELEGRAM §5 |
| 相关   | docs/phase0/PHASE-ROADMAP.md、docs/phase0/TELEGRAM-ADMIN-UI-REUSE.md、docs/DOC-STANDARD.md、web/README.md   |

本文件按参考计划 §24 模板（目标/范围/非目标/现状/设计/实施批次/风险/验收/回滚/交付物）编写，是 Phase 5 的阶段说明（ROADMAP §1.2 末段纪律：每个 Phase 开工前出阶段说明，本路线只冻结阶段间依赖与批次切分）。

## 目标

建立 RIP 自己的 React/TypeScript 前端应用 `web/`（独立 npm 工程，不依赖 telegram-bot 仓库整体）：复用其已验证的布局模型与解耦 primitives，按 ROADMAP §6.1 交付 Overview、Projects、Knowledge、Tool Center、Runs、Runtime 六页，统一 API Client、路由懒加载、主题与全局搜索，桌面与移动端适配纳入验收。基础版以「壳层可跑、逐页接数据」为节奏。

## 范围

- 新增 `web/` 目录（独立 npm 工程，包管理器 npm，依赖只进 `web/node_modules`，不全局安装）。
- 新增 `docs/phase5/` 阶段文档。
- `apps/server/` 只做只读增量（P5-3：rp_server.py 追加五端点与 RP_AUX_DB 只读 ATTACH 通道；既有 4 端点与鉴权行为零回归，以 selftest_auth.sh 全过为准）。
- 不修改：`crates/`、`migrations/`（不写 `platform*.db`，aux 通道只读 ATTACH）、RTIS 生产行为、M0 到 M8 已交付能力。

## 非目标

- 不在本阶段实现具体页面数据（逐页批次承接）。
- 不做视觉回归与 a11y 门禁（P5-9 批次）。
- 不做 PWA/manifest、离线缓存。
- 不实现 api-server crate（顺延登记见「实施批次」第 6 节）。
- 不整体依赖 telegram-bot 仓库，不复用其业务页面与 Telegram 认证（TELEGRAM §4 三级不复用清单）。

## 现状（2026-11-10 实测）

- 仓库此前无前端应用：`apps/` 下只有 `server/`（rp_server.py），API_ROUTES 实测四端点 `/api/stats`、`/api/resources`、`/api/reviews`、`/api/tasks`（rp_server.py:337 到 342），监听 `127.0.0.1:7800`（rp_server.py:38 到 39），`mode=ro` 只读（rp_server.py:460）。
- Rust 侧 crates 实测 11 个：audit-engine、context-broker、database、domain、quality-engine、resource-engine、runtime-gateway、search-engine、tool-engine、tool-registry（`ls crates/`）。
- 复用来源 telegram-bot `admin/` 处于活跃开发（盘点时点 2026-11-09，TELEGRAM §5），RIP 只在抽取时点取快照，不回同步。
- 宿主环境实测：node v24.22.2、npm 12.2.2 在位，`npm ping` 返回 PONG；npm 全局包基线 6 个（@mnemon-dev/mnemon、agent-browser、docx、novel-writer-skills、pptxgenjs、remotion）。

## 设计

### 技术栈（版本锁定，2026-11-10 逐包 npm view 实测）

| 依赖                                       | 版本      | 角色                                             |
| ------------------------------------------ | --------- | ------------------------------------------------ |
| vite                                       | 8.3.7     | 构建（Vite 8 为 Rolldown 内核）                   |
| react / react-dom                          | 19.3.1    | UI 运行时                                        |
| react-router                               | 8.5.1     | 路由（全路由 lazy）                               |
| typescript                                 | 7.0.2     | 类型检查（tsc --noEmit，native 版）               |
| tailwindcss + @tailwindcss/vite            | 4.3.5     | 样式（CSS-first token）                           |
| daisyui                                    | 5.7.50    | 组件样式层（只用 RIP 自定义主题，不启用内置主题） |
| vitest + jsdom + @testing-library/react    | 5.1.0 / 31.0.0 / 16.3.3 | 单测（首批 App 壳冒烟）             |
| prettier                                   | 3.9.10    | 格式化门（format:check）                          |
| @tanstack/react-query                      | 5.107.1   | 请求层（P5-1 已接线 Provider，数据面 P5-4 起用）  |
| lucide-react                               | 1.70.0    | 图标（壳层已接线）                                |
| zod                                        | 4.6.7     | Schema 校验（已锁定待用：P5-4 契约与 P5-5 表单）  |
| echarts                                    | 6.1.0     | 图表（P5-2 封装已落地：按需注册，冒烟用例实证）   |
| echarts-for-react                          | 3.0.6     | ECharts React 绑定层（P5-2 引入，参照仓同款组合） |

依赖分两类登记：**已接线**（react 系、react-router、@tanstack/react-query、lucide-react；P5-2 起 echarts 封装落地，echarts-for-react 同批引入）与**已锁定待用**（zod：为杜绝后续批次版本漂移，一次锁定，进打包产物随首个消费页面）。版本一律精确锁定（无 `^`），lockfile 为 `package-lock.json`。

### 主题系统

机制整建制沿用 TELEGRAM §2.3（实测锚点：admin/src/theme.ts、admin/index.html:11 到 33、admin/src/index.css:14 到 93），色值全部按 RIP 重定义（青碧色系，不搬 foxgold 金色系）：

- 双层 token：daisyUI 变量契约层（`@plugin "daisyui/theme"` 定义 `rip-dark`/`rip-light` 两主题）+ Tailwind 4 `@theme` 应用层（语义圆角 rounded-surface/rounded-control、统一控件高度、确定性 CJK 字体栈）。
- 首帧防闪烁：index.html 内联脚本在首绘前解析 `localStorage["rip_theme"]` 并设置 `data-theme`，与 `src/theme.ts` 运行时解析保持同一映射（白名单 + 退役 id 归一化 + 默认 `rip-dark`）。
- 主题无关语义面（edge/sidebar/surface-2）走 `[data-theme]` 块 + `@theme inline` 映射。

### 路由与分包

- 六个一级路由全部动态 import + Suspense 骨架（RouteFallback），首屏只带壳 + 当前路由（TELEGRAM §2.4）。
- vendor 分组用 Vite 8 Rolldown 的 `codeSplitting.groups`：vendor-react（react/react-dom/scheduler/react-router）、vendor-query、vendor-icons、vendor-echarts、vendor-zod。react-router 并入 vendor-react 是安全决策：RIP 单入口，不存在 telegram-bot 双入口把 170KB router 发给公开页访客的问题（教训原文在其 admin/vite.config.ts:17 到 21）。
- echarts 等重型库不进 vendor 组，等 P5-2 封装时按动态 import 按需加载（计划 §10.6）。
- 开发期 `/api` 代理到 `127.0.0.1:7800`（rp_server.py 四只读端点），前端不直连数据库。

## 实施批次

ROADMAP §6.3 的批次表（最小可验证单元与并行性照录，P5-1 已完成，判据按本文件「验收」第 2 节 npm 口径执行）：

| 批次  | 内容                                                                                                     | 并行性                                              |
| ----- | -------------------------------------------------------------------------------------------------------- | ---------------------------------------------------- |
| P5-1  | 脚手架：Vite 8 + React 19 + TS + Tailwind 4/daisyUI 5 + 全路由 lazy + vendor 分组 + 首帧主题内联          | 【并行】车道 B 起点，不依赖 Phase 2 到 4              |
| P5-2  | 基础件包：focus-trap、Dialogs、ui primitives、ErrorBoundary、泛化 useApi、EChart 封装（TELEGRAM §3 切点） | 【并行】依赖 P5-1                                    |
| P5-3  | 壳层：Layout 模式重写 + RIP 版 workspaces 数据模型 + 全局搜索面板换源                                     | 【并行】依赖 P5-2                                    |
| P5-4  | Application API 骨架（api-server crate，见下方顺延登记）                                                  | 【并行】可与 P5-1 到 P5-3 同步（接口契约先行）        |
| P5-5  | Overview 页 + Knowledge 页                                                                                | 【串行】依赖 P5-3 与 P5-4                            |
| P5-6  | Tool Center 页                                                                                            | 【串行】依赖 P5-3、P5-4 与 Phase 1 Registry          |
| P5-7  | Runs 页 + Runtime 页                                                                                      | 【串行】依赖 Phase 2 P2-4 起的数据面                  |
| P5-8  | Projects 页最小版                                                                                         | 【串行】依赖 P5-3 与 P5-4                            |
| P5-9  | 视觉回归与 a11y 门禁（playwright 静态服务器 + 页面矩阵 + axe AA 并轨 + CI 接线）                           | 【并行】可与 P5-5 到 P5-8 同步搭骨架，逐页入矩阵      |

本阶段的执行编排把 P5-1 切为 10 个实施批次（第 1/10 批即本文件所属批），阶段级切分以上表为准，批次合并与顺序以各批次验收结果为准，不与本表冲突。

### 偏差登记 1：api-server crate 顺延

- 原计划：ROADMAP P5-4 建 api-server crate（计划 §13:723 crate 清单），先覆盖 Phase 5 页面只读接口，并把 apps/server 既有 4 端点切到 engine 接口或桥接。
- 顺延决定：api-server crate 在车道 B 壳层批次（P5-1 到 P5-3、P5-9）内不实施，顺延到数据页批次开工前再排期。
- 理由：ROADMAP §1.2 车道表明文「P5-1、P5-2、P5-3、P5-9 只依赖 Phase 0 的 UI 复用盘点，不依赖任何新后端」；壳层与冒烟由 apps/server 既有 4 只读端点（dev 代理已接线）支撑足够。
- 不变的约束：ROADMAP §6.5 风险 1 的时序约束继续生效，即 P5-5 数据页开工前必须先落 api-server 骨架，或经裁决明确降级口径（临时桥接 apps/server）；「rp_server.py 直连 SQL 不再扩大」（盘点 §4）目标不变，任何新页面不得新增直连 SQL 的端点。
- 影响面：P5-4 的接口契约设计不延后（契约先行原则保留），仅 crate 实现排期顺延。
- 本阶段落地形态（P5-3 批次，2026-11-10）：api-server crate 仍顺延；本批在 apps/server/rp_server.py 增量落地五个只读端点（/api/overview、/api/knowledge、/api/tools、/api/runs、/api/runtime）与 RP_AUX_DB 只读 ATTACH 通道，作为数据页批次的过渡 API 面。crate 化（engine 接口化、迁往 Rust）留待 ROADMAP P5-4 正式批次；rp_server.py 直连 SQL 的架构债收敛目标不变：五端点全部只读、缺库缺表走 sqlite_master 守卫返回真实空态说明、不新增任何写路径。

### 偏差登记 2：npm 替代 pnpm

- 原文：ROADMAP §6.3 P5-1 判据写 `pnpm typecheck` 与 `pnpm build`，§6.4 验收命令写 `pnpm test:visual:gate`。
- 实际口径：包管理器固定为 npm（npm 12.2.2 实测在位、`npm ping` PONG；宿主机未装 pnpm，红线不在宿主机安装任何工具）。命令一一映射：`pnpm typecheck` 对应 `npm run typecheck`，`pnpm build` 对应 `npm run build`，`pnpm test:visual:gate` 在 P5-9 按同口径接线（脚本名保留 test:visual:gate）。
- 影响面：仅命令与 lockfile 形态（`package-lock.json`）不同，工程行为等价；后续 CI 接线统一 npm 口径，不再出现 pnpm 命令。

## 风险

1. 架构债复发（ROADMAP §6.5 风险 1）：新页面图省事直连 SQL 会复制 rp_server.py 模式；约束见偏差登记 1 的「不变的约束」。
2. telegram-bot 侧快照漂移（§6.5 风险 2）：admin 活跃开发，复制时点即冻结点，不回同步。
3. 视觉回归字体环境（§6.5 风险 3）：快照基线只能由 CI 字体环境生成；token.css 已内置确定性 CJK 字体栈，属预防措施，基线生成待 P5-9。
4. Playwright 浏览器二进制与 CJK 字体在无 sudo 约束下的可行性未验证（§6.5 风险 4，待验证）。
5. 主题对比度是设计目标值，本批未逐对实测（token.css 内已标注待验证），P5-9 axe AA 门禁收口。
6. TypeScript 7.0.2（native tsc）较新：本批 `tsc --noEmit` 与 vite build 实测通过，后续新增依赖或配置时需重验兼容性。
7. npm install 未做逐包许可审计（可参考 telegram-bot admin/DEPENDENCIES.md 的口径，待验证项）。

## 验收

### P5-3 批次判据（已执行，2026-11-10）

1. `bash apps/server/selftest_auth.sh`：11 PASS / 0 FAIL，退出码 0（鉴权、只读 405、路径穿越、静态安全全过）。
2. `bash apps/server/selftest_phase5.sh`（本批新增）：29 PASS / 0 FAIL，退出码 0（五端点库直查对账、aux 守卫降级、既有端点不受 aux 挂载影响、无 RP_AUX_DB 时空态说明）。
3. 计数对账（API vs sqlite 库直查，aux=migrations/dev-b5.db）：/api/knowledge total=46、/api/tools total=20（主库注册面）、/api/tasks total=42、/api/runs total=3；/api/runtime 表在数据空（真实空态说明）。knowledge=46、tasks=42、tool=20、tool_run=3 与库内实测一致。
4. `python3 apps/server/rp_server.py` 起服后 curl 五端点返回 JSON：?tool_id= 详情含 input/output Schema 与 tool_version（契约工具 rip.kb.rtis 主库未命中回退 aux，返回真实 JSON Schema 与 1 个版本；nmap 详情 Schema 为 NULL 属真实数据态）；?run_id= 详情含全字段（JSON 列解析）与 artifacts 列表；未知 id 404、非法 status 400。
5. 测试架修正登记：selftest_auth.sh 第 7 步对本批改动前的原版 rp_server.py 同样失败（git stash 实测复现：仓库根 .rp-workbench-token 存在，原脚本只清环境变量未清文件令牌来源，401 而非开发模式），属先在环境漂移非本批回归；修正为显式把 RP_AUTH_TOKEN_FILE 指向空文件，服务端行为零改动。
6. `grep -n "mode=ro" apps/server/rp_server.py`：主连接 URI（rp_server.py:108）、aux ATTACH URI（rp_server.py:129）、启动横幅口径不变；PRAGMA query_only = ON 覆盖整个连接（含挂载的 aux 库），migrations/ 下各库零写入。

### P5-2 批次判据（已执行，2026-11-10）

1. `npm install -E echarts-for-react@3.0.6`：exact 锁定入 package.json 与 package-lock.json；全局包对比仍 6 个无新增。
2. `npm run test -- --run`：退出码 0，5 个测试文件 25 用例全过。覆盖项：api 层拒收 text/html 响应（HTML 错误页抛 ApiError 不伪装成功）、401 派发 rip:auth-expired 事件、Bearer 头注入、global-search 全角折叠与 AND 多词匹配与前缀排序、focus-trap Tab 首尾循环与面板外拉回、EChart 懒加载注册链在 vitest 下实渲染 svg。
3. `npm run typecheck`、`npm run build`、`npm run format:check`：退出码全 0。
4. 解耦验收：`grep -rn telegram web/src` 无命中（退出码 1；参照仓字样从 src 注释中全部改写为「参照实现」并指向 docs/phase0 盘点文档）。
5. EChart 注册口径按 echarts 6 调整：v6 已把 barrel 重组为纯 install 函数再导出（typed + tree-shakable），v5 的 lib/* 深子路径被 exports 映射挡住（TS2307 实测），两版差异与 esm/core CJS 坑注释随组件留存。vendor-echarts chunk 随首个消费页面进入构建图（P5-5 起核对体积）。

### P5-1 批次判据（本批已执行，2026-11-10）

1. `npm install`：111 包入 `web/node_modules`，生成 `package-lock.json`；`npm ls -g --depth=0` 前后对比无全局新增（基线 6 个全局包，安装后仍 6 个）。
2. `npm run build`（内含 `tsc --noEmit`）：退出码 0；`dist/assets/` 出现 6 个路由级独立 chunk（OverviewPage、ProjectsPage、KnowledgePage、ToolCenterPage、RunsPage、RuntimePage）与 vendor 分组（vendor-react 255.50 kB、vendor-query 24.87 kB、vendor-icons 5.34 kB）。
3. `npm run test -- --run`：退出码 0，3 用例全过（壳层品牌与主导航、默认路由懒加载、未知路由重定向）。
4. `npm run typecheck`、`npm run format:check`：退出码 0。
5. `dist/index.html` 含首帧 `data-theme` 内联脚本片段。
6. 未进打包产物的依赖核验：vendor-echarts 与 vendor-zod 分组不出现在 `dist/assets/`（echarts、zod 尚无 import 点，与「已锁定待用」登记一致）。

### 阶段级验收命令（按偏差登记 2 的 npm 口径）

~~~bash
cd web
npm run typecheck && npm run build     # 构建门
npm run test -- --run                  # 单测门
npm run format:check                   # 格式门
# P5-9 落地后补：npm run test:visual:gate

cd /home/emo/code/pensting/research-platform
cargo test --offline --workspace       # api-server crate 落地批次起纳入（顺延登记 1）
grep -n "mode=ro" apps/server/rp_server.py | head -3   # 既有 4 端点只读口径不变
~~~

## 回滚

- `web/` 与 `docs/phase5/` 均为纯新增目录：`git clean` 或删除两目录即完全回滚，不影响任何既有文件。
- 无迁移、无 crates 改动、无 apps/server 改动、无全局安装（可复核 `npm ls -g --depth=0` 与基线一致）。
- apps/server 侧如曾为联调启动过进程，直接停止即可，rp_server.py 本身未被修改。

## 交付物

- `web/`：package.json、tsconfig.json、vite.config.ts、index.html（含首帧主题内联）、.gitignore、.prettierrc.json、.prettierignore、README.md、src/main.tsx、src/App.tsx、src/AppShell.tsx、src/theme.ts、src/nav.ts、src/styles/token.css、src/routes/（RouteFallback、PagePlaceholder、六页占位）、tests/app-smoke.test.tsx、package-lock.json。
- `apps/server/`：rp_server.py（9 只读端点 + RP_AUX_DB 通道）、selftest_phase5.sh（P5-3 新增）、selftest_auth.sh（第 7 步测试架修正，2026-11-10）。
- `docs/phase5/PHASE5-PLAN.md`（本文件）。
- 各批次验收结果随批次产出（本批自验记录见「验收」各批次判据）。
