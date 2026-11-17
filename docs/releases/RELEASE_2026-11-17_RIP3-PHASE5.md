# RELEASE：RIP3 Phase 5 收口（2026-11-17 至 2026-11-18）

> 阶段范围 = 补充计划 docs/rip3/RIP3-PHASE5-WEB-WORKBENCH-AND-KNOWLEDGE-DRIVEN-VERIFICATION-PLAN.md 全批次（P5-0 至 P5-9）+ Web 工作台 + 知识驱动验证引擎。本文为 Phase 5 阶段总账，逐条对账计划 §17 完成定义与 §14 六场景，证据全部取自批次报告原文与本收口批实测。

## 1. 提交链与批次报告（P5-0 至 P5-9c）

| 批次 | commit | 报告 |
| --- | --- | --- |
| P5-0 至 P5-5 | 60fd4cd5 / 66662bc1 / 293fe4f8 / ab5907e5 / 62d2d1fc / 2fdfef6c | docs/rip3/batches/P5-0 至 P5-5-BATCH-REPORT.md 六份 |
| P5-6 执行调度与 Activity | c49e89c9 | P5-6-BATCH-REPORT.md |
| P5-7 归一化收口/Evidence/RTIS 闭环 | bf55c9b3 + dbb582d1 + b1460702 | P5-7-BATCH-REPORT.md |
| P5-8 覆盖矩阵与自适应研究 | 4b949614（WIP）+ 092d46e8（收口）+ 13381445 + 0082cca7 | P5-8-BATCH-REPORT.md |
| P5-9a Workflow 模板与画布 | dd0860ae + 942d453c | P5-9a-BATCH-REPORT.md |
| P5-9b Evidence 与 Coverage 页面 | 90311c1b + 6f636123 | P5-9b-BATCH-REPORT.md |
| P5-9c 本地靶场实拍与端到端验收 | 9833aedc | P5-9c-BATCH-REPORT.md |
| 本收口批（selftest 钉值同步 + RELEASE） | 见本批 commit | 本文 |

CI：各批次 push 后 GitHub CI 全绿为硬门槛（P5-7 红 43942245579 修复实录在案）；P5-9c 收口 run 44183557099 四 job 全绿（Rust workspace 3m53s / 协议一致性载具 1m9s / Web 工作台 32s / Python worker 19s）。

## 2. 门禁终测（2026-11-18 收口批实测，静场串行）

| 门 | 结果 |
| --- | --- |
| cargo test --offline --workspace -- --test-threads=1 | exit 0，**775 passed 0 failed**（137 个测试二进制；Phase 4 收口基线 714，Phase 5 增 61） |
| cargo clippy --offline --workspace --all-targets -- -D warnings | exit 0 零告警 |
| python3 -m unittest discover -s workers/python/tests | exit 0，**232 passed**（skipped 11） |
| web npm run build / npm run test | 双 exit 0；build 635ms；vitest **19 files / 124 passed** |
| apps/server/selftest_auth.sh | exit 0，**11/11** |
| apps/server/selftest_phase*.sh 全集（14 脚本） | **14/14 全绿**（本批修四处预存钉值后，见第 5 节） |
| integrations/rtis/ | 零 diff（git status 实测），未跑 rtis.py test |

脚本教训：workspace 全量集成测试并发派发容器会挂死，收口面一律 --test-threads=1 串行（本机实录沿用）。

## 3. Phase 5 交付总账（九批要点，详见各批次报告）

