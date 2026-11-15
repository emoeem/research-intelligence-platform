//! P4-3：Tokenizer Adapter 校准抽样（R2 6.6 + ROADMAP §5.5「校准只修常数
//! 不改结构」）。
//!
//! 判据：
//! 1. adapter 在场时 est_tokens == 真实计数（切换一致性，全局单点承载）；
//! 2. 保守公式（est_tokens_formula）对抽样语料偏差 < 5%（校准达标线）。
//!
//! 守卫：tokenizer.json 缺席（非受控环境）SKIP。

use context_broker::{est_tokens, est_tokens_formula, tokenizer_available};

/// 抽样语料（中英混排 + 代码 + 短句，覆盖检索语料主要形态）。
const SAMPLES: [&str; 6] = [
    "SQL 注入的成因是未参数化拼接用户输入，防御核心是预编译语句与参数化查询。",
    "Unified Search API merges BM25 and vector channels via Reciprocal Rank Fusion with k=60.",
    "let hits = search::hybrid_search(&pool, &cfg, &filters, query, 10, 0.7)?;",
    "混合检索先把 BM25 词面通道与 pgvector 语义通道各自取 Top-K，再按 RRF 并归排序。",
    "bge-small-zh-v1.5 is a Chinese embedding model with 512 output dimensions.",
    "XSS 防御要点：输出编码、CSP、HttpOnly Cookie。",
];

#[test]
fn adapter_switches_est_tokens_to_real_counting() {
    let dsn_dir = std::env::var("EMBEDDING_TOKENIZER_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("models/bge-small-zh-v1.5"));
    if !dsn_dir.join("tokenizer.json").is_file() {
        eprintln!("SKIP: tokenizer.json 不在位（{dsn_dir:?}），adapter 回落口径");
        return;
    }
    assert!(tokenizer_available(), "adapter 应在位");
    for text in SAMPLES {
        let real = context_broker::tokenizer_count(text).expect("adapter 在场计数必有");
        assert_eq!(est_tokens(text), real, "est_tokens 必须等于真实计数（切换一致性）");
    }
}

#[test]
fn formula_deviation_within_five_percent() {
    let dsn_dir = std::env::var("EMBEDDING_TOKENIZER_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("models/bge-small-zh-v1.5"));
    if !dsn_dir.join("tokenizer.json").is_file() {
        eprintln!("SKIP: tokenizer.json 不在位（{dsn_dir:?}），校准无对照面");
        return;
    }
    for text in SAMPLES {
        let real = context_broker::tokenizer_count(text).expect("adapter 在场计数必有") as f64;
        let formula = est_tokens_formula(text) as f64;
        let dev = (formula - real).abs() / real.max(1.0);
        assert!(
            dev < 0.05,
            "保守公式偏差 {dev:.4} ≥ 5%（real={real} formula={formula}）：需校准常数（ROADMAP §5.5）sample={text:?}"
        );
    }
}
