# P5-6 批次报告：执行调度与 Activity（含 P5-7 归一化契约面收口）

> 批次类型：实现批（P5-6/P5-7 交叠半成品收口批）。规格源：Phase 5 补充计划
> §7 P5-6 + §5.7/§5.9；半成品现场：docs/HANDOVER-P5-WIP.md 第三节
> （evaluator.rs 结果归一化 + rp-verify evaluate 子命令已随 20f18d76 WIP
> 入库，clippy 零告警 + 38 测试全过）。本批在其上收口并交付 P5-6 调度面；
> P5-7 的 Evidence/Trace/RTIS 闭环剩余面随 P5-7 收口批交付（见 H 节）。

## A. 本批完成内容

1. **result-normalized.v1 契约面收口（P5-7 半成品核心缺口）**：
   - 新建 schemas/verification/result-normalized.v1.json（第五份契约
     Schema）：schema_version 恒 1 + item_id 形状（vi- 前缀 store 生成
     pattern）+ result_status 五态词表 enum + result_reason minLength 1 +
     error_category 七词值 enum（null = 无分类）；additionalProperties
     false；
   - contract.rs：RESULT_SCHEMA include_str 内嵌（磁盘文件为源，四份惯例
     同款）+ `validate_normalized_doc` 校验入口；
   - **evaluator 契约测试**（evaluator.rs tests）：正例矩阵六形态（干净无
     命中 NOT_REPRODUCED / 命中 INCONCLUSIVE / 超时 INCONCLUSIVE+timeout /
     取消 NOT_TESTED / 不可用 NOT_TESTED+tool_unavailable / 网络失败
     INCONCLUSIVE+network）全部过 Schema；负例矩阵（工具词值混入
     result_status / 分类词表外 / 空 reason / 版本漂移 / ID 形状违规 /
     多余字段）一律拒绝。
2. **rp-verify execute-plan 落库前契约校验**：`checked_normalized_doc`
   helper（normalized_doc 组装 + result-normalized.v1 校验，违规 emit
   错误帧 exit 1 同守卫拒绝口径），归一化文档进 dispatched 事件 payload
   与输出 record 前快失败。
3. **执行调度决策面**（crates/verification-engine/src/scheduler.rs 新
   模块，调度决策纯函数层与 evaluator 正交）：
   - `pending_items`：幂等恢复筛选（只派发未判定项；五态任意写入即该轮
     派发面已完成），服务重启/暂停恢复后重跑 execute-plan 不重复派发
     （P5-6 完成标准「服务重启后不出现重复派发」）；
   - `runtime_budget_ok`：运行期执行侧预算复核（Σ 已放行 + 本项 ≤ 计划
     max_requests；P5-4 G 遗留「执行侧预算复核随 P5-6」落地，planner
     生成侧封顶之外的第二道闸，§5.9）；
   - `budget_exhausted_result`：超预算项留痕结论 NOT_TESTED +
     budget_exhausted + 剩余量说明（未执行 ≠ 任何结论；过 store
     ResultGuard 联动断言）。
4. **rp-verify execute-plan 接入调度决策面**：幂等恢复（resumed_skipped
   统计）+ 运行期预算复核（超项 NOT_TESTED+budget_exhausted 落库 +
   item_skipped 事件）+ 输出帧扩展（resumed_skipped/budget_skipped/
   stopped_by）。
5. **计划级暂停/恢复**（P5-6「暂停/恢复/取消真实传播」）：plan-pause /
   plan-resume 新子命令（executing/ready→paused；paused→ready）；执行中
   进程在项间检查位读 plan 状态行（SQLite），跨进程真实传播；paused 非
   终态（PlanStatus::Paused 全链在位：020 CHECK 词表/domain enum/
   is_terminal false）；usage_text 同步。
6. **rp_server 验证计划控制端点**（写面白名单形状三）：POST
   /api/research/plans/<plan_id>/{pause,resume,cancel} → rp-verify 子
   进程（本进程零直连 SQL 写，workflow_write 同款纪律）；require_auth +
   AUTH_ENABLED 强制 + 守卫拒绝按 502 原样透出 code/message。
