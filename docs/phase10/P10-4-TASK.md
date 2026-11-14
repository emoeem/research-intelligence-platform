# 批次任务书：P10-4（Phase 10 第 4/4 批，发布与合规收口：双面许可证逐包清单、axe 对比度清零转阻断、统一字体视觉基线、安装/备份/迁移/升级四文档与 fresh clone 部署演练）

> 状态：已实施；日期：2026-11-14；适用版本：Phase 10 批次工作树（未 commit，由协调方收口）。
> 相关：docs/phase0/PHASE-ROADMAP.md §11.3（P10-6 判据原文）、docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md
> §17（最终验收标准）、docs/SESSION-HANDOFF.md §十（外部审阅采纳清单）、
> telegram-bot admin/DEPENDENCIES.md（清单口径参照）、docs/phase10/P10-4-BATCH-REPORT.md。

## 一、批次目标与范围

本批覆盖 ROADMAP §11.3 P10-6 判据（发布文档与独立部署演练，§17:911）与 SESSION-HANDOFF §十
Phase 10 新增三项（依赖许可证逐包清单、axe 无障碍对比度修复、视觉基线统一字体后建立）：

1. 许可证逐包清单（参照 admin/DEPENDENCIES.md 口径：逐包读实装值）：Rust 面用 cargo-deny
   成熟工具（deny.toml 合规门禁 + vendor 实装值逐包表），npm 面逐包读 node_modules 实装
   package.json；产物 docs/phase10/DEPENDENCIES-RUST.md 与 DEPENDENCIES-WEB.md。
2. axe 对比度修正清零并转阻断断言：light 主题 primary 压暗 + 全仓透明度稀释文字换实色
   muted token（token.css/shell.css/路由组件）；pages/workflows 两 spec 的 axe 断言扩双主题
   并把 color-contrast 豁免分支移除（与其他 serious/critical 违规同口径阻断）。
3. 统一字体环境生成 Playwright 视觉基线落盘：qa/visual/baseline/（44 页面组合 + 3 画布
   组合），spec 内注入钉死 CJK 字族，animations 禁用，maxDiffPixelRatio 0.02。
4. 安装/备份/迁移/升级四文档（docs/guides/）+ fresh clone 独立部署演练（§17 五场景各演练
   一遍）+ selftest_phase10_release.sh 收口脚本。

范围外（不动）：RTIS 生产行为（红线）；生产 migrations/platform*.db（零写，指纹留档）；
既有 Phase 0-9 已验收能力（对 web 仅色板可读性修正与基线新增，页面行为零改动）。

## 二、设计：许可证逐包清单（Rust 面 + npm 面）

1. 工具选型（复用优先）：cargo-deny 0.20.2（EmbarkStudios 维护，crates.io 发布版）经
   `cargo install --locked` 装用户目录 ~/.cargo/bin（与 PW_CHROMIUM 用户缓存同口径：用户级
   工具，零系统级写入，repo 外目录）；workspace 的 Cargo.toml/Cargo.lock/vendor 零增量，
   --offline 构建口径不变；deny 检查读 Cargo.lock + vendor 实装源，离线可复跑。
2. deny.toml（仓库根）：[licenses] allow 精确对齐实装集合（MIT/MIT-0/Apache-2.0/
   Unicode-3.0/Zlib），confidence-threshold 0.93，未声明许可 fail-closed 拒绝；
   workspace 自有 crate 标 `publish = false`（私有工程线元数据，不影响构建产物）经
   private.ignore 排除出第三方审计面。
3. 逐包表：Cargo.lock 解析 182 包（18 workspace + 164 第三方），第三方逐包读
   vendor/<crate>/Cargo.toml 的 license 字段（vendor 即构建实际使用的源，实装值口径同
   npm 面），164/164 全部有显式表达式；文档附直方图与特殊表达式逐包核对
   （LGPL 仅作为 r-efi 三选一 OR 的可选项，实际取 MIT/Apache-2.0 分支）。
4. npm 面：python 只读扫描 web/node_modules 全量 144 实装包（含 scope 与嵌套，顶层去重），
   逐包读实装 version/license；直接依赖 22 个（7 运行时 + 15 开发/测试）声明版本与实装
   版本逐一核对零偏差；直方图 MIT 109 / ISC 12 / Apache-2.0 10 / MPL-2.0 5 / BSD-3-Clause 4
   / BSD-2-Clause 2 / 0BSD 1 / BlueOak-1.0.0 1，零强著作权、零未声明。

## 三、设计：axe 对比度清零与阻断化

