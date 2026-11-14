# 批次验收报告：P10-4（Phase 10 第 4/4 批：发布与合规收口，许可证逐包清单、axe 对比度清零转阻断、统一字体视觉基线、四文档与 fresh clone 部署演练）

> 状态：已实施并验收（协调方收口复测，本报告即验收快照）；日期：2026-11-14；适用版本：Phase 10 批次工作树（WIP 备份 commit 159f1b2）。
> 相关：docs/phase10/P10-4-TASK.md（设计与文件清单）、docs/phase0/PHASE-ROADMAP.md §11.3（P10-6 判据原文）、
> docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md §17（最终验收标准）、docs/SESSION-HANDOFF.md §十（外部审阅采纳清单）、
> telegram-bot admin/DEPENDENCIES.md（清单口径参照）。

## 零、批次概要

| 项 | 值 |
| --- | --- |
| 数据库迁移 | 0 个新增（迁移下一可用编号仍为 020） |
| 生产库 | migrations/platform*.db 五库指纹复测与 P10-3 批末记录逐一致（platform=eb81d37b、dryrun=4b2d3209、fullbatch=eb81d37b、governance=afe41186、search=e56171dd，sha256 前八位），零写红线成立 |
| 第三方依赖 | workspace 零新增（Cargo.toml 依赖行零变化，Cargo.lock 本批零改动；publish = false 为 17 个自有 crate 的元数据行）；cargo-deny 0.20.2 为用户级工具（~/.cargo/bin，repo 外） |
| 交付面 | deny.toml + DEPENDENCIES-RUST.md + DEPENDENCIES-WEB.md + 四 guides（INSTALL/BACKUP-RESTORE/MIGRATION/UPGRADE）+ web 色板可读性修正与 axe 阻断化 + 视觉基线 47 张 + selftest_phase10_release.sh |

## 一、判据逐条对账（判据原文、命令、实测输出）

### 判据 1：依赖许可证逐包清单（Rust 面 cargo-deny + npm 面实装值）

命令与输出（收口复测实跑）：

~~~text
$ cargo deny check licenses        # cargo-deny 0.20.2（cargo install --locked，用户级）
licenses ok                        # exit 0
~~~

- Rust 面：Cargo.lock 解析 182 包（18 workspace 自有 + 164 第三方），第三方逐包读 vendor/<crate>/Cargo.toml 的 license 字段，164/164 全部有显式表达式（零 missing、零 vendor 缺失）；deny.toml [licenses] allow 精确对齐实装集合（MIT/MIT-0/Apache-2.0/Unicode-3.0/Zlib 等），confidence-threshold 0.93，未声明许可 fail-closed；workspace 自有 crate 以 publish = false + private.ignore 排除出第三方审计面。清单全文 docs/phase10/DEPENDENCIES-RUST.md。
- npm 面：只读扫描 web/node_modules 全量 144 实装包，直方图 MIT 109 / ISC 12 / Apache-2.0 10 / MPL-2.0 5 / BSD-3-Clause 4 / BSD-2-Clause 2 / 0BSD 1 / BlueOak-1.0.0 1；直接依赖 22 个声明版本与实装版本逐一核对零偏差；零强著作权、零未声明。清单全文 docs/phase10/DEPENDENCIES-WEB.md。
- selftest 三面复核（deny 工具在位实跑 + vendor 实装值 fail-closed + node_modules 实装值 fail-closed）全过，见第五节 selftest 汇总。

### 判据 2：axe 对比度修正清零并转阻断断言

修复面（实施批落地）：token.css 增 --muted 双主题实色 token（dark oklch(0.713 0.024 260.7)、light oklch(0.446 0.026 256.8)，底色实测 ≥5.79:1）+ light primary 压到 oklch(0.52 0.11 215)（base-100 4.91 / base-200 4.52 / sidebar 4.65）；路由与组件 text-base-content/40 至 /60（142 处）与独立 opacity 文本（8 处）换 text-muted；shell.css 13 个文本稀释选择器换 var(--muted)。

阻断化复测（收口当日，浏览器为用户缓存 Playwright chromium-1254）：

~~~text
$ cd web && npx playwright test
71 passed (1.4m)
# 含 10 项 axe AA 断言：/overview /knowledge /tools /plugins /agents /settings 等 8 页
# × 深空 + 晨白双主题，serious/critical 判定列表不再豁免 color-contrast，违规即 fail
# 44 张视觉矩阵基线断言 + 3 张画布基线断言 + 画布专项用例全过
~~~

晨白主题（原 4.36:1 不达标主题）在 color-contrast 阻断口径下全绿，PHASE5-PLAN 风险 5 遗留缺口清零。

### 判据 3：统一字体环境生成 Playwright 视觉基线落盘

