//! Review 候选队列（MODULE_BOUNDARIES §8.4：`Propose(Review) / Decide(review_id, decision)`）。
//!
//! 候选来源：d5 隔离清单（migrations/quarantine/*.jsonl，M1 第一批裁定不自动
//! 转实体、转人工待审）。`propose` 幂等入队（`(source_system, source_pk)` 冲突
//! 即跳过），`decide` 把决议写入 review 表（与 platform 库 review 表同形，
//! 便于迁移期对账；未决提案以 `decision='PENDING'`、`reviewed_at IS NULL` 表示）。
//!
//! 边界：`decide` 只记录决议，不自动改资源实体（M1 口径：D5 不自动合并或删除）；
//! 对资源实体的实际变更须走 [`crate::governor::Governor`]，受状态机与 VERIFIED
//! 守卫约束。

use crate::error::GovernanceError;
use database::{rusqlite, Pool};
use rusqlite::OptionalExtension;
use serde::Serialize;
use std::path::Path;

/// review 表 schema（与 platform 库 review 表同形；source_pk 冲突幂等）。
pub const REVIEW_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS review (
  review_id      TEXT PRIMARY KEY,
  target_type    TEXT NOT NULL,
  target_id      TEXT NOT NULL,
  decision       TEXT NOT NULL,
  confidence     REAL,
  reason         TEXT,
  reviewer       TEXT,
  reviewed_at    TEXT,
  source_system  TEXT NOT NULL,
  source_pk      TEXT NOT NULL,
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL,
  schema_version INTEGER NOT NULL DEFAULT 1,
  UNIQUE (source_system, source_pk)
);
"#;

/// 提案入队结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ProposeOutcome {
    /// 新入队。
    Inserted,
    /// 已在队（幂等跳过）。
    AlreadyPresent,
}

/// Propose 载荷。
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    /// 目标类型（d5_candidate_pair / asset_isolation / ...）。
    pub target_type: String,
    /// 目标标识（队列自有语义，如 `a|b`）。
    pub target_id: String,
    /// 初始决议值（缺省 PENDING）。
    pub decision: String,
    /// 置信度（可空）。
    pub confidence: Option<f64>,
    /// 理由。
    pub reason: Option<String>,
    /// 提案者（缺省 quality-engine）。
    pub reviewer: Option<String>,
    /// 源系统（幂等键一半）。
    pub source_system: String,
    /// 源主键（幂等键另一半，确定性派生）。
    pub source_pk: String,
}

impl Proposal {
    /// 构造 PENDING 提案。
    pub fn pending(
        target_type: &str,
        target_id: &str,
        source_system: &str,
        source_pk: &str,
    ) -> Self {
        Self {
            target_type: target_type.to_string(),
            target_id: target_id.to_string(),
            decision: "PENDING".to_string(),
            confidence: None,
            reason: None,
            reviewer: Some("quality-engine".to_string()),
            source_system: source_system.to_string(),
            source_pk: source_pk.to_string(),
        }
    }

    /// 确定性 review_id（`quality-engine:{source_pk}`，与 platform
    /// `eri-catalog:review_log:<id>` 命名约定同形）。
    pub fn review_id(&self) -> String {
        format!("quality-engine:{}", self.source_pk)
    }
}

/// review 行（decide 后返回）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReviewRow {
    /// review ID。
    pub review_id: String,
    /// 目标类型。
    pub target_type: String,
    /// 目标标识。
    pub target_id: String,
    /// 决议。
    pub decision: String,
    /// 置信度。
    pub confidence: Option<f64>,
    /// 理由。
    pub reason: Option<String>,
    /// 决议者。
    pub reviewer: Option<String>,
    /// 决议时间。
    pub reviewed_at: Option<String>,
}

/// 隔离清单批量入队报告。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QuarantineProposeReport {
    /// 扫描的清单文件。
    pub files: Vec<String>,
    /// 解析出的候选数。
    pub candidates: usize,
    /// 新入队数。
    pub inserted: usize,
    /// 已在队数（幂等跳过）。
    pub skipped: usize,
}

/// Review 队列存储。
#[derive(Clone)]
pub struct ReviewStore {
    pool: Pool,
}

impl ReviewStore {
    /// 构造并确保 schema 存在（幂等）。
    pub fn open(pool: Pool) -> Result<Self, GovernanceError> {
        pool.with_conn(|c| c.execute_batch(REVIEW_SCHEMA))?;
        Ok(Self { pool })
    }

