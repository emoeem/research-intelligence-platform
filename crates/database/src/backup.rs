//! 数据库备份与恢复（ROADMAP §11.3 P10-5 / SESSION-HANDOFF §十 backup/restore）。
//!
//! 三段式语义，全部走成熟库不手写轮子：
//!
//! 1. 快照：rusqlite `backup` feature（Online Backup API）把源库按页复制出
//!    WAL 一致的独立快照文件。源连接以只读打开（保证对源库零写入，红线
//!    ROADMAP §13）；目标快照写进归档同目录的临时文件。
//! 2. 归档：tar 容器（已 vendor 的 `tar` 0.4，与 plugin-registry P6-4 同款
//!    无压缩口径）。成员：库快照（[`DB_MEMBER`]）、任意附加文件（`extra/`
//!    前缀，备份范围含 rip-data / Artifact 存储时逐文件加入）、以及
//!    [`MANIFEST_MEMBER`]（JSON：逐成员 sha256 + 尺寸 + user_version）。
//!    压缩面（zstd 等）本批不引入：归档完整性由 sha256 承担，体积优化留
//!    P10-5 收口批次按需增量 vendor。
//! 3. 恢复：解包到目标同目录暂存目录，逐成员校验 sha256 与尺寸，再对库
//!    快照跑 `PRAGMA integrity_check` 与 user_version 对账，全部通过后才
//!    原子 rename 替换目标库（并清掉旧 `-wal`/`-shm`/`-journal` 侧车）。
//!    恢复是离线操作契约：目标库不应有活跃写入方。

use crate::error::{DbError, DbResult};
use data_encoding::HEXLOWER;
use rusqlite::OpenFlags;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 归档内库快照成员名（固定逻辑路径，与源库文件名无关）。
pub const DB_MEMBER: &str = "db/platform.db";

/// 归档内清单成员名。
pub const MANIFEST_MEMBER: &str = "manifest.json";

/// 归档格式版本（结构变更时递增；恢复端拒绝更高版本）。
pub const ARCHIVE_FORMAT_VERSION: u32 = 1;

/// 附加文件在归档内的成员前缀。
pub const EXTRA_PREFIX: &str = "extra";

/// 备份清单（归档内 [`MANIFEST_MEMBER`] 的 JSON 内容）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    /// 归档格式版本。
    pub format_version: u32,
    /// 创建时刻（Unix 毫秒；database 层零外部依赖，不做 ISO 格式化）。
    pub created_at_unix_ms: i64,
    /// 源库路径（记录用，恢复不依赖）。
    pub source_db: String,
    /// 库快照成员名。
    pub db_member: String,
    /// 库快照 sha256（小写 hex）。
    pub db_sha256: String,
    /// 库快照字节数。
    pub db_bytes: u64,
    /// 快照库的 `PRAGMA user_version`（迁移链断言用）。
    pub user_version: i64,
    /// 附加文件条目（顺序与归档内出现顺序无关，按 member 名对账）。
    pub extras: Vec<ExtraEntry>,
}

/// 一个附加文件的清单条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtraEntry {
    /// 归档内成员名（`extra/<文件名>`）。
    pub member: String,
    /// 备份时的原始路径（记录用）。
    pub original_path: String,
    /// 内容 sha256（小写 hex）。
    pub sha256: String,
    /// 字节数。
    pub bytes: u64,
}

/// 备份结果报告。
#[derive(Debug, Clone, Serialize)]
pub struct BackupReport {
    /// 归档文件路径。
    pub archive_path: PathBuf,
    /// 归档文件字节数。
    pub archive_bytes: u64,
    /// 库快照 sha256（小写 hex）。
    pub db_sha256: String,
    /// 库快照字节数。
    pub db_bytes: u64,
    /// 快照库 `PRAGMA user_version`。
    pub user_version: i64,
    /// 附加文件个数。
    pub extra_count: usize,
}

/// 恢复结果报告。
#[derive(Debug, Clone, Serialize)]
pub struct RestoreReport {
    /// 目标库路径（恢复后的文件）。
    pub dest_db: PathBuf,
    /// 恢复后库文件 sha256（应等于清单 `db_sha256`）。
    pub db_sha256: String,
    /// 恢复后库 `PRAGMA user_version`。
    pub user_version: i64,
    /// `PRAGMA integrity_check` 结果（应为 "ok"）。
    pub integrity: String,
    /// 落盘的附加文件个数（未指定 extras 输出目录则为 0）。
    pub extras_restored: usize,
}

