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
  relate --src KC --dst KC --type T [--confidence f] [--evidence-json J] [--source S]\n\
    追加知识关系（十型；CONTRADICTS 命中即回退候选进审核，D5）\n\
  list [--status S] [--workspace W]     卡片列表（按审核态过滤）\n\
  show --card KC                        卡片详情（关系/文档/审核日志）\n\
  review --card KC --decision approve|quarantine|reject|deprecate|candidate\n\
         [--reason R] [--reviewer X]    审核决定（低质量地板 + 硬标记阻断，日志留痕）\n\
  score --card KC                       计算并回填质量分（六权重 R2 5.3 缺省）\n\
  dedup-scan [--workspace W]            D2 去重扫描（SAME_CONTENT 候选边，不自动合并）\n\
  minhash-scan [--workspace W] [--threshold 0.5] [--num-perm 128]\n\
         [--run-db P] [--manifest-dir D]  D3 近似重复扫描（rip.gov.minhash 容器，NEAR_DUPLICATE 候选边）\n\
  embed-cards [--workspace W] [--run-db P] [--manifest-dir D]\n\
    D4 卡片语义向量补齐（rip.embed.text 容器 bge 本地推理，512 维幂等 upsert）\n\
  semantic-scan [--workspace W] [--threshold 0.92] [--top-k 10]\n\
         [--run-db P] [--manifest-dir D]  D4 语义候选（pgvector 余弦近邻，NEAR_DUPLICATE 候选边禁自动合并）\n\
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
    const BOOL_FLAGS: [&str; 2] = ["verified", "ai-derived"];
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

fn main() {
    let mut argv = std::env::args().peekable();
    let _bin = argv.next();
    if argv.peek().map(|s| s == "--help" || s == "-h").unwrap_or(false) {
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
                let ktype = KnowledgeType::parse(&require_flag(&args, "type"))
                    .map_err(IntelError::from)?;
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
                    opt_flag(&args, "trust").unwrap_or_else(|| "external_import".to_string()).as_str(),
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
                    workspace_id: opt_flag(&args, "workspace").unwrap_or_else(|| "default".to_string()),
                };
                let card = card_repo::create_card(&pool, &input)?;
                let mut out = serde_json::json!({ "knowledge_id": card.as_str() });
                if let (Some(d), Some(r)) = (opt_flag(&args, "document"), opt_flag(&args, "revision")) {
                    let document = domain::DocumentId::try_new(d)?;
                    let revision = domain::DocumentRevisionId::try_new(r)?;
                    card_repo::link_document(&pool, &document, &revision, &card)?;
                    out["linked_document"] = serde_json::json!(document.as_str());
                    out["linked_revision"] = serde_json::json!(revision.as_str());
                }
                Ok(out)
            }
            "relate" => {
                let src = KnowledgeCardId::try_new(require_flag(&args, "src"))?;
                let dst = KnowledgeCardId::try_new(require_flag(&args, "dst"))?;
                let rtype = RelationType::parse(&require_flag(&args, "type")).map_err(IntelError::from)?;
                let confidence = opt_flag(&args, "confidence")
                    .map(|s| s.parse::<f32>())
                    .transpose()
                    .map_err(|e| IntelError::Invalid(format!("confidence parse: {e}")))?
                    .unwrap_or(0.8);
                let evidence = opt_flag(&args, "evidence-json")
                    .map(|raw| serde_json::from_str::<serde_json::Value>(&raw))
                    .transpose()
                    .map_err(|e| IntelError::Invalid(format!("evidence-json parse: {e}")))?;
                let source = opt_flag(&args, "source").unwrap_or_else(|| "manual:rp-kc".to_string());
                let (rid, created) =
                    card_repo::add_relation(&pool, &src, &dst, rtype, evidence.as_ref(), &source, confidence)?;
                Ok(serde_json::json!({
                    "relation_id": rid.as_str(),
                    "created": created,
                    "relation_type": rtype.as_str(),
                }))
            }
            "list" => {
                let workspace = opt_flag(&args, "workspace").unwrap_or_else(|| "default".to_string());
                let status = match opt_flag(&args, "status") {
                    Some(raw) => Some(ReviewStatus::parse(&raw).map_err(IntelError::from)?),
                    None => None,
                };
                let items = card_repo::list_by_review(&pool, &workspace, status)?;
                Ok(serde_json::json!({ "items": items }))
            }
            "show" => {
                let card = KnowledgeCardId::try_new(require_flag(&args, "card"))?;
                let detail = card_repo::get_card(&pool, &card)?;
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
            "dedup-scan" => {
                let workspace = opt_flag(&args, "workspace").unwrap_or_else(|| "default".to_string());
                let created = card_repo::dedup_scan_same_content(&pool, &workspace)?;
                Ok(serde_json::json!({ "workspace": workspace, "same_content_relations_created": created }))
            }
            "embed-cards" => {
                let workspace = opt_flag(&args, "workspace").unwrap_or_else(|| "default".to_string());
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
                let workspace = opt_flag(&args, "workspace").unwrap_or_else(|| "default".to_string());
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
                let outcome =
                    intel_db::embed_worker::semantic_scan(&pool, &workspace, threshold, top_k, &cfg)?;
                serde_json::to_value(outcome).map_err(|e| IntelError::Invalid(e.to_string()))
            }
            "minhash-scan" => {
                let workspace = opt_flag(&args, "workspace").unwrap_or_else(|| "default".to_string());
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
                let outcome = intel_db::gov_worker::minhash_scan(&pool, &workspace, threshold, num_perm, &cfg)?;
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
