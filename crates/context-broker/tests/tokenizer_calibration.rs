//! P4-3：Tokenizer Adapter 校准抽样（R2 6.6 + ROADMAP §5.5「校准只修常数
//! 不改结构」）。
//!
//! 判据：
//! 1. model tokenizer 在场时 est_tokens == 真实计数（切换一致性，全局单点承载）；
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
    let Ok(raw_dir) = std::env::var("RIP_LLM_TOKENIZER_DIR") else {
        eprintln!("SKIP: RIP_LLM_TOKENIZER_DIR 未配置，使用估算回退口径");
        return;
    };
    let dsn_dir = std::path::PathBuf::from(raw_dir);
    if !dsn_dir.join("tokenizer.json").is_file() {
        eprintln!("SKIP: tokenizer.json 不在位（{dsn_dir:?}）");
        return;
    }
    assert!(tokenizer_available(), "model tokenizer 应在位");
    for text in SAMPLES {
        let real = context_broker::tokenizer_count(text).expect("model tokenizer 在场计数必有");
        assert_eq!(
            est_tokens(text),
            real,
            "est_tokens 必须等于真实计数（切换一致性）"
        );
    }
}

#[test]
fn tokenizer_mode_and_source_are_explicit() {
    if tokenizer_available() {
        assert_eq!(context_broker::tokenizer_mode(), "hf_model_tokenizer");
        assert!(context_broker::tokenizer_source().is_some());
    } else {
        assert_eq!(
            context_broker::tokenizer_mode(),
            "estimated_unicode_ratio_v1"
        );
        assert!(context_broker::tokenizer_source().is_none());
    }
}

/// no_truncation 防御（P4-4 收口后假设 H6）：tokenizer.json 若自带截断配置，
/// encode 会在帽处截断导致长文本计数被低估；adapter 显式 no_truncation 后
/// 超过任何模型输入帽的文本计数仍为全量（>512 token 判据）。
#[test]
fn long_text_count_not_truncated_by_model_cap() {
    let Ok(raw_dir) = std::env::var("RIP_LLM_TOKENIZER_DIR") else {
        eprintln!("SKIP: RIP_LLM_TOKENIZER_DIR 未配置，无模型 tokenizer 计数面");
        return;
    };
    let dsn_dir = std::path::PathBuf::from(raw_dir);
    if !dsn_dir.join("tokenizer.json").is_file() {
        eprintln!("SKIP: tokenizer.json 不在位（{dsn_dir:?}）");
        return;
    }
    // 1000 个互异英文词 ≈ >1000 token（远超常见 512 输入帽）。
    let text: String = (0..1000).map(|i| format!("word{i} ")).collect();
    let count = context_broker::tokenizer_count(&text).expect("model tokenizer 在场计数必有");
    assert!(
        count > 512,
        "长文本计数被模型输入帽截断：count={count}（no_truncation 未生效）"
    );
    // 公式对照（1000 ASCII 词 ≈ 5000+ 字符 / 4 ≈ 1250 token 上量级一致）。
    let formula = est_tokens_formula(&text);
    assert!(
        count > 900,
        "计数与语料规模不符：count={count} formula={formula}"
    );
}
