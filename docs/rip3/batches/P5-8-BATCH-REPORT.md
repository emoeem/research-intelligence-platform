# P5-8 批次报告：Coverage Matrix 与 Adaptive Research（收口批）

> 批次：P5-8（计划 §7）。半成品随 4b949614 WIP 入库（022 迁移 / coverage.rs /
> 指纹 v2 / rp-verify 四子命令，代码面当时已完成），本批收尾：hypothesis 实拍
> 链重跑 + hypothesis 集成测试 + 批次报告 + commit 收口。接手现场专档
> docs/HANDOVER-P5-WIP.md 第三节。

## A. 本批完成内容

1. **A0 前置核实**（复现交接质量核）：`cargo test --offline -p
   verification-engine` 42 lib + 2 provider + 6 store 全过；`cargo clippy
   --offline -p verification-engine --all-targets -- -D warnings` 零告警。
2. **hypothesis 实拍链重跑**（上轮 not_found 根因 = RP_DB_PATH 未随命令携带，
   环境未挂非代码缺陷）：六步全链 PASS，详见 E 节。
3. **hypothesis 集成测试 2 条**（tests/store.rs 惯例）：生命周期正例与守卫
   负例矩阵（必填四项空值 / 轮次预算 / 维度词表 / validated 终局不迁出）。
4. **定向复测**：verification-engine 全量 52 过（42 lib + 2 provider + 8
   store，新增 2）；clippy 全 targets 零告警。
5. **批次报告与收口 commit**（本文件）。

## B. 架构变化和复用说明

- 零架构变化、零新表、零迁移链推进（022 链头已随 4b949614 入库；本批只在
  测试面与实拍面工作）。
- 复用：`store()` 内存库 fixture 惯例（真实迁移 schema）、
  `PlanStoreError::GuardRejected` 断言惯例、rp-verify CLI 出站 JSON 形状
  （add_hypothesis 出站含十二字段全量回显）。
- validated 终局守卫的报错文案内嵌研究事实路径指引（finding 路径经 plan
  item 五态 + finding-link），与 P5-7 finding-link 面语义一致。

## C. 修改文件清单

| 文件 | 改动 |
| --- | --- |
| crates/verification-engine/tests/store.rs | use 行补 NewHypothesis + 追加 hypothesis_lifecycle_on_real_schema 与 hypothesis_guards_evidence_round_and_terminal 两测试 |
| docs/rip3/batches/P5-8-BATCH-REPORT.md | 本文件 |

（4b949614 WIP 批已入库面：migrations/022_coverage_matrix.sql、九钉值文件、
domain/verification.rs 七维词表、store.rs dimension + hypothesis CRUD、
coverage.rs、fingerprint.rs v2、planner.rs max_rounds、rp-verify.rs 四子
命令、provider/scheduler PlanItemRecord dimension 字段，不在本批 commit。）

## D. 数据库迁移、Schema 和兼容性变化

零变化。SQLite 链头 022（022_coverage_matrix.sql 随 WIP 批入库，41 套件钉
值全绿实证）；PG 链头 V0006 未动；schema_version 22 双写已在 022 内。

## E. 测试命令及真实结果

### E1 集成测试（A3/A4）

```
$ cargo test --offline -p verification-engine --test store
running 8 tests
test result: ok. 8 passed; 0 failed; ... finished in 0.25s

$ cargo test --offline -p verification-engine
test result: ok. 42 passed; ... (lib)
test result: ok. 2 passed;  ... (tests/provider.rs)
test result: ok. 8 passed;  ... (tests/store.rs)

$ cargo clippy --offline -p verification-engine --all-targets -- -D warnings
    Finished `dev` profile ... in 0.18s   （零告警）
```

### E2 hypothesis 实拍链（RP_DB_PATH 每条命令携带）

场景重建（原 /tmp/p5-8-e2e/verify.db 已被 /tmp 清理丢失，按等效 budget
max_rounds=2 重建；新计划 vp-20261117-152808-425-00a6 替代原
vp-20261116-200640-186-006a）：

```
$ rp-verify plan-create --task rt-p58-e2e --title 'P5-8 hypothesis 实拍链场景' \
    --target 127.0.0.1 --budget '{"max_requests":15,"max_rounds":2}'
→ plan_id vp-20261117-152808-425-00a6（draft）

$ rp-verify item-add --plan vp-… --provider nuclei --target 127.0.0.1:9443 \
    --fingerprint e3b0c442…b855 --dimension assets --requests 10
→ item_id vi-20261117-152817-284-016d（risk_class low_impact）

$ rp-verify coverage-show --plan vp-…
→ cells 七维 item_count assets=1 其余 0；blind_spots 七维全列
  （assets 有项但五态计数全零 = unjudged 盲区，符合「未测试与失败分开显示」设计）
```

