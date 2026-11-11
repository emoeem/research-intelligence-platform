# Phase 6 验收报告（ROADMAP §7.3 P6-1 到 P6-8）

| 项     | 值                                                                                               |
| ------ | ------------------------------------------------------------------------------------------------ |
| 状态   | 已验收（P6-1 到 P6-8 全批交付，验收命令逐批实跑）                                                 |
| 日期   | 2026-11-11（P6-8 收口日）                                                                        |
| 基准   | ROADMAP §7.3/§7.4/§7.5、计划 §14:806 到 813、PLUGIN-CONTRACT-DRAFT 全文                            |
| 相关   | docs/phase6/PHASE6-PLAN.md、docs/phase6/DUAL-MCP-BOUNDARY.md、docs/phase6/RTIS-REDTEAM-PLUGIN-NOTES.md、docs/phase0/PHASE-ROADMAP.md |

## 一、验收判据逐条对照（ROADMAP §7.3）

| 批次 | 判据（ROADMAP §7.3 最小可验证单元）                                    | 结果 | 证据                                                                                                |
| ---- | ---------------------------------------------------------------------- | ---- | --------------------------------------------------------------------------------------------------- |
| P6-1 | 两示例 Manifest 七条校验通过；缺 license 与浮动分支反例被拒             | ✓    | `cargo test -p plugin-registry --test static_chain` exit 0（28+11 项全过）；pinned HEAD 演进断言     |
| P6-2 | 生命周期全路径（禁用不删数据、卸载保留已入主库知识、升级失败回滚）      | ✓    | `cargo test -p plugin-runtime --test lifecycle_full_path` exit 0（6 项全过）                        |
| P6-3 | rip-data/plugins 目录树 §8 建立且可重定位；源码零绝对路径字面量        | ✓    | `layout_relocatable.rs` 5 项全过；`grep -rn "rip-data" crates/ apps/` 仅命中文档注释与相对缺省名     |
| P6-4 | 五类来源各一个最小插件走完 discover→validate→install                   | ✓    | `sources_five.rs` 6 项全过（local_dir/git/archive/mcp_server/http_api）                              |
| P6-5 | RTIS 语料经摄取管道入主库且记录 plugin_id；生产 RTIS 行为零变化        | ✓    | `knowledge_ingest_provenance.rs` 5 项全过；真实语料 15155 行（14717 poc + 125 skill_md + 313 method_case）；knowledge.db sha256 前后一致 |
| P6-6 | 验收「至少接入一个外部项目和一个知识插件」（计划 §14:813）             | ✓    | installed/ 三插件在位（knowledge + tool + project）；13 个 Tool Registry 行；rtis_mcp 真实握手 10 工具 |
| P6-7 | 依赖缺失拒绝 enable 输出机器可读清单；环检测用例拒绝                    | ✓    | `deps_cycle_and_missing.rs` 8 项全过（含 compatibility 三项与健康检查间隔下限 1s）                   |
| P6-8 | Plugin Center 网页 + 插件 API 端点；启停升级卸载走真实 API              | ✓    | selftest_phase6.sh PASS=49 FAIL=0；Playwright 真实后端 E2E + 截图 12 张                              |

## 二、ROADMAP §7.4 验收命令逐条对照

| 验收句（计划 §14:813）                                    | 实测（命令与结果）                                                                                              |
| --------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| 插件安装/启用/禁用/升级/卸载全操作回路走真实 API            | selftest_phase6.sh：install→enable→disable→upgrade(0.1.0→0.2.0)→ingest→uninstall 全 PASS=49 FAIL=0              |
| 启停前后生产库核心实体表行数不变（零写入口径）              | core_snapshot（knowledge/research_task/resource/tool/tool_version/tool_run/workflow/skill/evidence 九表）前后一致 |
| 仅 audit_event 追加插件生命周期事件                         | enable/disable 各追加 1 条 audit_event（plugin_db 内），核心表零写                                              |
| rp_server.py 不新增直连 SQL 写                              | 静态断言 `grep -nE "INSERT INTO (plugin\|knowledge_ingest)\|UPDATE plugin \|DELETE FROM plugin" rp_server.py` 零命中 |
| 写端点强制 Bearer Token                                     | 无令牌访问 /api/plugins 返回 401；鉴权关闭时写端点返回 403                                                       |
| installed/ 内容在位（DATA_RETAINED）                        | `test -d rip-data/plugins/installed/knowledge.rtis-redteam/0.1.0` PASS                                          |
| 三级加载（L1 目录 / L2 命中摘要 / L3 docs + Schema）        | selftest_phase6.sh：L1 无 capabilities 字段、L2 capability 清单 2 项 + context.summary、L3 docs.path = docs/usage.md |
| Context Broker 匹配面                                       | rp-context assemble exit 0（items=0, plugin items=0：CB 尚无插件目录源，P4 契约范畴，匹配面天然无插件条目）     |

