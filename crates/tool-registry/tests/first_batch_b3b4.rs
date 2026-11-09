//! P1-7c 验收（ROADMAP §2.3 P1-7c 判据原句，TOOL-CONTRACT §10.2 B3/B4 行）：
//! B3：T01 worker runtime 以 Registry 元数据条目登记可见（不暴露为可调用
//! 工具：登记后置 lifecycle=DISABLED，001 词表内的不可运行态）；
//! B4：T08 rtk 五子命令 Manifest 逐一过 P1-4 校验九条并登记。
//! 前置：rtk pytest 套件已在项目内临时 venv 补跑通过（V-2 关闭，
//! 27 passed，见各 Manifest provenance.tested 的标记断言）。
//! toolkit 本体零修改：外仓 git status 对受保护源文件必须零输出。
//! 开发库副本：migrations/dev-b34-b3.db 与 dev-b34-b4.db（*.db 已 gitignore，
//! 每次重建；两测试并行各用独立副本，互不冲突）。

use database::{migrate, DbPath, Pool, PoolConfig};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tool_registry::Registry;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn workspace_root() -> PathBuf {
    repo_root().parent().expect("workspace root").to_path_buf()
}

fn reset_dev_copy(name: &str) {
    let base = repo_root().join("migrations").join(name);
    for suffix in ["", "-wal", "-shm"] {
        let mut p = base.clone().into_os_string();
        p.push(suffix);
        let p = PathBuf::from(p);
        if p.exists() {
            std::fs::remove_file(&p).unwrap_or_else(|e| panic!("remove {}: {e}", p.display()));
        }
    }
}

fn fresh_registry(db_file: &str) -> Registry {
    reset_dev_copy(db_file);
    let pool = Pool::open(PoolConfig {
        path: DbPath::File(repo_root().join("migrations").join(db_file)),
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .expect("open dev copy pool");
    migrate(&pool, &repo_root().join("migrations")).expect("migrations 001..005");
    Registry::open(pool)
}

fn load_manifest(name: &str) -> Value {
    let text = std::fs::read_to_string(
        repo_root()
            .join("manifests/tools")
            .join(format!("{name}.json")),
    )
    .unwrap();
    serde_json::from_str(&text).unwrap()
}

#[test]
fn b3_t01_metadata_entry_visible_but_not_dispatchable() {
    let reg = fresh_registry("dev-b34-b3.db");
    let t01 = load_manifest("rip.ops.worker-runtime");
    let r = reg.register(&t01).unwrap();
    assert_eq!(r.tool_id, "rip.ops.worker-runtime");
    assert_eq!(r.version, "0.0.0");

    // B3 口径：登记可见（discover 列出），随后置 DISABLED（不可派发）。
    let found = reg.discover().unwrap();
    assert!(found.iter().any(|t| t.tool_id == "rip.ops.worker-runtime"));

    reg.pool()
        .with_conn(|c| {
            c.execute(
                "UPDATE tool SET lifecycle = 'DISABLED', updated_at = ?1
                 WHERE tool_id = 'rip.ops.worker-runtime'",
                ["2026-11-09T00:00:00Z"],
            )
        })
        .unwrap();

    let d = reg.query("rip.ops.worker-runtime").unwrap();
    assert_eq!(
        d.tool.lifecycle, "DISABLED",
        "metadata entry must be DISABLED"
    );
    // 元数据可见：DISABLED 后仍在 discover 面。
    let found = reg.discover().unwrap();
    assert!(found.iter().any(|t| t.tool_id == "rip.ops.worker-runtime"));
}

#[test]
fn b4_five_rtk_manifests_pass_nine_checks_and_register() {
    let reg = fresh_registry("dev-b34-b4.db");
    let five = [
        ("rip.evidence.rtk.auth", "T0"),
        ("rip.evidence.rtk.ledger", "T1"),
        ("rip.evidence.rtk.lint", "T0"),
        ("rip.evidence.rtk.mcp", "T2"),
        ("rip.evidence.rtk.scaffold", "T1"),
    ];
    for (id, permission) in five {
        let manifest = load_manifest(id);
        // V-2 关闭标记：每个 Manifest 的 tested 必须携带 pytest 实跑结论。
        let tested = manifest["provenance"]["tested"].as_str().unwrap();
        assert!(
            tested.contains("27 passed") && tested.contains("V-2"),
            "{id} provenance.tested must record the V-2 closure run: {tested}"
        );
        assert_eq!(
            manifest["permission_level"].as_str().unwrap(),
            permission,
            "{id} permission level"
        );
        reg.register(&manifest)
            .unwrap_or_else(|e| panic!("register {id} (nine checks): {e}"));
    }

    // 本副本恰 B4 五件（T01 在 b3 副本，见 b3 测试）。
    let found = reg.discover().unwrap();
    assert_eq!(found.len(), 5, "B4 = 5 entries, got {found:?}");
    assert!(!found.iter().any(|t| t.tool_id == "rip.ops.worker-runtime"));
    for (id, _) in five {
        let d = reg.query(id).unwrap();
        assert_eq!(d.current_version.as_deref(), Some("0.1.0"), "{id}");
        assert!(
            !d.versions[0]
                .provenance_json
                .as_deref()
                .unwrap_or_default()
                .is_empty(),
            "{id} provenance_json must be non-empty"
        );
        // Schema 获取面（P1-4 接口对 B4 工具同样可用）。
        let (inp, outp) = reg.schemas(id, None).unwrap();
        assert_eq!(inp, load_manifest(id)["input_schema"], "{id}");
        assert_eq!(outp, load_manifest(id)["output_schema"], "{id}");
    }
}

/// toolkit 本体零修改：外仓 git status 对受保护源（src/、pyproject.toml、
/// LICENSE）必须零输出；缓目录（.pytest_cache/__pycache__）不属本体。
#[test]
fn b4_toolkit_sources_untouched() {
    let output = Command::new("git")
        .args(["status", "--porcelain", "--", "toolkit/"])
        .current_dir(workspace_root())
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    for line in stdout.lines() {
        let touched = ["toolkit/src/", "toolkit/pyproject.toml", "toolkit/LICENSE"]
            .iter()
            .any(|p| line.contains(p));
        assert!(
            !touched,
            "protected toolkit source must not appear in git status: {line:?}"
        );
    }
    // 受保护源文件在位（防测试空转）。
    for p in [
        "toolkit/src/rtk/cli.py",
        "toolkit/src/rtk/auth.py",
        "toolkit/pyproject.toml",
        "toolkit/LICENSE",
    ] {
        assert!(workspace_root().join(p).is_file(), "missing: {p}");
    }
}