- **P5-0**：只读审计与设计冻结（页面复用矩阵 / 后端能力矩阵 / 差距清单 / ADR 草案）。
- **P5-1**：Research Workspace 页面族（总览/任务/资产/知识上下文，真实 API）。
- **P5-2**：验证契约与 Knowledge Matcher（双渠道 match：pg-intel + rtis-poc，命中与排除原因逐条可解释）。
- **P5-3**：Verification Planner（预算封顶 / 缓存决策 / 排序 / plan-build 落库）。
- **P5-4**：Nuclei Provider + rip-nuclei 镜像族 + 指纹去重（manifests digest 钉死 + egress 白名单 + 每族并发 1）。
- **P5-5**：execute-plan 执行链（020 验证编排域 + Provider 派发 + 五态归一化前契）。
- **P5-6**：调度决策面（幂等恢复 resumed_skipped / 运行期预算复核 budget_exhausted / plan-pause/resume/cancel 跨进程控制 + rp_server 控制端点 + ResearchPlansPage + Activity 接入）。
- **P5-7**：021 research_evidence（SHA256 append-only）+ Trace 六 ID 贯通（submit_correlated）+ RTIS Finding 闭环守卫面（finding-link 双守卫）+ Evidence 检索。
- **P5-8**：022 coverage/hypothesis（七维五态矩阵 / 盲区假设草稿 / max_rounds 轮次守卫）+ 指纹 v2 八段键（画像版本与知识审核状态补段）。
- **P5-9a**：十三步双模板（wf-pentest-research-v1 / wf-src-bughunt-v1，§6.1/§6.2 逐条映射，零新增节点类型 ADR / 循环上限以节点参数承载 ADR）+ 模板钉值集成测试 3 条。
- **P5-9b**：Evidence rp_server 端点 ×3 + UI Evidence 页与 Coverage 矩阵页（路由 16 → 18）+ execute_nuclei `-t` 旗标缺陷修复（真实 nuclei 执行链首通）。
- **P5-9c**：本地靶场落仓库（scripts/local_range.py + 检测模板）+ egress 链路三层修复贯通（nuclei `-proxy` env 代理承载 / 帧协议 stdout 静默 / host 派生 scheme 与端口白名单约束）+ 镜像换钉 sha256:9ff41ce9…（五处同步）+ RTIS Finding 闭环正例首拍（rt.py poc add 900064 → 命中 INCONCLUSIVE → 人工审证 CONFIRMED → finding-link → RTIS verified=1）+ §14 六场景演练 + 浏览器 11/11。

## 4. §17 完成定义逐条对账（计划原文判据）

| 判据 | 状态 | 证据 |
| --- | --- | --- |
| 五区桌面布局 + Research Workspace 页面族 | 达成 | P5-1/P5-6/P5-9b 路由与页面族（18 路由钉值，workspaces.test.ts） |
| 知识驱动验证引擎（Match → Planner → Provider → 五态归一） | 达成 | plan-build 双渠道实测（P5-9c E4-B）+ execute-plan 全链（P5-9b E3/P5-9c E2） |
| 五态纪律（退出码 0 不等于 CONFIRMED 等） | 达成 | 命中面 INCONCLUSIVE 真实呈现（P5-9c）+ ResultGuard/evaluator 契约矩阵（P5-6）+ finding-link 非 CONFIRMED 拒绝（P5-9c E4-E） |
| Evidence Artifact 采集与 SHA256 | 达成 | 021 表 + 双帧落档 hash/trace 三面贯通（P5-7/P5-9c E2） |
| RTIS 既有接口闭环（零直写零行为变更） | 达成 | finding-link 正例 900064 verified=1（P5-9c E3）+ integrations/rtis 零 diff |
| 覆盖矩阵七维五态与盲区 | 达成 | coverage-show + UI 矩阵页（P5-8/P5-9b 浏览器 16 断言） |
| Workflow 模板机制 | 达成 | 十三步双模板入库 + 钉值测试 + Workflow Studio 装载（P5-9a） |
| 完成标准七条（§7 P5-9） | 达成 | P5-9c 报告 I 节逐条对账表（模板选择/知识计划/本地靶场/Evidence/RTIS/Coverage 变化/幂等恢复） |
| 浏览器实测截图入库 | 达成 | browser-e2e/p5-9b/ 七张 + p5-9c/ 六张（16/16 与 11/11 断言全过） |

## 5. 本收口批：selftest 全集四处预存钉值同步（坏消息单列 + 教训）

全量全集口径首次串行复测暴露 4 红引入面全部先于 Phase 5（Phase 5 零引入，逐个有引入提交证据）：

| 脚本 | 预存钉值 | 引入 | 本批修正 |
| --- | --- | --- | --- |
| selftest_phase1_pg.sh | PG 链头钉 5 | V0006 随 RIP3 P4-1（29585fe7） | 期望改 6（注释带引入出处） |
| selftest_phase6.sh | install/upgrade 无 confirm | P10-2 信任级分级确认守卫 | local 带 confirm=true、remote 带 confirm_id（守卫正确用法，非绕过）；PASS 49（playwright 项按 V-9 环境面如实 SKIP 语义在脚本内） |
| selftest_phase9_rest.sh + docs/phase9/REST-API-CONTRACT.md | ROUTES 钉 17 / 文档缺 /api/documents 行与 cleanup-rollback 词 | /api/documents 随 RIP3 P1-1（41bf3359）；cleanup-rollback 词表既有 | 钉值改 18/19 + 文档补行补词 |
| selftest_phase10_plugin_eco.sh | 目录条目断言读 CLI stdout 摘要文件（entries int 4 含 catalog.json 自身）| catalog.json 发布产物随 RIP 2.0 收尾批（2744adaf）落 manifests/plugins | 断言改读 --out 原生文件按 plugin_id 非空计数（=3）+ 首条目取真插件条目 + stdout 分流 /dev/null 消除同文件竞态；PASS 53 |

