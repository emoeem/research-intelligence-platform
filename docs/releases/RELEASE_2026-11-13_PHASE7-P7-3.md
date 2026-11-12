# RELEASE 2026-11-13 Phase 7 P7-3：三类 Memory 与 Context Memory 雏形

> 本批后 Agent 有了可追溯的记忆面：Task Memory（run 内工作记忆，压缩留摘要驱逐留墓碑）、Evidence Memory（证据/产物引用台账，压缩/驱逐不动指针且落盘产物逐字节复算）、Research Memory（Context Memory 雏形，每条成果回链 manifest item/artifact/evidence id 且写时强制可解析，独立表独立生命周期不复用会话存储）。契约（计划 §9:478 原文「原始证据不能因上下文压缩而丢失」）以三层闸门落地：domain 生命周期跃迁表拒绝 Evidence/Research 记忆驱逐、013 CHECK 词值兜底、台账指针 FK 反向锚定权威证据行不可删。

## 元信息

| 项         | 值 |
| ---        | --- |
| 状态       | 生效中 |
| 日期       | 2026-11-13 |
| 适用版本   | 工作区未提交（基线 git 05ed5ee 之后，提交由编排层收口） |
| 相关       | docs/phase0/PHASE-ROADMAP.md §8.3 P7-4（内容行）、§8.5 风险 4、docs/RIP-2.0-PLATFORM-AND-TOOLING-PLAN.md §9:474/478、docs/SESSION-HANDOFF.md §十、docs/phase7/AGENT-MEMORY-REVIEW.md（013 评审记录） |
| 数据库迁移 | 013_agent_memory.sql（三表追加：agent_task_memory/agent_evidence_memory/agent_research_memory）；幂等验收：迁移目录副本对临时库连续 migrate 两次第二次 no-op 且 sqlite_master 快照零差异（migration_idempotent.rs），开发库副本 dev-p7.db 拷贝件 12 起点只落 013、二次 no-op、快照零差异、原件保持 12 未写（migration_dev_copy_p7_3.rs）；生产 platform.db 与 platform-dryrun.db 未触及 |
| 自动化测试 | `cargo test --offline -p agent-engine`：6 个测试二进制 25 通过 0 失败；全仓 `cargo test --offline --workspace`：463 通过 0 失败（上批 446，增量 17） |
| 静态检查   | `cargo clippy --offline --workspace --all-targets -- -D warnings` 通过（0 error）；rustfmt 已执行（范围：agent-engine 与 domain 改动面，fmt 顺带触碰的 domain/knowledge.rs 按 P7-2 纪律 git checkout 还原） |

批次序号说明：编排层批次 P7-3（第 3/7 批）内容对应 ROADMAP §8.3 表内 P7-4 行（三类 Memory）；上一批 P7-2 交接已提前覆盖 ROADMAP P7-3 行的 Broker 深度集成（L0 到 L3 装配、expand 记账、ambiguous 确认回路），本批按编排 ask 执行，迁移编号按 §1.4 实际合入顺序取 013。

## 一、交付内容（按批次判据逐条）

### 1. 013 迁移三表（反重复建模约束入表）

- `agent_task_memory`：run 内轮次工作记忆，lifecycle 词值 ACTIVE/COMPRESSED/EVICTED；压缩置空 content_json 留确定性摘要，驱逐全置空留墓碑行（不物理删行，审计可查）。
- `agent_evidence_memory`：证据/产物引用台账，指针（evidence_id/artifact_id FK）+ 登记时刻快照（content_hash/storage_ref，先例 008 context_manifest_item.content_hash）；UNIQUE(run_id, evidence_id) 防重；lifecycle 词值仅 ACTIVE/COMPRESSED（无 EVICTED/ARCHIVED，CHECK 兜底）。不设 evidence 实体字段（逐字段核对见 docs/phase7/AGENT-MEMORY-REVIEW.md）。
- `agent_research_memory`：Context Memory 雏形，title/summary/content_json/refs_json（回链三型数组）+ lifecycle ACTIVE/COMPRESSED/ARCHIVED（无 EVICTED）；独立表独立生命周期，不复用 context_manifest 等会话存储。

### 2. domain::agent Memory 域模型（纯类型，零新依赖）

- MemoryKind 三类词表 + MemoryLifecycle 四词值：跃迁表按类别区分（Task 3 对、Evidence 仅 ACTIVE 到 COMPRESSED 一对、Research 3 对），`can_transition_kind` 纯函数 + 三类条目 `transition_to`（非法对返回 DomainError::InvalidMemoryTransition）。
- 三类条目 try_new 载荷不变式：Task（ACTIVE 详情在场、COMPRESSED 空详情非空摘要、EVICTED 全空）、Evidence（hash 须 64 位小写 hex、快照字段非空、EVICTED/ARCHIVED 构造即拒绝）、Research（标题/摘要非空、回链至少一条、EVICTED 拒绝）。
- MemoryObjectRef 回链引用：manifest item（mem://manifest/{mid}/{iid}）、artifact、evidence 三型，to_uri/parse 往返；新增 DomainError::InvalidAgentMemory、UnknownMemoryLifecycle、InvalidMemoryTransition。

