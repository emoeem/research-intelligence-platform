//! rp-kc：RIP3 情报核 KnowledgeCard 治理 CLI（P3-1 起步面，JSON stdout，
//! rp-* 家族惯例）。
//!
//! 子命令：create / relate / list / show / review / score / dedup-scan。
//! 运行配置 RP_PG_DSN（intel-db lib.rs 缺省口径）。exit: ok=0，运行失败=1，
//! 用法错误=2。

use domain::kcard::{
    EvidenceLevel, KnowledgeCardId, KnowledgeType, QualityWeights, RelationType, ReviewStatus,
    RiskFlag, SourceTrust, VerificationStatus,
};
use intel_db::card_repo::{self, CardInput};
use intel_db::{open_pool, run_migrations, IntelError, IntelResult, PgConfig};
use std::path::PathBuf;

fn usage() -> ! {
    eprintln!(
        "usage: rp-kc <sub> [args]\n\
\n\
subcommands:\n\
  create --type T --title T [--summary S] [--content-file F | --content S] [--content-json J]\n\
         [--confidence f] [--trust t] [--evidence K2] [--verified] [--ai-derived]\n\
         [--flags F1,F2] [--workspace W] [--document D --revision R]\n\
    创建卡片（DISCOVERED；D2 规范化哈希随行；--flags 用 R2 5.3 词形）\n\
  donor-submit --donor-caller CALLER --type T --title T [--summary S]\n\
         [--content-file F | --content S] [--content-json J] [--confidence f]\n\
         [--flags F1,F2] [--workspace W] [--donor-channel mcp|cli]\n\
    供体写入即候选（MA-3 D5：create 落 DISCOVERED（source_trust=agent_derived\n\
    + ai_derived=TRUE 定死）→ 即 decide 转 CANDIDATE，review_log\n\
    reviewer=donor_caller 留痕；供体永不直写 APPROVED——approve 只经既有\n\
    review 双闸；收据 JSON stdout {{knowledge_id, review_status,\n\
    normalized_hash}}）；--donor-caller 必填 fail-closed；去重走既有 D2\n\
    语义（同哈希产 SAME_CONTENT 候选边不拒绝创建）\n\
  relate --src KC --dst KC --type T [--confidence f] [--evidence-json J] [--source S]\n\
    追加知识关系（十型；CONTRADICTS 命中即回退候选进审核，D5）\n\
  list [--status S] [--workspace W]     卡片列表（按审核态过滤）\n\
  show --card KC                        卡片详情（关系/文档/审核日志）\n\
  review --card KC --decision approve|quarantine|reject|deprecate|candidate\n\
         [--reason R] [--reviewer X]    审核决定（低质量地板 + 硬标记阻断，日志留痕）\n\
  score --card KC                       计算并回填质量分（六权重 R2 5.3 缺省）\n\
  dedup-scan [--workspace W]            D2 去重扫描（SAME_CONTENT 候选边，不自动合并）\n\
  batch-review --decision D [--status S] [--min-score f] [--workspace W]\n\
         [--reason R] [--reviewer X] [--dry-run]  批量审核（硬标记与地板双闸逐卡生效，dry-run 预览）\n\
  minhash-scan [--workspace W] [--threshold 0.5] [--num-perm 128]\n\
         [--run-db P] [--manifest-dir D]  D3 近似重复扫描（rip.gov.minhash 容器，NEAR_DUPLICATE 候选边）\n\
  embed-cards [--workspace W] [--run-db P] [--manifest-dir D]\n\
    D4 卡片语义向量补齐（rip.embed.text 容器 bge 本地推理，512 维幂等 upsert）\n\
  semantic-scan [--workspace W] [--threshold 0.92] [--top-k 10]\n\
         [--run-db P] [--manifest-dir D]  D4 语义候选（pgvector 余弦近邻，NEAR_DUPLICATE 候选边禁自动合并）\n\
  search --query Q [--mode bm25|vector|hybrid|rerank] [--limit N] [--type T]\n\
         [--workspace W] [--mmr-lambda f] [--include-unapproved] [--run-db P] [--manifest-dir D]\n\
    统一检索入口（P4-1：BM25 pg_search + 向量 pgvector + RRF/MMR 融合；\n\
    权限前置谓词 SQL 内，缺省仅 APPROVED + 硬标记排除）\n\
  bench --qrel-file F [--k N] [--workspace W] [--run-db P]\n\
         [--manifest-dir D] [--rerank-manifest-dir D]\n\
    P4-4 检索基准（R2 6.8：qrel JSON 逐查询 × 四模式，Recall@K/Precision@K/\n\
    MRR/NDCG@K + latency，行级 JSON 输出）\n\
\n\
env: RP_PG_DSN（缺省 postgres://postgres:postgres@127.0.0.1:5432/postgres）\n\
help: rp-kc --help（stdout, exit 0）"
    );
    std::process::exit(2);
}

