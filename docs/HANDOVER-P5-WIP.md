# HANDOVER：Phase 5 断点续作专档（2026-11-17，P5-8 已收口：P5-9/RELEASE 待做）

> 主交接入口仍是 docs/HANDOVER-RIP3-CONTINUATION.md（唯一入口原则不变）；
> 本文件聚焦 Phase 5 中断点的精确现场。接手会话读完主 HANDOVER 第四、五节
> （贯穿纪律与环境事实）后读本文件即可续作。本档取代前版断点（P5-8 收尾）。

## 一、状态快照

| 项 | 值 |
| --- | --- |
| HEAD | 092d46e8（P5-8 收口批：hypothesis 实拍链六步 PASS + 集成测试 2 条 + P5-8-BATCH-REPORT.md，已 push）；前一编排批 13381445（rip3-p5-finish 收官工作流入库） |
| HEAD CI | P5-8 批 run 44115292783 查询时点 in_progress，终态由 rip3-p5-finish 收口确认段兜底核实（红则修复批） |
| 工作树 | 4 项用户未提交 WSW 素材（M docs/README.md、M Phase 5 计划书、?? WEB-SECURITY-WORKBENCH-PLAN.md、?? docs/ui-ux/）不提交不还原不清理；批次 commit 显式 add 禁 add -A |
| Phase 5 进度 | P5-0 至 P5-8 八批已收口入库；P5-9（三子批 9a/9b/9c）/RELEASE 未开 |
| 收官编排 | rip3-p5-finish 动态工作流（.zcode/workflows/rip3-p5-finish.dwf.ts）：P5-8 收口确认 → P5-9 三子批 → RELEASE，有动态工作流工具面会话 `CreateWorkflow saved: { name: "rip3-p5-finish" }` 启动；无工具面按 P5-CONTINUATION-WORKFLOW.md（仓库外手册 /media/emo/code/pensting/P5-CONTINUATION-WORKFLOW.md）d 案手动 |

## 二、Phase 5 已收口批次账目（提交链按时间序，P5-5 起补全）

| 批次 | 提交 | 报告 | CI |
| --- | --- | --- | --- |
| P5-0 至 P5-5 | 60fd4cd5/66662bc1/293fe4f8/ab5907e5/62d2d1fc/2fdfef6c | 六份批次报告 | 43931997748 success（b1460702 之前的绿基线） |
| P5-6 执行调度与 Activity | c49e89c9 | P5-6-BATCH-REPORT.md | 43939188733 success |
| P5-7 归一化收口/Evidence/RTIS 闭环 | bf55c9b3 + 修正 dbb582d1 + 报告 b1460702 | P5-7-BATCH-REPORT.md | bf55c9b3 run 43942245579 **红**（钉值漏改三处）→ 修正批后 b1460702 run 43943622788 success |

### P5-6 交付要点（详情看报告）

- result-normalized.v1 第五份契约 Schema + evaluator 契约测试正负例矩阵 +
  rp-verify checked_normalized_doc 落库前快失败；
- scheduler.rs 调度决策面：pending_items 幂等恢复（服务重启/暂停恢复不重复
  派发）+ runtime_budget_ok 运行期预算复核（P5-4 G 遗留落地）+
  budget_exhausted_result 超项 NOT_TESTED+budget_exhausted 留痕；
- plan-pause/plan-resume 子命令（跨进程经 SQLite 状态行项间检查位真实传播；
  paused 是 020 词表既有预留首次消费，迁移链零推进）；
- rp_server POST /api/research/plans/&lt;id&gt;/{pause,resume,cancel}（写面形状三，
  rp-verify 子进程零直连 SQL 写 + 鉴权强制）；
- Web 面：ResearchPlansPage 验证计划页（/research/plans 第五 tab）+
  ActivityPlansLive 运行中计划 5s 轮询接 Activity 任务队列页签（G10 收口）；
- 实拍：全链 execute-plan 到 NOT_REPRODUCED（退出码 0 不等于 CONFIRMED）+
  预算复核 cap15 到 10 放行/10 超项留痕 + 幂等恢复 resumed_skipped=1 + 取消
  stopped_by=cancelled + 控制端点冒烟（200/502/405）。

### P5-7 交付要点（详情看报告）

- 021 迁移 research_evidence（Evidence Artifact 采集与 SHA256，payload
  append-only 不可变；evidence_type 词表三值）+ 迁移链钉值九文件同步 021；
- store Evidence 面（record_evidence 词表/形状守卫 + SHA256 +
  evidence_collected 事件 020 预留面启用 + find_evidence_by_hash 重复识别
  + list_evidence）+ 2 集成测试；