7. **Web 面（Activity 注入 + 验证计划页）**：
   - ResearchPlansPage（web/src/routes/research/ResearchPlansPage.tsx
     新页）：/api/research/plans 真实列表 + 状态/文本过滤 + 五态计数徽标
     （RESULT_STATUS_TONES）+ 暂停/恢复/取消控制按钮（apiSend POST；终态
     不渲染按钮不渲染编造操作）+ busy/actionError 反馈；
   - ActivityPlansLive（web/src/components/ActivityPlansLive.tsx 新
     组件）：运行中（executing/paused）验证计划实时呈现（5s 轮询），接
     Layout activityTabs「任务队列」页签（P5-0 差距 G10 空壳收口）；
   - hooks.ts useApi 扩展 refetchIntervalMs 透传（缺省不轮询，既有调用
     面行为不变）；
   - 路由 /research/plans + workspaces.ts research 子导航第五 tab
     （验证计划，shieldCheck 图标键三处同步：WorkspaceIconKey 词表/
     WORKSPACE_ICON_KEYS/Layout ICONS map）。

## B. 架构变化和复用说明

- **派发语义口径（如实登记）**：P5-6 规格的「使用 Workflow Engine 派发」
  本批以「research_plan 状态机（020 编排容器）+ tool_engine 容器派发
  （Run 语义）+ tool_run_id/trace_id 列（Trace 语义锚）」承载，即 Task/
  Run/Trace 语义复用达成；workflow-engine DSL 实例化派发的完整面随 P5-9
  模板批次（十三步模板走 workflow DSL，P5-0 审计 G6「节点型扩展需 DSL
  评估」判定的落点）。理由：验证计划项数量运行期才定，静态 workflow
  JSON 定义无法承载逐项动态派发；plan 状态机 + tool_run 是既有能力
  （R2 原则 G 继承优先）。
- 不建第二套任务队列/Workflow Runtime/工具注册表：调度决策纯函数面在
  verification-engine 内（scheduler.rs），工具派发复用 tool-engine，
  控制面经 SQLite 状态行跨进程传播（无新常驻进程）。
- 成熟库优先：本批零新依赖（react-query 轮询用既有 useApi 透传）。

## C. 修改文件清单

| 文件 | 改动 |
| --- | --- |
| schemas/verification/result-normalized.v1.json | 新增（第五份契约 Schema） |
| crates/verification-engine/src/contract.rs | RESULT_SCHEMA + validate_normalized_doc + embed 测试更新 |
| crates/verification-engine/src/evaluator.rs | 契约测试正负例矩阵 + validate_normalized_doc import |
| crates/verification-engine/src/scheduler.rs | 新增（调度决策纯函数层 + 4 单测） |
| crates/verification-engine/src/lib.rs | scheduler 模块注册与导出 |
| crates/verification-engine/src/bin/rp-verify.rs | checked_normalized_doc + execute-plan 调度接入 + plan-pause/plan-resume + usage |
| apps/server/rp_server.py | RESEARCH_PLAN_CONTROL_ACTIONS + research_plan_control + do_POST 形状三 |
| web/src/routes/research/shared.tsx | 验证计划类型面（ResearchPlanRow/List + PLAN_*/RESULT_* 词表与 tones） |
| web/src/routes/research/ResearchPlansPage.tsx | 新增（验证计划页：列表/过滤/五态计数/控制按钮） |
| web/src/components/ActivityPlansLive.tsx | 新增（Activity 运行中计划实时面） |
| web/src/components/Layout.tsx | queue 页签注入 ActivityPlansLive + ShieldCheck 图标 |
| web/src/workspaces.ts | shieldCheck 图标键 + research 第五 tab |
| web/src/App.tsx | /research/plans 路由 |
| web/src/lib/hooks.ts | useApi refetchIntervalMs 透传 |
| web/tests/workspaces.test.ts | 路由数钉值 15→16（P5-6 子页，注释同步） |

## D. 数据库迁移、Schema 和兼容性变化

- 迁移链零推进（SQLite 020 链头不动；paused 词值在 020 CHECK 词表内已有
  预留，domain PlanStatus::Paused 全链在位，本批只是首次消费）。