fn help_only() -> ! {
    println!("rp-kc：RIP3 KnowledgeCard 治理 CLI（子命令清单见 stderr usage；此行输出后 exit 0）");
    std::process::exit(0);
}

struct Args {
    flags: std::collections::HashMap<String, String>,
    subs: Vec<String>,
}

fn parse_args(iter: impl Iterator<Item = String>) -> Args {
    const BOOL_FLAGS: [&str; 4] = ["verified", "ai-derived", "dry-run", "include-unapproved"];
    let mut flags = std::collections::HashMap::new();
    let mut subs = Vec::new();
    let mut i = iter.peekable();
    while let Some(arg) = i.next() {
        if let Some(name) = arg.strip_prefix("--") {
            if BOOL_FLAGS.contains(&name) {
                flags.insert(name.to_string(), "true".to_string());
            } else {
                let value = i.next().unwrap_or_else(|| usage());
                flags.insert(name.to_string(), value);
            }
        } else {
            subs.push(arg);
        }
    }
    Args { flags, subs }
}

fn require_flag(args: &Args, name: &str) -> String {
    args.flags.get(name).cloned().unwrap_or_else(|| usage())
}

fn opt_flag(args: &Args, name: &str) -> Option<String> {
    args.flags.get(name).cloned()
}

fn die_json(code: &str, message: &str) -> ! {
    println!("{{\"error\":{{\"code\":{code:?},\"message\":{message:?}}}}}");
    // RP_DEBUG=1 时透出错误源链（postgres 参数序列化面的根因定位）。
    if std::env::var("RP_DEBUG").as_deref() == Ok("1") {
        eprintln!("DEBUG chain: {message}");
    }
    std::process::exit(1);
}

/// 检索面嵌入容器配置（--run-db/--manifest-dir 绝对化，与治理扫描面同口径）。
fn embed_cfg_from(args: &Args) -> IntelResult<intel_db::embed_worker::EmbedWorkerConfig> {
    let mut cfg = intel_db::embed_worker::EmbedWorkerConfig::default();
    if let Some(p) = opt_flag(args, "run-db") {
        cfg.run_db = PathBuf::from(p);
    }
    if let Some(p) = opt_flag(args, "manifest-dir") {
        cfg.manifest_dir = PathBuf::from(p);
    }
    cfg.run_db = std::path::absolute(&cfg.run_db)?;
    cfg.workspace_root = std::path::absolute(&cfg.workspace_root)?;
    cfg.sqlite_migrations_dir = std::path::absolute(&cfg.sqlite_migrations_dir)?;
    cfg.manifest_dir = std::path::absolute(&cfg.manifest_dir)?;
    Ok(cfg)
}