- Trace 六 ID 贯通：execute_nuclei 改 submit_correlated + DispatchOptions
  打包 + execute-plan 一次动作一 trace 锚（tr-exec-*）贯通 set_result/
  dispatched 事件/证据双帧；
- RTIS Finding 闭环：record_verification（rt.py poc update --verified
  sanctioned 写路径）+ finding-link 子命令（非 CONFIRMED 拒绝/无
  rtis_poc_id 拒绝两守卫）+ evidence-list 检索子命令；
- 实拍：evidence 双帧落档（hash+trace+run 三面贯通）+ finding-link 双负例
  + 五态守卫负例（error_category 在场写 NOT_REPRODUCED 被 ResultGuard 拒）。

## 三、P5-8 半成品盘点（已随 4b949614 WIP 入库）

> 质量核：verification-engine 42 lib + 2 provider + 6 store 全过；clippy 全
> targets 零告警；domain 73 过；database/agent-engine/plugin-runtime/
> workflow-engine 钉值面 41 套件全绿。

| 文件 | 改动 | 状态 |
| --- | --- | --- |
| migrations/022_coverage_matrix.sql | 新文件：research_plan_item.dimension 七维 CHECK 列（**ALTER 单行**）+ research_hypothesis 表（§5.10 必含字段全落 + round/status）+ schema_version 22 + user_version 双写 | 编译过 + 41 套件钉值全绿 |
| 九个钉值测试文件 | 021→022 全同步（含逐枚清单与 uv/uv1 变体） | 全绿 |
| crates/domain/src/verification.rs | CoverageDimension 七维词表（as_str/parse/ALL + 单测） | 73 过 |
| crates/verification-engine/src/store.rs | dimension 接入（NewItem/PlanItemRecord serde skip/add_item 词表守卫/get_item 尾列 22）+ hypothesis CRUD（add_hypothesis 四重守卫 + set_hypothesis_status validated 终局 + list/get） | 编译过 |
| crates/verification-engine/src/coverage.rs | **新文件**：coverage_matrix 七维五态分计数 + 盲区判定（uncovered/unjudged）+ generate_blind_spot_hypotheses 假设草稿（来源依据/负向对照/停止条件内置）+ 3 单测 | 全过 |
| crates/verification-engine/src/fingerprint.rs | dedup_fingerprint_v2 八段键（画像版本+知识审核状态补段 = P5-4 G 遗留落地；v1/v2 值域独立跨版本失效安全侧）+ 2 单测 | 全过 |
| crates/verification-engine/src/planner.rs | PlanBudget.max_rounds（serde default 3）+ planner item dimension 缺省 assets | 过 |
| crates/verification-engine/src/bin/rp-verify.rs | coverage-show/hypothesis-add/hypothesis-list/hypothesis-status 四子命令 + fingerprint v2 参数 + item-add --dimension + usage | 编译过 |
| crates/verification-engine/{provider.rs,scheduler.rs,tests/provider.rs,tests/store.rs} | PlanItemRecord 初始化器补 dimension 字段 | 过 |

### 已实拍 PASS

- coverage-show：七维矩阵 + 盲区可见（assets 有项、其余六维 uncovered）；
- 指纹 v2 补段失效：profile v1/v2 与 knowledge approved/recalled 三键互异。

### 未完成清单（已全部收口，092d46e8，留档对账）

1. hypothesis 实拍链重跑：DONE（六步 PASS；原 /tmp 库被清，等效重建
   max_rounds=2 场景，新计划 vp-20261117-152808-425-00a6；实拍原文见
   P5-8-BATCH-REPORT.md E2）。
2. hypothesis 集成测试 2 条：DONE（tests/store.rs 生命周期/守卫负例矩阵，
   store 8 过全量 52 过）。
3. P5-8-BATCH-REPORT.md：DONE（A-K 十一节，教训三条 + RP_DB_PATH 教训）。
4. commit + push + CI：DONE（092d46e8 已 push；CI 终态由工作流收口确认段
   兜底）。

## 四、剩余批次（P5-8 收尾之后）

1. **P5-9 Workflow 模板、画布集成和阶段验收**（补充计划 §7 + §6）：
   通用渗透十三步模板 + SRC 十三步模板（沿 018 控制面与既有 DSL/XYFlow
   画布，workflows/ 目录 JSON 定义惯例）；节点 Schema 表单；画布保存和
   版本；运行轨迹/日志/Evidence；暂停/恢复/失败重试；端到端真实浏览器
   测试（web/scripts/rip3_p5_browser_e2e.mjs 断言模板扩展 + 截图入库）。
   完成标准：从 Project 创建 ResearchTask → 检索知识生成计划 → 本地靶场
   验证 → Evidence → Finding 走 RTIS 接口 → Coverage 实际变化 → Workflow
   可恢复且不重复运行已完成节点。
   完成标准里「本地靶场验证」即 P5-7 报告 G1 遗留的 finding-link 正例
   RTIS 写面实拍位（本地靶场真实 CONFIRMED 才回填 rt.py poc update）。
