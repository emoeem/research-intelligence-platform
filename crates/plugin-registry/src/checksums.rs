//! 包内容校验和 checksums.sha256（PLUGIN-CONTRACT §5.1 INSTALLED 写入、
//! §5.2 install 行、§6 local_dir 校验列）。
//!
//! 格式与 GNU sha256sum 输出一致：`<64hex><空格><空格><相对路径>`，相对
//! 路径使用 `/` 分隔（跨平台可对账）。目录遍历按路径字典序产出，重复
//! write/verify 得到逐字节相同的清单（幂等）。`checksums.sha256` 与
//! `plugin.lock` 本身不入清单（前者是清单本体，后者在 checksums 之后写）。
//!
//! verify 的 fail-closed 语义：清单内 mismatch、清单内文件缺失、盘上存在
//! 但清单未列（unlisted）三类都算失败，不做「只核对已列条目」的弱校验。

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;

use tool_registry::hash::sha256_hex;

/// 校验和清单文件名（PLUGIN-CONTRACT §8 目录骨架）。
pub const CHECKSUMS_FILENAME: &str = "checksums.sha256";

/// 校验和错误（机器可读）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChecksumError {
    /// 稳定错误码：not_dir / walk_failed / read_failed / write_failed /
    /// no_checksum_file / bad_line。
    pub code: String,
    /// 人类可读原因。
    pub message: String,
}

impl ChecksumError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ChecksumError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "checksum error [{}]: {}", self.code, self.message)
    }
}

impl std::error::Error for ChecksumError {}

/// 校验失败明细（verify 返回 `Vec<ChecksumIssue>`，机器可读）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChecksumIssue {
    /// 清单内的文件与盘上内容不一致。
    Mismatch {
        /// 相对路径。
        path: String,
        /// 清单记录的 sha256。
        expected: String,
        /// 实际重算的 sha256。
        actual: String,
    },
    /// 清单内的文件在盘上不存在。
    Missing {
        /// 相对路径。
        path: String,
    },
    /// 盘上存在但清单未列出的文件。
    Unlisted {
        /// 相对路径。
        path: String,
    },
    /// 清单行格式非法。
    BadLine {
        /// 行号（1 起）。
        line: usize,
        /// 行原文。
        raw: String,
    },
}

impl std::fmt::Display for ChecksumIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChecksumIssue::Mismatch {
                path,
                expected,
                actual,
            } => write!(f, "mismatch {path}: expected {expected}, actual {actual}"),
            ChecksumIssue::Missing { path } => write!(f, "missing {path}"),
            ChecksumIssue::Unlisted { path } => write!(f, "unlisted {path}"),
            ChecksumIssue::BadLine { line, raw } => write!(f, "bad line {line}: {raw:?}"),
        }
    }
}

/// 校验和清单视图：条目（相对路径 到 sha256）与清单文件自身的 sha256
/// （plugin.lock 的包指纹来源，lock.rs 用）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChecksumManifest {
    /// 相对路径 到 sha256（字典序）。
    pub entries: BTreeMap<String, String>,
    /// checksums.sha256 文件字节内容的 sha256（写入后取得；读取路径时为
    /// 该文件重算值）。
    pub file_sha256: String,
    /// 清单文件绝对/相对路径（诊断用）。
    #[serde(skip_serializing)]
    pub file_path: PathBuf,
}

impl ChecksumManifest {
    /// 单行 JSON（机器可读）。
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"entries\":{}}".to_string())
    }
}

/// 是否跳过（清单文件与 lock 文件不参与哈希清单）。
fn skip_file(name: &str) -> bool {
    name == CHECKSUMS_FILENAME || name == crate::lock::PLUGIN_LOCK_FILENAME
}

/// 递归收集 `root` 下全部文件（相对路径 字典序），跳过清单与 lock。
fn walk(root: &Path) -> Result<Vec<PathBuf>, ChecksumError> {
    let mut out = Vec::new();
    walk_inner(root, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk_inner(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), ChecksumError> {
    let entries = fs::read_dir(dir).map_err(|e| {
        ChecksumError::new("walk_failed", format!("read_dir {:?}: {e}", dir.display()))
    })?;
    for entry in entries {
        let entry =
            entry.map_err(|e| ChecksumError::new("walk_failed", format!("dir entry: {e}")))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            walk_inner(&path, out)?;
        } else if !skip_file(&name) {
            out.push(path);
        }
    }
    Ok(())
}

fn relpath(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn sha256_file(path: &Path) -> Result<String, ChecksumError> {
    let mut file = fs::File::open(path).map_err(|e| {
        ChecksumError::new("read_failed", format!("open {:?}: {e}", path.display()))
    })?;
    let mut acc = Sha256Accumulator::default();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| {
            ChecksumError::new("read_failed", format!("read {:?}: {e}", path.display()))
        })?;
        if n == 0 {
            break;
        }
        acc.update(&buf[..n]);
    }
    Ok(acc.finish())
}

/// 分块累积器：复用 tool-registry 的 sha2 封装（RustCrypto sha2 0.10），
/// 分块读取避免句柄层重复小读；内存占用为单文件字节数，插件包文件规模
/// （worker 脚本、文档、28 MiB 级知识库）下可接受。
#[derive(Default)]
struct Sha256Accumulator {
    data: Vec<u8>,
}

impl Sha256Accumulator {
    fn update(&mut self, chunk: &[u8]) {
        self.data.extend_from_slice(chunk);
    }

    fn finish(&self) -> String {
        sha256_hex(&self.data)
    }
}

