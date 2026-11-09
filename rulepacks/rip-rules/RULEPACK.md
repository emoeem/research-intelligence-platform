# RIP L0 规则包（rip-rules）

- 包 id：rip-rules
- 包版本：1.0.0（rules.toml pack_version 同源）
- 策略版本信号：policy-1（rules.toml policy_version，与 crates/context-broker
  BudgetPolicy 默认策略同源；变化即 L0 与预算相关缓存整体失效，设计稿 §7.2）
- 定位：四级上下文的 L0 层（计划 §8:437-443 层表 L0 行「RIP 全局能力说明与
  核心规则」，加载时机「会话启动」）。规则包不计入候选检索，封顶内全量装配，
  永不驱逐（设计稿 §3 L0 行、§5.3 第 1 条）。

## 内容物

- `rules.toml`：结构化规则清单（[[rules]] 数组，id/title/tier/body），
  本包唯一机器读取面；解析落点 crates/context-broker/src/sources/rulepack.rs。
- `RULEPACK.md`：本说明（人读面；引用空间保留 ref_id `rp-rules:pack` 可展开全文）。

## 规则来源口径

规则正文逐条对应既有冻结文档：计划（docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md）
的按需加载与可解释条款（:121、:444、:794）、宿主机纪律（§18:920）、Artifact
引用条款（:794、:474）；ROADMAP §13 红线（生产库零写入）；设计稿 §1.2/§12
的 RTIS Adapter 纪律；架构盘点:152 的 T0 只读分级。

## 变更纪律

1. 改规则先改 rules.toml（body 与 title 同步），再升 pack_version。
2. 预算口径变化走 policy_version（与 BudgetPolicy 策略文件同步改），不与
   规则正文混在一个变更里。
3. 规则 id 只增不改不复用：引用追踪（context_ref_log）按 rp-rules:<id>
   贯穿，复用 id 会污染历史 Manifest 的可解释性。
4. 新规则必须有正文里可核的出处引用（计划/ROADMAP/设计稿/盘点行号）。