六步实拍链：

1. **正例 add** PASS：hypothesis_id `hi-20261117-152827-395-0021`，status
   proposed，round 1，必填四项 + 五个可选字段全量回显（出站 JSON 十二字段）。
2. **负例一（空 source）** PASS 拒绝：`guard rejected: hypothesis
   source_evidence must not be empty (no evidence, no hypothesis)`，exit 1。
3. **负例二（round 3 超 max_rounds=2）** PASS 拒绝：`guard rejected:
   hypothesis round 3 outside budget max_rounds 2 (stop condition: round
   budget exhausted)`，exit 1。
4. **hypothesis-list** PASS：仅正例在库（两条负例守卫拒绝零落库）。
5. **状态推进** PASS：proposed → in_plan → validated（两步均 200 出站回显）。
6. **终局负例（validated 后改 proposed）** PASS 拒绝：`guard rejected:
   hypothesis hi-… is validated (terminal; finding path goes through plan
   item five-state + finding-link)`，exit 1。

### E3 CI（E2 节格式）

push 后回填：run id 与四 job 结论（见文末补记）。

## F. 性能数据

不适用（功能收口批，无性能面改动）。

## G. 未解决问题与风险

1. **原实拍库丢失已重建**：/tmp 清理把 /tmp/p5-8-e2e/verify.db 与
   /tmp/p5-7-e2e/ 连带清掉；P5-8 场景已等效重建（budget max_rounds=2 保持
   轮次守卫负例语义）。P5-9 批三本地靶场实拍需重建 nuclei 干净模板
   /tmp/p5-7-e2e/template.yaml（never-match 面），随该批登记。
2. risk_class 推断仍全部 low_impact（P5-4 G3 沿用，rtis-poc 元数据面无风险
   字段；Provider Capability 声明面推断待后续批）。
3. egress_allowlist 派发面真实目标实拍（P5-5 G1）与 finding-link 正例 RTIS
   写面实拍（P5-7 G1）随 P5-9 批三本地靶场。
4. ZAP Adapter 未接入（P5-5 G3 沿用，Phase 5 收口后扩展项）。

## H. 下一批依赖和明确的非目标

- P5-9a 依赖：018 控制面与既有 DSL、workflows/ 目录 JSON 定义惯例
  （parse-pipeline-v1.json / review-card-v1.json）、XYFlow 画布与
  workflow-engine 版本面（均已在库）。
- 非目标：不写十三步模板（P5-9a）、不接 ZAP、不动 runtime-gateway 内核、
  不做迁移链推进。

## I. UI 截图与浏览器验收证据

不适用（CLI 测试面批）。coverage-show 七维矩阵文本出站已留档 E2；Coverage
矩阵 UI 页随 P5-9b。

## J. RTIS 对账和数据安全回归结果

- 零 RTIS 写操作（本批只测 3.0 侧 store 守卫与 CLI 出站形状）。
- 生产 migrations/platform*.db 零触碰；实拍库为 /tmp 一次性内存等效重建。
- commit 前 `git status --porcelain -- integrations/rtis` 核对零 diff。

## K. 本批教训实录（HANDOVER 第三节指定三条）

1. **迁移链尾钉值多形状定位法**：演进链尾先按五个形状全量定位九个钉值文件
   再改：`vec![1, 2`、`vec![13, 14`、`(2X, "_`（链尾名二元组）、`uv1?`（变
   量名变体）、`001..`/`013..`/`010..`（区间与枚数）。P5-7 CI 红
   43942245579 实录三处漏改均落在第二至四形状。
2. **批量替换 sub() 整文件跳过陷阱**：整文件 sub() 遇一处 MISS 会跳过整个
   文件写入，已匹配的对也不落盘；每对独立判定独立写。
3. **ALTER 必须单行书写**：migrate.rs 执行器行守卫把多行 ALTER 切断，重放
   场景炸 CHECK 语法错误（022 research_plan_item.dimension 列实录）。

另补一条接手面教训：**RP_DB_PATH 必须随每条 CLI 命令**（Bash 工具环境不跨
调用持久；漏带即落内存空库，症状 not_found 但代码无缺陷；上轮 hypothesis
实拍链中断即此因）。