- 契约 Schema 第五份 result-normalized.v1 冻结为 1；SCHEMA_VERSION 常量
  不变（四份 v1 惯例：文件级 Schema 独立演进，契约测试钉面）。
- rp_server API：新增写端点形状三（/api/research/plans/<id>/{action}），
  既有端点零改动；读面响应形状不变。

## E. 测试命令及真实结果

```text
cargo test --offline -p verification-engine
  → 38 lib passed（含 scheduler 4 新测 + evaluator 契约 2 新测）
  → 2 provider + 4 store integration passed；0 failed
cargo clippy --offline -p verification-engine --all-targets -- -D warnings
  → Finished dev profile，零告警
cd web && npm run build
  → ✓ built in 465ms（chunk 大小警告为既有态）
cd web && npm run test
  → Test Files 19 passed (19)；Tests 124 passed (124)（钉值 15→16 修正后）
bash apps/server/selftest_auth.sh
  → PASS=11 FAIL=0
```

## E2. CI 各 Job 状态和运行链接

- 本批 push 后 CI run 见 GitHub Actions（push 后 gh run watch 至绿，红不
  进下一批；基线 HEAD 5c03fe1d run 43931997748 success）。

## F. 性能数据

- 调度决策为纯函数面（O(n) 过滤，无 IO）；Activity 轮询 5s 一次
  plan-list 子进程（限流面与既有 plan-list 读端点同源）。无新增基准项。

## G. 未解决问题与风险

1. **取消传播的项内看门狗依赖 rp-tool cancel**（tool_run
   cancel_requested_at）：计划级 plan-cancel/plan-pause 在项间检查位生
   效（实拍 stopped_by=cancelled），项内取消真实生效链（rp-tool cancel →
   tool-engine 看门狗 → 容器终止）属 tool-engine 既有能力，本批未重拍。
2. **并发限制语义**：本批执行面逐项串行派发（每族并发 1，§5.9 默认）；
   多计划并行时的全局并发闸（跨 plan 的工具族互斥）未实施（既有
   tool-engine 单计划面内并发 1 已满足当前单计划执行入口）。
3. **P5-7 剩余面**（Evidence Artifact 采集与 SHA256 / Trace 六 ID 贯通 /
   RTIS Finding 读取引用写入 / 重复 Finding 识别 / 五态守卫负例实拍）
   随 P5-7 收口批交付：本批只收口 evaluator 归一化契约面（半成品现场
   缺口）。
4. ZAP Adapter 未接入（P5-5 G3 遗留沿用，Phase 5 收口后扩展项）。
5. egress_allowlist 派发面真实目标实拍（P5-5 G1 遗留）沿用：本批实拍走
   NetworkPlan::None 本机面；egress 链路实拍随 P5-7/P5-9 本地靶场批次。

## H. 下一批依赖和明确的非目标

- P5-7 收口批依赖：本批的 NormalizedResult/契约面（Evidence 采集在
  result_assigned 事件链上挂 artifact 记录）、tool_run_id（Trace 六 ID
  锚）、rtis_poc_id（RTIS Finding 引用面）。
- 非目标：不实施 workflow-engine DSL 实例化派发（P5-9）；不做跨计划全局
  并发闸；不动 020 迁移链；RTIS Finding 写面走 rt.py 既有接口零直写。

## I. UI 截图与浏览器验收证据

- 本批 Web 面为列表/控制面增量（Activity queue 页签 + /research/plans
  页）；端到端真实浏览器逐功能点实测与截图随 P5-9 收口批统一执行
  （web/scripts/rip3_p5_browser_e2e.mjs 断言模板扩展；P5-1 视觉门基线
  不受本批影响，vitest 124 全过含 workspaces 数据模型钉值）。

## J. RTIS 对账和数据安全回归结果

- RTIS 面零触碰：本批零 rt.py 调用变更（record_use 既有口径不变）；POC
  正文零复制（契约与调度面不携带 POC 内容）；15155 对账零回归（RTIS 库
  未动）。
- 生产 migrations/platform*.db 零触碰（实拍与冒烟全部走 /tmp 临时库与
  verify2.db 副本；RP_DB_PATH 环境隔离）。