    /// 入队一条提案（幂等：同 `(source_system, source_pk)` 已存在则跳过）。
    pub fn propose(&self, p: &Proposal) -> Result<ProposeOutcome, GovernanceError> {
        if p.target_type.trim().is_empty() || p.target_id.trim().is_empty() {
            return Err(GovernanceError::InvalidDecision(
                "proposal requires target_type and target_id".to_string(),
            ));
        }
        let now = domain::time::now_iso();
        let n = self.pool.with_tx(|c| -> rusqlite::Result<usize> {
            c.execute(
                "INSERT INTO review (review_id, target_type, target_id, decision,
                    confidence, reason, reviewer, reviewed_at, source_system, source_pk,
                    created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9, ?10, ?10)
                 ON CONFLICT(source_system, source_pk) DO NOTHING",
                rusqlite::params![
                    p.review_id(),
                    p.target_type,
                    p.target_id,
                    p.decision,
                    p.confidence,
                    p.reason,
                    p.reviewer,
                    p.source_system,
                    p.source_pk,
                    now,
                ],
            )
        })?;
        Ok(if n == 1 {
            ProposeOutcome::Inserted
        } else {
            ProposeOutcome::AlreadyPresent
        })
    }

    /// 批量入队（幂等重放安全）。
    pub fn propose_all(
        &self,
        proposals: &[Proposal],
    ) -> Result<QuarantineProposeReport, GovernanceError> {
        let mut inserted = 0usize;
        let mut skipped = 0usize;
        for p in proposals {
            match self.propose(p)? {
                ProposeOutcome::Inserted => inserted += 1,
                ProposeOutcome::AlreadyPresent => skipped += 1,
            }
        }
        Ok(QuarantineProposeReport {
            files: Vec::new(),
            candidates: proposals.len(),
            inserted,
            skipped,
        })
    }

    /// 写决议（§8.4 Decide）。仅 PENDING 可决议；重复 decide 返回 `AlreadyDecided`。
    /// 决议只落 review 表，不自动改资源实体（M1 口径）。
    pub fn decide(
        &self,
        review_id: &str,
        decision: &str,
        reviewer: &str,
        confidence: Option<f64>,
        reason: Option<&str>,
    ) -> Result<ReviewRow, GovernanceError> {
        let decision_clean = decision.trim();
        if decision_clean.is_empty() {
            return Err(GovernanceError::InvalidDecision(
                "decision must be non-empty".to_string(),
            ));
        }
        if decision_clean.eq_ignore_ascii_case("PENDING") {
            return Err(GovernanceError::InvalidDecision(
                "decision cannot be PENDING (use propose for queuing)".to_string(),
            ));
        }
        let now = domain::time::now_iso();
        let updated = self.pool.with_tx(|c| -> rusqlite::Result<usize> {
            c.execute(
                "UPDATE review SET decision = ?1, reviewer = ?2, confidence = ?3,
                        reason = COALESCE(?4, reason), reviewed_at = ?5, updated_at = ?5
                 WHERE review_id = ?6 AND decision = 'PENDING'",
                rusqlite::params![decision_clean, reviewer, confidence, reason, now, review_id,],
            )
        })?;
        if updated == 0 {
            return match self.get(review_id)? {
                Some(_) => Err(GovernanceError::AlreadyDecided(review_id.to_string())),
                None => Err(GovernanceError::NotFound(review_id.to_string())),
            };
        }
        self.get(review_id)?
            .ok_or_else(|| GovernanceError::NotFound(review_id.to_string()))
    }

    /// 待决清单（decision=PENDING）。
    pub fn pending(&self) -> Result<Vec<ReviewRow>, GovernanceError> {
        self.list_where("decision = 'PENDING' ORDER BY review_id")
    }

    /// 按 review_id 读取。
    pub fn get(&self, review_id: &str) -> Result<Option<ReviewRow>, GovernanceError> {
        let row = self.pool.with_conn(|c| {
            c.query_row(
                "SELECT review_id, target_type, target_id, decision, confidence,
                        reason, reviewer, reviewed_at
                 FROM review WHERE review_id = ?1",
                [review_id],
                row_to_review,
            )
            .optional()
        })?;
        Ok(row)
    }

    /// 统计（按决议值分组）。
    pub fn counts(&self) -> Result<Vec<(String, i64)>, GovernanceError> {
        let rows = self
            .pool
            .with_conn(|c| -> rusqlite::Result<Vec<(String, i64)>> {
                let mut stmt =
                    c.prepare("SELECT decision, COUNT(*) FROM review GROUP BY decision")?;
                let rows = stmt
                    .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
                    .collect::<Result<Vec<_>, rusqlite::Error>>()?;
                Ok(rows)
            })?;
        Ok(rows)
    }

    fn list_where(&self, cond: &str) -> Result<Vec<ReviewRow>, GovernanceError> {
        let rows = self
            .pool
            .with_conn(|c| -> rusqlite::Result<Vec<ReviewRow>> {
                let sql = format!(
                    "SELECT review_id, target_type, target_id, decision, confidence,
                        reason, reviewer, reviewed_at FROM review WHERE {cond}"
                );
                let mut stmt = c.prepare(&sql)?;
                let rows = stmt
                    .query_map([], row_to_review)?
                    .collect::<Result<Vec<_>, rusqlite::Error>>()?;
                Ok(rows)
            })?;
        Ok(rows)
    }
}

