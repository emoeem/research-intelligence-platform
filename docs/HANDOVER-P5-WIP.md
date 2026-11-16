# HANDOVER：Phase 5 断点续作专档（2026-11-17，实施工程师子代理中止时点）

> 主交接入口仍是 docs/HANDOVER-RIP3-CONTINUATION.md（唯一入口原则不变）；
> 本文件聚焦 Phase 5 中断点的精确现场：哪些已收口、半成品是什么、剩余批次
> 怎么接。接手会话读完主 HANDOVER 第一、四、五、七节后读本文件即可续作。

## 一、状态快照

| 项 | 值 |
| --- | --- |
| HEAD | 20f18d76（交接专档 + 半成品 WIP 入库，已 push 至 origin/main；P5-5 批 = 2fdfef6c） |
| HEAD CI | run 43930302701（停止时点 in_progress，接手先核对结论；红则先修复） |
| 工作树 | 干净（P5-6/P5-7 交叠半成品已随 20f18d76 WIP 入库，见第三节） |
| Phase 5 进度 | P5-0 至 P5-5 六批已收口入库；P5-6/P5-7 半成品在途；P5-8/P5-9/RELEASE 未开 |
| 收官编排 | rip3-finish.dwf.ts 语义手动驱动中（Agent 子代理逐批实施 + 编排面门禁复测）；子代理被用户手动停止，接手会话按第六节方式续 |

## 二、Phase 5 已收口批次账目（提交链按时间序）

| 批次 | 提交 | 报告 | 交付要点 |
| --- | --- | --- | --- |
| P5-0 只读审计 | 60fd4cd5 | P5-0-BATCH-REPORT.md | 页面复用矩阵 + 后端能力复用矩阵 + 差距清单 G1-G10 + B3 存储判定冻结（编排执行态落 SQLite 020+ 运营面，PG 链本阶段零新表） |
| P5-1 Web 骨架 | 66662bc1 | P5-1-BATCH-REPORT.md | ResearchPage 占位退役（四 tab：总览/任务/资产/知识上下文）+ data.tsx 表格基元 + 视觉门基线四张 + rip3_p5_browser_e2e.mjs 11 断言 + vitest 124 + playwright 71 含 axe AA |
| P5-2 验证契约 | 293fe4f8 | （P5-2 报告随链） | SQLite 020 迁移三表（research_plan/plan_item/plan_event）+ domain::verification 五态词表 + 契约 Schema 四份 v1 + verification-engine crate（store/ResultGuard 五态守卫/fingerprint 六段）/rp-verify 九子命令 + rp_server 三只读端点 |
| CI 修复 + P5-3 代码 | ab5907e5 | P5-3-BATCH-REPORT.md（62d2d1fc 批补） | 020 链尾演进八处测试钉值更新 + matcher.rs 双渠道知识匹配（pg-intel 走 rp-kc hybrid 禁绕行 + rtis-poc 走 rt.py 元数据面）+ 结构化适用性判定逐条可解释 |
| P5-4 Planner | 62d2d1fc | P5-4-BATCH-REPORT.md | draft_from_match（排除转 skipped 带原因）+ 预算后端封顶（cap15→10+5 实拍）+ 六段指纹缓存（POC 版本/params 变化失效实测）+ rp-verify plan-build |
| P5-5 Provider Adapter | 2fdfef6c | P5-5-BATCH-REPORT.md | RtisPocProvider（rt.py poc get/use sanctioned，RTIS 零 diff）+ NucleiProvider（tool_engine → podman_host 容器派发端到端 SUCCEEDED）+ rip-nuclei 镜像族（nuclei@sha256:582d5546 v3.11.1，digest sha256:19ecdd80 三处同步）+ rip.security.nuclei Manifest 九条校验 + podman_host POLICY→POLICY_DENIED 词表修复 |

批次报告全目录：docs/rip3/batches/P5-0 至 P5-5 六份。

## 三、P5-6/P5-7 交叠半成品盘点（已随 20f18d76 入库，WIP 备份先例）

> 注：交接专档提交（20f18d76）按 WIP 备份惯例把半成品一并入库；接手即在其
> 基础上收口，无工作树残留。质量核：clippy -p verification-engine 全 targets
> 零告警 + cargo test -p verification-engine 38 passed 0 failed。

| 文件 | 改动 | 状态 |
| --- | --- | --- |
| crates/verification-engine/src/evaluator.rs | **新文件（244 行）**：结果归一化五态判定（ProviderOutcome → NormalizedResult；五态纪律硬约束逐条落为规则：干净无命中=NOT_REPRODUCED、命中=INCONCLUSIVE 待复核、失败/超时=INCONCLUSIVE+error_category、不可用/取消=NOT_TESTED、FALSE_POSITIVE 只能显式判定） | 编译过 + clippy 零告警 + 38 测试全过（含 evaluator 面）；result-normalized.v1 契约测试与批次报告未写 |
| crates/verification-engine/src/bin/rp-verify.rs | +228 行（evaluate 子命令面等） | 编译过 + 38 测试全过；全链实跑未做 |
| crates/verification-engine/src/lib.rs | 注册 evaluator 模块与导出 | 随上 |
| crates/verification-engine/src/provider.rs | +19 行（归一化对接小改） | 随上 |
| crates/verification-engine/{Cargo.toml,tests/provider.rs} | 小改 | 随上 |

