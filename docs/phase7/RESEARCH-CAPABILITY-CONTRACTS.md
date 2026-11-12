# 研究能力契约化设计（P7-6，RESEARCH-CAPABILITY-CONTRACTS）

| 项 | 值 |
| --- | --- |
| 状态 | 生效中（编排批次 P7-6 第 2/3 批，SESSION-HANDOFF §十追加项 2，用户硬性验收 a） |
| 日期 | 2026-11-13 |
| 适用版本 | git 本批（ crates/research-engine 新增，workspace 510 passed） |
| 相关 | docs/SESSION-HANDOFF.md §十追加项 2、docs/phase0/PHASE-ROADMAP.md §8.4、docs/DOC-STANDARD.md、schemas/research-capability/、crates/research-engine/ |

## 一、本批是什么，为什么这样做

本批把 SESSION-HANDOFF §九 Phase 7 验收第 2 条「研究能力以输入输出契约加测试交付，不只是 prompt」落成代码：七个研究能力（Research Planner / Source Discovery / Evidence Extraction / Verification / Synthesis / Citation / Evaluator）各交付一份输入契约（request-v1）加一份输出契约（result-v1）加一个确定性 mock 纯函数加测试。为什么先契约后实现：Agent 循环（P7-2）目前只有工具调用与证据登记，研究动作（规划、找源、抽证据、核验、综合、引文、评估）还没有可调用的面；如果直接写实现，agent_loop 接线时接口还要返工。先把输入输出钉死成 Schema 加类型，Phase 8 换实现体时契约面不动，P7-2 已验闭环也不动。

「不是 prompt」的落地口径（自验第 4 条实测）：`grep -rniE 'prompt|llm|gpt|claude' crates/research-engine/src` 零命中（exit 1）。七能力实现体全部是纯函数：无 IO、无时钟、无随机、无网络、无模型调用，同输入恒同输出（测试 `all_capabilities_are_deterministic` 七能力逐一断言）。

## 二、契约面设计

### 2.1 双份惯例与信封结构

14 份 Schema 落 `schemas/research-capability/`（七能力 x request-v1/result-v1），沿用 `schemas/tool-contract/` 的双份惯例与描述风格（每字段带语义来源说明）。信封字段对齐 tool-contract：

| 信封字段 | request-v1 | result-v1 | 对齐说明 |
| --- | --- | --- | --- |
| protocol_version | const "1" | const "1" | 同 TOOL-CONTRACT 协议版本口径 |
| capability_id / capability_version | const 能力名 / semver | 同左 | 对齐 tool_id / tool_version 形状（段内字符集同口径） |
| correlation | 六 ID 关联锚（run_id/trace_id 必填，task_id 可缺省） | 转 provenance 锚数组 | 对齐 P7-5 六 ID 体系（domain::ids） |
| input / result | 能力专属载荷 | 能力专属载荷 | 形状由各能力 Schema 冻结 |
| status | 无 | SUCCEEDED/FAILED | TOOL-CONTRACT 四终态子集：mock 纯函数无 TIMED_OUT/CANCELLED 路径 |
| error | 无 | VALIDATION/EXECUTION | TOOL-CONTRACT 5.3 词表子集；mock 只产 VALIDATION |
| provenance | 无 | 六 ID 锚数组（minItems 1） | citation/provenance 锚面，见 2.2 |

条件必填由 status 驱动（Schema allOf 同口径）：SUCCEEDED 必填 result，FAILED 必填 error，summary 与 provenance 恒必填。

### 2.2 六 ID 锚（provenance/citation 落点）

provenance 锚对象为 `{kind, id}`，kind 词表 = resource/run/trace/task/artifact/evidence，与 `crates/domain/src/ids.rs` 六 ID 类型一一对应。Rust 侧锚只能从域类型构造（`ProvenanceAnchor::run(&RunId)` 等六个构造器，crates/research-engine/src/contract.rs），非法 ID 在域层校验即拒绝；请求信封 correlation 的 run_id/trace_id/task_id 同样经 RunId/TraceId/TaskId 校验后入信封（129 字符 run_id 在 envelope 层直接 ContractError，测试 `invalid_correlation_is_rejected_without_envelope`）。每个结果信封至少携带 correlation 派生的 run/trace 锚（task 随缺省），加各能力 payload 锚（如 extraction 的 evidence/artifact 锚、discovery 的 resource 锚）。

