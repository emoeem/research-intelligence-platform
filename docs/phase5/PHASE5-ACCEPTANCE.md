# Phase 5 阶段验收报告：Web Workbench 基础版（P5-1 到 P5-10）

| 项     | 值                                                                                              |
| ------ | ------------------------------------------------------------------------------------------------ |
| 状态   | 阶段验收通过（视觉快照基线与实时心跳两项待验证，见「坏消息」节）                                  |
| 日期   | 2026-11-10                                                                                       |
| 基准   | 计划 §14 Phase 5（796 到 806 行）、ROADMAP §6（351 到 401 行）、docs/phase5/PHASE5-PLAN.md        |
| 范围   | research-platform/web/（新增 npm 工程）、apps/server/rp_server.py 只读增量、docs/phase5/          |
| 红线   | migrations/ 零变更（本批 git status migrations/ 实测 0 行输出）；apps/server 既有 4 端点零回归；不在宿主机装依赖 |

## 一、交付物与批次

| 批次  | 内容                                                                                | 状态   |
| ----- | ------------------------------------------------------------------------------------ | ------ |
| P5-1  | 脚手架（Vite 8.3.7/React 19.3.1/TS 7.0.2/Tailwind 4.3.5/daisyUI 5.7.50，全路由 lazy + vendor 分组 + 首帧主题内联） | 通过   |
| P5-2  | 基础件包（api/hooks/format/global-search/ui/Dialogs/focus-trap/ErrorBoundary/EChart） | 通过   |
| P5-3  | apps/server 只读 API 增量（五端点 + RP_AUX_DB 只读 ATTACH 通道，ROADMAP P5-4 落地形态，crate 顺延） | 通过   |
| P5-4  | 壳层（Layout 图标轨 + tab 条 + 抽屉 + re-key ErrorBoundary + workspaces 纯数据模型 + ⌘K 面板） | 通过   |
| P5-5  | Overview 页（/api/overview + /api/stats，KPI 可点击，三态统一）                       | 通过   |
| P5-6  | Knowledge 页（q= 检索 + 防抖 + 详情侧栏）                                             | 通过   |
| P5-7  | Tool Center（目录 + lifecycle/governance 过滤 + 详情 Schema/版本历史，试运行禁用占位） | 通过   |
| P5-8  | Runs/Runtime 页（列表 + 详情 JSON 块 + artifact 预览入口 + 节点心跳 + 真实空态）       | 通过   |
| P5-9  | Projects 最小版（/api/tasks 42 条 + by_state/by_type chips + ?task_id 高亮，时间线裁剪） | 通过   |
| P5-10 | 两视口适配收口 + Playwright 视觉矩阵 + axe AA 尝试 + 本验收文档                        | 通过   |

批次与 ROADMAP §6.3 的次序映射与偏差登记（api-server crate 顺延、npm 替代 pnpm）见 PHASE5-PLAN.md「实施批次」与「偏差登记」节。

## 二、门禁实测（2026-11-10，research-platform/web）

| 门禁                              | 命令                            | 退出码 | 结果                                        |
| --------------------------------- | ------------------------------- | ------ | --------------------------------------------- |
| 类型门                            | `npm run typecheck`             | 0      | tsc --noEmit 零错误                           |
| 构建门                            | `npm run build`                 | 0      | vite build 成功，11 个 JS chunk（含 6 路由独立 chunk + vendor-react/query/icons/echarts 分组，入口 chunk 无页面代码） |
| 单测门                            | `npm run test -- --run`         | 0      | 11 个测试文件 69 用例全过（vitest，jsdom）    |
| 视觉门                            | `npm run test:visual:gate`      | 0      | build + Playwright 30 用例全过（21.3s）       |
| 格式门                            | `npm run format:check`          | 0      | All matched files use Prettier code style!    |
| 服务端自验                        | `bash apps/server/selftest_phase5.sh` | 0 | PASS=31 FAIL=0（q 别名与 governance 过滤对账行在列） |
| 服务端自验（鉴权回归）            | `bash apps/server/selftest_auth.sh`   | 0 | PASS=11 FAIL=0（既有 4 端点 + Bearer + 只读 405 + 路径穿越零回归） |
| 红线核验                          | `git status --short migrations/` | 0 行输出 | platform*.db 与迁移目录零变更                 |

