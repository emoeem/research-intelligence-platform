# RELEASE：Phase 7 P7-6 研究能力契约化（2026-11-13）

> 本批把「研究能力不只是 prompt」落成可验收的代码面：七个研究能力
> （Research Planner / Source Discovery / Evidence Extraction / Verification /
> Synthesis / Citation / Evaluator）各以输入输出契约 Schema（14 份
> request-v1/result-v1，沿用 tool-contract 双份惯例）加确定性 mock 纯函数加
> 16 个契约测试交付，新增 crates/research-engine（workspace 增量成员，零新增
> 第三方依赖）与 rp-research CLI（list/run 契约验收入口，四道门）。用户可感知
> 的变化：一条命令即可按契约运行任一能力并得到 Schema 校验过的结果信封；
> 反 prompt 断言实测零命中（交付物是确定性 mock 不是模型调用体）。零迁移，
> 不动 P7-2 已验闭环，agent_loop 真实接线留 Phase 8 尾洞。

| 项 | 值 |
| --- | --- |
| 状态 | 生效中（编排批次 P7-6 第 2/3 批，SESSION-HANDOFF §十追加项 2，用户硬性验收 a） |
| 日期 | 2026-11-13 |
| 数据库迁移 | 零迁移（契约面纯 Schema 加代码，不占 016 号段；migrations/ 目录零改动，生产库零触及） |
| 自动化测试 | `cargo test --offline --workspace`：510 passed 0 failed（P7-5 基线 494 + research-engine 集成测试 16 用例；新增 crate 无单元测试用例，行为覆盖在集成测试） |
| 静态检查 | `cargo clippy --offline --workspace --all-targets -- -D warnings` 通过（0 error；本批修复 2 处 doc list 缩进告警） |
| 批次自验 | 五条 ask 判据逐条实测（见「三、五条自验逐条实测」） |
| 相关 | docs/phase7/RESEARCH-CAPABILITY-CONTRACTS.md（设计稿）、docs/SESSION-HANDOFF.md §十追加项 2、docs/phase0/PHASE-ROADMAP.md §8.4、schemas/research-capability/ |

## 一、改动清单（全为新文件，不推倒既有能力）

