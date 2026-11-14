# RELEASE：RIP 2.0 Phase 10 生态与发布（阶段收口入库）

| 项       | 值 |
| ---      | --- |
| 日期     | 2026-11-14 |
| 阶段     | Phase 10（第 10/10 阶段，2.0 计划最后实施阶段）：生态与发布 |
| 批次     | P10-1 到 P10-4 四批全交付（rmcp 迁移整改批为 Phase 10 前置批，已随 8adbf1c 先行入库） |
| 阶段提交 | 本收口提交（commit 短哈希按仓库回填惯例补记，先例 05ed5ee/f02d9b3） |
| 前置     | rmcp 迁移阻断性整改批（外部审计 docs/phase9/MCP_MATURE_LIBRARY_AUDIT_2026-11-14.md §5 十项任务）已落地，Phase 10 开批前提满足 |
| 总账口径 | 批次汇总 / 五门禁与扩展门禁入库当日复测 / fresh clone 独立部署演练 / 已知取舍 / 回滚方式 |

## 一、批次汇总

### P10-1 可靠性基座（第 1/4 批）

判据：ROADMAP §11.3 P10-5（backup/restore 真实实演、FTS 重建校验、数据保留归档）。

- 备份/恢复实链：`rp-admin backup / verify-archive / restore`（crates/database/src/backup.rs + 新 bin rp-admin，rusqlite 启用既有 backup feature，vendor 零新增目录、Cargo.lock 仅 +5 行依赖边）；备份 → 中段 8192 字节受控破坏 → 恢复 → 恢复产物 sha256 与 manifest db_sha256 逐字节一致（Online Backup API 语义：快照对齐 manifest 非源文件逐字节）。
- FTS 重建校验：恢复副本 `rp-search-index build --rebuild` 与基线索引 doc_count/fts_rows 81/81 一致，抽样关键词命中集 diff 为空；Rust 载具 crates/search-engine/tests/rebuild_after_restore.rs 双用例。
- 数据保留归档：`rp-admin reten-archive`（crates/database/src/retention.rs，019 external_api_audit 保留 180 天策略）实测 archived=2 deleted=2，归档库 DDL 复刻行数一致。
- 门禁：selftest_phase10_backup.sh 16/16 exit 0；cargo test --offline --workspace 615 passed 0 failed（批内基线）；批次报告 docs/phase10/P10-1-BATCH-REPORT.md。

### P10-2 插件生态收口（第 2/4 批）

判据：P10-1（信任级驱动确认强度与自动更新策略生效、目录发布形态）+ P10-2（升级失败自动回滚实演、镜像 digest 回滚实演、回滚窗口清理）。

- 信任级策略面：crates/plugin-registry/src/trust_policy.rs 单一策略表（四档 confirmation ladder 单调 + D10 仅 official 自动更新提示 + remote 要求 typed id 与 server 字段）；crates/plugin-runtime/tests/trust_gate_and_window.rs 门控用例。
- 目录发布形态：crates/plugin-registry/src/catalog.rs（catalog.json 发布/校验/按目录安装，content_sha256 记录，override 降档生效）。
- 升级失败自动回滚：隔离库实演（升级包缺 capability entry，自动回滚 0.1.0 旧版可用）；镜像 digest 回滚：crates/tool-engine/src/image_ledger.rs + rp-image-pin 子命令（真实台账 + webresearch manifest 验证链）；回滚窗口清理：升级成功后 report.cleaned=[0.1.0]、dirs_removed=1。
- Web 分级确认：web/src/components/TrustConfirmDialog.tsx + rp_server 确认闸（写请求体 confirm/confirm_id 按档位与策略表一致）；真实浏览器四档实测 4 passed，截图五张入库 docs/phase10/browser-e2e/ + RUN-LOG.md。
- 门禁：点名门禁 15 目标 138 passed 0 failed；selftest_phase10_plugin_eco.sh 52/52 exit 0；全仓 633 passed（P10-1 基线 615 + 本批 18）；批次报告 docs/phase10/P10-2-BATCH-REPORT.md。

### P10-3 运行与质量门（第 3/4 批）

判据：P10-3（双 Runner 调度与心跳、单节点离线不丢任务）+ P10-4（T0/T1/T2 越界拒绝矩阵与 secrets 不泄露核查进 CI）+ autoresearch 性能基准计量闭环。

