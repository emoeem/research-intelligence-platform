//! 验证去重指纹（P5-2 契约面 + P5-4 Planner 消费）。
//!
//! 口径（补充计划 §5.8 缓存键组成）：指纹 = sha256(规范化拼接串)，拼接顺序
//! target|endpoint|poc|provider|params|authctx，各段以 `\u{1f}`（单元分隔符）
//! 分隔避免段内容粘连歧义；authctx 只存不可逆指纹（调用方传入的即指纹串，
//! 本函数不做明文落库）。任何一段变化都会改变指纹（P5-4 缓存失效判据）。

use sha2::{Digest, Sha256};

/// 去重指纹：sha256(六段规范化拼接)，64 位小写 hex。
///
/// 各段语义与来源（P5-4 Planner 承载规范化）：
/// - `target`：规范化目标（Asset Dedup 输出，host/scheme/port）；
/// - `endpoint`：规范化端点（可空，路径+方法+参数形态）；
/// - `poc_ref`：POC/模板引用（rtis-poc:<id> 或 nuclei template id）；
/// - `provider`：provider_id + "@" + provider_version；
/// - `params`：检测参数规范化 JSON 文本（键序稳定）；
/// - `authctx`：认证上下文指纹（不可逆，缺省 `-`）。
pub fn dedup_fingerprint(
    target: &str,
    endpoint: &str,
    poc_ref: &str,
    provider: &str,
    params: &str,
    authctx: &str,
) -> String {
    let joined = [
        target,
        endpoint,
        poc_ref,
        provider,
        params,
        if authctx.is_empty() { "-" } else { authctx },
    ]
    .join("\u{1f}");
    let mut hasher = Sha256::new();
    hasher.update(joined.as_bytes());
    let digest = hasher.finalize();
    hex(&digest)
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_64_hex() {
        let a = dedup_fingerprint("host:443", "/api/x", "rtis-poc:900008", "rtis-poc@0.1.0", "{}", "-");
        let b = dedup_fingerprint("host:443", "/api/x", "rtis-poc:900008", "rtis-poc@0.1.0", "{}", "-");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn any_segment_change_changes_fingerprint() {
        let base = dedup_fingerprint("host:443", "/api/x", "rtis-poc:900008", "rtis-poc@0.1.0", "{}", "-");
        for changed in [
            dedup_fingerprint("host2:443", "/api/x", "rtis-poc:900008", "rtis-poc@0.1.0", "{}", "-"),
            dedup_fingerprint("host:443", "/api/y", "rtis-poc:900008", "rtis-poc@0.1.0", "{}", "-"),
            dedup_fingerprint("host:443", "/api/x", "rtis-poc:900009", "rtis-poc@0.1.0", "{}", "-"),
            dedup_fingerprint("host:443", "/api/x", "rtis-poc:900008", "rtis-poc@0.2.0", "{}", "-"),
            dedup_fingerprint("host:443", "/api/x", "rtis-poc:900008", "rtis-poc@0.1.0", "{\"k\":1}", "-"),
            dedup_fingerprint("host:443", "/api/x", "rtis-poc:900008", "rtis-poc@0.1.0", "{}", "authtok"),
        ] {
            assert_ne!(base, changed, "segment change must invalidate fingerprint");
        }
    }

    #[test]
    fn separator_prevents_segment_collision() {
        // "host|poc" 粘连与分段不同的两条指纹必须不同。
        let a = dedup_fingerprint("host:1", "x", "poc:1", "p@1", "-", "-");
        let b = dedup_fingerprint("host:1x", "poc:1", "p@1", "-", "-", "-");
        // 参数段不同（- vs poc:1 已在段内），join 结果不同。
        assert_ne!(a, b);
    }
}