1. 缺口定位（实测先行）：light 主题全页 axe 扫描（wcag2a/wcag2aa/wcag21aa，全 impact 级）
   12 至 23 节点/页 color-contrast serious 违规；dark 主题零违规。根因两类：
   (a) 透明度稀释文字（text-base-content/40 至 /60 与 shell.css opacity 0.5 至 0.65，
   包括 .search-kbd 与 .topbar-item 的父子叠乘 0.65×0.8）；数学核对：alpha 混色在浅底
   （base-100 L0.985）上不可达 4.5:1（base-content 压到 L0.14 时 /40 混色仍只有 2.65:1），
   结论是必须换实色；
   (b) light primary L=0.55 在浅底 4.36:1（active 标签/侧栏高亮文字）。
2. 修复面：token.css 增两主题 --muted 实色 token（dark oklch(0.713 0.024 260.7)、
   light oklch(0.446 0.026 256.8)，全部底色实测 ≥5.79:1）+ light primary 压到
   oklch(0.52 0.11 215)（base-100 4.91 / base-200 4.52 / sidebar 4.65）；路由与组件
   text-base-content/40 至 /60（142 处）与独立 opacity-50/60 文本 span（8 处）换
   text-muted；shell.css 13 个文本稀释选择器换 var(--muted)（图标类 opacity 保留，
   axe 不检纯图标）；/70 至 /80 稀释实测 ≥5.94:1 保留最小 diff。
3. 断言阻断化：两 spec 的 axe 测试扩双主题（深空 + 晨白，桌面 1440×900，与视觉矩阵同
   mock 数据形状），serious/critical 判定列表不再豁免 color-contrast，违规即 fail；
   contrastFindings 收集与 afterAll 打印移除。

## 四、设计：统一字体视觉基线

1. 统一字体环境（参照项目 a1796e8 教训：基线只能由统一字体环境生成）：两个 spec 的
   openPage 在页面注入 `:root { --font-sans: "Noto Sans CJK SC", ... !important; --font-mono: ... }`
   钉死单一 CJK 字族（机器无关：跨机器或字体环境变更时按 config 注释整目录重落基线，
   不做单张补丁）。
2. playwright.config.ts：snapshotPathTemplate 指向 qa/visual/baseline/{spec 文件}/；
   expect.toHaveScreenshot（animations disabled、caret hide、maxDiffPixelRatio 0.02）；
   基线重生成命令 `npm run build && npx playwright test --update-snapshots`（浏览器走
   PW_CHROMIUM 用户缓存，零系统级安装，P7-1 caveat 口径不变）。
3. 基线集合：pages.spec 视觉矩阵 11 页 × 2 主题 × 2 视口 = 44 张 viewport 级 PNG +
   workflows.spec 画布三态（桌面深空/桌面晨白/移动端）3 张；快照前等待 400/500ms 让
   echarts 画布与 react-flow 入场动画收尾。
4. ROADMAP 风险 3 原文（「快照基线不本地生成，基线只能由 CI 字体环境生成」）在本批的
   落实方式变更：CI 尚无浏览器面（本地口径执行），基线改由「统一字体环境注入 + 用户缓存
   浏览器 +PW_CHROMIUM 显式注入」的本地确定性环境生成并入库；config 注释同步改写
   （原注释声明基线不本地生成，与本批冲突，已按新口径更新）。

## 五、设计：四文档与 fresh clone 部署演练（§17）

1. 四文档（docs/guides/）：INSTALL.md（前提红线/离线门禁/数据面双路径/服务双形态/React
   工作台/冒烟清单/宿主机干净度核对）、BACKUP-RESTORE.md（备份范围与 manifest 形态/备份
   校验/恢复/恢复后四路对账/保留归档/演练记录/已知限制）、MIGRATION.md（迁移链 019 表/
   执行器语义/应用路径/新增迁移流程/ETL 对账）、UPGRADE.md（平台升级六步/增量 vendor/
   插件升级回滚/镜像 digest/RTIS 回滚/失败回滚判据速查）。全部命令以实测为准（演练发现
   与文档不一致时改文档不改实现，ROADMAP §11.5 风险 3 口径）。
2. fresh clone 演练载具：本地 git clone 到 /tmp/rip-deploy-check + rsync 批次工作树
   （批次未 commit，演练对象 = 协调方收口后的仓库状态；排除 target/node_modules/
   rip-data/platform.db 等运行时与构建产物，untracked dev-*.db 移除保持 fresh 口径）；
   cargo test --offline --workspace + 冒烟三连（resolve/search/一次工具 run）+ §17 五场景
   各演练一遍（研究一个开源项目 = rp-research planner 契约面；外部 AI 调用工具 = rp-mcp
   stdio JSON-RPC initialize/tools/list/tools/call；知识库治理 = rp-review
   propose/pending/decide/counts；VM 故障恢复 = recovery_drill 自动化载具（VM 长演练排除
   口径）；工作流恢复 = selftest_phase8_workflow.sh）+ 备份恢复 roundtrip（ETL dryrun 库）。