fn row_to_review(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReviewRow> {
    Ok(ReviewRow {
        review_id: row.get(0)?,
        target_type: row.get(1)?,
        target_id: row.get(2)?,
        decision: row.get(3)?,
        confidence: row.get(4)?,
        reason: row.get(5)?,
        reviewer: row.get(6)?,
        reviewed_at: row.get(7)?,
    })
}

/// 从隔离清单目录（migrations/quarantine/）读候选并转提案。
///
/// 支持文件（M1 第一批产出）：
/// - `d5_candidates.jsonl`：D5 疑似重复候选对，target = `{a}|{b}`，置信度取
///   源初筛 score，理由携带源初筛 decision 与人工待审口径。
/// - `assets_isolation.jsonl`：assets.db 隔离登记（不抽取），target =
///   `{engagement}:{fingerprint_md5}`。
pub fn proposals_from_quarantine_dir(
    dir: &Path,
) -> Result<(Vec<Proposal>, Vec<String>), GovernanceError> {
    let mut proposals = Vec::new();
    let mut files = Vec::new();

    let d5_path = dir.join("d5_candidates.jsonl");
    if d5_path.is_file() {
        files.push(d5_path.display().to_string());
        for (lineno, line) in read_jsonl(&d5_path)?.into_iter().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let v: serde_json::Value = serde_json::from_str(line).map_err(|e| {
                GovernanceError::InvalidDecision(format!(
                    "{}:{} bad json: {e}",
                    d5_path.display(),
                    lineno + 1
                ))
            })?;
            let rec = &v["record"];
            let (a, b) = (
                rec["resource_a"].as_str().unwrap_or_default(),
                rec["resource_b"].as_str().unwrap_or_default(),
            );
            if a.is_empty() || b.is_empty() {
                return Err(GovernanceError::InvalidDecision(format!(
                    "{}:{} missing resource_a/resource_b",
                    d5_path.display(),
                    lineno + 1
                )));
            }
            let mut p = Proposal::pending(
                "d5_candidate_pair",
                &format!("{a}|{b}"),
                "quality-engine",
                &format!(
                    "quarantine/d5_candidates.jsonl:{}",
                    rec["id"].as_i64().unwrap_or(lineno as i64 + 1)
                ),
            );
            p.confidence = rec["score"].as_f64();
            p.reason = Some(format!(
                "{}；源初筛 decision={} score={}",
                v["quarantine_reason"].as_str().unwrap_or("D5 候选对"),
                rec["decision"].as_str().unwrap_or("UNKNOWN"),
                rec["score"].as_f64().unwrap_or(0.0),
            ));
            proposals.push(p);
        }
    }

    let assets_path = dir.join("assets_isolation.jsonl");
    if assets_path.is_file() {
        files.push(assets_path.display().to_string());
        for (lineno, line) in read_jsonl(&assets_path)?.into_iter().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let v: serde_json::Value = serde_json::from_str(line).map_err(|e| {
                GovernanceError::InvalidDecision(format!(
                    "{}:{} bad json: {e}",
                    assets_path.display(),
                    lineno + 1
                ))
            })?;
            let engagement = v["engagement"].as_str().unwrap_or("unknown");
            let fp = v["fingerprint_md5"].as_str().unwrap_or("nofp");
            let mut p = Proposal::pending(
                "asset_isolation",
                &format!("{engagement}:{fp}"),
                "quality-engine",
                &format!("quarantine/assets_isolation.jsonl:{}", lineno + 1),
            );
            p.reason = Some(format!(
                "{}；处置：{}",
                v["reason"].as_str().unwrap_or("隔离登记"),
                v["disposition"].as_str().unwrap_or("-")
            ));
            proposals.push(p);
        }
    }

    Ok((proposals, files))
}