### 2.3 Schema 装载与校验路径

Schema 以编译期 `include_str!` 内嵌进 crate（crates/research-engine/src/contract.rs，14 份常量），运行期零路径依赖，CLI 在任意 CWD 可跑。校验复用 vendor 已锁定的 jsonschema 0.56（与 tool-registry 的 jsonschema_lite 同用法同版本，default-features 关闭远程/文件引用，fail-closed）：`check_all_schema_documents()` 对 14 份文档做 draft 2020-12 meta-schema 全量门（rp-research list 每次执行 + 测试断言），`validate_against()` 对实例做契约符合性校验。

## 三、七能力契约签名与 mock 规则（签名表）

| capability_id | 输入要点 | 输出要点 | mock 确定性规则（边界声明的简称，细则见四） |
| --- | --- | --- | --- |
| research.planner | goal（必填），max_steps，workspace_id | plan_id/goal_digest/steps/step_count | 固定六步流水线（发现,抽取,核验,综合,引文,评估）按 max_steps 截断；ID 为 sha256 派生 16 hex |
| research.source-discovery | query（必填），resource_hints，limit | sources/total | hint 直录（过 ResourceId 域校验）优先，固定 mock 目录三族次之，limit 截断 |
| research.evidence-extraction | evidence_refs（必填），artifact_refs，extract_as | extractions/extracted_count | 逐条过 EvidenceId/ArtifactId 校验；content_hash = sha256("mock-extraction"+id)，storage_ref = mock://evidence/&lt;id&gt; |
| research.verification | claims（必填），policy | verdicts/verified_count/unverified_count | 互异合法证据数达阈值即 VERIFIED（strict 2 / advisory 1），非法引用只进 notes；无 CONTRADICTED |
| research.synthesis | topic+items（必填） | synthesis_id/sections/topic_digest | 逐项展开一节：note 原样进 body，无 note 落固定占位文案；每节锚定自己的证据 |
| research.citation | synthesis_id+anchors（必填），style | citations/style/synthesis_ref | 按输入顺序编号（footnote [n] / inline (n)，缺省 inline）；锚过 EvidenceId/ArtifactId 校验 |
| research.evaluator | subject+checks（必填，1 到 3 项互异） | subject/scores/overall_score/verdict | mock 启发式：completeness 看 ID 非空，provenance_coverage 看规范前缀（syn-/cite-/plan-），consistency 看长度 64；overall 均值向下取整，60 分二分 PASS/REVIEW |

## 四、mock 边界声明（什么不是真的）

本批交付的是契约面加确定性 mock，以下边界全部在 crate 文档注释与 Schema description 中同口径声明，Phase 8 替换实现体时契约面不变：

1. planner 的「规划」是固定流水线截断，不是目标分析；rationale 为固定模板文案。
2. discovery 不做真实检索：三族固定目录（rulepack rip-rules / registry manifest 索引 / query 派生知识文档入口）为占位，artifact/plugin 两族词表先行冻结未启用；Phase 8 接 context-broker 装配面。
3. extraction 不读 evidence 表：content_hash 是「mock-extraction + evidence_id」的 sha256，不是登记时刻权威哈希；storage_ref 为 mock:// 占位地址；Phase 8 以 evidence/artifact 表读面替换（013 迁移 agent_evidence_memory 的快照口径届时对齐权威值）。
4. verification 是结构化规则判定，不做内容比对，无 CONTRADICTED 判定；策略阈值（strict 2 / advisory 1）是 mock 语义，真实核验语义 Phase 8 另行评审。
5. synthesis 逐项展开不做归并改写；citation 不校验回链可解析性（synthesis_id 透传）；evaluator 三项评分是 ID 形状启发式，不是质量评估。

## 五、验收入口与门禁（本批实测）

### 5.1 rp-research CLI（契约验收入口，crates/research-engine/src/bin/rp-research.rs）

