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

/// 去重指纹 v2（RIP3 Phase 5 P5-8，§5.8 缓存键八段补段）：在六段基础上
/// 追加「目标技术画像版本」与「相关知识审核状态」两段（P5-4 G 遗留：
/// 画像影响适用性的变化与知识撤回/审核状态变化必须使缓存失效）。
///
/// v1 ([`dedup_fingerprint`]) 与 v2 是两套独立键：既有存储行（020 起
/// dedup_fingerprint 列）的缓存决策口径不变；v2 供 planner 新键面切换
/// （切换后 v1/v2 值域空间独立，同条件重放跨版本视为缓存失效 = Fresh，
/// 安全侧默认）。八段拼接顺序：
/// target|endpoint|poc|provider|params|authctx|profile_version|knowledge_state
// 八段平坦签名是 §5.8 缓存键面的契约形状（逐段与计划书键面一一对应），
// 打包 struct 会破坏调用面对段语义的可读性；先例性豁免。
#[allow(clippy::too_many_arguments)]
pub fn dedup_fingerprint_v2(
    target: &str,
    endpoint: &str,
    poc_ref: &str,
    provider: &str,
    params: &str,
    authctx: &str,
    profile_version: &str,
    knowledge_state: &str,
) -> String {
    let joined = [
        target,
        endpoint,
        poc_ref,
        provider,
        params,
        if authctx.is_empty() { "-" } else { authctx },
        if profile_version.is_empty() {
            "-"
        } else {
            profile_version
        },
        if knowledge_state.is_empty() {
            "-"
        } else {
            knowledge_state
        },
    ]
    .join("\u{1f}");
    let mut hasher = Sha256::new();
    hasher.update(joined.as_bytes());
    let digest = hasher.finalize();
    hex(&digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_64_hex() {
        let a = dedup_fingerprint(
            "host:443",
            "/api/x",
            "rtis-poc:900008",
            "rtis-poc@0.1.0",
            "{}",
            "-",
        );
        let b = dedup_fingerprint(
            "host:443",
            "/api/x",
            "rtis-poc:900008",
            "rtis-poc@0.1.0",
            "{}",
            "-",
        );
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
        assert!(a
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn any_segment_change_changes_fingerprint() {
        let base = dedup_fingerprint(
            "host:443",
            "/api/x",
            "rtis-poc:900008",
            "rtis-poc@0.1.0",
            "{}",
            "-",
        );
        for changed in [
            dedup_fingerprint(
                "host2:443",
                "/api/x",
                "rtis-poc:900008",
                "rtis-poc@0.1.0",
                "{}",
                "-",
            ),
            dedup_fingerprint(
                "host:443",
                "/api/y",
                "rtis-poc:900008",
                "rtis-poc@0.1.0",
                "{}",
                "-",
            ),
            dedup_fingerprint(
                "host:443",
                "/api/x",
                "rtis-poc:900009",
                "rtis-poc@0.1.0",
                "{}",
                "-",
            ),
            dedup_fingerprint(
                "host:443",
                "/api/x",
                "rtis-poc:900008",
                "rtis-poc@0.2.0",
                "{}",
                "-",
            ),
            dedup_fingerprint(
                "host:443",
                "/api/x",
                "rtis-poc:900008",
                "rtis-poc@0.1.0",
                "{\"k\":1}",
                "-",
            ),
            dedup_fingerprint(
                "host:443",
                "/api/x",
                "rtis-poc:900008",
                "rtis-poc@0.1.0",
                "{}",
                "authtok",
            ),
        ] {
            assert_ne!(base, changed, "segment change must invalidate fingerprint");
        }
    }

    /// v2 两段键补段（P5-4 G 遗留落地）：画像版本与知识审核状态任一变化
    /// 使指纹失效；v1/v2 值域独立（同输入不同键 = 跨版本缓存失效）。
    #[test]
    fn v2_profile_and_knowledge_segments_invalidate() {
        let base = dedup_fingerprint_v2(
            "host:443",
            "/api/x",
            "rtis-poc:900008",
            "rtis-poc@0.1.0",
            "{}",
            "-",
            "profile-v1",
            "approved",
        );
        assert_eq!(
            base,
            dedup_fingerprint_v2(
                "host:443",
                "/api/x",
                "rtis-poc:900008",
                "rtis-poc@0.1.0",
                "{}",
                "-",
                "profile-v1",
                "approved",
            )
        );
        for changed in [
            dedup_fingerprint_v2(
                "host:443",
                "/api/x",
                "rtis-poc:900008",
                "rtis-poc@0.1.0",
                "{}",
                "-",
                "profile-v2",
                "approved",
            ),
            dedup_fingerprint_v2(
                "host:443",
                "/api/x",
                "rtis-poc:900008",
                "rtis-poc@0.1.0",
                "{}",
                "-",
                "profile-v1",
                "recalled",
            ),
            dedup_fingerprint_v2(
                "host:443",
                "/api/x",
                "rtis-poc:900008",
                "rtis-poc@0.1.0",
                "{}",
                "-",
                "",
                "approved",
            ),
            dedup_fingerprint_v2(
                "host:443",
                "/api/x",
                "rtis-poc:900008",
                "rtis-poc@0.1.0",
                "{}",
                "-",
                "profile-v1",
                "",
            ),
        ] {
            assert_ne!(
                base, changed,
                "profile/knowledge segment change must invalidate"
            );
        }
        // v1/v2 值域独立：同六段输入跨版本键不同（跨版本缓存失效安全侧默认）。
        let v1 = dedup_fingerprint(
            "host:443",
            "/api/x",
            "rtis-poc:900008",
            "rtis-poc@0.1.0",
            "{}",
            "-",
        );
        let v2 = dedup_fingerprint_v2(
            "host:443",
            "/api/x",
            "rtis-poc:900008",
            "rtis-poc@0.1.0",
            "{}",
            "-",
            "-",
            "-",
        );
        assert_ne!(v1, v2);
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
