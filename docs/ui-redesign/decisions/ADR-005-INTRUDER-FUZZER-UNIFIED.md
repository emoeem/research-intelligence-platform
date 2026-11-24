# ADR-005：Intruder 与 Fuzzer 复用同一执行工作台

- 日期：2026-11-24
- 状态：Accepted
- 范围：Web Security 工作区 /websec/fuzzer、FuzzerJobsPage、现有 WSW FuzzSpec API 与 Rust 执行引擎

## 背景

M5 的 UI 计划分别列出 Fuzzer 与 Intruder。代码审计发现，现有 WSW Rust FuzzSpec 已提供三种位置组合模式：single、cartesian、zip。FuzzerJobsPage 已通过 /api/wsw/fuzz/jobs 创建、轮询、查看与取消 Job，后端将 FuzzSpec 交由 rp-wsw 子进程执行。

在当前实现里，single 是逐个位置注入，其余位置保留模板值；cartesian 是多位置笛卡尔积；zip 是等长列表同步迭代。因此它们分别可用 Burp Intruder 的 Sniper、Cluster Bomb、Pitchfork 语义帮助用户理解，但不是新的后端枚举或独立 API。

## 决策

1. 不新增第二个 Intruder 页面、第二套任务表或重复 API。
2. 现有 /websec/fuzzer 工作区页签命名为 Intruder / Fuzzer，同一 Job 页面同时承担两类工作方式。
3. 模式选项显示：
   - Single / Sniper：逐位置测试，其他位置保留原值。
   - Cartesian / Cluster Bomb：所有位置做笛卡尔积组合。
   - Zip / Pitchfork：按相同索引同步配对，列表必须等长。
4. UI 文案只解释既有语义。发送给 API 的枚举值继续为 single、cartesian、zip，不更改序列化格式和 Rust FuzzSpec。
5. 保留现有安全要求：明确 Scope 确认、显式请求/并发/速率/间隔/时长预算，以及 Rust 引擎 fail-closed 预检。

## 后果

- M5 功能对账将同一执行链计为 Fuzzer / Intruder 两种模式，不虚构一个独立的 Intruder 后端。
- UI 不承诺 Burp 完整功能等价。结构化七类位置、Payload Source、Mutator、Processor 与分析规则仍以 Rust FuzzSpec 实际支持范围为准。
- 新增攻击模式或组合算法时，必须先扩展 FuzzSpec、Rust 预检与测试，再扩展 UI；不得只在前端模拟。