```bash
cargo run --offline -p research-engine --bin rp-research -- list
cargo run --offline -p research-engine --bin rp-research -- run <capability_id> --input '<json>'
```

run 子命令四道门：装配请求信封（correlation 缺省面按输入确定性派生，同输入恒同 ID 可重放）→ request-v1 Schema 校验（违规 exit 2）→ `execute_request` 统一派发 → result-v1 符合性自检（内部不变量，违规 exit 3）→ 单行 JSON 进 stdout，SUCCEEDED exit 0 / FAILED exit 1。list 子命令先过 14 份 meta-schema 全量门再列词表（schema_check: ok）。

### 5.2 本批门禁实测数（全部实际执行）

| 门禁 | 命令 | 实测 |
| --- | --- | --- |
| 测试 | `cargo test --offline --workspace` | 510 passed 0 failed（P7-5 基线 494 + research-engine 新增 16 用例） |
| 静态检查 | `cargo clippy --offline --workspace --all-targets -- -D warnings` | 通过（0 error） |
| 七能力实测 | 七次 `run` 子命令 | 七次 exit 0，输出均为 SUCCEEDED 信封且各自过了 result-v1 门（CLI 门 3 违规即 exit 3） |
| 反 prompt | `grep -rniE 'prompt|llm|gpt|claude' crates/research-engine/src` | 零命中（exit 1） |
| 零新增依赖 | `git diff Cargo.lock` | 仅新增 workspace 成员自身条目（research-engine 依赖边表：domain/jsonschema/serde/serde_json/sha2 全为既有 package），零第三方 package |

### 5.3 测试覆盖面（crates/research-engine/tests/capability_contract_p7_6.rs，16 用例）

14 份 Schema meta-schema 全量门与 $id/const 对齐、请求 Schema 七能力正例加五类反例（缺 protocol_version/串能力/缺 trace_id/超长 run_id/未知字段）、七能力信封 typed 往返恒等加 result-v1 符合性、typed request 过 Schema 往返、七能力确定性、逐能力 mock 行为（截断/去重/阈值/占位文案/编号/启发式）、非法入参产 FAILED 信封且信封仍过 result-v1（七能力循环）、信封头违规不产信封、六 ID 锚域类型接线与 correlation 锚派生、词表完整性（tool_id 字符集口径）。

## 六、与 agent-engine 的单向弱耦合（Phase 8 尾洞）

本 crate 不依赖 agent-engine（依赖面：domain/serde/serde_json/jsonschema/sha2）。Phase 8 的 agent_loop 真实接线是 agent-engine 侧新增对本 crate 的引用（把七能力 mock 替换为真实实现或把 mock 作为 fallback），方向单向，不推翻 P7-2 已验闭环；本批不接 agent_loop，不写任何数据库（零迁移，契约面纯 Schema 加代码，不占 016 号段）。

## 七、坏消息与已知限制

1. mock 的 provenance 锚含 correlation 派生锚与 payload 锚，但全部锚在库内自洽（无 artifact/evidence 表行对应）；Phase 8 接真实数据面后锚才可回链查询（与 P7-5 trace 关联面打通）。
2. request-v1 对 input 的校验与 typed 层（serde deny_unknown_fields 加域校验）是两层防线：Schema 层先拦形状，typed 层拦域口径（如 ResourceId 前缀、EvidenceId 非空）。两层规则目前人工对齐， Schema 或类型单侧变更需双侧同步（测试 `request_schemas_accept_samples_and_reject_malformed` 与 `invalid_input_yields_schema_valid_failed_envelope` 锚定两侧）。
3. 本机宿主 python 未装 jsonschema 库（红线不在宿主机装依赖），本批 Schema 符合性独立校验走 crate 内 jsonschema 0.56（vendor 离线）路径；Rust jsonschema 与 python jsonschema 的方言差异未比对本批不做。
4. 14 份 Schema 为编译期内嵌快照：磁盘文件为源，改 Schema 必须重编译（cargo 增量会重跑 include_str），不存在运行期漂移；反之磁盘文件与二进制快照的一致性由 include_str 同路径保证，无独立对账（已知取舍）。