- 双 Runner 故障转移：crates/runtime-gateway/tests/multi_runner_failover.rs 4 passed + 1 ignored bench（主节点离线转投备用不落 WAITING_FOR_RUNNER、全离线落 WAITING 后任一候选恢复即续跑 [WAITING_FOR_RUNNER, PREPARING, RUNNING, SUCCEEDED]、批量恢复 report skips、set_primary 候选序重排）；派发延迟基准 METRIC dispatch_ms=0.427（n=30 中位数）。
- 安全矩阵：crates/tool-engine/tests/security_matrix.rs 7 passed（T3/t2/T9/空串登记拒绝、T2 缺授权引用 POLICY_DENIED 且零落行、secrets 键拒绝、manifest secrets 只带名不带值、planted 假想 secret 值在 tool/tool_version/tool_run 全行全列与终态信封逐字节扫描零命中）；ci.yml 注释点名随 workspace 全量门常驻 CI。
- 性能基准：autoresearch MCP 工具链计量闭环（init/run×3/log/export），assemble_ms 中位数 2.399、dispatch_ms 中位数 0.474；基线口径、20% 不回归阈值与可比性边界落 docs/phase10/PERF-BASELINE.md，基准载具为 #[ignore] 本地测试不进 CI。
- 门禁：点名门禁 22 目标 106 passed 0 failed 1 ignored；clippy 零告警；批次报告 docs/phase10/P10-3-BATCH-REPORT.md。

### P10-4 发布与合规收口（第 4/4 批）

判据：P10-6（发布文档与独立部署演练）+ SESSION-HANDOFF §十 Phase 10 新增三项（许可证逐包清单、axe 对比度、视觉基线）。

- 许可证逐包清单：cargo-deny 0.20.2（EmbarkStudios，cargo install --locked 用户级）+ deny.toml（allow 精确对齐实装集合，fail-closed）；Rust 面 164/164 第三方显式 license 表达式（docs/phase10/DEPENDENCIES-RUST.md），npm 面 144 实装包直方图零强著作权零未声明（docs/phase10/DEPENDENCIES-WEB.md）。
- axe 对比度清零转阻断：--muted 双主题实色 token + light primary 压暗 + 142 处透明度稀释文字换实色 + shell.css 13 选择器修正；两 spec 的 axe 断言扩双主题且 color-contrast 豁免移除（serious/critical 即 fail）。
- 统一字体视觉基线：qa/visual/baseline/ 47 张（44 页面组合 + 3 画布组合）落盘，字体注入钉死 + animations 禁用 + maxDiffPixelRatio 0.02。
- 四文档：docs/guides/INSTALL.md、BACKUP-RESTORE.md、MIGRATION.md、UPGRADE.md；fresh clone 独立部署演练 §17 五场景全过（批次报告 docs/phase10/P10-4-BATCH-REPORT.md 第五节逐条命令与回显）。
- 门禁：selftest_phase10_release.sh 29/29 exit 0；批次报告同文。

## 二、阶段级门禁入库当日复测（2026-11-14，协调方收口）

| 门 | 命令 | 结果 |
| --- | --- | --- |
| Rust 全量测试 | cargo test --offline --workspace | exit 0，644 passed 0 failed（116 目标） |
| Rust 静态检查 | cargo clippy --offline --workspace --all-targets -- -D warnings | exit 0，零告警 |
| Worker unittest | python3 -m unittest discover -s workers/python/tests | 211 tests OK（skipped=11） |
| Web 构建/单测/格式 | npm run build + npm run test + prettier --check | build exit 0；Test Files 18 passed，Tests 120 passed；prettier 全过 |
| 视觉门 | cd web && npx playwright test | 71 passed (1.4m)，双主题 axe AA 阻断口径全过 |
| 许可证合规 | cargo deny check licenses | licenses ok（0.20.2） |
| 自验脚本 | selftest_phase10_backup 16/16 + plugin_eco 52/52 + release 29/29 + auth 11/11 | 全部 exit 0 |
| 生产库红线 | sha256sum migrations/platform*.db 五库 | 指纹与 P10-3 批末记录逐一致（platform=eb81d37b、dryrun=4b2d3209、fullbatch=eb81d37b、governance=afe41186、search=e56171dd 前八位），零写成立 |