真实 API 对账（各批次实测记录见 PHASE5-PLAN.md 各批次判据节）：Overview 46/42/81/700/165/20/3、Knowledge 46 与 q= 检索 3/3/0、Tools 20 与 governance=ACTIVE 20、Runs 3 条（221/208/927 ms）与详情四 JSON 块、Tasks 42 与六态三类计数、Runtime 零行真实空态，页面取值与 curl 输出逐项一致。

## 三、视觉矩阵与 axe AA（P5-10）

- 矩阵：6 页 × 2 主题（rip-dark/rip-light）× 2 视口（1440x900/390x844）= 24 组合 + 6 次 axe 扫描 = 30 用例，全过。
- 断言（按 ROADMAP 风险 3 裁剪，无快照基线）：零 console.error、零 pageerror、无横向溢出（scrollWidth <= innerWidth + 1）、每页关键内容标记可见（数据来自 route-mock 真实形状 JSON，离线静态服务器模式照搬 TELEGRAM §2.5：scripts/visual-server.mjs 服务 dist + SPA fallback，全部 /api/** 在 page.route 层 mock）。
- 基线来源声明：未生成任何本地快照基线（遵守 ROADMAP 风险 3：基线只能由 CI 字体环境生成；本机无该环境）。像素级视觉回归与快照 diff 属待验证，待 CI 接线后在统一字体环境生成基线。
- axe AA 尝试结果（wcag2a/wcag2aa/wcag21aa，桌面深空每页一扫）：
  - serious/critical 级非色彩违规：0 处（门禁断言项）。
  - color-contrast（已知待修项，不阻断门禁，收集于输出）：5 页 13 节点（/projects x2、/knowledge x2、/tools x3、/runs x3、/runtime x3；/ 总览页 0）。对应 PHASE5-PLAN 风险 5（主题对比度为设计目标值），待逐对比实测后调色收口。

## 四、浏览器二进制口径（ROADMAP §6.2 实测记录）

- 首选路径（用户缓存目录）尝试失败：`npx playwright install chromium` 两次实测均中断（storage.googleapis.com TLS 连接重置；第二次下载至 20% 断连，共 197.8 MiB）。
- 回退实现：驱动系统既有 chromium（/usr/sbin/chromium，CachyOS 自带，零安装零下载，比用户缓存口径更保守）；playwright.config.ts 以 `PW_CHROMIUM` 环境变量可覆盖（默认 /usr/sbin/chromium）。launch 验证：setContent 读回 ok（LAUNCH_OK 实测）。
- 结论：浏览器二进制可得（系统 chromium 口径），视觉矩阵在本机真实执行（30 用例非降级模拟）；仅快照基线与 CJK 字体一致性声明依赖 CI 环境，属待验证。

## 五、坏消息与待验证清单（如实声明）

0. **第 1 轮 cargo test 门禁失败与修复（2026-11-11 补记）**：编排侧全量 `cargo test --offline --workspace` 首轮在 `crates/database/tests/migrations_context_broker.rs` 失败两用例（SQLITE_READONLY 1032 / disk I/O 5898）。排查结论与处置：
   - **根因 1（测试缺陷，已修）**：同文件两用例并行共用 `migrations/dev-p4.db`，`reset_dev_copy()` 会删掉对方打开中的库文件。同款竞态模式遍布三个迁移测试文件（migrations_context_broker / migrations_tool_contract / migrations_agent_data_plane，分别对应 dev-p4 / dev-p13 / dev-p71），统一加进程内 `static DEV_COPY_LOCK: Mutex` 串行化（路径契约不变）；修复后该二进制连跑 5 次稳定，全 workspace 连续两轮全绿。
   - **根因 2（运行时数据外部丢失，已恢复）**：三族镜像（rip-tool-base:smoke / rip-tool-webresearch / rip-tool-coreutils）被外部清理（podman images 实测仅余用户自有镜像；11-10 凌晨 PHASE4 验收时在库，本阶段各批未触 podman）。按 Containerfile 自带规程处置：重拉同 digest 基础镜像（python:3.12-slim@4840d514，docker.io 首次中断重试成功）→ 重建三镜像 → 新 digest 三处同步（tool-manifest.json/yaml 双份、podman_host_e2e.rs MANIFEST_DIGEST 与 core_utils_e2e/podman_host_egress/web_research_e2e 常量、containerfiles/README.md 台账三行，旧值见 git 历史）；drill3 与 core_utils/egress/web_research 全部恢复通过。
   - **网络抖动说明**：恢复期间外网持续抖动（playwright CDN 中断、docker.io TLS timeout、example.com 一次 502/一次 SSL EOF），egress 与 web_research 首跑失败均为抖动，数分钟内自愈重跑即绿；此类抖动不影响门禁判定（重跑即绿且断言本身真实打外网）。


1. **视觉快照基线待验证**：未生成基线（无 CI 字体环境，ROADMAP 风险 3）；当前门禁为行为断言（console/pageerror/溢出/标记），不含像素 diff。
2. **axe color-contrast 13 节点待修**：清单见上节；根因为主题对比度是设计目标值（token.css 标注待验证），调色后重跑 axe 收口。
3. **Runtime 实时心跳待验证**：Gateway 常驻编排未落地（PHASE2-ACCEPTANCE §7 第 2 条，约 137 行），Runtime 页展示登记节点与最近心跳快照；真实节点数据接入后需回归节点行渲染（当前仅单测覆盖）。
4. **artifact 预览入口待真实数据回归**：入口仅在 artifact 行存在时渲染，dev-b5 现无 artifact 数据（仅单测覆盖）。
5. **npm 依赖许可审计待验证**：未做逐包审计（含 P5-10 新增 @playwright/test 1.65.0、@axe-core/playwright 4.14.0），可参考 telegram-bot admin/DEPENDENCIES.md 口径补做。
6. **两处环境级让步**：apps/server/selftest_auth.sh 第 7 步为测试架修正（P5-3 登记，git stash 证明先在环境漂移非回归）；7800 端口被 11月09 既有带令牌实例占用，全部联调用 7801 dev 实例（RP_API_TARGET 代理），未动该实例。
7. **Playwright/axe 版本随 npm ^ 语义**：devDeps @playwright/test 1.65.0 与 @axe-core/playwright 4.14.0 已精确锁定，后续升级需重验（chromium 可执行路径口径可能变化）。

## 六、回滚

- web/ 与 docs/phase5/ 为新增目录，删除即完全回滚；apps/server 变更（rp_server.py 五端点 + 参数扩展、selftest_phase5.sh 新增、selftest_auth.sh 测试架修正）可按 git 历史单文件还原，均为只读增量。
- migrations/ 零变更（红线核验）；npm 全局目录无 RIP web 相关包（2026-11-11 逐包核验 `npm ls -g --depth=0`：@mnemon-dev/mnemon、agent-browser、billion-context、docx、novel-writer-skills、pptxgenjs、remotion 共 7 个，其中 billion-context 为外部于 11-10 后新装、与 rip-web 无关；计数口径不作断言，避免外部安装导致基线漂移）；浏览器二进制零安装（系统 chromium 复用）。

## 七、交付物清单

- web/：脚手架与配置（package.json、tsconfig.json、vite.config.ts、index.html、.gitignore、.prettierrc.json、.prettierignore）、src/（main/App/workspaces/theme/lib 四模块/components 九模块/styles 两文件/routes 十页面）、tests/（7 个单测文件 69 用例）、playwright.config.ts、scripts/visual-server.mjs、qa/visual/pages.spec.ts、README.md、package-lock.json。
- apps/server/：rp_server.py（9 只读端点 + RP_AUX_DB 通道）、selftest_phase5.sh、selftest_auth.sh（测试架修正）。
- docs/phase5/：PHASE5-PLAN.md（阶段说明 + 各批次判据 + 偏差登记）、本文件。