/// 计算并写入 `root/checksums.sha256`（幂等：重复调用结果逐字节相同）。
pub fn write_checksums(root: &Path) -> Result<ChecksumManifest, ChecksumError> {
    let files = walk(root)?;
    let mut entries = BTreeMap::new();
    for path in &files {
        let rel = relpath(root, path);
        let sum = sha256_file(path)?;
        entries.insert(rel, sum);
    }
    let body = render_body(&entries);
    let target = root.join(CHECKSUMS_FILENAME);
    fs::write(&target, &body).map_err(|e| {
        ChecksumError::new("write_failed", format!("write {:?}: {e}", target.display()))
    })?;
    Ok(ChecksumManifest {
        file_sha256: sha256_hex(body.as_bytes()),
        entries,
        file_path: target,
    })
}

fn render_body(entries: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    for (path, sum) in entries {
        out.push_str(sum);
        out.push_str("  ");
        out.push_str(path);
        out.push('\n');
    }
    out
}

/// 读取并校验 `root/checksums.sha256`：清单行解析、盘上重算、三类失败
/// （mismatch / missing / unlisted）全收集后一次性返回。无失败即 Ok。
pub fn verify_checksums(
    root: &Path,
) -> Result<ChecksumManifest, (ChecksumError, Vec<ChecksumIssue>)> {
    let target = root.join(CHECKSUMS_FILENAME);
    let body = match fs::read_to_string(&target) {
        Ok(b) => b,
        Err(e) => {
            return Err((
                ChecksumError::new(
                    "no_checksum_file",
                    format!("read {:?}: {e}", target.display()),
                ),
                Vec::new(),
            ))
        }
    };
    let file_sha256 = sha256_hex(body.as_bytes());
    let mut listed: BTreeMap<String, String> = BTreeMap::new();
    let mut issues = Vec::new();
    for (idx, line) in body.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        // sha256sum 格式：<64hex> + 两个空格 + 路径。按字节解析（路径可能
        // 含非 ASCII 字节，禁止在字符边界不安全处切片）。
        let bytes = line.as_bytes();
        if bytes.len() < 67
            || !bytes[..64]
                .iter()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            || &bytes[64..66] != b"  "
        {
            issues.push(ChecksumIssue::BadLine {
                line: idx + 1,
                raw: line.to_string(),
            });
            continue;
        }
        let path = String::from_utf8_lossy(&bytes[66..]).into_owned();
        let hex = String::from_utf8_lossy(&bytes[..64]).into_owned();
        listed.insert(path, hex);
    }
    // 清单内逐条重算。
    for (path, expected) in &listed {
        let full = root.join(path);
        match sha256_file(&full) {
            Ok(actual) => {
                if actual != *expected {
                    issues.push(ChecksumIssue::Mismatch {
                        path: path.clone(),
                        expected: expected.clone(),
                        actual,
                    });
                }
            }
            Err(_) => issues.push(ChecksumIssue::Missing { path: path.clone() }),
        }
    }
    // 盘上未列出的文件（fail-closed）。
    for path in walk(root).map_err(|e| (e, Vec::new()))? {
        let rel = relpath(root, &path);
        if !listed.contains_key(&rel) {
            issues.push(ChecksumIssue::Unlisted { path: rel });
        }
    }
    if !issues.is_empty() {
        return Err((
            ChecksumError::new(
                "verification_failed",
                format!("{} checksum issue(s)", issues.len()),
            ),
            issues,
        ));
    }
    Ok(ChecksumManifest {
        entries: listed,
        file_sha256,
        file_path: target,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pkg(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("p6-checksum-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("nested")).unwrap();
        fs::write(dir.join("a.txt"), b"alpha").unwrap();
        fs::write(dir.join("nested/b.bin"), vec![1u8, 2, 3, 250]).unwrap();
        dir
    }

    #[test]
    fn write_then_verify_roundtrip_and_idempotence() {
        let dir = make_pkg("roundtrip");
        let first = write_checksums(&dir).unwrap();
        assert_eq!(first.entries.len(), 2);
        assert!(first.entries.contains_key("a.txt"));
        assert!(first.entries.contains_key("nested/b.bin"));
        let second = write_checksums(&dir).unwrap();
        assert_eq!(
            first.file_sha256, second.file_sha256,
            "idempotent byte-for-byte"
        );
        verify_checksums(&dir)
            .unwrap_or_else(|(e, issues)| panic!("verify failed: {e} {issues:?}"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn tamper_detected_and_restore_recovers() {
        let dir = make_pkg("tamper");
        write_checksums(&dir).unwrap();
        fs::write(dir.join("a.txt"), b"tampered").unwrap();
        let (_, issues) = verify_checksums(&dir).unwrap_err();
        assert!(
            matches!(&issues[0], ChecksumIssue::Mismatch { path, .. } if path == "a.txt"),
            "{issues:?}"
        );
        fs::write(dir.join("a.txt"), b"alpha").unwrap();
        verify_checksums(&dir).unwrap();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_and_unlisted_detected() {
        let dir = make_pkg("missing");
        write_checksums(&dir).unwrap();
        fs::remove_file(dir.join("a.txt")).unwrap();
        fs::write(dir.join("extra.txt"), b"extra").unwrap();
        let (_, issues) = verify_checksums(&dir).unwrap_err();
        assert!(issues
            .iter()
            .any(|i| matches!(i, ChecksumIssue::Missing { path } if path == "a.txt")));
        assert!(issues
            .iter()
            .any(|i| matches!(i, ChecksumIssue::Unlisted { path } if path == "extra.txt")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn bad_line_detected() {
        let dir = make_pkg("badline");
        write_checksums(&dir).unwrap();
        fs::write(dir.join(CHECKSUMS_FILENAME), "not-a-checksum-line\n").unwrap();
        let (_, issues) = verify_checksums(&dir).unwrap_err();
        assert!(
            matches!(&issues[0], ChecksumIssue::BadLine { line: 1, .. }),
            "{issues:?}"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
