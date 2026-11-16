# P5-2 批次报告：研究任务与验证契约（Schema 版本号 + 契约测试）

> 批次类型：实现批（契约与持久化面）。规格源：Phase 5 补充计划 §7 P5-2 +
> §5.5/§5.7/§5.8；审计依据：P5-0-BATCH-REPORT.md 差距清单 G2/G3/G8 与
> B3 存储判定冻结。

## A. 本批完成内容

1. **SQLite 020 迁移**（G2 收口，研究编排域落运营面）：
   `migrations/020_research_verification.sql`——research_plan（编排容器：
   task_ref 引用 RTIS 镜像 + status 六态 + budget_json）+
   research_plan_item（knowledge_ref / **rtis_poc_id 只存 RTIS POC 数字 ID
   引用** / provider_id+version / target_ref / **dedup_fingerprint** /
   applicability 三态 + reasons / risk_class / 请求与超时预算 /
   **result_status 五态 CHECK 词表** / error_category 七分类 / tool_run_id +
   trace_id 六 ID 关联）+ research_plan_event（append-only 事件流，九词表）。
   schema_version=20 登记 + user_version 双写（DEC-002）。
2. **域词表**（G3 收口，五态与任务状态分离）：`crates/domain/src/verification.rs`
   = ResultStatus 五态 / ApplicabilityStatus 三态 / PlanStatus 六态（终态
   completed/cancelled 不可迁出）/ RiskClass 三级 / ErrorCategory 七分类，
   全部 as_str/parse 词表封闭 + 词值与 020 CHECK 逐字一致性断言；domain 零
   依赖纪律保持（无 serde，手工词值转换）。
3. **契约 Schema 四份 v1**（带版本号）：`schemas/verification/` =
   verification-plan.v1.json / verification-item.v1.json /
   verification-event.v1.json / provider-capability.v1.json（P5-5 Adapter
   消费），draft 2020-12，$id 尾与文件名一致性断言在契约测试内。
4. **verification-engine crate**（RIP-VRVE 契约与持久化面）：
   - contract.rs：四份 Schema include_str 内嵌 + jsonschema 0.56 校验入口 +
     SCHEMA_VERSION=1 常量 + ID 前缀守卫（vp-/vi-/ve-）；
   - store.rs：PlanStore（020 三表 CRUD + 出站契约校验 + 事件留痕）+
     **ResultGuard 五态守卫**（CONFIRMED/FALSE_POSITIVE 必须带 result_reason；
     **error_category 在场时禁止写 NOT_REPRODUCED**——执行失败/超时 ≠ 未复现；
     词表封闭）+ excluded 项必须带原因 + 终态计划冻结 + 指纹形状守卫 +
     find_latest_by_fingerprint（P5-4 缓存决策消费面）；
   - fingerprint.rs：dedup_fingerprint = sha256(target|endpoint|poc|provider|
     params|authctx)（\u{1f} 分隔防粘连；任一段变化即失效——P5-4 缓存失效判据）；
   - bin/rp-verify CLI：plan-create/get/list/status + item-add/get/list +
     result-set + event-list + fingerprint 九子命令，JSON stdout 错误帧
     （rp-doc 惯例），RP_DB_PATH dev 副本 + 迁移幂等应用。
5. **Application API 读面**（G8 首步）：/api/research/plans（列表）+
   /api/research/plans/<id>（plan+items+events 聚合）+
   /api/research/items/<id>（五态详情），经 `_rp_verify_json` 子进程通道
   （RP_VERIFY_BIN，进程边界即数据边界，零直连 DB），404/400 口径对齐既有面。

## B. 架构变化和复用说明

- 存储判定按 P5-0-B3 冻结执行：编排执行态落 SQLite 020（与 workflow_run/
  tool_run 同族），**migrations_pg 链零新表**（selftest_phase1_pg 钉值不动）。
- 复用：database Pool/migrate（迁移执行器自动发现 020）、domain::time、
  jsonschema 0.56 与 sha2 0.10（research-engine/tool-registry 既有 vendor
  条目，**零新增 package**，Cargo.lock 只有依赖边变化）、rp-doc 子进程惯例。
- 不建第二套内核：verification-engine 是补充计划 §5.1 定名的验证引擎契约面，
  执行派发（P5-5/P5-6）将单向依赖 tool-engine（反方向禁止），检索（P5-3）
  走 intel-db 统一检索，不绕行。
- RTIS 红线：rtis_poc_id 只存数字 ID 引用；POC 正文/利用细节零复制；Finding
  事实不经本域落库（result_status 是研究结论状态，不是 RTIS Finding）。

## C. 修改文件清单

```text
migrations/020_research_verification.sql          # 新增（编排域三表）
crates/domain/src/lib.rs                          # verification 模块注册
crates/domain/src/verification.rs                 # 新增（五态/三态/六态词表）
schemas/verification/verification-plan.v1.json    # 新增
schemas/verification/verification-item.v1.json    # 新增
schemas/verification/verification-event.v1.json   # 新增
schemas/verification/provider-capability.v1.json  # 新增
crates/verification-engine/（Cargo.toml + src/lib.rs/contract.rs/store.rs/
                            fingerprint.rs + src/bin/rp-verify.rs + tests/store.rs）
Cargo.toml                                        # workspace 成员 + verification-engine
Cargo.lock                                        # 新 crate 依赖边（零新 package）
apps/server/rp_server.py                          # RP_VERIFY_BIN + 三只读端点
```

