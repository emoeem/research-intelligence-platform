//! SHA-256（content_hash 计算用）。
//!
//! 依据（TOOL-CONTRACT §2.5 第 9 条）：tool_version.content_hash 存
//! Manifest 规范化 JSON 的 sha256。
//!
//! 复用优先裁决（P1-4 复核）：原为按 FIPS 180-4 手写实现（当时 vendor
//! 锁定集内无 sha crate），按工作区复用优先纪律替换为 RustCrypto
//! `sha2` 0.10 稳定线；标准测试向量保留，锁定行为与旧实现一致。

use sha2::{Digest, Sha256};

/// 计算输入字节的 SHA-256，返回小写 hex（64 字符）。
pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fips_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn long_input_matches_known_vector() {
        // 一百万个 'a'（期望值经 python3 hashlib.sha256 独立实证）。
        let data = vec![b'a'; 1_000_000];
        assert_eq!(
            sha256_hex(&data),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }
}