语义注记：这批半成品实际横跨 P5-6（执行调度）与 P5-7（结果归一化）的交叠
面——evaluator 是 P5-7 的核心判定层提前落地。接手时按「P5-6 执行调度收口
+ P5-7 归一化收口」合并或分批均可，以批次报告口径自洽为准。

## 四、剩余批次（Phase 5 收口前）

1. **P5-6 执行调度与 Activity**（补充计划 §7）：Workflow Engine 派发、
   Task/Run/Trace 语义复用、暂停/恢复/取消真实传播、并发/预算/超时限制、
   Activity 面板实时呈现；验收 = 前后端任务状态一致、刷新不丢任务、取消
   真实生效、服务重启不重复派发、部分失败不丢 Evidence。
2. **P5-7 结果归一化、Evidence 与 RTIS 闭环**：evaluator 收口（半成品基础上
   补单测 + result-normalized.v1 契约测试）、Evidence Artifact 采集与
   SHA256、Trace 六 ID、RTIS Finding 读取/引用/既有接口写入、重复 Finding
   识别、五态守卫负例实拍。
3. **P5-8 Coverage Matrix 与自适应研究**：七维覆盖矩阵（Assets/API/
   Identity/Input/Business Flows/File Processing/Data Flows）、假设生成
   （来源依据/负向对照/停止条件，无证据不生成 Finding）、预算与最大轮次；
   planner 遗留的两段指纹键（画像版本 + 知识审核状态）在本批补段。
4. **P5-9 Workflow 模板与画布集成**：渗透/SRC 十三步模板（沿 018 控制面与
   现有 DSL/XYFlow 画布）、节点 Schema 表单、端到端真实浏览器验收截图
   （browser-e2e 目录，模板 web/scripts/rip3_p5_browser_e2e.mjs）。
5. **Phase 5 收口**：docs/releases/RELEASE_2026-11-17_RIP3-PHASE5.md（验收
   对账逐条带证据）+ HANDOVER 队列勾销。

Phase 5 之后：Phase 6 报告智能 → Phase 7 知识回流 → Phase 8 规模测试 →
收官自查（R2 §17 + D8 评估，docs/HANDOVER-RIP3-CONTINUATION.md 队列），按
既定编排模式继续。

## 五、接手前必读与环境事实

主 HANDOVER 第四节（贯穿纪律）与第五节（环境事实）全量适用，本阶段新增：

- **verification-engine crate 已存在**（crates/verification-engine：matcher/
  planner/provider/store/contract/fingerprint/evaluator 七模块 + rp-verify
  CLI + 三表 store），续作禁止重建同语义模块；
- **020 迁移链头**：SQLite 020 = 研究编排域三表（research_plan/plan_item/
  plan_event）；链尾推进须同步八处测试钉值（ab5907e5 实录清单）；
- **rip-nuclei 镜像族**在位（nuclei v3.11.1 digest sha256:19ecdd80，模板锚
  默认目录不联网装；.nuclei-ignore 空表必须就位坑位见 P5-5 报告）；
- **检索面缓存**：intel-db search_cache（SEARCH_CACHE_VERSION v3），改缓存
  键结构必须递增版本；MMR 只能做在 0-1 相关分上（rerank 后）；
- **测试栈**：rp_server 7801（库 cp 副本 rip-data/dev/）+ vite 5173，
  token 走 localStorage rip_auth_token（真值 .rp-workbench-token）；
- **UI 截图脚本**：web/scripts/p5_current_ui_shots.mjs（现状）与
  rip3_p5_browser_e2e.mjs（断言模板）；
- **RTIS 接口实测口径**：rt.py poc search --q <英文> --json（中文零命中，
  组件名用英文）；poc get/use 为 sanctioned 写路径（RTIS 自有审计，非直写）；
- **五态纪律**（补充计划 §5.7，evaluator.rs 已落为规则）：工具退出码 0 ≠
  CONFIRMED；执行失败/超时 ≠ NOT_REPRODUCED；未测试不得统计为通过；
  FALSE_POSITIVE 只能显式判定。

## 六、接手方式

- **续用编排模式**（推荐，前六批即此模式）：派实施工程师子代理，注入本
  文件第三、四、五节 + 主 HANDOVER 第四五节纪律，从 P5-6 半成品续作；
- **手动 d 案**：按第四节批次顺序逐批推进，每批 §17.6 A-H 报告 + commit +
  push + CI watch 至绿；
- 接手第一步：`git status -sb` 核对工作树干净 → `gh run view <20f18d76 的
  run id>` 核对 CI 结论（红先修复）→ 读 P5-4/P5-5 两份批次报告的 G 节遗留 →
  开 P5-6/P5-7 收口（半成品已入库，直接在其上补契约测试与批次报告）。

## 七、编排面账目（本工作流运行实录）

| 事项 | 结果 |
| --- | --- |
| 环境预检 | 过（git 干净 / podman 6.2.0 / rip-pg17 就绪） |
| Phase 5 实施子代理 | P5-0 至 P5-5 六批交付后被用户手动停止（P5-6/P5-7 交叠半成品随后以 WIP 备份入库 = 20f18d76） |
| 编排面门禁复测 | Phase 5 未到收口点，未触发；子代理批内自测口径 = workspace 136 套件 0 FAILED + clippy 零告警（P5-5 提交信息实录） |
| 待续 | P5-6 起剩余批次（第四节）+ Phase 6/7/8 + 收官自查 |
