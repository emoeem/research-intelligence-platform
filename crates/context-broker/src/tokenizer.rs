//! Tokenizer Adapter（RIP3 P4-3，R2 6.6 Context Broker 升级项）：
//! HF tokenizers（Rust 版）加载 `bge-small-zh-v1.5/tokenizer.json` 真实计数，
//! 替换 [`crate::estimate::est_tokens`] 的字符近似口径；模型目录缺席时回落
//! est_tokens 保守公式（库 crate 不强制运行时文件，CACHE/BUDGET 语义不变）。
//!
//! 装载口径（HANDOVER P4-3 速查）：
//! - `EMBEDDING_TOKENIZER_DIR` env 优先（指向含 tokenizer.json 的目录）；
//! - 缺席回落仓库内 `models/bge-small-zh-v1.5/tokenizer.json`（相对 cwd，
//!   仓库根工作流下的自然路径）；
//! - 两者皆不可用 → None（回落保守公式，进程内不重试）。
//!
//! 线程安全：[`tokenizers::Tokenizer`] Send+Sync，OnceLock 全局共享单例；
//! encode 失败不 panic（返回 None 由调用方回落）。

use std::path::PathBuf;
use std::sync::OnceLock;

/// 缺省 tokenizer 目录（仓库内烘焙模型面，相对 cwd）。
pub const DEFAULT_TOKENIZER_DIR: &str = "models/bge-small-zh-v1.5";

/// 全局 adapter 单例（首次调用时装载；失败不重试，进程内回落保守口径）。
static ADAPTER: OnceLock<Option<TokenizerAdapter>> = OnceLock::new();

/// HF tokenizer 真实计数 adapter。
pub struct TokenizerAdapter {
    tokenizer: tokenizers::Tokenizer,
    source: PathBuf,
}

impl TokenizerAdapter {
    /// 从目录装载（目录内须有 tokenizer.json）。
    /// 显式 no_truncation：计数器语义要求全量 token 数——tokenizer.json 若
    /// 自带截断配置（如换用 reranker tokenizer），encode 会在模型输入帽处
    /// 截断导致长文本计数被低估（预算语义错误）。
    pub fn from_dir(dir: &std::path::Path) -> Result<Self, String> {
        let path = dir.join("tokenizer.json");
        if !path.is_file() {
            return Err(format!("tokenizer.json missing: {}", path.display()));
        }
        let mut tokenizer = tokenizers::Tokenizer::from_file(&path)
            .map_err(|e| format!("tokenizer load {}: {e}", path.display()))?;
        tokenizer
            .with_truncation(None)
            .map_err(|e| format!("tokenizer truncation reset failed: {e}"))?;
        Ok(Self { tokenizer, source: path })
    }

    /// 装载来源（诊断面）。
    pub fn source(&self) -> &std::path::Path {
        &self.source
    }

    /// 真实 token 计数（encode 单序列，不加 special tokens；失败 None）。
    pub fn count(&self, text: &str) -> Option<u64> {
        let encoding = self.tokenizer.encode(text, false).ok()?;
        Some(encoding.get_ids().len() as u64)
    }
}

/// 装载顺序：`EMBEDDING_TOKENIZER_DIR` env → 仓库缺省目录；任一可用即成。
/// 返回 None = 双面皆缺席（调用方回落保守公式）。
pub fn global() -> Option<&'static TokenizerAdapter> {
    ADAPTER
        .get_or_init(|| {
            let dirs = [
                std::env::var("EMBEDDING_TOKENIZER_DIR").ok().filter(|s| !s.trim().is_empty()).map(PathBuf::from),
                Some(PathBuf::from(DEFAULT_TOKENIZER_DIR)),
            ];
            for dir in dirs.into_iter().flatten() {
                match TokenizerAdapter::from_dir(&dir) {
                    Ok(adapter) => return Some(adapter),
                    Err(_) => continue,
                }
            }
            None
        })
        .as_ref()
}

/// adapter 是否可用（诊断与测试守卫面）。
pub fn available() -> bool {
    global().is_some()
}

/// 真实 token 计数（adapter 缺席 None）。
pub fn count_tokens(text: &str) -> Option<u64> {
    global().and_then(|a| a.count(text))
}
