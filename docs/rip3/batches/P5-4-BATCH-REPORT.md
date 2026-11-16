# P5-4 批次报告：Verification Planner（去重指纹/缓存决策/预算后端生效/执行前预览）

> 批次类型：实现批。规格源：Phase 5 补充计划 §7 P5-4 + §5.5/§5.8/§5.9；
> 审计依据：P5-0-BATCH-REPORT.md。

## A. 本批完成内容

1. **Planner**（crates/verification-engine/src/planner.rs）：
   - `draft_from_match`：MatchResult → PlanDraft——excluded 候选全部转
     skipped（带排除原因，不静默丢弃）；命中候选按 **风险序（passive →
     low_impact → active_controlled）+ 同级渠道分降序** 排列；
   - **预算后端生效**：单项预算 = min(单项标准 10, 计划余量)，Σ ≤ cap；
     超预算候选 → skipped（reason=budget_exhausted: 剩余量说明）——封顶在
     planning 生成侧生效（执行侧复核随 P5-6 派发面），不是前端输入框；
   - **缓存决策**：每条候选生成六段指纹（P5-2 fingerprint），查
     find_latest_by_fingerprint：同指纹且五态终局 → **CacheHit{prior_item_id,
     prior_result}**（不占请求预算，原因面标注 cache_hit + 历史项引用）；
     否则 Fresh——缓存只避免重复执行，不把历史结果当当前实时证据（§5.8）；
   - `persist_draft`：plan-create + 逐条 item-add 落库（缓存命中项也落库，
     cache_decision 与预算来源写进 applicability_reasons 供 UI 与调度面）。
2. **rp-verify plan-build 子命令**：match → planner → 落库一键面，输出
   plan_id + 完整预览（planned/skipped/预算 Σ/cap）+ 渠道统计。
3. **缓存失效验证**（测试 + 实拍双覆盖）：POC 引用版本变化、检测参数变化
   → 指纹变化 → Fresh（缓存正确失效）；同条件重放 + 历史项终局 → CacheHit。

## B. 架构变化和复用说明

- Planner 是纯函数面（store 注入做指纹查询），与 matcher/store 同 crate
  分层：matcher（检索）→ planner（裁剪/排序/缓存/预算）→ store（持久化 +
  五态守卫），执行派发（P5-5/P5-6）再叠加。
- 复用：P5-2 store 全部守卫（excluded 无原因拒、指纹形状守卫、五态守卫）、
  P5-3 matcher 输出形状、P5-2 fingerprint 六段口径。
- 低影响默认：planner 不自动升级风险（Provider 能力不得升级风险，§5.5
  规则 5）；active_controlled 只能由调用方显式传入。

## C. 修改文件清单

```text
crates/verification-engine/src/planner.rs            # 新增（Planner + CacheDecision + 6 单测）
crates/verification-engine/src/store.rs              # NewItem 加 Serialize（预览出站）
crates/verification-engine/src/lib.rs                # planner 注册 + re-exports
crates/verification-engine/src/bin/rp-verify.rs      # plan-build 子命令
```

## D. 数据库迁移、Schema 和兼容性变化

零迁移（020 面字段位已冻结：request_budget/timeout_budget_s/dedup_fingerprint
在 P5-2 落位，本批只是语义接通）。

## E. 测试命令及真实结果

```text
cargo test --offline -p verification-engine   # 22 lib（含 planner 6 单测）+ 4 integration 全过
cargo clippy --offline -p verification-engine --all-targets -- -D warnings  # 零告警
# 单测覆盖面（预算/缓存/排序/跳过四条纪律线）:
#   budget_cap_enforced_in_planning_backend    # cap 15 → 10+5 Σ=15，第三项 budget_exhausted skipped
#   cache_hit_on_same_fingerprint_and_no_request_consumed   # 同指纹终局 → CacheHit，Σ请求=0
#   poc_version_change_invalidates_cache       # poc@v2 / params 变化 → Fresh（失效）
#   excluded_candidates_become_skipped_with_reasons
#   ordering_low_impact_first_by_score_desc    # 同级分降序：b(2.0) 先于 a(1.0)
# CLI 实拍（dev 副本 p5-planner-smoke.db，用后即删）:
#   plan-build（cap 15）→ plan_id vp-...-004a，planned=1（rtis-poc 900008 fresh 10/15）
#     skipped=1（pg-intel SQL 注入原理 no component anchor）
#   同条件重放（首项未执行）→ Fresh（正确：缓存只认终局五态）
#   首项 result-set NOT_REPRODUCED 后重放 → CacheHit{prior_item_id: vi-...-0088,
#     prior_result: NOT_REPRODUCED} + "cache_hit (no request consumed)"
```

## E2. CI 各 Job 状态和运行链接

随批 push 后以 HEAD run 为准。

## F. 性能数据

- Planner 纯函数处理 3 候选 < 1ms（单测 0.15s 含 6 测试整体）；指纹 sha256
  单次 < 1μs 量级；find_latest_by_fingerprint 走 ix_research_plan_item_fp
  索引。
- Matcher 延迟基线见 P5-3 报告 F 节（pg-intel 1437ms 冷 / rtis-poc 113ms）。

## G. 未解决问题与风险

1. **执行侧预算复核随 P5-6**：planning 侧 Σ ≤ cap 已生效，但运行期请求
   计数（Provider 实际消耗）守卫在派发面落地（本批字段位已备）。
2. 缓存决策的「条件未变化」目前以指纹六段全等为准（target/endpoint/poc/
   provider/params/authctx）；「目标技术画像版本」与「知识审核状态变化」
   两段未入指纹（§5.8 建议键的其余两段）——画像版本随 P5-8 覆盖矩阵落地
   时补段（指纹口径变更会显式升 SCHEMA_VERSION 并同步契约测试）。
3. risk_class 推断当前全部 low_impact（rtis-poc 元数据面无风险字段）；
   Provider Capability 契约（P5-5）接入后按声明面推断。

## H. 下一批依赖和明确的非目标

- P5-5 依赖：provider-capability.v1 契约（P5-2 已冻结）+ 本批 PlanDraft
  执行项形状 + tool-engine 提交链（审计已确认状态机与 cancel 面）。
- 非目标：不派发执行（P5-6）、不接 Nuclei/ZAP 镜像（P5-5）、不做五态
  归一化（P5-7）、不改预算字段位。

## I. UI 截图与浏览器验收证据

不适用（Scan Center 计划预览页随 P5-5/P5-6 接 /api/research 面时一起落）。

## J. RTIS 对账和数据安全回归结果

零 RTIS 写操作；缓存决策只引用 3.0 侧历史项（不读 RTIS 生产库）。生产库
零触碰（smoke 用副本用后即删）。
