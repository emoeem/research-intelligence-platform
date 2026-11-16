# P5-3 批次报告：Knowledge Matcher（复用 Phase 4 统一检索）

> 批次类型：实现批。规格源：Phase 5 补充计划 §7 P5-3 + §5.4；审计依据：
> P5-0-BATCH-REPORT.md。代码面随提交 ab5907e5 入库（与 P5-2 CI 修复同提交，
> 消息内两段分记）。

## A. 本批完成内容

1. **双渠道匹配器**（crates/verification-engine/src/matcher.rs）：
   - **pg-intel 渠道**：`rp-kc search --query X --mode hybrid --limit N`
     子进程——Phase 4 统一检索入口（pg_search BM25 + pgvector + RRF 融合 +
     query-vector/检索双缓存），**禁绕行纪律的落地面**；
   - **rtis-poc 渠道**：`python3 integrations/rtis/rt.py poc search --q X
     --json` 元数据面（RTIS 既有接口；integrations/rtis 零修改）；
   - `KnowledgeChannel` trait（生产两实现 + 测试 mock 注入）。
2. **结构化适用性判定**（§5.4 纪律：向量分只是排序信息，不决定执行）：
   - 组件名规范化匹配（profile 组件 vs 知识 component 字段，双向 contains）；
   - 版本一致性（双侧在场才判；缺任一侧记 **version condition unverified**
     不静默通过）；
   - 知识带组件锚但画像无该组件 → 排除（不凭标题相似度放行）；
   - 无组件锚的通用知识：无画像输入时按 generic knowledge 放行（原因明示），
     有画像时排除；
   - RTIS verified 位作为命中原因标注；
   - 每条命中/排除逐条可解释（Knowledge Context 页解释面数据源）。
3. **渠道失败降级**：单渠道错误 → 空渠道 + ChannelStat.error 如实标注
   （unified-search sqlite_error 同款 fail-open），不阻断整体。
4. **has_applicable 信号**：全部候选被排除或渠道零返回 → false，调用方
   （P5-8 自适应研究）据此进入补充研究状态而非报告安全。
5. **rp-verify match 子命令**：双渠道实跑入口。
6. **命中率坑位继承**：RTIS poc search 中文零命中（组件名英文优先，kingdee
   有命中金蝶没有），usage 与本报告双处登记，不做中文转写（画像输入负责）。

## B. 架构变化和复用说明

- 检索复用面（零重建）：rp-kc hybrid（P4-1 三通道融合 + P4-3 缓存）原样
  消费；RTIS 元数据面经 rt.py 既有 CLI（铁律接口面）；本模块不直连
  knowledge.db、不直连 PG。
- matcher 是纯函数面（channels 注入），测试零网络；生产渠道经子进程边界。
- P5-4 planner（同 crate）直接消费 MatchResult（本批输出形状即 Planner 输入
  契约）。

## C. 修改文件清单

```text
crates/verification-engine/src/matcher.rs            # 新增（604 行，含 6 mock 单测）
crates/verification-engine/src/lib.rs                # matcher 模块注册 + re-exports
crates/verification-engine/src/bin/rp-verify.rs      # match 子命令
crates/agent-engine/tests/migration_idempotent.rs 等 8 文件   # P5-2 CI 修复（钉值 19→20，见提交 ab5907e5）
```

## D. 数据库迁移、Schema 和兼容性变化

零迁移（本批不改存储面；P5-2 的 020 链尾演进引发的八处测试钉值更新见
ab5907e5 提交消息，属于 P5-2 收口修正）。

## E. 测试命令及真实结果

```text
cargo test --offline -p verification-engine   # 22 lib（含 matcher 6 mock 单测）+ 4 integration 全过
cargo test --offline --workspace              # 135 套件 0 FAILED（钉值修复后全量）
cargo clippy --offline --workspace --all-targets -- -D warnings   # 零告警
# 实拍（K3Cloud 画像，双渠道真跑）:
target/debug/rp-verify match --target https://t.example:443 \
  --components "kingdee k3cloud" --profile '{"kingdee k3cloud":"8.2"}'
# → channels: pg-intel 1437ms（1 条候选）/ rtis-poc 113ms（1 条），总 1550ms
#   HIT  rtis-poc 900008 "K3Cloud ServiceFacade 错误码判读"
#        | component match: [kingdee k3cloud] vs [kingdee k3cloud servicefacade]
#        | version condition unverified (knowledge version missing) | RTIS verified
#   EXC  pg-intel "SQL 注入原理" | no component anchor (cannot verify applicability)
#   has_applicable: true
```

## E2. CI 各 Job 状态和运行链接

ab5907e5 run 43923909735（随批确认）；P5-1 CI 43919530975 绿、P5-2 CI
43922203815 红（钉值）已由本批修复。

## F. 性能数据（Matcher 检索延迟基线，§11.4）

| 渠道 | 延迟（实测） | 备注 |
| --- | --- | --- |
| pg-intel（rp-kc hybrid） | 1437ms（冷，容器 embed 通道参与） | P4-3 缓存命中后 237ms 量级（P4-3 实录） |
| rtis-poc（rt.py 元数据面） | 113ms | FTS5 本地面 |
| 全链（两渠道串行） | 1550ms | 单进程顺序派发；并行化随 P5-6 调度面评估 |

Token 使用：本批为检索面，无 Context Broker 组装（预算面随 P5-4/P5-6 计入）。

## G. 未解决问题与风险

1. 组件匹配是 contains 双向口径（kingdee k3cloud vs kingdee k3cloud
   servicefacade 命中）；跨组件误配（nginx vs nginx-ingress）由版本与
   tags 过滤缓解，精确匹配策略随 P5-4 预览面校准（正/负/边界样本测试已
   覆盖版本不匹配面）。
2. pg-intel 渠道 1437ms 冷启动偏高（embed 派发面）；P4-3 双缓存命中后
   显著回落，P5-6 若需并行双渠道再评估。
3. 统一检索缓存命中状态在子进程边界内（rp-kc 内部），本模块记录渠道延迟
   作为间接观察；显式缓存命中计数随 P5-8 统计面。

## H. 下一批依赖和明确的非目标

- P5-4 依赖：本批 MatchResult 形状（hits/excluded/channels）+ P5-2 指纹与
  find_latest_by_fingerprint。
- 非目标：不做计划落库（P5-4）、不做 Provider 执行（P5-5）、不改统一检索
  内核与 RTIS。

## I. UI 截图与浏览器验收证据

不适用（Knowledge Context 页解释面升级随 P5-4 预览面一起接本批输出）。

## J. RTIS 对账和数据安全回归结果

零 RTIS 写操作（只读 poc search 元数据面）；integrations/rtis 零修改；
knowledge.db 只读。生产库零触碰。
