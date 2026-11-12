//! 三类 Memory 与 Context Memory 雏形（编排批次 P7-3 第 3/7 批；ROADMAP
//! §8.3 P7-4 内容行 + SESSION-HANDOFF §十 Context Memory 雏形追加项）。
//!
//! 三模块分工（013 迁移三表，域模型在 domain::agent，本目录只做数据面与
//! 读写 API，不重复建模）：
//! - [`task`]：Task Memory（agent_task_memory）：run 内轮次工作记忆，压缩
//!   留确定性摘要、驱逐留墓碑行（EVICTED，审计可查）。
//! - [`evidence`]：Evidence Memory（agent_evidence_memory）：证据/产物引用
//!   台账（指针 + 登记时刻快照），防丢失机制面：压缩/驱逐不动指针，
//!   [`EvidenceMemoryStore::resolve`] 以 evidence/artifact 既有表为权威做
//!   三方哈希对账与落盘产物逐字节复算；本台账无驱逐 API（契约面）。
//! - [`research`]：Research Memory（agent_research_memory）：Context Memory
//!   雏形，可追溯研究成果：每条记忆回链 manifest item/artifact/evidence id
//!   （写时校验可解析），压缩/归档只置空详情、回链原样保留；独立表独立
//!   生命周期，不复用任何会话/上下文存储。
//!
//! 压缩/驱逐契约（计划 §9:478 原文「原始证据不能因上下文压缩而丢失」）：
//! 压缩/驱逐执行后调用 [`verify_run_memory_integrity`]，全部 Evidence 与
//! Artifact 引用仍可解析（回链行存在）且 content_hash 校验通过（快照 =
//! evidence.content_hash = artifact.sha256，落盘产物逐字节复算）。契约用例
//! 见 `tests/memory_contract_p7_3.rs`。

pub mod evidence;
pub mod research;
pub mod task;

pub use evidence::{EvidenceMemoryStore, ResolvedEvidence, StoredEvidenceMemory};
pub use research::{ResearchMemoryStore, ResolvedRef, StoredResearchMemory};
pub use task::{StoredTaskMemory, TaskMemoryStore};

use crate::error::AgentError;
use database::Pool;

/// 三类 Memory 统一入口（三个子存储共享一个 Pool）。
#[derive(Clone)]
pub struct MemoryStore {
    task: TaskMemoryStore,
    evidence: EvidenceMemoryStore,
    research: ResearchMemoryStore,
}

impl MemoryStore {
    /// 在连接池之上打开（013 三表由迁移执行器保证在场）。
    pub fn open(pool: Pool) -> Self {
        Self {
            task: TaskMemoryStore::new(pool.clone()),
            evidence: EvidenceMemoryStore::new(pool.clone()),
            research: ResearchMemoryStore::new(pool),
        }
    }

    /// Task Memory 子存储。
    pub fn task(&self) -> &TaskMemoryStore {
        &self.task
    }

    /// Evidence Memory 子存储（引用台账，无驱逐 API）。
    pub fn evidence(&self) -> &EvidenceMemoryStore {
        &self.evidence
    }

    /// Research Memory 子存储（Context Memory 雏形）。
    pub fn research(&self) -> &ResearchMemoryStore {
        &self.research
    }
}

/// 压缩/驱逐契约核对报告（[`verify_run_memory_integrity`] 产物）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryIntegrityReport {
    /// 核对的 run。
    pub run_id: String,
    /// 证据台账核对条数（每条做三方哈希对账 + 落盘复算）。
    pub evidence_ledger_checked: usize,
    /// 研究记忆核对条数（每条做全量回链解析）。
    pub research_memories_checked: usize,
    /// 研究记忆回链解析总条数（manifest item/artifact/evidence 三型合计）。
    pub refs_checked: usize,
    /// 失败明细（空串数组即全过）。
    pub failures: Vec<String>,
}

impl MemoryIntegrityReport {
    /// 契约是否全过（零失败）。
    pub fn is_ok(&self) -> bool {
        self.failures.is_empty()
    }
}

/// 压缩/驱逐契约核对（013 头注契约面的执行入口）：
/// - 逐条证据台账行 [`EvidenceMemoryStore::resolve`]（回链行存在 + 三方
///   哈希一致 + 落盘产物逐字节复算）；
/// - 逐条研究记忆 [`ResearchMemoryStore::resolve_refs`]（全部回链可解析）。
///
/// 任一失败进 `failures`，不中断（全量报告面）。
pub fn verify_run_memory_integrity(
    pool: &Pool,
    run_id: &str,
) -> Result<MemoryIntegrityReport, AgentError> {
    let evidence = EvidenceMemoryStore::new(pool.clone());
    let research = ResearchMemoryStore::new(pool.clone());
    let mut failures: Vec<String> = Vec::new();
    let mut refs_checked = 0usize;

    let ledger = evidence.list_by_run(run_id)?;
    for row in &ledger {
        match evidence.resolve(row.entry.memory_id()) {
            Ok(resolved) => {
                if !resolved.db_hash_consistent {
                    failures.push(format!(
                        "evidence memory {}: db hash mismatch ({})",
                        resolved.memory_id, resolved.detail
                    ));
                }
                if resolved.disk_hash_verified == Some(false) {
                    failures.push(format!(
                        "evidence memory {}: on-disk artifact hash mismatch ({})",
                        resolved.memory_id, resolved.detail
                    ));
                }
            }
            Err(e) => failures.push(format!(
                "evidence memory {} unresolved: {e}",
                row.entry.memory_id()
            )),
        }
    }

    let memories = research.list_by_run(run_id)?;
    for row in &memories {
        match research.resolve_refs(row.entry.memory_id()) {
            Ok(refs) => {
                refs_checked += refs.len();
                for r in refs.iter().filter(|r| !r.resolved) {
                    failures.push(format!(
                        "research memory {}: broken back-link {} ({}): {}",
                        row.entry.memory_id(),
                        r.ref_type,
                        r.ref_id,
                        r.detail
                    ));
                }
            }
            Err(e) => failures.push(format!(
                "research memory {}: refs unreadable: {e}",
                row.entry.memory_id()
            )),
        }
    }

    Ok(MemoryIntegrityReport {
        run_id: run_id.to_string(),
        evidence_ledger_checked: ledger.len(),
        research_memories_checked: memories.len(),
        refs_checked,
        failures,
    })
}