复测环境注记：视觉门浏览器为用户缓存 Playwright chromium-1254（npx playwright install chromium，零系统级安装），与基线生成环境（chromium-1234 口径）相邻版本，47 张基线断言全过未触发重落；cargo-deny 为用户级工具（~/.cargo/bin）。复测期并发负载诱发过 1 例 cargo flake 与 3 例 vitest flake，隔离/静场复跑全过（P10-4 报告第三节修正记录 2/3），非批次回归。

## 三、fresh clone 独立部署演练（§17，对象 = 收口提交仓库状态）

真 git clone（file:/// 本地 clone --no-hardlinks，HEAD 159f1b2）到 /tmp/rip-deploy-check，全量离线门禁与五场景、roundtrip 全过，逐条命令与回显见 docs/phase10/P10-4-BATCH-REPORT.md 第五节。摘要：

1. cargo test --offline --workspace（vendor 基线验证，零网络）：644 passed 0 failed exit 0。
2. 场景 1 研究一个开源项目：rp-research planner 契约面 exit 0（plan_id、六步流水线、provenance 双 ID）。
3. 场景 2 外部 AI 调用工具：rp-mcp stdio（initialize 协商 → tools/list 六条全含 inputSchema → get_tool_schema source=run_db → run_tool SUCCEEDED，run_id + artifact_id + evidence_refs 齐）。
4. 场景 3 知识库治理：rp-review propose/pending(70)/decide approve/counts(69+1) 回路闭环。
5. 场景 4 VM 故障恢复：recovery_drill 3 passed。场景 5 工作流恢复：selftest_phase8_workflow.sh 77/77 exit 0（首轮 25 FAIL 为漏做 INSTALL 路径 A 数据面初始化，按文档顺序重跑全过，演练顺序教训见 P10-4 报告第三节修正记录 5）。
6. 备份恢复 roundtrip：ETL dryrun 库备份 → verify → 受控破坏 → 恢复，恢复产物 sha256 与 manifest db_sha256（fd9b53e9 前十六位）逐字节一致。

## 四、已知取舍

1. CI 无浏览器面：视觉门与基线断言保持本地执行（ci.yml 注释载明）；CI 覆盖 Rust 全量/clippy/unittest/web 构建/vitest/parity。
2. 性能基线为进程内 debug 测量（Mock 数据面、单机口径），绝对值不对外引用，不回归门只做同口径相对比较（PERF-BASELINE.md）。
3. rustfmt 版本漂移：cargo fmt --all -- --check 不作门禁（本机与 CI 侧 rustfmt 版本差，P10-1 起口径一致），各批触碰文件按本机 rustfmt 归一。
4. 服务端插件来源拉取的完整 confirm 后受理链路需真实上游可达（P10-2 已声明的环境面限制，selftest 形态无关断言覆盖）。
5. 许可证清单为 2026-11-14 快照性质（Cargo.lock/vendor 与 node_modules 实装状态），后续新增依赖按清单口径补审计（DEPENDENCIES 两文档维护约定节）。

## 五、回滚方式

Phase 10 无迁移、无 schema 变更、生产 platform*.db 零触碰：整阶段 revert 即回滚到 8adbf1c（rmcp 批）后状态；四批之间互不依赖可分批 revert；cargo-deny 为 repo 外用户级工具（cargo uninstall 即清）；视觉基线 47 张与色板修正可独立 revert。RTIS 生产行为零改动（integrations/rtis/ 本阶段零触碰）。

## 六、队列收口

Phase 10 收口后，HANDOVER-RIP2-CONTINUATION.md 第三节队列 1 到 6 项全部完成，剩第 7 项「项目完成自查与收尾（修订版闭环）」：2.0 计划 §17 全绿自查修复循环 + GitHub 知识库与武器库补齐（ChatGPT 步骤与实战校验三面按 2026-11-14 用户裁决删除暂缓，恢复时启用）。

## 七、收尾批次（队列第 7 项交付，2026-11-14 同日入库）

### 知识库补齐（GitHub 面）

三源插件 Manifest（json+yaml 双份，入库于 SK-2）之外补齐 P10-2 目录发布形态的发布产物：`manifests/plugins/catalog.json`（catalog-publish 生成，trust-default official + secskills 降档 community，3 entries 含 content_sha256 完整性），catalog-verify sha 一致 exit 0；课程原文与 *.db 不入库红线不变。

### 武器库补齐（首批安全工具，source-analysis pack 转正）