### 3. agent-engine memory/ 三模块与读写 API（crates/agent-engine/src/memory/）

- `task.rs` TaskMemoryStore：append/get/list_by_run/compress_run/evict_run（压缩幂等、驱逐幂等、墓碑保留，单测覆盖）。
- `evidence.rs` EvidenceMemoryStore：record（快照与权威值三方一致才落行，漂移/缺席/重复全拒绝零残留）、compress_run（只推进词值，指针零改动）、resolve（回链行存在性 + 快照=evidence.content_hash=artifact.sha256 三方对账 + 落盘产物逐字节复算，引用形态 ripartifact:// 如实标注不伪装）。无驱逐 API。
- `research.rs` ResearchMemoryStore：record（写时逐条回链解析，任一不可解析整行拒绝）、compress/archive（只置空详情，title/summary/refs 原样，CAS 词值推进）、resolve_refs（全量回链重放，漂移如实报告）。无驱逐 API。
- `mod.rs` MemoryStore 门面 + verify_run_memory_integrity（压缩/驱逐契约核对入口：逐条台账 resolve + 逐条研究记忆 resolve_refs，失败全量进报告不中断）。

### 4. AgentLoop 可选接线（默认关闭，P7-2 行为零变化）

- `AgentLoop::with_memory`：挂 MemoryStore 后每步落 Task Memory 轮次载荷 + Evidence Memory 台账指针，run 收尾落 Research Memory（回链 = L3 展开条目 + 各步 artifact/evidence，回链不足如实拒绝不伪造）并经既有 append_memory_refs 在 agent_run.memory_refs 追加 `mem://memory/{id}`；全部写失败降级 warning 不炸 run。
- RegisteredOutcome 增 content_hash 字段（additive，Evidence Memory 快照来源）；RunReport 增 research_memory_id 字段（additive，未挂 Memory 为 null）。

## 二、自验命令与实测输出

```bash
# 1. 本批门禁（ask 指定）
cargo test --offline -p agent-engine
# 6 个测试二进制 25 通过 0 失败（lib 19 / closed_loop 2 / memory_contract 2 /
#   migration_dev_copy 1 / migration_idempotent 1 / rp-agent bin 0）

# 2. 全仓回归
cargo test --offline --workspace   # 463 通过 0 失败
cargo clippy --offline --workspace --all-targets -- -D warnings   # 0 error

# 3. 013 幂等（新库全量 + 开发库副本两条路径）
cargo test --offline -p agent-engine --test migration_idempotent --test migration_dev_copy_p7_3
#   applied_first = [1..13]，第二次 no-op，sqlite_master 快照零差异；
#   dev-p7.db 拷贝件 user_version 12 只落 013，二次 no-op，原件保持 12（只读复核）

# 4. 压缩/驱逐契约（ROADMAP P7-4 判据原文）
cargo test --offline -p agent-engine --test memory_contract_p7_3
#   compression_and_eviction_keep_all_evidence_and_artifact_refs_resolvable：
#   闭环 run（SUCCEEDED，落盘产物）→ 三类记忆落账核对 → Task 压缩+驱逐、
#   Evidence 台账压缩、Research 压缩 → verify_run_memory_integrity is_ok=true
#   （evidence_ledger_checked>=1，refs_checked>=2），落盘产物复算 Some(true)

# 5. CLI 回归（默认路径不挂 Memory，行为同 P7-2）
cp migrations/dev-p7.db /tmp/dev-p73-smoke.db && cargo run --offline -q -p agent-engine \
  --bin rp-agent -- run agent.research.minimal \
  --goal "检索 nmap 端口扫描相关知识并调用登记工具" --db /tmp/dev-p73-smoke.db
#   SUCCEEDED，research_memory_id: null（CLI 未挂 Memory 的如实形态）
```

## 三、坏消息与已知限制（如实记录）

- CLI 未挂 Memory：rp-agent 默认路径不写三类记忆（报告 research_memory_id 恒为 null）；CLI 侧 --with-memory 开关与记忆查看子命令属后续批次，本批读写 API 以 Rust 面 + 测试为验收面。
- Research Memory 回链的 DB 层无外键（SQLite 不支持 JSON 列外键）：断链靠写时全量解析 + resolve_refs 重放兜底，单测验证目标行被删后如实报 resolved=false。
- 压缩摘要是确定性计数摘要（compressed N chars of round R），语义摘要属模型在环面（ROADMAP §8.5 风险 3，P7-5）。
- 四个既有迁移链尾断言按编号冻结规则顺延至 013（crates/database 三个测试 + plugin-runtime 链尾距离断言 3 改 4），均为追加式断言更新，无既有能力回退。
- 生产库与生产行为未触及：platform*.db 未写（迁移只在内存库/临时副本/开发库副本执行）；RTIS 生产行为无涉。