## 三、ROADMAP §7.5 风险逐条关闭

| 风险条目                                              | 状态 | 关闭依据                                                                                      |
| ----------------------------------------------------- | ---- | --------------------------------------------------------------------------------------------- |
| §7.5.1 rip-data 目录定位与 gitignore                   | ✓    | P6-3：布局可重定位（RIP_DATA_DIR + rip.plugins.dir）、gitignore `rip-data/` 生效、源码零绝对路径字面量 |
| §7.5.2 仓库顶层 LICENSE 裁决                           | 部分 | 维持 UNLICENSED（rule 4 通过），LICENSE 文件选型移交维护者（RTIS-REDTEAM-PLUGIN-NOTES §一）   |
| §7.5.3 knowledge.db 条数口径差                         | ✓    | V-12 关闭：README 14717 与只读 COUNT 实查一致（权威口径），rtis.py 头注 14664 为陈旧快照       |
| §7.5.4 双 MCP 并存混淆                                 | ✓    | docs/phase6/DUAL-MCP-BOUNDARY.md 明示边界（命名空间/代码/数据面/鉴权/生命周期五维隔离）        |
| §7.5.5 插件写面绕过 Application API                    | ✓    | 写端点经 rp-plugin 子进程（状态机 + audit-engine），rp_server.py 零直连 SQL 写（静态断言）    |
| §7.5.6 RTIS 红线                                       | ✓    | RTIS 生产路径零改动（knowledge.db/rtis.py sha256 前后一致、git status 干净、RTIS_RESOLVER_MODE 在位） |

## 四、自动化测试与静态检查汇总

| 门禁                              | 命令                                                       | 结果                                     |
| --------------------------------- | ---------------------------------------------------------- | ---------------------------------------- |
| Rust workspace 测试                | `cargo test --offline --workspace`                         | exit 0（72 个 test result: ok）           |
| Rust clippy 零警告                 | `cargo clippy --offline --workspace --all-targets -- -D warnings` | exit 0                            |
| Python selftest Phase 6            | `bash apps/server/selftest_phase6.sh`                      | exit 0（PASS=49 FAIL=0）                  |
| Python selftest Phase 5 回归       | `bash apps/server/selftest_phase5.sh`                      | exit 0（PASS=31 FAIL=0）                  |
| vitest 单测                        | `cd web && npx vitest run`                                 | 12 files / 73 tests 全过                  |
| TypeScript 类型检查                | `cd web && npx tsc --noEmit`                               | exit 0                                    |
| Playwright 插件面视觉矩阵 + E2E    | `npx playwright test tests/plugins.spec.ts -c playwright.plugins.config.ts` | 8 passed + 1 skipped（mock 矩阵）+ 真实后端 E2E 通过 |
| Context Broker 冒烟                | `cargo run -p context-broker --bin rp-context -- assemble --task-id p6-8-check ...` | exit 0，匹配面无插件条目 |

## 五、真实浏览器实测截图（SESSION-HANDOFF §九）

12 张截图在 web/qa/plugins/：

| 类别              | 文件                                                                                          | 内容                                        |
| ----------------- | --------------------------------------------------------------------------------------------- | ------------------------------------------- |
| 视觉矩阵（mock）  | plugins-page-{rip-dark,rip-light}-{desktop,mobile}.png（4 张）                                | /plugins 目录（四分类 + 状态徽章 + trust）  |
| 视觉矩阵（mock）  | plugins-detail-{rip-dark,rip-light}-{desktop,mobile}.png（4 张）                              | /plugins/:id 详情（capability 清单 + 兼容性列 + 依赖图 + 审计 trail） |
| 真实后端 E2E      | e2e-before-enable.png、e2e-enabled.png、e2e-disabled.png、e2e-catalog.png（4 张）             | 真实 rp_server 后端：DISABLED→ENABLED→DISABLED→目录 |

## 六、已知取舍与移交

1. Context Broker L1 插件目录源未接入：CB 的 L1 目录源是 Phase 4 交付（tool/knowledge），不含 plugin 表。Phase 6 插件目录的 L1 消费（Context Broker 按需加载插件元数据）需 CB 扩展插件源，归 Phase 7 Agent Engine 接线批次。
2. `rtis.py` 头注条数（14664）为陈旧快照未修正：rtis.py 零 diff 是红线，头注修正归 RTIS kit 自身维护。
3. LICENSE 文件选型：维持 UNLICENSED（rule 4 通过），LICENSE 文件移交维护者。
4. FTS/向量索引对接：摄取行进检索面属 Phase 4 search-engine 轴向，本阶段只落数据面行。
5. upgrade 的数据迁移钩子与回滚窗口清理：归 P10-2。
6. docs/SESSION-HANDOFF.md、docs/phase5/*、web/ 既有页面的改动为并行批次产物，本阶段未触碰。