| 文件 | 内容 |
| --- | --- |
| Cargo.toml | workspace members 增 crates/research-engine（1 行） |
| Cargo.lock | 新增 workspace 成员自身条目（research-engine 依赖边表：domain/jsonschema/serde/serde_json/sha2 全为既有 package，零第三方 package） |
| crates/research-engine/Cargo.toml | 新 crate 清单（jsonschema 0.56 与 tool-registry 同版本钉面，sha2 0.10 复用 vendor 既有） |
| crates/research-engine/src/lib.rs | crate 门面与边界声明（不依赖 agent-engine，零迁移，无 IO） |
| crates/research-engine/src/contract.rs | 契约面：CapabilityId 七能力词表、14 份 Schema include_str! 内嵌与 meta-schema 全量门、CapabilityRequest/CapabilityResult 信封、Correlation 六 ID 锚（RunId/TraceId/TaskId 域校验）、ProvenanceAnchor 六构造器、execute_request 统一派发、digest16/digest_hex 确定性摘要 |
| crates/research-engine/src/capabilities/（mod.rs + planner.rs/discovery.rs/extraction.rs/verification.rs/synthesis.rs/citation.rs/evaluator.rs） | 七能力确定性 mock 纯函数（typed 进 typed 出，CapabilityExec 公共执行面） |
| crates/research-engine/src/bin/rp-research.rs | list/run 子命令（四道门：request-v1 校验 → 派发 → result-v1 自检 → 终态退出码） |
| crates/research-engine/tests/capability_contract_p7_6.rs | 16 用例（Schema 全量门/正反例/往返/确定性/逐能力行为/FAILED 信封自洽/六 ID 接线/词表） |
| schemas/research-capability/*.json（14 份） | 七能力 x request-v1/result-v1（$defs 内联自包含，仅 #/$defs 内部引用，fail-closed） |
| docs/phase7/RESEARCH-CAPABILITY-CONTRACTS.md | 设计稿（契约签名表、mock 边界声明、Phase 8 接线预告） |

## 二、设计要点（为什么这样做）

1. **契约先行**：Agent 循环（P7-2）的研究动作还没有可调用面，先钉死输入输出（Schema 加 typed 结构体）再换实现，Phase 8 接线不返工，P7-2 闭环不动。实现体是纯函数（无 IO/时钟/随机），同输入恒同输出由测试 `all_capabilities_are_deterministic` 七能力逐一断言。
2. **六 ID 锚收口**：provenance/citation 锚的 kind 词表 = resource/run/trace/task/artifact/evidence，与 domain::ids 六 ID 一一对应；锚只能从域类型构造，非法 ID 域层即拒（129 字符 run_id 在信封层 ContractError 实测）。每个结果信封至少带 correlation 的 run/trace 锚，对齐 P7-5 trace 关联体系。
3. **Schema 装载路径**：14 份编译期 include_str! 内嵌，运行期零路径依赖；校验复用 vendor 已锁 jsonschema 0.56（与 tool-registry jsonschema_lite 同用法，default-features 关闭外部 $ref，fail-closed）；list 子命令每次先过 14 份 meta-schema 全量门。
4. **错误语义对齐 TOOL-CONTRACT 5.3**：入参载荷非法落 FAILED 信封（category=VALIDATION，retryable=false），信封仍过 result-v1（测试七能力循环断言）；信封头非法（协议版本/关联锚）不产信封直接 ContractError，与 tool-engine 派发语义同构。

## 三、五条自验逐条实测（ask 判据）

1. **workspace 全过**：`cargo test --offline --workspace` 实测 510 passed 0 failed（批次开工前基线复测 494 passed 0 failed，零回归；新增 16 全绿）。
2. **clippy 零告警**：`cargo clippy --offline --workspace --all-targets -- -D warnings` 通过（首跑 2 处 doc list 缩进告警在 evaluator.rs，修复后复跑通过）。
3. **七能力各一次实测**：七次 `cargo run --offline -p research-engine --bin rp-research -- run <capability_id> --input '<json>'` 全部 exit 0，输出均为 SUCCEEDED 信封；输出经 CLI 门 3 逐次过对应 result-v1 Schema（违规即 exit 3，未触发）；14 份 Schema 全部被加载校验覆盖（7 份 request-v1 在 CLI 门 1 与测试正反例、7 份 result-v1 在 CLI 门 3 与测试往返、14 份 meta-schema 在 list 全量门与测试）。
4. **反 prompt 断言**：`grep -rniE 'prompt|llm|gpt|claude' crates/research-engine/src` 零命中（exit 1）。
5. **零新增外部依赖**：`git diff Cargo.lock` 仅新增 research-engine 自身 [[package]] 条目（workspace 成员登记必需），依赖全为既有 package，vendor/ 目录零改动。

## 四、已知取舍与遗留

1. **mock 锚无库行对应**：结果信封的 provenance 锚库内自洽，暂不可回链 evidence/artifact 表行；Phase 8 接真实数据面后与 P7-5 trace 关联面打通（设计稿 §七如实声明）。
2. **Schema 层与 typed 层双层防线人工对齐**：两层规则由测试锚定，单侧变更需双侧同步（设计稿 §七.2）。
3. **宿主 python 无 jsonschema 库**（红线不装系统依赖）：Schema 符合性独立校验走 crate 内 jsonschema 0.56 vendor 路径；python jsonschema 方言差异未比对。
4. **设计稿 §七.4**：Schema 内嵌快照与磁盘文件一致性靠 include_str 同路径保证，无独立对账机制（已知取舍）。
5. **P7-7（Agent Studio 页）未开工**；Phase 7 阶段级验收三站（浏览器实测/独立验收/入库推送）待 P7-6 与 P7-7 完成后统一执行（沿用 P7-5 批次口径）。

## 五、回滚方式

全批为新增文件加 Cargo.toml 一行 members 注册：`git revert` 本批提交即可整体回滚，无迁移无数据面影响；schemas/research-capability/ 与 crates/research-engine/ 可整目录移除，workspace 回到 510 前基线（494 passed）。