2. **Phase 5 收口**：docs/releases/RELEASE_2026-11-17_RIP3-PHASE5.md
   （验收对账逐条带证据）+ 全量门禁统一复测（cargo test --offline
   --workspace / clippy --all-targets -D warnings / python3 -m unittest
   discover -s workers/python/tests / web build+test / apps/server/
   selftest_phase*.sh + auth）+ HANDOVER 队列勾销（主 HANDOVER 第三节
   Phase 5 行 + 状态表）。

## 五、本阶段新增环境事实与坑位实录（接手必读）

主 HANDOVER 第五节全量适用，Phase 5 实施新增：

- **迁移链头 022**（SQLite：research_evidence 021 + coverage/hypothesis
  022）；**链尾钉值多形状定位法**（P5-7 CI 红 43942245579 教训）：演进
  链尾先按五个形状全量定位九文件再改：`vec!\[1, 2`、`vec!\[13, 14`、
  `(2X, "_`（链尾名）、`uv1?`、`001\.\.`/`013\.\.`/`010\.\.` 区间与枚数；
- **批量替换脚本陷阱**：整文件 sub() 遇一处 MISS 会跳过整个文件写入，
  已匹配的对也不落盘；每对独立判定独立写；
- **ALTER 必须单行书写**（migrate.rs 执行器行守卫会把多行 ALTER 切断，
  重放场景炸 CHECK 语法错误，022 本批实录）；
- **RP_DB_PATH 必须随每条 CLI 命令**（Bash 工具环境不跨调用持久；漏带
  即落内存空库，症状 = not_found 但代码无缺陷）；
- **DispatchOptions**（provider.rs）：execute_nuclei 的 task_id/trace_id/
  network/template_path 打包面（clippy 八参数阈值豁免先例 allow 三处在
  agent-engine）；execute_nuclei 已走 submit_correlated（trace_id 落列）；
- **checked_normalized_doc**（rp-verify）：归一化文档落库/发事件前契约
  校验点（result-normalized.v1，违规 exit 1）；
- **podman shim**：/tmp/podman-shim 会被清，重建：
  `mkdir -p /tmp/podman-shim && printf '#!/bin/sh\nexec distrobox-host-exec podman "$@"\n' > /tmp/podman-shim/podman && chmod +x /tmp/podman-shim/podman && export PATH="/tmp/podman-shim:$PATH"`；
- **nuclei 模板 fixture**：/tmp/p5-7-e2e/template.yaml（never-match 干净
  面）；实拍库 /tmp/p5-7-e2e/verify2.db（P5-6 场景 4 计划）与
  /tmp/p5-8-e2e/verify.db（P5-8 场景 plan vp-20261116-200640-186-006a）；
- **Web 契约测试钉值**：web/tests/workspaces.test.ts 路由数 16（P5-6 加
  /research/plans 后；P5-9 若再加 tab 记得同步）；
- **rp_server 控制端点**：/api/research/plans/&lt;id&gt;/{pause,resume,cancel}
  （形状三，len(parts)==5）；测试服务 7801 + token 走
  .rp-workbench-token（curl 前 `tr -d '\r\n '`）。

## 六、接手方式

- 首选：有动态工作流工具面的会话（仓库根）
  `CreateWorkflow saved: { name: "rip3-p5-finish" }`（或 path 直提
  .zcode/workflows/rip3-p5-finish.dwf.ts）自主跑完 P5-8 收口确认 →
  P5-9 三子批 → RELEASE；受阻处置站（hole）按通知处置即可；
- 无工具面会话：按 /media/emo/code/pensting/P5-CONTINUATION-WORKFLOW.md
  d 案手动（段 B P5-9 三子批 → 段 C 全量门禁 + RELEASE；段 A 已收口）；
- 每批纪律不变：A-J 报告落 docs/rip3/batches/ + commit（中文长描述，显式
  add 禁 add -A）+ push（SSH 断连重试 2-3 次）+ CI 红不进下一批 + 批内
  定向测试（全量门禁只在收口时编排面静场串行复测）；
- 硬红线不变：RTIS 只走 rt.py CLI 零直写、POC 正文零复制、新增域永不落
  PG 链（SQLite 运营面 022+ 沿 P5-0-B3 判定）、五态纪律、授权审批不内置、
  宿主机零系统级安装、生产 migrations/platform*.db 零触碰、不 reset/
  clean/checkout 改写历史。