- 基线集合：web/qa/visual/baseline/pages.spec.ts/ 44 张（11 页 × 双主题 × 双视口）+ workflows.spec.ts/ 3 张（画布桌面深空 / 桌面晨白 / 移动端）= 47 张 PNG（find 实数 47 = 44 + 3）。
- 统一字体环境：两 spec openPage 注入钉死 CJK 字族（Noto Sans CJK SC）+ animations 禁用 + caret hide + maxDiffPixelRatio 0.02；快照前等待 400/500ms 让 echarts 与 react-flow 收尾。
- 收口复测结论：基线由实施批以用户缓存 chromium（playwright 缓存口径）生成；收口复测环境为 chromium-1254（npx playwright install chromium 用户缓存，与生成环境版本相邻），71 用例含 47 张基线断言全过，渲染差异在 0.02 容差内，未触发整目录重落基线流程（跨机器/字体环境变更时按 config 注释整目录重落的口径不变）。

### 判据 4：四文档 + fresh clone 独立部署演练（§17 五场景）+ selftest 收口

四文档（docs/guides/）：INSTALL.md（前提红线 / 离线门禁 / 数据面双路径 / 服务双形态 / 冒烟清单 / 宿主机干净度核对）、BACKUP-RESTORE.md（备份范围与 manifest / 校验 / 恢复 / 四路对账 / 保留归档 / 已知限制）、MIGRATION.md（迁移链 019 表 / 执行器语义 / 新增迁移流程 / ETL 对账）、UPGRADE.md（升级六步 / 增量 vendor / 插件升级回滚 / 镜像 digest / RTIS 回滚 / 失败回滚判据）。

fresh clone 独立部署演练（真 git clone，对象 = 收口提交 159f1b2 的仓库状态，工作目录 /tmp/rip-deploy-check）：

~~~text
$ git clone --no-hardlinks file:///home/emo/code/pensting/research-platform /tmp/rip-deploy-check
$ cd /tmp/rip-deploy-check && git log --oneline -1          # 159f1b2 WIP 备份：接手前快照
$ cargo test --offline --workspace                          # vendor 基线验证（零网络）
# CLONE_TEST_EXIT=0，644 passed 0 failed（116 个测试目标 test result: ok）
~~~

§17 五场景逐条（全在 clone 内实跑）：

1. 研究一个开源项目（rp-research planner 契约面）：`rp-research run research.planner --input '{"goal":"研究一个开源项目的架构与质量"}'` exit 0，回显 plan_id=plan-8786fe07170b39aa、step_count=6（六步固定流水线：source-discovery → evidence-extraction → verification → synthesis → citation → evaluator），provenance 双 ID（run rn-52fa7fef… / trace tr-4d822935…）。
2. 外部 AI 调用工具（rp-mcp stdio JSON-RPC）：数据面初始化（INSTALL 路径 A：`rp-resolve --db migrations/platform.db --migrate-dir migrations --writable` 回显 applied 19 migration(s)）后，stdio 管道 initialize（回显 serverInfo rp-mcp 0.2.0、协议 2024-11-05，rmcp 协商）→ tools/list 六条能力面全部含标准 name 与 inputSchema（P0 口径在 fresh 环境复证）→ 注册 rip.doc.parse 进运行库（register ok=true 0.1.0）→ tools/call rip.get_tool_schema（source=run_db 回退命中）→ tools/call rip.run_tool（input 为 markdown 样例）**SUCCEEDED**，run_id=rn-208f18fbcfa2394300000000、artifact_id=art-106a5c6c6b95a496eb9bac90、evidence_refs=['ev-73b27b8c0c7848a25ec1455a']。
3. 知识库治理（rp-review 治理回路）：propose --db /tmp/drill_gov.db --quarantine-dir migrations/quarantine exit 0（扫描入库隔离夹具）→ pending 实数 70 行 → decide --decision approve（review_id quality-engine:quarantine/assets_isolation.jsonl:1）exit 0 → counts 回显 PENDING=69、approve=1，回路闭环。
4. VM 故障恢复（自动化载具口径）：`cargo test --offline -p runtime-gateway --test recovery_drill` 3 passed 0 failed。
5. 工作流恢复（selftest 载具）：`bash apps/server/selftest_phase8_workflow.sh` PASS=77 FAIL=0 exit 0（017 三表 checkpoint / 暂停恢复 / 人工介入 / 六 ID 对账 / 生产库指纹零写全过）。