3. selftest_phase10_release.sh： bins 离线构建、许可证三面复核（deny + vendor/npm 实装值
   fail-closed）、发布文档在位与阻断断言形状（豁免分支已移除）、冒烟三连（隔离副本）、
   dash 扫描、宿主机快照 diff、生产库指纹前后一致。

## 六、实施步骤与文件清单

| 文件 | 动作 | 内容 |
| --- | --- | --- |
| deny.toml | 新 | cargo-deny 许可证合规配置（allow 精确对齐实装集合） |
| crates/*/Cargo.toml、tests/Cargo.toml | 改 | workspace 自有 crate 标 publish = false（17 个，元数据） |
| docs/phase10/DEPENDENCIES-RUST.md | 新 | Rust 面 164 第三方逐包清单 + 直方图 + cargo-deny 验收 |
| docs/phase10/DEPENDENCIES-WEB.md | 新 | npm 面 22 直接依赖 + 144 实装直方图 + 非 MIT/ISC/Apache 明细 |
| web/src/styles/token.css | 改 | --muted 双主题 token + light primary 压暗 + 注释 |
| web/src/styles/shell.css | 改 | 13 个文本稀释选择器换 var(--muted) |
| web/src/routes/*.tsx、web/src/components/*.tsx | 改 | text-base-content/40 至 /60 与 opacity-50/60 文本换 text-muted |
| web/qa/visual/pages.spec.ts、workflows.spec.ts | 改 | axe 双主题阻断化 + toHaveScreenshot + 字体注入 |
| web/playwright.config.ts | 改 | snapshotPathTemplate + expect.toHaveScreenshot 配置 |
| web/qa/visual/baseline/{pages,workflows}.spec.ts/*.png | 新 | 47 张统一字体基线 |
| docs/guides/INSTALL.md、BACKUP-RESTORE.md、MIGRATION.md、UPGRADE.md | 新 | 四文档 |
| apps/server/selftest_phase10_release.sh | 新 | 发布收口自验脚本 |
| docs/phase10/P10-4-TASK.md、P10-4-BATCH-REPORT.md | 新 | 本任务书与批次报告 |
| docs/README.md | 改 | Phase 10 行登记（四文档与 phase10 目录入口） |

成熟库优先核对：cargo-deny（成熟审计工具，用户级安装，零 workspace 增量）；muted token
与色板修正是设计 token 面（无库可替）；快照门禁复用 @playwright/test 既有 toHaveScreenshot
（零新增 npm 依赖）；备份/恢复/保留归档/FTS/治理/研究/MCP/工作流全部复用既有栈（P10-1
至 P10-3 与 Phase 2-9 交付面），本批零新增 workspace 依赖（Cargo.toml 依赖行零变化，
publish = false 为元数据行）。

## 七、验收命令

~~~bash
cd /home/emo/code/pensting/research-platform
cargo test --offline --workspace                                # 全量门（vendor 增量后干净态复验）
cargo clippy --offline --workspace --all-targets -- -D warnings # 静态检查
cargo-deny check licenses                                       # 许可证合规（工具用户级安装）
cd web && npm run build && npx playwright test                  # 视觉门 71 用例（含基线断言）
bash apps/server/selftest_phase10_release.sh                    # 发布收口自验
# fresh clone 演练（§17）：见批次报告第五节逐条命令原文
~~~

## 八、风险、已知限制与回滚

1. cargo-deny 为用户级工具安装（~/.cargo/bin，repo 外）：其余机器复跑需同口径安装
   （cargo install cargo-deny --locked）；selftest 对工具缺席取「实装值复核」降级路径
   （不硬依赖工具二进制在位）。
2. 视觉基线与浏览器/字体环境强耦合：跨机器复跑以 PW_CHROMIUM + Noto Sans CJK SC 为准；
   环境不同基线必漂，按 config 注释整目录重落（文档化流程，不做阈值放宽掩盖漂移）。
3. 色板可读性修正改变了既有页面渲染观感（muted 实色比原透明度稀释略深）：属 axe AA
   门禁的目标行为（light 主题 4.36:1 不达标的既知缺口，PHASE5-PLAN 风险 5 遗留项清零）；
   视觉基线同批重落，无旧基线可比对问题。
4. fresh clone 演练的对象是批次工作树（未 commit）：真 git-clone 演练在协调方 commit 后
   即可按本文第五节载具复跑（rsync 载具已把工作树状态原样带过去，等价于 checkout 批次
   提交后的仓库）。
5. 回滚方式：整批 revert 工作树文件（无迁移、无 schema 变更、生产库零触碰）；publish =
   false 元数据行与 web 色板可各自独立 revert（互不依赖）。