## D. 数据库迁移、Schema 和兼容性变化

- 020 迁移幂等（CREATE TABLE IF NOT EXISTS + INSERT OR IGNORE + user_version
  双写），只在开发库副本与测试内存库执行；生产 migrations/platform*.db 零触碰。
- rp_server 新端点只增不改；契约 Schema v1 冻结（升级需同步 SCHEMA_VERSION
  常量与契约测试，upgrade path 明确）。

## E. 测试命令及真实结果

```text
cargo test --offline -p domain            # 65 passed（含 verification 词表 3 项）
cargo test --offline -p verification-engine   # 11 lib + 4 integration 全过：
  # contract: schema $id 尾断言 / 最小 plan/item/event/capability 文档通过 /
  # 非法 status 与坏 ID / 坏指纹与 poc-id=0 拒绝 / shell transport 拒绝
  # store（真实 020 schema 内存库）: 计划生命周期与五态流（事件四类型 + 指纹查找命中）/
  # error_category 在场禁 NOT_REPRODUCED / 无依据 CONFIRMED 拒 / FALSE_POSITIVE 空格依据拒 /
  # excluded 无原因拒 / 终态计划冻结 / 坏指纹拒 / 坏 ID NotFound
cargo clippy --offline -p verification-engine --all-targets -- -D warnings  # 零告警
cargo metadata --offline                   # Cargo.lock 三方一致
# CLI 冒烟（dev 副本 p5-verify-smoke.db，已清场）:
target/debug/rp-verify plan-create ...      # vp-20261116-174544-929-008c draft
target/debug/rp-verify fingerprint ...      # 53b3a788...2202（64 hex）
target/debug/rp-verify item-add ...         # vi-... applicable + 原因
target/debug/rp-verify plan-status ... ready
target/debug/rp-verify result-set --status NOT_REPRODUCED --error-category timeout
  # → {"error":{"code":"result_set","message":"guard rejected: error_category=timeout present:
  #    execution failure/timeout must not be recorded as NOT_REPRODUCED ..."}} exit=1（守卫实拍）
target/debug/rp-verify result-set --status INCONCLUSIVE --error-category timeout  # 接受
target/debug/rp-verify event-list ...       # plan_created/item_planned/plan_status/result_assigned
# 服务端通道冒烟（p5-platform.db 副本 + 7801 测试栈）:
curl /api/research/plans                    # {"items":[...],"available":true,"source":"rp-verify"}
curl /api/research/plans/vp-20261116-174659-143-0020   # plan+items+events 聚合（1 items / 2 events）
python3 -c "ast.parse(rp_server.py)"        # syntax ok
```

## E2. CI 各 Job 状态和运行链接

随批 push 后以 HEAD run 为准；P5-1 批次 CI run 43919530975 四 job 全绿
（本批开批前提确认）。

## F. 性能数据

- rp-verify 子进程冷启动 + 迁移幂等检查 < 1s（CLI 冒烟实测）；查询面全部
  主键/索引路径（plan_id/item_id/dedup_fingerprint 均有索引）。
- Matcher/Planner 性能基线随 P5-3/P5-4 建立（本批无检索执行面）。

## G. 未解决问题与风险

1. 预算语义目前是计划项上的声明字段（request_budget/timeout_budget_s +
   plan budget_json），**后端强制执行随 P5-4**（完成标准：预算约束在后端
   生效，不是只有前端输入框）——本批已把字段位冻结。
2. result_status 五态目前由 rp-verify result-set 手工写入；与 tool-engine
   执行结果的自动归一化随 P5-7（退出码 0 ≠ CONFIRMED 的归一化规则在
   result-normalized v1 契约落地）。
3. 研究域 UI（Scan Center 计划列表/详情页）随 P5-5 批次接入本批 API 面；
   本批 API 是纯读面（写面 CLI 先行，HTTP 写端点随 P5-6 派发面一起开）。

## H. 下一批依赖和明确的非目标

- P5-3 依赖：intel-db SearchFilters/hybrid_search/rerank_search +
  context-broker 组装面 + rt.py poc search 元数据面（已实测）；本批的
  knowledge_ref/rtis_poc_id 引用形状即 Matcher 输出的锚位。
- 非目标：不写 HTTP 写端点（P5-6）、不做 Provider 执行（P5-5）、不动
  migrations_pg、不做 RTIS 写面（result 回填 RTIS 属 P7 知识回流域）。

## I. UI 截图与浏览器验收证据

不适用（本批无 UI 改动；Scan Center 页随 P5-5 落地）。

## J. RTIS 对账和数据安全回归结果

零 RTIS 写操作；rtis_poc_id 只存引用（Schema 层就是 INTEGER 引用字段，
无内容列）。integrations/rtis 零修改。生产库零触碰（测试全部内存库 + dev
副本）。15155 对账口径不受影响（020 是新增表，无既有表结构变更）。