修复语义全部为"断言更新到真实基线"，原校验意图零删除零放宽。教训一条：**selftest 钉值必须随链头与契约面演进同步**，旧钉值在"只跑本阶段 selftest"的历史口径下长期隐身（Phase 4 RELEASE 只跑 phase4 + auth），全量全集口径才暴露；后续 RELEASE 维持全集口径。

## 6. §14 六场景对账（引用 P5-9c-BATCH-REPORT.md E4 节证据表）

| 场景 | 结论 | 证据位 |
| --- | --- | --- |
| A 从 Project 建研究 | 达成（UI 创建编排容器未建如实登记） | 浏览器 p5-9c/01/02 + 模板入库 version-chip |
| B 知识生成验证计划 | 达成 | plan-build 双渠道 + 排除原因 + 预算面（P5-9c E4-B） |
| C 本地靶场执行 | 达成 | egress 全链命中 + Evidence 双帧（P5-9c E2） |
| D 重复执行与缓存 | 达成 | fingerprint 同输入同指纹 + profile-version 失效 + 幂等恢复（P5-6 实拍沿用） |
| E 无命中补充研究 | 达成 | 盲区假设登记 + 无证据 Finding 守卫拒（P5-9c E4-E） |
| F RTIS 与证据闭环 | 达成 | CONFIRMED → finding-link → RTIS verified（P5-9c E3） |

## 7. 遗留转登记（未完成与限制，坏消息单列）

1. ZAP Adapter 未接入（completion 由 Nuclei 满足，Phase 5 后扩展项，P5-5 G3）。
2. 多模板批量与标签过滤（P5-5 G2，扩展项）。
3. P5-6 G1 项内取消看门狗重拍、G2 跨计划全局并发闸（tool-engine 既有能力，沿用登记）。
4. pg-intel 渠道 rp-kc search exit 1（P5-9c 演练环境实测；渠道错误如实透出不静默；PG 检索面随 Phase 8 批统一核查）。
5. 场景 A"创建 ResearchTask"UI 编排容器未建（现状只读镜像 + 模板选择面；计划链由 plan-create/plan-build CLI 面承载）。
6. egress 白名单约束边界（egress_proxy.rs 边界声明沿用）：代理层策略只约束遵守代理的工具；nuclei 经 P5-9c wrapper `-proxy` 修复后纳入约束面，其它第三方工具接入须同口径实测。
7. agent_run/artifact 两锚在验证链无实体（Evidence 档承载 Artifact 语义，P5-7 G2 登记）。
8. P4-4 G 遗留沿用：tool-engine 容器 stdout 并发读、MMR 尺度、缓存键已修；D4 阈值 0.83 到 0.92 区间维持。

## 8. RTIS 对账零回归声明

本阶段全程只走 rt.py CLI / rtis MCP 工具面（poc add / poc get / poc update --verified sanctioned 既有接口），零直写零行为变更；POC 正文零复制（900064 元数据面承载，检测模板正文在仓库 scripts/local_range_template.yaml 不进 RTIS）；零触碰既有 POC 条目 verified 状态；收口实测 `git status --porcelain -- integrations/rtis/` 零输出。15155 口径对账零回归。

## 9. 下一阶段入口

- **WSW-0 只读审计与 POC**（用户任务书第三阶段，Phase 5 收口后即启动）：Rust 网络能力审计 / 成熟库评估（hudsucker、http-mitm-proxy、slinger、hyper、rustls、tokio-tungstenite）/ 复用矩阵 / HTTP-TLS-MITM POC / ADR / WSW-1 任务书；UI 长期规范 = docs/ui-ux/AI-IMPLEMENTATION-GUIDE.md。WSW-0 完成前禁止大规模 Proxy/MITM 实现。
- RIP3 主队列续段：Phase 6 报告智能 → Phase 7 知识回流 → Phase 8 规模测试（主 HANDOVER 第三节序 5 到 7）。