/// 一次性暂存目录守卫：Drop 时递归删除；`defuse` 后保留（恢复成功路径）。
struct TempDirGuard {
    path: PathBuf,
    armed: bool,
}

impl TempDirGuard {
    fn new(path: PathBuf) -> DbResult<Self> {
        std::fs::create_dir_all(&path)?;
        Ok(Self { path, armed: true })
    }

    fn defuse(&mut self) {
        self.armed = false;
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

/// 单文件守卫：Drop 时删除临时文件；`defuse` 后保留。
struct TempFileGuard {
    path: PathBuf,
    armed: bool,
}

impl TempFileGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn defuse(&mut self) {
        self.armed = false;
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// 解包校验后的暂存态（库快照 + extras 均已通过哈希与完整性校验）。
struct StagedArchive {
    guard: TempDirGuard,
    manifest: BackupManifest,
    db_staged: PathBuf,
    extras: Vec<(ExtraEntry, PathBuf)>,
}

/// 文件 sha256（流式读取，小写 hex）。
fn sha256_file(path: &Path) -> DbResult<(String, u64)> {
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut bytes = 0u64;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        bytes += n as u64;
    }
    Ok((HEXLOWER.encode(&hasher.finalize()), bytes))
}

/// 备份：在线快照 + tar 归档 + manifest。
///
/// `extra_files` 逐个以 `extra/<文件名>` 成员加入（重名或非普通文件报错）；
/// 备份范围覆盖 rip-data / Artifact 存储时由调用方逐文件列出（目录遍历归
/// CLI/脚本层，库层保持单文件语义）。
pub fn backup_to_archive(
    source_db: &Path,
    archive_path: &Path,
    extra_files: &[PathBuf],
) -> DbResult<BackupReport> {
    if !source_db.is_file() {
        return Err(DbError::Backup(format!(
            "source db not found: {}",
            source_db.display()
        )));
    }
    if let Some(parent) = archive_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // 1) Online Backup API 快照：源只读打开（对源库零写入），目标临时文件。
    let snapshot_tmp = archive_path.with_extension(format!("snapshot-{}.tmp", std::process::id()));
    let mut snap_guard = TempFileGuard::new(snapshot_tmp.clone());
    let src = rusqlite::Connection::open_with_flags(
        source_db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    src.busy_timeout(Duration::from_secs(30))?;
    {
        let mut dst = rusqlite::Connection::open(&snapshot_tmp)?;
        dst.busy_timeout(Duration::from_secs(30))?;
        let b = rusqlite::backup::Backup::new(&src, &mut dst)?;
        b.run_to_completion(64, Duration::from_millis(5), None)?;
        drop(b);
        drop(dst); // 显式落盘（关连接刷尾页）
    }
    let (db_sha256, db_bytes) = sha256_file(&snapshot_tmp)?;
    let user_version: i64 = {
        let snap = rusqlite::Connection::open_with_flags(
            &snapshot_tmp,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        snap.query_row("PRAGMA user_version", [], |r| r.get(0))?
    };

    // 2) 附加文件登记：basename 进 extra/ 前缀，重名即拒绝。
    let mut extras = Vec::new();
    for p in extra_files {
        if !p.is_file() {
            return Err(DbError::Backup(format!(
                "extra file not found or not a regular file: {}",
                p.display()
            )));
        }
        let name = p.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
            DbError::Backup(format!("extra file name not utf-8: {}", p.display()))
        })?;
        let member = format!("{EXTRA_PREFIX}/{name}");
        if extras.iter().any(|e: &ExtraEntry| e.member == member) {
            return Err(DbError::Backup(format!(
                "duplicate extra file name: {name}"
            )));
        }
        let (sha, bytes) = sha256_file(p)?;
        extras.push(ExtraEntry {
            member,
            original_path: p.display().to_string(),
            sha256: sha,
            bytes,
        });
    }

    let manifest = BackupManifest {
        format_version: ARCHIVE_FORMAT_VERSION,
        created_at_unix_ms: unix_ms(),
        source_db: source_db.display().to_string(),
        db_member: DB_MEMBER.to_string(),
        db_sha256: db_sha256.clone(),
        db_bytes,
        user_version,
        extras: extras.clone(),
    };
    let manifest_json = serde_json::to_vec_pretty(&manifest)
        .map_err(|e| DbError::Backup(format!("manifest serialize: {e}")))?;

    // 3) tar 归档：库快照 + extras + manifest。
    let file = std::fs::File::create(archive_path)?;
    let mut builder = tar::Builder::new(file);
    builder
        .append_path_with_name(&snapshot_tmp, DB_MEMBER)
        .map_err(|e| DbError::Backup(format!("tar append db: {e}")))?;
    for e in &extras {
        let original = Path::new(&e.original_path);
        builder
            .append_path_with_name(original, &e.member)
            .map_err(|e| DbError::Backup(format!("tar append extra: {e}")))?;
    }
    let mut header = tar::Header::new_gnu();
    header.set_size(manifest_json.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder
        .append_data(
            &mut header,
            MANIFEST_MEMBER,
            std::io::Cursor::new(&manifest_json),
        )
        .map_err(|e| DbError::Backup(format!("tar append manifest: {e}")))?;
    let archive_file = builder
        .into_inner()
        .map_err(|e| DbError::Backup(format!("tar finish: {e}")))?;
    let archive_bytes = archive_file.metadata()?.len();
    snap_guard.defuse(); // 成功路径：快照已入档，临时文件清理失败不影响交付
    let _ = std::fs::remove_file(&snapshot_tmp);

    Ok(BackupReport {
        archive_path: archive_path.to_path_buf(),
        archive_bytes,
        db_sha256,
        db_bytes,
        user_version,
        extra_count: extras.len(),
    })
}

/// 解包归档到暂存目录并全量校验（成员名安全、逐成员 sha256/尺寸、清单
/// 版本、库快照 integrity_check 与 user_version 对账）。
///
/// 暂存守卫 Drop 即清暂存；调用方在成功路径 `defuse`。
fn extract_and_verify(archive_path: &Path) -> DbResult<StagedArchive> {
    if !archive_path.is_file() {
        return Err(DbError::Restore(format!(
            "archive not found: {}",
            archive_path.display()
        )));
    }
    let staging =
        archive_path.with_extension(format!("restore-{}-{}.tmp", std::process::id(), unix_ms()));
    let guard = TempDirGuard::new(staging.clone())?;
    let ar = std::fs::File::open(archive_path)?;
    let mut archive = tar::Archive::new(ar);
    let mut staged: Vec<String> = Vec::new();
    for entry in archive
        .entries()
        .map_err(|e| DbError::Restore(format!("tar read: {e}")))?
    {
        let mut entry = entry.map_err(|e| DbError::Restore(format!("tar entry: {e}")))?;
        let raw = entry
            .path()
            .map_err(|e| DbError::Restore(format!("tar path: {e}")))?
            .to_path_buf();
        // 成员名安全：拒绝绝对路径与 .. 越界（归档属不可信输入）。
        if raw.is_absolute()
            || raw
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(DbError::Restore(format!(
                "unsafe archive member path: {}",
                raw.display()
            )));
        }
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let flat = flatten_member(&raw)?;
        let out = staging.join(&flat);
        let mut f = std::fs::File::create(&out)?;
        std::io::copy(&mut entry, &mut f)?;
        staged.push(flat);
    }
    if !staged.iter().any(|m| m == MANIFEST_MEMBER) {
        return Err(DbError::Restore(format!(
            "archive missing {MANIFEST_MEMBER}"
        )));
    }
    let manifest_raw = std::fs::read(staging.join(MANIFEST_MEMBER))?;
    let manifest: BackupManifest = serde_json::from_slice(&manifest_raw)
        .map_err(|e| DbError::Restore(format!("manifest parse: {e}")))?;
    if manifest.format_version > ARCHIVE_FORMAT_VERSION {
        return Err(DbError::Restore(format!(
            "archive format {} newer than supported {}",
            manifest.format_version, ARCHIVE_FORMAT_VERSION
        )));
    }

    // 库快照校验：成员在场 + sha256 + 尺寸 + integrity_check + user_version。
    let db_flat = flatten_member(Path::new(&manifest.db_member))?;
    if !staged.contains(&db_flat) {
        return Err(DbError::Restore(format!(
            "archive missing db member {}",
            manifest.db_member
        )));
    }
    let db_staged = staging.join(&db_flat);
    let (sha, bytes) = sha256_file(&db_staged)?;
    if sha != manifest.db_sha256 {
        return Err(DbError::Restore(format!(
            "db member sha256 mismatch: manifest {} actual {sha}",
            manifest.db_sha256
        )));
    }
    if bytes != manifest.db_bytes {
        return Err(DbError::Restore(format!(
            "db member size mismatch: manifest {} actual {bytes}",
            manifest.db_bytes
        )));
    }
    let snap = rusqlite::Connection::open_with_flags(
        &db_staged,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let integrity: String = snap.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    if integrity != "ok" {
        return Err(DbError::Restore(format!(
            "staged snapshot integrity_check: {integrity}"
        )));
    }
    let user_version: i64 = snap.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if user_version != manifest.user_version {
        return Err(DbError::Restore(format!(
            "snapshot user_version {user_version} != manifest {}",
            manifest.user_version
        )));
    }

    // extras 逐成员校验（清单声明制：归档里多出的成员不拦截，缺失或哈希
    // 不符必拦）。
    let mut verified_extras = Vec::new();
    for e in &manifest.extras {
        let flat = flatten_member(Path::new(&e.member))?;
        let p = staging.join(&flat);
        if !staged.contains(&flat) {
            return Err(DbError::Restore(format!(
                "archive missing extra member {}",
                e.member
            )));
        }
        let (sha, bytes) = sha256_file(&p)?;
        if sha != e.sha256 || bytes != e.bytes {
            return Err(DbError::Restore(format!(
                "extra member {} sha256/size mismatch",
                e.member
            )));
        }
        verified_extras.push((e.clone(), p));
    }
    Ok(StagedArchive {
        guard,
        manifest,
        db_staged,
        extras: verified_extras,
    })
}

/// 校验归档（不做恢复替换）：清单逐成员哈希 + 库快照完整性全过即返回清单。
pub fn verify_archive(archive_path: &Path) -> DbResult<BackupManifest> {
    let mut staged = extract_and_verify(archive_path)?;
    staged.guard.defuse(); // 校验完成，暂存留给 Drop 清理
    Ok(staged.manifest)
}

/// 恢复：校验通过后原子替换目标库。
///
/// - `extras_out`：`Some(dir)` 时把已校验 extras 逐个复制进该目录（按
///   `extra/<文件名>` 的文件名落盘）；`None` 则只恢复库文件。
/// - 目标库旧 `-wal`/`-shm`/`-journal` 侧车在替换成功后删除（恢复是离线
///   操作契约：目标库不应有活跃写入方，见模块文档）。
pub fn restore_from_archive(
    archive_path: &Path,
    dest_db: &Path,
    extras_out: Option<&Path>,
) -> DbResult<RestoreReport> {
    let mut staged = extract_and_verify(archive_path)?;
    if let Some(parent) = dest_db.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // 同目录同文件系统临时名 + rename 原子替换。
    let dest_tmp = dest_db.with_extension(format!("restore-{}.tmp", std::process::id()));
    std::fs::copy(&staged.db_staged, &dest_tmp)?;
    std::fs::rename(&dest_tmp, dest_db)?;
    // 旧侧车属于被替换的旧主文件，必须清掉防止新主文件被旧 WAL 污染。
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut sidecar = dest_db.as_os_str().to_os_string();
        sidecar.push(suffix);
        let sidecar = PathBuf::from(sidecar);
        if sidecar.exists() {
            std::fs::remove_file(&sidecar)?;
        }
    }

    // extras 落盘（复制语义，逐文件 sha256 已在解包校验过）。
    let mut extras_restored = 0usize;
    if let Some(dir) = extras_out {
        std::fs::create_dir_all(dir)?;
        for (e, staged_path) in &staged.extras {
            let name = e.member.rsplit('/').next().unwrap_or(&e.member).to_string();
            std::fs::copy(staged_path, dir.join(&name))?;
            extras_restored += 1;
        }
    }
    staged.guard.defuse();
    Ok(RestoreReport {
        dest_db: dest_db.to_path_buf(),
        db_sha256: staged.manifest.db_sha256,
        user_version: staged.manifest.user_version,
        integrity: "ok".to_string(),
        extras_restored,
    })
}

/// tar 成员路径展平成暂存文件名（`/` -> `__`），已在上游做安全校验。
fn flatten_member(p: &Path) -> DbResult<String> {
    let parts: Vec<String> = p
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => Some(s.to_string_lossy().to_string()),
            _ => None,
        })
        .collect();
    if parts.is_empty() {
        return Err(DbError::Restore(format!(
            "unsafe archive member path: {}",
            p.display()
        )));
    }
    Ok(parts.join("__"))
}

/// Unix 毫秒（manifest 时间戳用）。
fn unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatten_member_rejects_empty_and_normalizes() {
        assert_eq!(
            flatten_member(Path::new("db/platform.db")).unwrap(),
            "db__platform.db"
        );
        assert!(flatten_member(Path::new("")).is_err());
    }

    #[test]
    fn backup_rejects_missing_source() {
        let err = backup_to_archive(Path::new("/nonexistent/x.db"), Path::new("/tmp/x.tar"), &[]);
        assert!(matches!(err, Err(DbError::Backup(_))));
    }
}