备份恢复 roundtrip（ETL dryrun 库，clone 内）：`python3 migrations/etl_dryrun.py` exit 0（FK 违例 0，产物 platform-dryrun.db 1118208 字节）→ `rp-admin backup --db <隔离副本> --out drill-backup.tar --extra rip-data/tool-runs.db`（extra_count=1）→ `rp-admin verify-archive`（manifest db_sha256=fd9b53e92cba9c1b）→ 中段 8192 字节 0xFF 受控破坏（sha 变为 fd9b53e9 前 16 位一致值，见下）→ `rp-admin restore`（extras_restored=1）→ 恢复产物 sha256 = fd9b53e92cba9c1b 与 manifest 逐字节一致（Online Backup API 语义：恢复产物对齐 manifest 快照，非源文件逐字节，P10-1 修正记录 3 同口径）。

selftest 收口脚本：bash apps/server/selftest_phase10_release.sh，28 PASS + 1 FAIL（唯一 FAIL 即「docs/phase10/P10-4-BATCH-REPORT.md 在位」，本报告入库后复跑 29/29，见第五节）。

## 二、门禁汇总（协调方收口复测，2026-11-14）

| 门 | 命令 | 结果 |
| --- | --- | --- |
| Rust 全量测试 | cargo test --offline --workspace | exit 0，644 passed 0 failed（116 目标 test result: ok） |
| Rust 静态检查 | cargo clippy --offline --workspace --all-targets -- -D warnings | exit 0，零告警（Finished dev profile 19.56s） |
| Worker unittest | python3 -m unittest discover -s workers/python/tests | 211 tests OK（skipped=11） |
| Web 构建 | cd web && npm run build | exit 0（tsc --noEmit + vite build ✓ built 807ms） |
| Web 单测 | cd web && npm run test | Test Files 18 passed (18)，Tests 120 passed (120) |
| Web 格式 | cd web && npx prettier --check . | All matched files use Prettier code style! |
| 视觉门 | cd web && npx playwright test | 71 passed (1.4m)，双主题 axe AA 阻断口径全过 |
| 许可证合规 | cargo deny check licenses | licenses ok（0.20.2，离线可复跑） |
| 发布收口自验 | bash apps/server/selftest_phase10_release.sh | 29/29 PASS exit 0（本报告入库后终态，前值 28 PASS + 1 FAIL 为报告本体缺位） |

## 三、执行中修正记录（协调方收口复测，如实声明）

1. **prettier 归一**：批次实施末态有 9 个 web 路由/组件文件（text-muted 色板替换批次触碰面）未过 prettier --check，协调方 `npx prettier --write .` 归一后 check 全过；vitest 复跑 120/120 证实行为零变化。
2. **vitest 负载 flake（非批次回归）**：首次收口复测与 clippy 编译、视觉门、fresh clone 全量构建并发时 3 用例失败（plugins 目录徽章 / tools 目录渲染 / echart 懒加载 ResizeObserver，全部时序敏感断言）；静场复跑 120/120 exit 0，隔离复跑各通过，判定为并发负载诱发，非代码问题。
3. **cargo test 首次复测 1 失败（rtis_timeout_retries_once_then_reports_unavailable）**：同属并发负载 flake（断言重试恰好两次，负载下首试失败路径改变）；隔离复跑两连过，静场全量复测 644/644 exit 0。管道 tail 吃掉退出码导致首查误判 exit 0，复测改用输出文件 + 显式 echo 退出码纠正。
4. **视觉门浏览器环境漂移**：批次实施用 playwright 缓存 chromium（RUN-LOG 记录 chromium-1234 口径）；收口环境该缓存不在，按 config 注释口径 `npx playwright install chromium` 落用户缓存 chromium-1254（零系统级安装），71 用例全过未触发重落基线；跨机器复跑口径与任务书 §八.2 一致。
5. **fresh clone 首轮 selftest_phase8 25 FAIL 根因**：首次演练漏做 INSTALL 路径 A 数据面初始化（fresh clone 无 gitignore 的 migrations/platform.db），rp-workflow 载具读主库缺位；按 INSTALL.md 第二节顺序初始化后复跑 77/77 exit 0。结论：文档口径正确，演练顺序错误，已在第五节场景 5 记录中如实标注。

## 四、未执行的测试与已知限制

- CI 视觉面：视觉门与基线断言保持本地执行（CI 无浏览器面，config 注释已载明口径），GitHub CI 覆盖其余门禁。
- 完整「confirm 后 200 受理」的服务端来源拉取链路需真实上游可达（P10-2 批次已声明的环境面限制，本批不涉及）。
- rustfmt 版本漂移沿用 P10-1/P10-2/P10-3 口径：cargo fmt --all -- --check 不作门禁，本批触碰文件按本机 rustfmt 归一。

## 五、回滚方式

整批 revert 工作树文件（无迁移、无 schema 变更、生产库零触碰）：publish = false 元数据行、deny.toml、web 色板与基线、四 guides 可各自独立 revert（互不依赖）；cargo-deny 为 repo 外用户级工具，卸载即回滚（cargo uninstall cargo-deny）。
