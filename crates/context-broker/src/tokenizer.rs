//! Model-aware prompt Tokenizer Adapter.
//!
//! `RIP_LLM_TOKENIZER_DIR` is the only accepted runtime tokenizer source for
//! Context Broker prompt budgeting. It must point to a model-matched Hugging Face
//! directory containing `tokenizer.json`. The BGE embedding tokenizer is not a
//! generation tokenizer and is deliberately not used here.
//!
//! If no model tokenizer is configured, callers use the conservative Unicode
//! estimate in `estimate.rs`; the mode/source are exposed for diagnostics.
//!
//! Thread safety: `tokenizers::Tokenizer` is Send+Sync and is shared through a
//! process-wide OnceLock. A failed first load falls back to estimation for the
//! lifetime of the process; configuration must be set before startup.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Process-wide model tokenizer adapter (loaded once from `RIP_LLM_TOKENIZER_DIR`).
static ADAPTER: OnceLock<Option<TokenizerAdapter>> = OnceLock::new();

/// HF tokenizer used only for generation-model prompt budgeting.
pub struct TokenizerAdapter {
    tokenizer: tokenizers::Tokenizer,
    source: PathBuf,
}

impl TokenizerAdapter {
    /// Load a model-matched tokenizer from a directory containing `tokenizer.json`.
    /// Truncation is disabled so long prompt counts are never silently capped.
    pub fn from_dir(dir: &Path) -> Result<Self, String> {
        let path = dir.join("tokenizer.json");
        if !path.is_file() {
            return Err(format!("tokenizer.json missing: {}", path.display()));
        }
        let mut tokenizer = tokenizers::Tokenizer::from_file(&path)
            .map_err(|e| format!("tokenizer load {}: {e}", path.display()))?;
        tokenizer
            .with_truncation(None)
            .map_err(|e| format!("tokenizer truncation reset failed: {e}"))?;
        Ok(Self {
            tokenizer,
            source: path,
        })
    }

    /// Exact tokenizer file used for this counter (diagnostic source).
    pub fn source(&self) -> &Path {
        &self.source
    }

    /// Count one text sequence without adding special tokens. Failure returns None.
    pub fn count(&self, text: &str) -> Option<u64> {
        let encoding = self.tokenizer.encode(text, false).ok()?;
        Some(encoding.get_ids().len() as u64)
    }
}

/// Load the configured model tokenizer; missing/invalid configuration returns None.
pub fn global() -> Option<&'static TokenizerAdapter> {
    ADAPTER
        .get_or_init(|| {
            let raw = std::env::var("RIP_LLM_TOKENIZER_DIR").ok()?;
            if raw.trim().is_empty() {
                return None;
            }
            TokenizerAdapter::from_dir(&PathBuf::from(raw)).ok()
        })
        .as_ref()
}

/// Whether a model-specific prompt tokenizer is available.
pub fn available() -> bool {
    global().is_some()
}

/// Model tokenizer source path, if configured and loaded.
pub fn source() -> Option<&'static Path> {
    global().map(TokenizerAdapter::source)
}

/// Counter mode for logs and diagnostics.
pub fn mode() -> &'static str {
    if available() {
        "hf_model_tokenizer"
    } else {
        "estimated_unicode_ratio_v1"
    }
}

/// Exact token count using the configured model tokenizer; None means estimated fallback.
pub fn count_tokens(text: &str) -> Option<u64> {
    global().and_then(|adapter| adapter.count(text))
}