`containerfiles/rip-tool-source/`（pack_id 与 manifests/packs/planned/source-analysis.json 冻结存根同源，自研薄包装 self-owned 裁决落地）：

- 三工具（T1，podman_host，network.mode=none，纯标准库 src_tools.py argv op 分派，D-TC-03 帧协议）：rip.src.deps-manifest（四格式依赖清单归一化）、rip.src.static-check（秘钥形态/高危调用/TODO 密度规则集）、rip.src.code-search（目录限定 regex 检索）。
- 镜像族建成（python:3.12-slim@sha256:4840d514 受控拉取后离线构建），产物 digest sha256:8251ba32e6ae4d107190f2b2279ae3774facdc8e074ae000266be3aa1fd0ea29 三处同步（两 manifest 通道 + src_analysis_e2e.rs 常量）并登记 containerfiles/README.md 台账；distrobox 侧经 distrobox-host-exec shim 调宿主 podman 6.2.0 构建（本机口径注记）。
- 实测：容器内三 op 冒烟全过（npm 清单归一 3 条、AWS key/eval 命中、regex 行号对账）；rp-tool validate 三份全过 + dev 副本注册 + /api/tools 露出（三工具 VALIDATED/ACTIVE）；Rust 端到端 crates/tool-engine/tests/src_analysis_e2e.rs 3 passed（podman shim 在位时真实容器全链 5.7s，CI 无镜像环境按 V-9 digest 守卫跳过，registry 负例常跑）；宿主纯函数单测 workers/python/tests/test_src_tools.py 21 用例全过；存根转正（status delivered + 交付验收记录）。

### §17 全绿自查与遗留清账

docs/phase10/PLAN-17-SELFCHECK.md：14 条判据逐条实证对账全绿（判据 5 VM 面为 ADR §3.1 冻结挂起口径，非缺口）；修复循环两项入库：

1. **strict JSON 重复键拒绝**（HANDOVER §七遗留清账）：tool_registry::strict_json 新模块（零新增依赖），tool manifest（rp-tool parse_manifest）与 plugin manifest（plugin-registry parse_document）两 JSON 通道接线，与 YAML 通道对齐 fail-closed；tool-registry 5 用例 + plugin-registry json_duplicate_key_rejected（含嵌套 network 块形态）单测锁定；存量全量 manifest 回归零拒绝（26 份工具 Manifest + 三插件）。
2. **README 与 M8_READINESS 状态统一**：README 升 2.0 现状口径（Phase 0 到 10 + 架构图补全），M8_READINESS 加历史快照注记（限制 4/5 已关闭留痕，1/2/3 如实存留）。

遗留台账更新：rtis.py 自测复测现值 10 过 7 败（原 4 败口径漂移，RTIS 侧非平台引入）；视觉基线遗留项勾销（P10-4 已建）。已知遗留逐项登记于 HANDOVER §七与 PLAN-17-SELFCHECK.md。

### 收尾批次门禁（入库当日复测）

| 门 | 结果 |
| --- | --- |
| 点名门 | cargo test --offline -p tool-registry -p plugin-registry 全绿（strict_json 5 用例 + json dup key 拒绝用例在列） |
| 点名门 | cargo test --offline -p tool-engine --test src_analysis_e2e 3 passed（V-9 digest 守卫，podman shim 在位时真实容器全链） |
| 点名门 | python3 -m unittest workers.python.tests.test_src_tools 21 用例 OK |
| manifest 回归 | rp-tool validate 全量 + rp-plugin manifest-inspect 三插件全过（strict JSON 零误拒） |
| 全量门 | 静场复跑全绿（记录见下） |

复测纪律记录（如实）：首轮全量复测与编译并发时出现三处并发负载表现，其中两处为 flake（vitest 5 败与视觉门 1 张基线快照，静场复跑核验），一处为**守卫真实拦截**：rtis.py 自测运行触发 resolver 审计追加（integrations/rtis/adapter_audit.jsonl 6 行），协调方原拟随批入库，被 tool-engine first_batch_b5 守卫（integrations/rtis/ 跟踪文件零改动红线）拒绝；处置 = 还原该文件（运行时审计痕迹不进跟踪面），守卫复位。教训入册：RTIS 侧运行（含自测）会写跟踪态审计文件，跑后必须核对 git status 并还原，B5 守卫是最后防线且有效。
