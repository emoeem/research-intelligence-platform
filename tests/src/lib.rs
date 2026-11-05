//! M2-B ERI 行为对照测试载具。
//!
//! 提供：路径解析（可被环境变量覆盖）、eri_p2.py 子进程封装（生产库副本上跑，
//! resolve 的 usage 写入不落生产）、Rust resolve 封装、前置条件跳过逻辑。
#![deny(missing_docs)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// 对照查询组（任务指定 3 组）。
pub const KEYWORD_GROUPS: [&[&str]; 3] = [&["fuzzing"], &["payload", "bypass"], &["android"]];

/// resolve 返回上限（与 eri DEFAULT_TOP_K 一致）。
pub const LIMIT: usize = 5;

/// eri resolve JSON 输出中命中元素必备字段（MODULE_BOUNDARIES §8.4 兼容口径）。
pub const REQUIRED_HIT_FIELDS: [&str; 8] = [
    "resource_id",
    "name",
    "canonical_url",
    "quality",
    "grade",
    "license_status",
    "relevance_score",
    "relevance_reasons",
];

fn workspace_root() -> PathBuf {
    // tests crate 的 manifest 目录是 <root>/tests。
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("manifest parent")
        .to_path_buf()
}

/// eri 源目录（eri_p2.py / db/catalog.db 所在），可用 RPI_ERI_DIR 覆盖。
pub fn eri_dir() -> PathBuf {
    std::env::var("RPI_ERI_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            workspace_root()
                .join("..")
                .join("security-research-upgrade")
                .join("eri")
        })
}

/// platform dryrun 库路径，可用 RPI_PLATFORM_DB 覆盖。
pub fn platform_db() -> PathBuf {
    std::env::var("RPI_PLATFORM_DB")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            workspace_root()
                .join("migrations")
                .join("platform-dryrun.db")
        })
}

/// migrations 目录。
pub fn migrations_dir() -> PathBuf {
    workspace_root().join("migrations")
}

/// 前置条件检查：满足则 Ok，否则返回原因（测试打印 skip）。
pub fn parity_prereq() -> Result<(), String> {
    if std::env::var("RPI_SKIP_ERI_PARITY").is_ok() {
        return Err("RPI_SKIP_ERI_PARITY is set".to_string());
    }
    if !platform_db().exists() {
        return Err(format!("platform db missing: {}", platform_db().display()));
    }
    let eri = eri_dir();
    let p2 = eri.join("eri_p2.py");
    let catalog = eri.join("db").join("catalog.db");
    if !p2.exists() {
        return Err(format!("eri_p2.py missing: {}", p2.display()));
    }
    if !catalog.exists() {
        return Err(format!("catalog.db missing: {}", catalog.display()));
    }
    if which_python().is_none() {
        return Err("python3 not found on PATH".to_string());
    }
    Ok(())
}

fn which_python() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|p| p.join("python3"))
            .find(|p| p.is_file())
    })
}

static TMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// 生成唯一临时目录（不自动清理；系统临时目录自行回收）。
pub fn fresh_tmp_dir(tag: &str) -> PathBuf {
    let n = TMP_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("rp-parity-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create tmp dir");
    dir
}

/// 用 sqlite backup API 把 catalog.db 复制成一致快照（避免 WAL 半态拷贝）。
pub fn snapshot_catalog(dst_dir: &Path) -> PathBuf {
    let src = eri_dir().join("db").join("catalog.db");
    let dst = dst_dir.join("catalog.db");
    let python = which_python().expect("python3 checked by parity_prereq");
    let script = format!(
        "import sqlite3, sys\n\
         src = sqlite3.connect({src:?})\n\
         dst = sqlite3.connect({dst:?})\n\
         src.backup(dst)\n\
         dst.close(); src.close()\n",
        src = src.display().to_string(),
        dst = dst.display().to_string(),
    );
    let out = Command::new(&python)
        .arg("-I")
        .arg("-c")
        .arg(&script)
        .output()
        .expect("spawn python3 for catalog snapshot");
    assert!(
        out.status.success(),
        "catalog snapshot failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    dst
}

/// 跑 eri_p2.py resolve（子进程，--json），返回解析后的顶层 JSON。
pub fn run_eri_resolve(catalog_copy: &Path, keywords: &[&str]) -> serde_json::Value {
    let python = which_python().expect("python3 checked by parity_prereq");
    let p2 = eri_dir().join("eri_p2.py");
    let out = Command::new(&python)
        .arg("-I")
        .arg(&p2)
        .arg("resolve")
        .arg("--task-id")
        .arg("m2-parity")
        .arg("--keywords")
        .arg(keywords.join(","))
        .arg("--limit")
        .arg(LIMIT.to_string())
        .arg("--json")
        .env("ERI_DB", catalog_copy)
        .output()
        .expect("spawn python3 eri_p2.py");
    assert!(
        out.status.success(),
        "eri_p2.py resolve failed (kw={keywords:?}): stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("eri resolve stdout is not JSON (kw={keywords:?}): {e}"))
}

/// 跑 Rust resolve（in-process，只读 platform dryrun 库）。
pub fn run_rust_resolve(keywords: &[&str]) -> resource_engine::ResolveResult {
    let pool = database::Pool::open(database::PoolConfig::readonly_file(platform_db()))
        .expect("open platform dryrun db");
    let repo = resource_engine::SqliteResourceRepository::new(pool);
    let mut q =
        resource_engine::ResolveQuery::new(keywords.iter().map(|s| s.to_string()).collect());
    q.task_id = "m2-parity".to_string();
    q.limit = LIMIT;
    resource_engine::ResourceRepository::resolve(&repo, &q).expect("rust resolve")
}