fn read_jsonl(path: &Path) -> Result<Vec<String>, GovernanceError> {
    std::fs::read_to_string(path)
        .map(|content| content.lines().map(String::from).collect())
        .map_err(|e| GovernanceError::InvalidDecision(format!("read {}: {e}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::{DbPath, PoolConfig};
    use std::time::Duration;

    fn store() -> ReviewStore {
        let pool = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(1),
        })
        .unwrap();
        ReviewStore::open(pool).unwrap()
    }

    fn sample() -> Proposal {
        let mut p = Proposal::pending(
            "d5_candidate_pair",
            "gh:a/x|gh:b/y",
            "quality-engine",
            "quarantine/d5_candidates.jsonl:1",
        );
        p.confidence = Some(0.61);
        p.reason = Some("源初筛 decision=NEEDS_REVIEW".to_string());
        p
    }

    #[test]
    fn propose_is_idempotent_by_source_pk() {
        let s = store();
        assert_eq!(s.propose(&sample()).unwrap(), ProposeOutcome::Inserted);
        assert_eq!(
            s.propose(&sample()).unwrap(),
            ProposeOutcome::AlreadyPresent
        );
        let pending = s.pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(
            pending[0].review_id,
            "quality-engine:quarantine/d5_candidates.jsonl:1"
        );
        assert_eq!(pending[0].decision, "PENDING");
        assert!(pending[0].reviewed_at.is_none());
    }

    #[test]
    fn decide_writes_review_row_once() {
        let s = store();
        s.propose(&sample()).unwrap();
        let row = s
            .decide(
                "quality-engine:quarantine/d5_candidates.jsonl:1",
                "DISTINCT",
                "human-reviewer",
                Some(0.9),
                Some("名字近似但描述无重叠，判独立项目"),
            )
            .unwrap();
        assert_eq!(row.decision, "DISTINCT");
        assert_eq!(row.reviewer.as_deref(), Some("human-reviewer"));
        assert!(row.reviewed_at.is_some());
        // 不可重复决议
        let err = s
            .decide(
                "quality-engine:quarantine/d5_candidates.jsonl:1",
                "MERGE",
                "x",
                None,
                None,
            )
            .unwrap_err();
        assert!(matches!(err, GovernanceError::AlreadyDecided(_)), "{err:?}");
        // 不存在
        let err2 = s
            .decide("quality-engine:missing", "DISTINCT", "x", None, None)
            .unwrap_err();
        assert!(matches!(err2, GovernanceError::NotFound(_)));
        // counts 反映决议
        assert_eq!(s.counts().unwrap(), vec![("DISTINCT".to_string(), 1)]);
        assert!(s.pending().unwrap().is_empty());
    }

    #[test]
    fn decide_rejects_bad_values() {
        let s = store();
        s.propose(&sample()).unwrap();
        assert!(matches!(
            s.decide(
                "quality-engine:quarantine/d5_candidates.jsonl:1",
                "  ",
                "x",
                None,
                None
            ),
            Err(GovernanceError::InvalidDecision(_))
        ));
        assert!(matches!(
            s.decide(
                "quality-engine:quarantine/d5_candidates.jsonl:1",
                "pending",
                "x",
                None,
                None
            ),
            Err(GovernanceError::InvalidDecision(_))
        ));
    }

    #[test]
    fn propose_from_real_quarantine_fixtures() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("migrations")
            .join("quarantine");
        if !dir.is_dir() {
            // 容器构建等无 fixture 环境：跳过（与 parity 测试同一纪律）
            eprintln!(
                "SKIP propose_from_real_quarantine_fixtures: {} missing",
                dir.display()
            );
            return;
        }
        let s = store();
        let (proposals, files) = proposals_from_quarantine_dir(&dir).unwrap();
        // 真实清单：d5_candidates.jsonl 68 条 + assets_isolation.jsonl 2 条
        assert_eq!(proposals.len(), 70, "fixture-driven: 68 d5 + 2 assets");
        assert_eq!(files.len(), 2);
        let report = s.propose_all(&proposals).unwrap();
        assert_eq!(report.inserted, 70);
        assert_eq!(s.pending().unwrap().len(), 70);
        // 幂等重放
        let report2 = s.propose_all(&proposals).unwrap();
        assert_eq!(report2.inserted, 0);
        assert_eq!(report2.skipped, 70);
        // 决议一条真实候选
        let first = &proposals[0];
        s.decide(
            &first.review_id(),
            "KEEP_BOTH",
            "m4-smoke",
            Some(0.8),
            Some("复核独立"),
        )
        .unwrap();
        assert_eq!(s.pending().unwrap().len(), 69);
    }
}