fn main() {
    let mut argv = std::env::args().peekable();
    let _bin = argv.next();
    if argv
        .peek()
        .map(|s| s == "--help" || s == "-h")
        .unwrap_or(false)
    {
        help_only();
    }
    let args = parse_args(argv);
    let sub = args.subs.first().cloned().unwrap_or_else(|| usage());
    let pool = match open_pool(&PgConfig::default()) {
        Ok(p) => p,
        Err(e) => die_json("pg_unavailable", &e.to_string()),
    };
    {
        // 迁移引导（refinery 幂等；V0004/V0005 治理表面随启动就绪）。
        let mut conn = match pool.get() {
            Ok(c) => c,
            Err(e) => die_json("pg_unavailable", &e.to_string()),
        };
        if let Err(e) = run_migrations(&mut conn) {
            die_json("runtime_failed", &e.to_string());
        }
    }

    let result: IntelResult<serde_json::Value> = (|| {
        match sub.as_str() {
            "create" => {
                let ktype =
                    KnowledgeType::parse(&require_flag(&args, "type")).map_err(IntelError::from)?;
                let content = if let Some(f) = opt_flag(&args, "content-file") {
                    std::fs::read_to_string(PathBuf::from(f)).map_err(IntelError::from)?
                } else {
                    opt_flag(&args, "content").unwrap_or_default()
                };
                let structured = opt_flag(&args, "content-json")
                    .map(|raw| serde_json::from_str::<serde_json::Value>(&raw))
                    .transpose()
                    .map_err(|e| IntelError::Invalid(format!("content-json parse: {e}")))?;
                let confidence = opt_flag(&args, "confidence")
                    .map(|s| s.parse::<f32>())
                    .transpose()
                    .map_err(|e| IntelError::Invalid(format!("confidence parse: {e}")))?
                    .unwrap_or(0.5);
                let trust = SourceTrust::parse(
                    opt_flag(&args, "trust")
                        .unwrap_or_else(|| "external_import".to_string())
                        .as_str(),
                )
                .map_err(IntelError::from)?;
                let evidence = match opt_flag(&args, "evidence") {
                    Some(raw) => Some(EvidenceLevel::parse(&raw).map_err(IntelError::from)?),
                    None => None,
                };
                let flags = match opt_flag(&args, "flags") {
                    Some(raw) => raw
                        .split(',')
                        .filter(|s| !s.trim().is_empty())
                        .map(|s| RiskFlag::parse(s.trim()).map_err(IntelError::from))
                        .collect::<IntelResult<Vec<RiskFlag>>>()?,
                    None => Vec::new(),
                };
                let input = CardInput {
                    knowledge_type: ktype,
                    title: require_flag(&args, "title"),
                    summary: opt_flag(&args, "summary").unwrap_or_default(),
                    content,
                    structured_content: structured,
                    confidence,
                    verification: if args.flags.contains_key("verified") {
                        VerificationStatus::Verified
                    } else {
                        VerificationStatus::Unverified
                    },
                    evidence_level: evidence,
                    source_trust: trust,
                    ai_derived: args.flags.contains_key("ai-derived"),
                    quality_flags: flags,
                    workspace_id: opt_flag(&args, "workspace")
                        .unwrap_or_else(|| "default".to_string()),
                    donor_caller_id: None,
                    donor_channel: None,
                };
                let card = card_repo::create_card(&pool, &input)?;
                let mut out = serde_json::json!({ "knowledge_id": card.as_str() });
                if let (Some(d), Some(r)) =
                    (opt_flag(&args, "document"), opt_flag(&args, "revision"))
                {
                    let document = domain::DocumentId::try_new(d)?;
                    let revision = domain::DocumentRevisionId::try_new(r)?;
                    card_repo::link_document(&pool, &document, &revision, &card)?;
                    out["linked_document"] = serde_json::json!(document.as_str());
                    out["linked_revision"] = serde_json::json!(revision.as_str());
                }
                Ok(out)
            }
            "donor-submit" => {
                // MA-3（D5）供体写入即候选唯一入口：--donor-caller 必填
                // fail-closed；source_trust/ai_derived 定死（供体 = AI 派生
                // 事实，原则 C，不暴露覆盖旗标）；两步 create→decide 同一
                // 治理面（card_repo/decide/review_log 本体，非第二写入路径）。
                let donor = require_flag(&args, "donor-caller");
                if donor.trim().is_empty() {
                    return Err(IntelError::Invalid(
                        "donor-submit requires --donor-caller (fail-closed)".to_string(),
                    ));
                }
                let channel = match opt_flag(&args, "donor-channel") {
                    Some(raw) => {
                        if raw != "mcp" && raw != "cli" {
                            return Err(IntelError::Invalid(format!(
                                "donor-channel must be 'mcp' or 'cli', got {raw:?}"
                            )));
                        }
                        raw
                    }
                    None => "cli".to_string(),
                };
                let ktype =
                    KnowledgeType::parse(&require_flag(&args, "type")).map_err(IntelError::from)?;
                let content = if let Some(f) = opt_flag(&args, "content-file") {
                    std::fs::read_to_string(PathBuf::from(f)).map_err(IntelError::from)?
                } else {
                    opt_flag(&args, "content").unwrap_or_default()
                };
                if content.trim().is_empty() {
                    return Err(IntelError::Invalid(
                        "donor-submit requires non-empty content".to_string(),
                    ));
                }
                let structured = opt_flag(&args, "content-json")
                    .map(|raw| serde_json::from_str::<serde_json::Value>(&raw))
                    .transpose()
                    .map_err(|e| IntelError::Invalid(format!("content-json parse: {e}")))?;
                let confidence = opt_flag(&args, "confidence")
                    .map(|s| s.parse::<f32>())
                    .transpose()
                    .map_err(|e| IntelError::Invalid(format!("confidence parse: {e}")))?
                    .unwrap_or(0.5);
                let flags = match opt_flag(&args, "flags") {
                    Some(raw) => raw
                        .split(',')
                        .filter(|s| !s.trim().is_empty())
                        .map(|s| RiskFlag::parse(s.trim()).map_err(IntelError::from))
                        .collect::<IntelResult<Vec<RiskFlag>>>()?,
                    None => Vec::new(),
                };
                let input = CardInput {
                    knowledge_type: ktype,
                    title: require_flag(&args, "title"),
                    summary: opt_flag(&args, "summary").unwrap_or_default(),
                    content,
                    structured_content: structured,
                    confidence,
                    verification: VerificationStatus::Unverified,
                    evidence_level: None,
                    source_trust: SourceTrust::AgentDerived,
                    ai_derived: true,
                    quality_flags: flags,
                    workspace_id: opt_flag(&args, "workspace")
                        .unwrap_or_else(|| "default".to_string()),
                    donor_caller_id: Some(donor.clone()),
                    donor_channel: Some(channel),
                };
                let card = card_repo::create_card(&pool, &input)?;
                // 写入即候选：DISCOVERED → CANDIDATE 立即转移（合法转移表
                // Discovered→Candidate 在位；review_log reviewer=donor_caller
                // 留痕——reconciliation 人工确认语义同构）。
                card_repo::decide(
                    &pool,
                    &card,
                    ReviewStatus::Candidate,
                    &format!("donor-submit 写入即候选（donor={donor}）"),
                    &donor,
                )?;
                // 收据（brain-mark 收据语义；normalized_hash = D2 去重对账锚）。
                let norm = card_repo::normalized_hash(&input.content);
                Ok(serde_json::json!({
                    "knowledge_id": card.as_str(),
                    "review_status": "CANDIDATE",
                    "normalized_hash": norm,
                }))
            }
            "relate" => {
                let src = KnowledgeCardId::try_new(require_flag(&args, "src"))?;
                let dst = KnowledgeCardId::try_new(require_flag(&args, "dst"))?;
                let rtype =
                    RelationType::parse(&require_flag(&args, "type")).map_err(IntelError::from)?;
                let confidence = opt_flag(&args, "confidence")
                    .map(|s| s.parse::<f32>())
                    .transpose()
                    .map_err(|e| IntelError::Invalid(format!("confidence parse: {e}")))?
                    .unwrap_or(0.8);
                let evidence = opt_flag(&args, "evidence-json")
                    .map(|raw| serde_json::from_str::<serde_json::Value>(&raw))
                    .transpose()
                    .map_err(|e| IntelError::Invalid(format!("evidence-json parse: {e}")))?;
                let source =
                    opt_flag(&args, "source").unwrap_or_else(|| "manual:rp-kc".to_string());
                let (rid, created) = card_repo::add_relation(
                    &pool,
                    &src,
                    &dst,
                    rtype,
                    evidence.as_ref(),
                    &source,
                    confidence,
                )?;
                Ok(serde_json::json!({
                    "relation_id": rid.as_str(),
                    "created": created,
                    "relation_type": rtype.as_str(),
                }))
            }
            "list" => {
                let workspace =
                    opt_flag(&args, "workspace").unwrap_or_else(|| "default".to_string());
                let status = match opt_flag(&args, "status") {
                    Some(raw) => Some(ReviewStatus::parse(&raw).map_err(IntelError::from)?),
                    None => None,
                };
                let items = card_repo::list_by_review(&pool, &workspace, status)?;
                Ok(serde_json::json!({ "items": items }))
            }
            "show" => {
                let card = KnowledgeCardId::try_new(require_flag(&args, "card"))?;
                let workspace =
                    opt_flag(&args, "workspace").unwrap_or_else(|| "default".to_string());
                let detail = card_repo::get_card(&pool, &workspace, &card)?;
                serde_json::to_value(detail).map_err(|e| IntelError::Invalid(e.to_string()))
            }
            "review" => {
                let card = KnowledgeCardId::try_new(require_flag(&args, "card"))?;
                let decision = match require_flag(&args, "decision").to_lowercase().as_str() {
                    "approve" => ReviewStatus::Approved,
                    "quarantine" => ReviewStatus::Quarantined,
                    "reject" => ReviewStatus::Rejected,
                    "deprecate" => ReviewStatus::Deprecated,
                    "candidate" => ReviewStatus::Candidate,
                    other => {
                        return Err(IntelError::Invalid(format!(
                            "unknown decision {other:?} (approve/quarantine/reject/deprecate/candidate)"
                        )))
                    }
                };
                let reason = opt_flag(&args, "reason").unwrap_or_default();
                let reviewer = opt_flag(&args, "reviewer").unwrap_or_else(|| "manual".to_string());
                card_repo::decide(&pool, &card, decision, &reason, &reviewer)?;
                Ok(serde_json::json!({
                    "knowledge_id": card.as_str(),
                    "review_status": decision.as_str(),
                }))
            }
            "score" => {
                let card = KnowledgeCardId::try_new(require_flag(&args, "card"))?;
                let score = card_repo::score_card(&pool, &card, &QualityWeights::default())?;
                Ok(serde_json::json!({ "knowledge_id": card.as_str(), "quality_score": score }))
            }
            "batch-review" => {
                // 批量审核面（R2 5.4 批量审核；低质量双闸在 decide 内逐卡生效）。
                let workspace =
                    opt_flag(&args, "workspace").unwrap_or_else(|| "default".to_string());
                let status = ReviewStatus::parse(
                    opt_flag(&args, "status")
                        .unwrap_or_else(|| "CANDIDATE".to_string())
                        .as_str(),
                )
                .map_err(IntelError::from)?;
                let min_score = opt_flag(&args, "min-score")
                    .map(|s| s.parse::<f32>())
                    .transpose()
                    .map_err(|e| IntelError::Invalid(format!("min-score parse: {e}")))?;
                let to = match require_flag(&args, "decision").to_lowercase().as_str() {
                    "approve" => ReviewStatus::Approved,
                    "quarantine" => ReviewStatus::Quarantined,
                    "reject" => ReviewStatus::Rejected,
                    other => {
                        return Err(IntelError::Invalid(format!(
                            "unknown decision {other:?} (approve/quarantine/reject)"
                        )))
                    }
                };
                let reason =
                    opt_flag(&args, "reason").unwrap_or_else(|| "batch-review".to_string());
                let reviewer = opt_flag(&args, "reviewer").unwrap_or_else(|| "batch".to_string());
                let dry_run = args.flags.contains_key("dry-run");
                let mut conn = pool.get()?;
                let rows = conn.query(
                    "SELECT knowledge_id, title, quality_score, quality_flags FROM knowledge_cards
                     WHERE workspace_id = $1 AND review_status = $2
                     ORDER BY quality_score DESC NULLS LAST",
                    &[&workspace, &status.as_str()],
                )?;
                drop(conn);
                let mut applied: Vec<String> = Vec::new();
                let mut skipped: Vec<serde_json::Value> = Vec::new();
                for r in &rows {
                    let id: String = r.get(0);
                    let title: String = r.get(1);
                    let score: Option<f32> = r.get(2);
                    let flags: Vec<String> = r.get(3);
                    let card = KnowledgeCardId::try_new(id.clone())?;
                    // 硬标记与地板双闸预检（approve 面；与 decide 内闸一致）。
                    let hard_hit = flags.iter().any(|f| {
                        RiskFlag::parse(f)
                            .map(|rf| rf.is_hard_block())
                            .unwrap_or(false)
                    });
                    // 地板：--min-score 覆盖（approve 面），缺省 = LOW_QUALITY_FLOOR。
                    let floor = match (to, min_score) {
                        (ReviewStatus::Approved, Some(custom)) => custom,
                        (ReviewStatus::Approved, None) => card_repo::LOW_QUALITY_FLOOR,
                        _ => f32::MIN,
                    };
                    let score_low = matches!(
                        (to, score),
                        (ReviewStatus::Approved, Some(s)) if s < floor
                    );
                    if to == ReviewStatus::Approved && (hard_hit || score_low) {
                        skipped.push(serde_json::json!({
                            "knowledge_id": id, "title": title,
                            "reason": if hard_hit { "hard risk flag" } else { "below floor" },
                        }));
                        continue;
                    }
                    if dry_run {
                        applied.push(id);
                        continue;
                    }
                    match card_repo::decide(&pool, &card, to, &reason, &reviewer) {
                        Ok(()) => applied.push(id),
                        Err(e) => skipped.push(serde_json::json!({
                            "knowledge_id": id, "title": title, "reason": e.to_string(),
                        })),
                    }
                }
                Ok(serde_json::json!({
                    "workspace": workspace,
                    "from_status": status.as_str(),
                    "decision": to.as_str(),
                    "dry_run": dry_run,
                    "applied": applied,
                    "skipped": skipped,
                }))
            }
            "feedback-list" => {
                let workspace =
                    opt_flag(&args, "workspace").unwrap_or_else(|| "default".to_string());
                let status = opt_flag(&args, "status");
                let limit = opt_flag(&args, "limit")
                    .map(|s| s.parse::<i64>())
                    .transpose()
                    .map_err(|e| IntelError::Invalid(format!("limit parse: {e}")))?
                    .unwrap_or(100);
                intel_db::maintenance::list_feedback_candidates(
                    &pool,
                    &workspace,
                    status.as_deref(),
                    limit,
                )
            }
            "feedback-show" => {
                let candidate = require_flag(&args, "candidate-id");
                let workspace =
                    opt_flag(&args, "workspace").unwrap_or_else(|| "default".to_string());
                intel_db::maintenance::get_feedback_candidate(&pool, &workspace, &candidate)?
                    .ok_or_else(|| {
                        IntelError::Invalid(format!("feedback candidate not found: {candidate}"))
                    })
            }
            "feedback-reject" => {
                let candidate = require_flag(&args, "candidate-id");
                let workspace =
                    opt_flag(&args, "workspace").unwrap_or_else(|| "default".to_string());
                let reviewer = require_flag(&args, "reviewer");
                let reason = require_flag(&args, "reason");
                intel_db::maintenance::reject_feedback_candidate(
                    &pool, &workspace, &candidate, &reviewer, &reason,
                )?;
                Ok(serde_json::json!({"candidate_id":candidate,"review_status":"REJECTED"}))
            }
            "feedback-ingest" => {
                let report_id = require_flag(&args, "report-id");
                let workspace =
                    opt_flag(&args, "workspace").unwrap_or_else(|| "default".to_string());
                let outcome = intel_db::maintenance::ingest_report(&pool, &report_id, &workspace)?;
                serde_json::to_value(outcome).map_err(|e| IntelError::Invalid(e.to_string()))
            }
            "feedback-promote" => {
                let candidate = require_flag(&args, "candidate-id");
                let workspace =
                    opt_flag(&args, "workspace").unwrap_or_else(|| "default".to_string());
                let reviewer = require_flag(&args, "reviewer");
                let reason = require_flag(&args, "reason");
                let card = intel_db::maintenance::promote_feedback_candidate(
                    &pool, &workspace, &candidate, &reviewer, &reason,
                )?;
                Ok(
                    serde_json::json!({"candidate_id":candidate,"knowledge_id":card.as_str(),"review_status":"CANDIDATE"}),
                )
            }
            "feedback-reuse" => {
                let candidate = require_flag(&args, "candidate-id");
                let reviewer = require_flag(&args, "reviewer");
                let reason = require_flag(&args, "reason");
                let card = intel_db::maintenance::reuse_duplicate_candidate(
                    &pool, &candidate, &reviewer, &reason,
                )?;
                Ok(
                    serde_json::json!({"candidate_id":candidate,"knowledge_id":card.as_str(),"reused":true}),
                )
            }
            "maintenance-audit" => {
                let workspace =
                    opt_flag(&args, "workspace").unwrap_or_else(|| "default".to_string());
                let outcome = intel_db::maintenance::run_maintenance_audit(&pool, &workspace)?;
                serde_json::to_value(outcome).map_err(|e| IntelError::Invalid(e.to_string()))
            }
            "maintenance-report" => {
                let run_id = require_flag(&args, "run-id");
                intel_db::maintenance::load_maintenance_report(&pool, &run_id)?.ok_or_else(|| {
                    IntelError::Invalid(format!("maintenance run not found: {run_id}"))
                })
            }
            "dedup-scan" => {
                let workspace =
                    opt_flag(&args, "workspace").unwrap_or_else(|| "default".to_string());
                let created = card_repo::dedup_scan_same_content(&pool, &workspace)?;
                Ok(
                    serde_json::json!({ "workspace": workspace, "same_content_relations_created": created }),
                )
            }
            "search" => {
                // 统一检索入口（R2 6.1：唯一检索面；R2 6.4 结构化过滤 + 13 号
                // 冻结物第五节权限前置谓词在 SQL 内）。
                let query = require_flag(&args, "query");
                let mode = opt_flag(&args, "mode").unwrap_or_else(|| "hybrid".to_string());
                let limit = opt_flag(&args, "limit")
                    .map(|s| s.parse::<u32>())
                    .transpose()
                    .map_err(|e| IntelError::Invalid(format!("limit parse: {e}")))?
                    .unwrap_or(10);
                let lambda = opt_flag(&args, "mmr-lambda")
                    .map(|s| s.parse::<f32>())
                    .transpose()
                    .map_err(|e| IntelError::Invalid(format!("mmr-lambda parse: {e}")))?
                    .unwrap_or(intel_db::search::DEFAULT_MMR_LAMBDA);
                let mut filters = intel_db::search::SearchFilters::new(
                    &opt_flag(&args, "workspace").unwrap_or_else(|| "default".to_string()),
                );
                if let Some(t) = opt_flag(&args, "type") {
                    filters.knowledge_type =
                        Some(KnowledgeType::parse(&t).map_err(IntelError::from)?);
                }
                if args.flags.contains_key("include-unapproved") {
                    filters.approved_only = false;
                }
                match mode.as_str() {
                    "bm25" => {
                        let hits = intel_db::search::bm25_search(&pool, &filters, &query, limit)?;
                        serde_json::to_value(hits).map_err(|e| IntelError::Invalid(e.to_string()))
                    }
                    "vector" => {
                        let cfg = embed_cfg_from(&args)?;
                        let hits =
                            intel_db::search::vector_search(&pool, &cfg, &filters, &query, limit)?;
                        serde_json::to_value(hits).map_err(|e| IntelError::Invalid(e.to_string()))
                    }
                    "hybrid" => {
                        let cfg = embed_cfg_from(&args)?;
                        let hits = intel_db::search::hybrid_search(
                            &pool, &cfg, &filters, &query, limit, lambda,
                        )?;
                        serde_json::to_value(hits).map_err(|e| IntelError::Invalid(e.to_string()))
                    }
                    "rerank" => {
                        // rerank 全链（P4-3）：hybrid 检索 + 正文批量读取 +
                        // rip.rerank.text 容器 pairwise 排序（R2 6.2 Reranking）。
                        let cfg = embed_cfg_from(&args)?;
                        let mut rcfg = intel_db::search::RerankWorkerConfig::default();
                        if let Some(p) = opt_flag(&args, "rerank-manifest-dir") {
                            rcfg.manifest_dir = PathBuf::from(p);
                        }
                        rcfg.manifest_dir = std::path::absolute(&rcfg.manifest_dir)?;
                        rcfg.run_db = std::path::absolute(&rcfg.run_db)?;
                        rcfg.workspace_root = std::path::absolute(&rcfg.workspace_root)?;
                        rcfg.sqlite_migrations_dir =
                            std::path::absolute(&rcfg.sqlite_migrations_dir)?;
                        let hits = intel_db::search::rerank_search(
                            &pool, &cfg, &rcfg, &filters, &query, limit,
                        )?;
                        serde_json::to_value(hits).map_err(|e| IntelError::Invalid(e.to_string()))
                    }
                    other => Err(IntelError::Invalid(format!(
                        "unknown mode {other:?} (bm25/vector/hybrid/rerank)"
                    ))),
                }
            }
            "bench" => {
                // P4-4 检索基准（R2 6.8）：qrel 文件逐查询 × 四模式，指标行
                // JSON 输出（聚合口径由报告面计算——各查询 qrel 分母不同）。
                let qrel_path = require_flag(&args, "qrel-file");
                let k = opt_flag(&args, "k")
                    .map(|s| s.parse::<u32>())
                    .transpose()
                    .map_err(|e| IntelError::Invalid(format!("k parse: {e}")))?
                    .unwrap_or(10);
                let workspace =
                    opt_flag(&args, "workspace").unwrap_or_else(|| "default".to_string());
                let qrels: Vec<serde_json::Value> = serde_json::from_str(
                    &std::fs::read_to_string(&qrel_path)
                        .map_err(|e| IntelError::Invalid(format!("qrel read {qrel_path}: {e}")))?,
                )
                .map_err(|e| IntelError::Invalid(format!("qrel parse: {e}")))?;
                let mut queries = Vec::new();
                for q in &qrels {
                    let text = q["query"]
                        .as_str()
                        .ok_or_else(|| IntelError::Invalid("qrel query missing".to_string()))?
                        .to_string();
                    let relevant: std::collections::HashSet<String> = q["relevant"]
                        .as_array()
                        .ok_or_else(|| IntelError::Invalid("qrel relevant missing".to_string()))?
                        .iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect();
                    queries.push(intel_db::bench::BenchQuery {
                        query: text,
                        relevant,
                    });
                }
                let filters = intel_db::search::SearchFilters::new(&workspace);
                let cfg = embed_cfg_from(&args)?;
                let mut rcfg = intel_db::search::RerankWorkerConfig::default();
                if let Some(p) = opt_flag(&args, "rerank-manifest-dir") {
                    rcfg.manifest_dir = PathBuf::from(p);
                }
                rcfg.manifest_dir = std::path::absolute(&rcfg.manifest_dir)?;
                rcfg.run_db = std::path::absolute(&rcfg.run_db)?;
                rcfg.workspace_root = std::path::absolute(&rcfg.workspace_root)?;
                rcfg.sqlite_migrations_dir = std::path::absolute(&rcfg.sqlite_migrations_dir)?;
                let rows =
                    intel_db::bench::run_benchmark(&pool, &cfg, &rcfg, &filters, &queries, k)?;
                serde_json::to_value(rows).map_err(|e| IntelError::Invalid(e.to_string()))
            }
            "embed-cards" => {
                let workspace =
                    opt_flag(&args, "workspace").unwrap_or_else(|| "default".to_string());
                let mut cfg = intel_db::embed_worker::EmbedWorkerConfig::default();
                if let Some(p) = opt_flag(&args, "run-db") {
                    cfg.run_db = PathBuf::from(p);
                }
                if let Some(p) = opt_flag(&args, "manifest-dir") {
                    cfg.manifest_dir = PathBuf::from(p);
                }
                cfg.run_db = std::path::absolute(&cfg.run_db)?;
                cfg.workspace_root = std::path::absolute(&cfg.workspace_root)?;
                cfg.sqlite_migrations_dir = std::path::absolute(&cfg.sqlite_migrations_dir)?;
                cfg.manifest_dir = std::path::absolute(&cfg.manifest_dir)?;
                let outcome = intel_db::embed_worker::embed_cards(&pool, &workspace, &cfg)?;
                serde_json::to_value(outcome).map_err(|e| IntelError::Invalid(e.to_string()))
            }
            "semantic-scan" => {
                let workspace =
                    opt_flag(&args, "workspace").unwrap_or_else(|| "default".to_string());
                let threshold = opt_flag(&args, "threshold")
                    .map(|s| s.parse::<f32>())
                    .transpose()
                    .map_err(|e| IntelError::Invalid(format!("threshold parse: {e}")))?
                    .unwrap_or(intel_db::embed_worker::DEFAULT_SEMANTIC_THRESHOLD);
                let top_k = opt_flag(&args, "top-k")
                    .map(|s| s.parse::<u32>())
                    .transpose()
                    .map_err(|e| IntelError::Invalid(format!("top-k parse: {e}")))?
                    .unwrap_or(10);
                let mut cfg = intel_db::embed_worker::EmbedWorkerConfig::default();
                if let Some(p) = opt_flag(&args, "run-db") {
                    cfg.run_db = PathBuf::from(p);
                }
                if let Some(p) = opt_flag(&args, "manifest-dir") {
                    cfg.manifest_dir = PathBuf::from(p);
                }
                cfg.run_db = std::path::absolute(&cfg.run_db)?;
                cfg.workspace_root = std::path::absolute(&cfg.workspace_root)?;
                cfg.sqlite_migrations_dir = std::path::absolute(&cfg.sqlite_migrations_dir)?;
                cfg.manifest_dir = std::path::absolute(&cfg.manifest_dir)?;
                let outcome = intel_db::embed_worker::semantic_scan(
                    &pool, &workspace, threshold, top_k, &cfg,
                )?;
                serde_json::to_value(outcome).map_err(|e| IntelError::Invalid(e.to_string()))
            }
            "minhash-scan" => {
                let workspace =
                    opt_flag(&args, "workspace").unwrap_or_else(|| "default".to_string());
                let threshold = opt_flag(&args, "threshold")
                    .map(|s| s.parse::<f64>())
                    .transpose()
                    .map_err(|e| IntelError::Invalid(format!("threshold parse: {e}")))?
                    .unwrap_or(0.5);
                let num_perm = opt_flag(&args, "num-perm")
                    .map(|s| s.parse::<u32>())
                    .transpose()
                    .map_err(|e| IntelError::Invalid(format!("num-perm parse: {e}")))?
                    .unwrap_or(128);
                let mut cfg = intel_db::gov_worker::GovWorkerConfig::default();
                if let Some(p) = opt_flag(&args, "run-db") {
                    cfg.run_db = PathBuf::from(p);
                }
                if let Some(p) = opt_flag(&args, "manifest-dir") {
                    cfg.manifest_dir = PathBuf::from(p);
                }
                cfg.run_db = std::path::absolute(&cfg.run_db)?;
                cfg.workspace_root = std::path::absolute(&cfg.workspace_root)?;
                cfg.sqlite_migrations_dir = std::path::absolute(&cfg.sqlite_migrations_dir)?;
                cfg.manifest_dir = std::path::absolute(&cfg.manifest_dir)?;
                let outcome = intel_db::gov_worker::minhash_scan(
                    &pool, &workspace, threshold, num_perm, &cfg,
                )?;
                serde_json::to_value(outcome).map_err(|e| IntelError::Invalid(e.to_string()))
            }
            _ => usage(),
        }
    })();

    match result {
        Ok(value) => println!("{}", serde_json::to_string(&value).unwrap_or_default()),
        Err(IntelError::Invalid(m)) => die_json("invalid", &m),
        Err(e) => {
            if std::env::var("RP_DEBUG").as_deref() == Ok("1") {
                eprintln!("DEBUG error debug: {e:#?}");
            }
            die_json("runtime_failed", &e.to_string())
        }
    }
}
