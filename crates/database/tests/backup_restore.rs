//! P10-1 集成测试：backup()/restore() 真实实演（ROADMAP §11.3 P10-5 判据
//! 「备份到恢复的 roundtrip 演练通过，数据一致性核对」）。
//!
//! 载具：真实 migrations/（001..019）在临时目录开发副本上执行，生产
//! migrations/platform*.db 零接触。三个用例：
//! 1. roundtrip + 受控破坏副本恢复（破坏 -> 恢复 -> 行数/内容/sha256 对账）；
//! 2. 归档被篡改（成员字节翻转）必须拒绝（sha256 mismatch）；
//! 3. 归档缺库快照成员必须拒绝（成员在场守卫）。

use database::{
    backup_to_archive, migrate, restore_from_archive, verify_archive, DbPath, Pool, PoolConfig,
};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn tmp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("rip-backup-test-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn migrations_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../migrations")
        .canonicalize()
        .expect("migrations dir must exist")
}

fn writable(path: &Path) -> Pool {
    Pool::open(PoolConfig {
        path: DbPath::File(path.to_path_buf()),
        readonly: false,
        max_connections: 2,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap()
}

fn readonly(path: &Path) -> Pool {
    Pool::open(PoolConfig::readonly_file(path)).unwrap()
}

/// 用真实迁移链建开发副本 + 三行 resource 样本（含中文描述，保真比对用）。
fn build_db(path: &Path) {
    let pool = writable(path);
    let applied = migrate(&pool, &migrations_dir()).unwrap();
    assert_eq!(
        applied.last().map(|m| m.version),
        Some(19),
        "真实迁移链 001..019 应全应用"
    );
    pool.with_conn(|c| {
        c.execute_batch(
            "INSERT INTO resource (resource_id, identity, canonical_url, resource_type, name,
               description, lifecycle, governance, source_system, created_at, updated_at)
             VALUES
               ('gh:p10/tool-a', 'p10a', 'https://github.com/p10/tool-a', 'github_repo',
                'Tool A', 'a fuzzing tool P10校验样本', 'INDEXED', 'ACTIVE',
                'eri', '2026-11-14T00:00:00Z', '2026-11-14T00:00:00Z'),
               ('gh:p10/tool-b', 'p10b', 'https://github.com/p10/tool-b', 'github_repo',
                'Tool B', 'proxy scanner', 'INDEXED', 'ACTIVE',
                'eri', '2026-11-14T00:00:00Z', '2026-11-14T00:00:00Z'),
               ('gh:p10/tool-c', 'p10c', 'https://github.com/p10/tool-c', 'github_repo',
                'Tool C', NULL, 'INDEXED', 'QUARANTINED',
                'eri', '2026-11-14T00:00:00Z', '2026-11-14T00:00:00Z');",
        )
    })
    .unwrap();
}

fn resource_count(pool: &Pool) -> i64 {
    pool.with_conn(|c| c.query_row("SELECT COUNT(*) FROM resource", [], |r| r.get(0)))
        .unwrap()
}

fn sha256_of(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    let mut f = std::fs::File::open(path).unwrap();
    let mut h = Sha256::new();
    std::io::copy(&mut f, &mut h).unwrap();
    data_encoding::HEXLOWER.encode(&h.finalize())
}

#[test]
fn roundtrip_with_controlled_corruption_recovery() {
    let dir = tmp_dir("roundtrip");
    let db = dir.join("platform.db");
    build_db(&db);
    let before = readonly(&db);
    let count_before = resource_count(&before);
    assert_eq!(count_before, 3);
    let user_version_before: i64 = before
        .with_conn(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
        .unwrap();
    let sample_before: String = before
        .with_conn(|c| {
            c.query_row(
                "SELECT description FROM resource WHERE resource_id='gh:p10/tool-a'",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();

    // 备份（含一个附加文件，覆盖 ROADMAP P10-5 备份范围语义的文件维度）
    let extra = dir.join("artifact-sample.txt");
    std::fs::write(&extra, "artifact payload P10").unwrap();
    let archive = dir.join("p10.tar");
    let report = backup_to_archive(&db, &archive, std::slice::from_ref(&extra)).unwrap();
    assert_eq!(report.user_version, user_version_before);
    assert_eq!(report.extra_count, 1);
    assert!(report.db_bytes > 0, "快照非空");
    // Online Backup API 快照与源文件不必逐字节相同（目的库头部字段如
    // change counter 归位），快照等价性由恢复产物对账承载（见下方断言）。
    let manifest = verify_archive(&archive).unwrap();
    assert_eq!(manifest.db_sha256, report.db_sha256);
    assert_eq!(manifest.extras[0].sha256, sha256_of(&extra));

    // 受控破坏副本：中段覆写 8KB 垃圾（模拟位腐）
    let damaged = dir.join("damaged.db");
    std::fs::copy(&db, &damaged).unwrap();
    {
        use std::io::{Seek, SeekFrom, Write};
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .open(&damaged)
            .unwrap();
        let len = f.metadata().unwrap().len();
        f.seek(SeekFrom::Start(len / 2)).unwrap();
        f.write_all(&[0xFFu8; 8192]).unwrap();
    }
    assert_ne!(
        sha256_of(&damaged),
        report.db_sha256,
        "受控破坏后副本必须与备份快照不同"
    );

    // 恢复：校验通过后原子替换受损副本
    let extras_out = dir.join("extras-out");
    let rreport = restore_from_archive(&archive, &damaged, Some(&extras_out)).unwrap();
    assert_eq!(rreport.db_sha256, report.db_sha256);
    assert_eq!(rreport.user_version, user_version_before);
    assert_eq!(rreport.integrity, "ok");
    assert_eq!(rreport.extras_restored, 1);
    assert_eq!(
        sha256_of(&damaged),
        report.db_sha256,
        "恢复后文件应与备份快照逐字节一致"
    );

    // 数据一致性核对：行数 / user_version / 抽样内容
    let after = readonly(&damaged);
    assert_eq!(resource_count(&after), count_before);
    let user_version_after: i64 = after
        .with_conn(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(user_version_after, user_version_before);
    let sample_after: String = after
        .with_conn(|c| {
            c.query_row(
                "SELECT description FROM resource WHERE resource_id='gh:p10/tool-a'",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(sample_after, sample_before);
    let restored_extra = extras_out.join("artifact-sample.txt");
    assert_eq!(
        std::fs::read_to_string(&restored_extra).unwrap(),
        "artifact payload P10"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn restore_rejects_tampered_archive_member() {
    let dir = tmp_dir("tamper");
    let db = dir.join("platform.db");
    build_db(&db);
    let archive = dir.join("p10.tar");
    let _ = backup_to_archive(&db, &archive, &[]).unwrap();

    // 翻转库快照成员数据区内的一个字节（偏移 4096 必在首个成员数据内）
    let tampered = dir.join("tampered.tar");
    let mut bytes = std::fs::read(&archive).unwrap();
    assert!(bytes.len() > 8192);
    bytes[4096] ^= 0x01;
    std::fs::write(&tampered, &bytes).unwrap();

    let dest = dir.join("dest.db");
    std::fs::copy(&db, &dest).unwrap();
    let err = restore_from_archive(&tampered, &dest, None);
    assert!(
        matches!(err, Err(database::DbError::Restore(_))),
        "篡改归档必须被 sha256 门禁拒绝，got {err:?}"
    );
    // 拒绝后目标库不得被替换（原子性）
    assert_eq!(sha256_of(&dest), sha256_of(&db), "拒绝时目标库零改动");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn restore_rejects_archive_without_db_member() {
    let dir = tmp_dir("no-db-member");
    let db = dir.join("platform.db");
    build_db(&db);
    let report = backup_to_archive(&db, &dir.join("ok.tar"), &[]).unwrap();

    // 手工构造只有 manifest 没有库快照成员的归档
    let manifest = database::BackupManifest {
        format_version: database::ARCHIVE_FORMAT_VERSION,
        created_at_unix_ms: 0,
        source_db: db.display().to_string(),
        db_member: database::DB_MEMBER.to_string(),
        db_sha256: report.db_sha256.clone(),
        db_bytes: report.db_bytes,
        user_version: report.user_version,
        extras: vec![],
    };
    let bare = dir.join("bare.tar");
    let f = std::fs::File::create(&bare).unwrap();
    let mut b = tar::Builder::new(f);
    let json = serde_json::to_vec_pretty(&manifest).unwrap();
    let mut h = tar::Header::new_gnu();
    h.set_size(json.len() as u64);
    h.set_mode(0o644);
    h.set_cksum();
    b.append_data(
        &mut h,
        database::MANIFEST_MEMBER,
        std::io::Cursor::new(&json),
    )
    .unwrap();
    b.into_inner().unwrap();

    let dest = dir.join("dest.db");
    std::fs::copy(&db, &dest).unwrap();
    let err = restore_from_archive(&bare, &dest, None);
    assert!(
        matches!(err, Err(database::DbError::Restore(_))),
        "缺库快照成员必须被拒绝，got {err:?}"
    );
    assert_eq!(sha256_of(&dest), sha256_of(&db), "拒绝时目标库零改动");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn retention_moves_old_audit_rows_to_archive() {
    let dir = tmp_dir("retention");
    let db = dir.join("platform.db");
    build_db(&db);
    let pool = writable(&db);
    // 019 同款 DDL（真实迁移链已含 019，这里直接灌数据）
    pool.with_conn(|c| {
        c.execute_batch(
            "INSERT INTO external_api_audit (ts, event, method, path, status, auth_enabled)
             VALUES ('2020-01-01T00:00:00Z','request','GET','/old-1',200,0),
                    ('2020-05-01T00:00:00Z','request','POST','/old-2',500,0),
                    ('2099-01-01T00:00:00Z','request','GET','/new-1',200,1),
                    ('2099-02-01T00:00:00Z','request','GET','/new-2',200,1);",
        )
    })
    .unwrap();
    let archive_db = dir.join("audit-archive.db");
    let reports =
        database::run_retention(&pool, &database::builtin_policies(), &archive_db).unwrap();
    assert_eq!(reports.len(), 1);
    match &reports[0].outcome {
        database::RetentionOutcome::Archived {
            archived,
            deleted,
            rows_after,
            ..
        } => {
            assert_eq!((*archived, *deleted), (2, 2), "旧行恰两条入档");
            assert_eq!(*rows_after, 2, "新行保留");
        }
        other => panic!("expected archived, got {other:?}"),
    }
    let arc = Pool::open(PoolConfig::readonly_file(&archive_db)).unwrap();
    let arc_rows: Vec<String> = arc
        .with_conn(|c| -> rusqlite::Result<Vec<String>> {
            let mut s = c.prepare("SELECT path FROM external_api_audit ORDER BY path")?;
            let v = s
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, rusqlite::Error>>()?;
            Ok(v)
        })
        .unwrap();
    assert_eq!(arc_rows, vec!["/old-1", "/old-2"], "归档库含被归档行");
    let _ = std::fs::remove_dir_all(&dir);
}
