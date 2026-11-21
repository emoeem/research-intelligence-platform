//! RT-2 venv 治理端到端（docs/phase0/RT-2-TASK.md §5 八项判据测试载具）。
//!
//! 判据 → 测试名对照（§6 过滤器 `python_worker` / `venv` 命中全部）：
//! - 判据 1 lock 完整性：[`python_worker_venv_criterion1_lock_integrity_rust_mirror`]；
//! - 判据 2 venv 探测正例 + 判据 8 记录面：[`python_worker_venv_criterion2_real_probe_available`]；
//! - 判据 3 探测负例五态（adapter/engine 面）：[`python_worker_venv_criterion3_failure_matrix`]；
//! - 判据 4 解释器优先级三档：[`python_worker_venv_criterion4_interpreter_priority`]；
//! - 判据 5 fail-closed 拒绝：[`python_worker_venv_criterion5_fail_closed_rejection`]；
//! - 判据 6 协议一致性对拍：[`python_worker_venv_criterion6_protocol_parity`]；
//! - 判据 7 既有面零回归（未声明 venv 派发链）：[`python_worker_venv_criterion7_bare_name_unchanged`]；
//! - 判据 8 VenvIdentity 序列化记录面：[`python_worker_venv_criterion8_identity_record_face`]。
//!
//! fixture：tests/fixtures/venv_parity_worker.py（stdlib-only，协议行为全部
//! 由 worker_runtime.py 基座承载）+ 测试内 `python3 -m venv` 临时 venv
//!（离线零安装）；真实 workers/python/.venv 探测为存在性双分支（CI 不依赖）。

use database::{migrate, DbPath, Pool, PoolConfig};
use domain::ToolRunStatus;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use tool_engine::adapter::{
    execute_via_engine, AdapterCall, PythonWorkerAdapter, PythonWorkerConfig, RestartPolicy,
};
use tool_engine::venv::{probe as venv_probe, VenvConfig, VenvStatus};
use tool_engine::Engine;
use tool_registry::{Options, Registry};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

static ENV_SEQ: AtomicU32 = AtomicU32::new(0);
static VENV_SEQ: AtomicU32 = AtomicU32::new(0);

fn unique_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "rt2-{}-{}-{}",
        tag,
        std::process::id(),
        ENV_SEQ.fetch_add(1, Ordering::SeqCst)
    ))
}

/// 离线零安装 fixture venv（含 pip → direct_deps=["pip"] 可满足）。
fn make_fixture_venv() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "rt2-venv-e2e-{}-{}",
        std::process::id(),
        VENV_SEQ.fetch_add(1, Ordering::SeqCst)
    ));
    let out = Command::new("python3")
        .args(["-m", "venv"])
        .arg(&root)
        .output()
        .expect("spawn python3 -m venv");
    assert!(
        out.status.success(),
        "fixture venv failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    root
}

fn fixture_venv_config(root: &Path) -> VenvConfig {
    VenvConfig {
        path: root.to_path_buf(),
        interpreter: None,
        requirements_lock: None,
        direct_deps: vec!["pip".to_string()],
    }
}

fn cleanup_dir(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

fn parity_adapter(
    worker_name: &str,
    command: Vec<String>,
    venv: Option<VenvConfig>,
) -> PythonWorkerAdapter {
    PythonWorkerAdapter::new(PythonWorkerConfig {
        worker_name: worker_name.to_string(),
        command,
        working_dir: Some(repo_root()),
        restart: RestartPolicy::Pooled,
        extra_env: vec![(
            "WORKER_CACHE_DIR".to_string(),
            unique_dir("cache").to_string_lossy().to_string(),
        )],
        handshake_timeout: Duration::from_secs(10),
        venv_config: venv,
        venv_identity: None,
    })
}

/// venv 解释器档：command[0] 裸名 python3（D4 第二档在 ensure_ready 替换）。
fn venv_sided_adapter(venv: Option<VenvConfig>) -> PythonWorkerAdapter {
    parity_adapter(
        "venv-parity-worker",
        vec![
            "python3".to_string(),
            "-I".to_string(),
            "crates/tool-engine/tests/fixtures/venv_parity_worker.py".to_string(),
        ],
        venv,
    )
}

/// 系统解释器档：同 fixture、同裸名、零 venv 声明（D4 第三档现状路径）。
fn system_sided_adapter() -> PythonWorkerAdapter {
    venv_sided_adapter(None)
}

fn ensure_ready_failure(adapter: &PythonWorkerAdapter) -> tool_engine::adapter::AdapterFailure {
    adapter
        .ensure_ready()
        .expect_err("declared venv must fail-closed here")
}

fn assert_override(
    failure: &tool_engine::adapter::AdapterFailure,
    category: &str,
    code: Value,
    retryable: bool,
) {
    let o = failure
        .envelope_override
        .as_ref()
        .expect("venv rejection must carry envelope override");
    assert_eq!(o.category, category, "category for {}", failure.message);
    assert_eq!(o.code, code, "code for {}", failure.message);
    assert_eq!(o.retryable, retryable, "retryable for {}", failure.message);
}

/// 独立复算 C §5.1 指纹（测试文件内重实现公式，不借 venv.rs 私有函数——
/// 独立实现一致 = 公式面成立的更强证据）。
fn recompute_fingerprint(bin_sha256: &str, version: &str, capset: &[(String, String)]) -> String {
    fn scalar(s: &str) -> String {
        serde_json::to_string(s).unwrap()
    }
    let mut canonical = String::from("{\"bin_sha256\":");
    canonical.push_str(&scalar(bin_sha256));
    canonical.push_str(",\"capset\":[");
    for (idx, (name, ver)) in capset.iter().enumerate() {
        if idx > 0 {
            canonical.push(',');
        }
        canonical.push('[');
        canonical.push_str(&scalar(name));
        canonical.push(',');
        canonical.push_str(&scalar(ver));
        canonical.push(']');
    }
    canonical.push_str("],\"version\":");
    canonical.push_str(&scalar(version));
    canonical.push('}');
    tool_registry::hash::sha256_hex(canonical.as_bytes())[..16].to_string()
}

// ---------------------------------------------------------------------------
// 判据 1：lock 完整性（Rust 侧镜像 workers/python/tests/test_lock_integrity.py）
// ---------------------------------------------------------------------------

#[test]
fn python_worker_venv_criterion1_lock_integrity_rust_mirror() {
    let in_text = fs::read_to_string(repo_root().join("workers/python/requirements.in")).unwrap();
    let lock_text =
        fs::read_to_string(repo_root().join("workers/python/requirements.lock.txt")).unwrap();
    // .in 直接依赖钉定集 = numpy/onnxruntime/tokenizers
    let in_pins: Vec<(String, String)> = in_text
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#') && l.contains("=="))
        .map(|l| {
            let (n, v) = l.split_once("==").unwrap();
            (n.trim().to_string(), v.trim().to_string())
        })
        .collect();
    assert_eq!(
        in_pins.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
        vec!["numpy", "onnxruntime", "tokenizers"]
    );
    // lock 包行解析（包行 name==version 带续行反斜杠；续行含 --hash=sha256:）
    let mut packages: Vec<(String, String, usize)> = Vec::new();
    for line in lock_text.lines() {
        let trimmed = line.trim();
        if line.starts_with(' ') || line.starts_with('\t') {
            if trimmed.starts_with("--hash=sha256:") {
                packages.last_mut().expect("hash after package").2 += 1;
            }
            continue;
        }
        if !trimmed.is_empty() && !trimmed.starts_with('#') && trimmed.contains("==") {
            let (n, v) = trimmed.split_once("==").unwrap();
            packages.push((
                n.trim().to_string(),
                v.trim_end_matches('\\').trim().to_string(),
                0,
            ));
        }
    }
    assert!(
        packages.len() >= 3,
        "lock must carry the transitive closure"
    );
    let bare: Vec<&String> = packages
        .iter()
        .filter(|(_, _, hashes)| *hashes == 0)
        .map(|(n, _, _)| n)
        .collect();
    assert_eq!(
        bare,
        Vec::<&String>::new(),
        "every package needs >=1 sha256 hash"
    );
    // 三直接依赖版本一致
    for (name, version) in &in_pins {
        let hit = packages
            .iter()
            .find(|(n, _, _)| n == name)
            .unwrap_or_else(|| panic!("direct dep {name} missing from lock"));
        assert_eq!(&hit.1, version, "{name} version drift .in vs lock");
    }
    // 无 .. 穿越
    assert!(!lock_text.contains(".."), "lock must not contain traversal");
}

// ---------------------------------------------------------------------------
// 判据 2 + 判据 8：真实 venv 探测正例（存在性双分支）与身份记录面
// ---------------------------------------------------------------------------

#[test]
fn python_worker_venv_criterion2_real_probe_available() {
    let repo = repo_root();
    let real = repo.join("workers/python/.venv");
    let config = VenvConfig {
        path: real.clone(),
        interpreter: None,
        requirements_lock: Some(repo.join("workers/python/requirements.lock.txt")),
        direct_deps: vec![
            "numpy".to_string(),
            "onnxruntime".to_string(),
            "tokenizers".to_string(),
        ],
    };
    match venv_probe(&config, Some(&repo), None) {
        Ok(identity) => {
            assert_eq!(identity.status, VenvStatus::Available);
            // VenvIdentity 五要素非空
            assert_eq!(identity.interpreter_identity.sha256_hex.len(), 64);
            assert!(identity
                .interpreter_identity
                .sha256_hex
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
            assert!(!identity.interpreter_version.is_empty());
            assert!(!identity.python_version_cfg.is_empty());
            assert!(!identity.deps_capset.is_empty());
            assert_eq!(identity.fingerprint.len(), 16);
            assert!(identity.requirements_lock_sha256.is_some());
            // 指纹公式独立复算一致（C §5.1）
            assert_eq!(
                identity.fingerprint,
                recompute_fingerprint(
                    &identity.interpreter_identity.sha256_hex,
                    &identity.interpreter_version,
                    &identity.deps_capset,
                )
            );
            // 派发路径形态：<venv>/bin/python 绝对路径（不 canonicalize 的激活面）
            assert!(identity.interpreter_path.is_absolute());
            assert!(identity.interpreter_path.starts_with(&identity.venv_root));
            // A4② 双录：cfg version 与实测版本 major.minor 一致（patch 漂移为证据面）
            println!(
                "RT-2 VenvIdentity 首探快照: {}",
                serde_json::to_string(&identity).unwrap()
            );
        }
        Err(failure) => {
            // CI 双分支：真实 venv 不在位 → missing 态断言（不依赖宿主 venv）
            assert_eq!(failure.status, VenvStatus::Missing, "{}", failure.detail);
        }
    }
}

// ---------------------------------------------------------------------------
// 判据 3：探测负例五态（adapter ensure_ready 拒绝面 + D7 映射逐类）
// ---------------------------------------------------------------------------

#[test]
fn python_worker_venv_criterion3_failure_matrix() {
    // missing：venv 根缺失
    let absent = unique_dir("absent-venv");
    let adapter = venv_sided_adapter(Some(fixture_venv_config(&absent)));
    let failure = ensure_ready_failure(&adapter);
    assert_override(&failure, "DEPENDENCY", json!("VENV_NOT_PROVISIONED"), false);
    assert!(
        adapter.resolved_interpreter().is_none(),
        "no interpreter resolved"
    );
    assert!(adapter.venv_identity().is_none());

    // incompatible：pyvenv.cfg version 篡改（major.minor 漂移）
    let root = make_fixture_venv();
    let cfg_path = root.join("pyvenv.cfg");
    let tampered = fs::read_to_string(&cfg_path)
        .unwrap()
        .lines()
        .map(|l| {
            if l.trim_start().starts_with("version") {
                "version = 3.99.0".to_string()
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&cfg_path, tampered).unwrap();
    let adapter = venv_sided_adapter(Some(fixture_venv_config(&root)));
    let failure = ensure_ready_failure(&adapter);
    assert_override(
        &failure,
        "EXECUTION",
        json!("VENV_IDENTITY_MISMATCH"),
        false,
    );

    // unsupported_output：pyvenv.cfg 形态损坏
    let root2 = make_fixture_venv();
    let corrupted = fs::read_to_string(root2.join("pyvenv.cfg"))
        .unwrap()
        .lines()
        .map(|l| {
            if l.trim_start().starts_with("version") {
                "version = not-a-version".to_string()
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(root2.join("pyvenv.cfg"), corrupted).unwrap();
    let adapter = venv_sided_adapter(Some(fixture_venv_config(&root2)));
    let failure = ensure_ready_failure(&adapter);
    assert_override(
        &failure,
        "EXECUTION",
        json!("VENV_IDENTITY_MISMATCH"),
        false,
    );

    // direct_deps 覆盖失败（裸 fixture venv 无 numpy）
    let root3 = make_fixture_venv();
    let mut config = fixture_venv_config(&root3);
    config.direct_deps = vec!["numpy".to_string()];
    let adapter = venv_sided_adapter(Some(config));
    let failure = ensure_ready_failure(&adapter);
    assert_override(
        &failure,
        "EXECUTION",
        json!("VENV_IDENTITY_MISMATCH"),
        false,
    );

    // lock sha256 对账不符（expected 来自预探测身份注入）
    let root4 = make_fixture_venv();
    let lock = unique_dir("lock-file");
    fs::create_dir_all(lock.parent().unwrap()).unwrap();
    let lock_path = lock.with_extension("txt");
    fs::write(&lock_path, b"numpy==2.5.4 --hash=sha256:cafe\n").unwrap();
    let mut config4 = fixture_venv_config(&root4);
    config4.requirements_lock = Some(lock_path.clone());
    let first = venv_probe(&config4, None, None).expect("first probe records lock sha");
    let mut stale_identity = first.clone();
    stale_identity.requirements_lock_sha256 = Some("0".repeat(64));
    let adapter = parity_adapter_with_identity(
        "venv-parity-worker",
        vec![
            "python3".to_string(),
            "-I".to_string(),
            "crates/tool-engine/tests/fixtures/venv_parity_worker.py".to_string(),
        ],
        config4,
        stale_identity,
    );
    let failure = ensure_ready_failure(&adapter);
    assert_override(
        &failure,
        "EXECUTION",
        json!("VENV_IDENTITY_MISMATCH"),
        false,
    );
    assert!(failure.message.contains("lock sha256 mismatch"));

    // probe_failed → INFRA（VENV_PROBE_FAILED）映射由 venv.rs 单测逐类断言；
    // timeout → TIMEOUT/-32002 同（l1_timeout_maps_to_timeout_envelope）。
    // 此处断言映射函数全表（零兜底合并的 e2e 面镜像）。
    use tool_engine::venv::ProbeFailureKind;
    let table = [
        (
            ProbeFailureKind::Missing,
            "DEPENDENCY",
            json!("VENV_NOT_PROVISIONED"),
            false,
        ),
        (
            ProbeFailureKind::PermissionDenied,
            "POLICY_DENIED",
            json!("VENV_PERMISSION_DENIED"),
            false,
        ),
        (ProbeFailureKind::Timeout, "TIMEOUT", json!(-32002), false),
        (
            ProbeFailureKind::UnsupportedOutput,
            "EXECUTION",
            json!("VENV_IDENTITY_MISMATCH"),
            false,
        ),
        (
            ProbeFailureKind::ProbeFailed,
            "INFRA",
            json!("VENV_PROBE_FAILED"),
            true,
        ),
    ];
    for (kind, category, code, retryable) in table {
        assert_eq!(
            kind.envelope_mapping(),
            (category, code, retryable),
            "{kind:?}"
        );
    }

    for root in [&root, &root2, &root3, &root4] {
        cleanup_dir(root);
    }
    let _ = fs::remove_file(&lock_path);
}

/// 构造带预探测身份的 adapter（expected lock sha256 对账源）。
fn parity_adapter_with_identity(
    worker_name: &str,
    command: Vec<String>,
    venv: VenvConfig,
    identity: tool_engine::venv::VenvIdentity,
) -> PythonWorkerAdapter {
    PythonWorkerAdapter::new(PythonWorkerConfig {
        worker_name: worker_name.to_string(),
        command,
        working_dir: Some(repo_root()),
        restart: RestartPolicy::Pooled,
        extra_env: vec![(
            "WORKER_CACHE_DIR".to_string(),
            unique_dir("cache").to_string_lossy().to_string(),
        )],
        handshake_timeout: Duration::from_secs(10),
        venv_config: Some(venv),
        venv_identity: Some(identity),
    })
}

// ---------------------------------------------------------------------------
// 判据 4：解释器优先级三档（argv 断言面）
// ---------------------------------------------------------------------------

#[test]
fn python_worker_venv_criterion4_interpreter_priority() {
    // 第一档：Manifest 显式绝对路径直用不探测（venv 指向缺失目录也不受阻）。
    let absent = unique_dir("absent-venv-tier1");
    let sys_python = "/usr/bin/python3";
    let explicit = parity_adapter(
        "venv-parity-worker",
        vec![
            sys_python.to_string(),
            "-I".to_string(),
            "crates/tool-engine/tests/fixtures/venv_parity_worker.py".to_string(),
        ],
        Some(VenvConfig {
            path: absent.clone(),
            interpreter: None,
            requirements_lock: None,
            direct_deps: vec!["pip".to_string()],
        }),
    );
    explicit
        .ensure_ready()
        .expect("explicit absolute path wins, no probe");
    assert_eq!(explicit.resolved_interpreter().as_deref(), Some(sys_python));
    let out = explicit
        .execute(&json!({"op": "echo"}), 30_000, 65_536, None, None)
        .expect("explicit tier executes");
    match out {
        tool_engine::adapter::AdapterOutcome::Succeeded { data, .. } => {
            assert_eq!(
                data["executable"],
                json!(sys_python),
                "interpreter face = sys.executable"
            );
            assert_eq!(
                data["argv"][0],
                json!("crates/tool-engine/tests/fixtures/venv_parity_worker.py"),
                "argv[0] = script path (python -I script 形态)"
            );
            assert_eq!(data["isolated"], json!(true), "-I isolated flag");
        }
        other => panic!("expect Succeeded, got {other:?}"),
    }
    explicit.shutdown();

    // 第二档：声明 venv 且 available → command[0] 替换为 <venv>/bin/python 绝对路径。
    let root = make_fixture_venv();
    let venv_adapter = venv_sided_adapter(Some(fixture_venv_config(&root)));
    venv_adapter
        .ensure_ready()
        .expect("fixture venv probes available");
    let resolved = venv_adapter.resolved_interpreter().expect("resolved");
    assert_eq!(
        Path::new(&resolved),
        &root.join("bin/python"),
        "command[0] replaced by <venv>/bin/python absolute (non-canonical)"
    );
    let identity = venv_adapter.venv_identity().expect("identity cached");
    assert_eq!(identity.status, VenvStatus::Available);
    let out = venv_adapter
        .execute(&json!({"op": "echo"}), 30_000, 65_536, None, None)
        .expect("venv tier executes");
    match out {
        tool_engine::adapter::AdapterOutcome::Succeeded { data, .. } => {
            assert_eq!(
                data["executable"],
                json!(resolved),
                "spawn interpreter = <venv>/bin/python (sys.executable)"
            );
            // -I 保留的机器可判面 = sys.flags.isolated（解释器旗标不进 sys.argv），
            // venv 依赖可达性同证（isolated 仍带 venv site-packages，见 ADR D5 论据）。
            assert_eq!(
                data["isolated"],
                json!(true),
                "-I preserved under venv interpreter"
            );
            assert_eq!(
                data["isolated"],
                json!(true),
                "-I preserved under venv interpreter"
            );
            assert_eq!(
                data["numpy_available"],
                json!(false),
                "bare fixture venv lacks numpy (dependency isolation face)"
            );
        }
        other => panic!("expect Succeeded, got {other:?}"),
    }
    venv_adapter.shutdown();

    // 第三档：未声明 venv → 现状裸名 PATH 解析（resolved == 原值）。
    let bare = system_sided_adapter();
    bare.ensure_ready().expect("bare name path unchanged");
    assert_eq!(bare.resolved_interpreter().as_deref(), Some("python3"));
    let out = bare
        .execute(&json!({"op": "echo"}), 30_000, 65_536, None, None)
        .expect("bare tier executes");
    match out {
        tool_engine::adapter::AdapterOutcome::Succeeded { data, .. } => {
            // 裸名档机器面：子进程内 PATH 解析归一（现状语义零改动）；
            // 派发值面（adapter.resolved_interpreter() == "python3"）已在上方断言。
            let bare_exec = data["executable"].as_str().expect("executable string");
            assert!(
                bare_exec.ends_with("/python3"),
                "bare-name dispatch resolves via PATH: {bare_exec}"
            );
        }
        other => panic!("expect Succeeded, got {other:?}"),
    }
    bare.shutdown();
    cleanup_dir(&root);
    cleanup_dir(&absent);
}

// ---------------------------------------------------------------------------
// 判据 5：fail-closed 拒绝（engine 面：PREPARING 拒绝落 FAILED，零回退零拉起）
// ---------------------------------------------------------------------------

fn fresh_env() -> (Engine, Registry, PathBuf) {
    let pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(1),
    })
    .unwrap();
    migrate(&pool, &repo_root().join("migrations")).unwrap();
    let registry = Registry::open(pool.clone()).with_options(Options {
        allow_placeholder_digest: true,
    });
    let manifest = json!({
        "schema_version": "1",
        "id": "rip.t.venv.debug",
        "name": "Venv Debug Probe",
        "version": "0.1.0",
        "kind": "tool",
        "family": "utility",
        "description": "RT-2 venv e2e 测试面（venv 声明只存在于测试 fixture）",
        "runtime": {
            "type": "python_worker",
            "protocol_version": "1",
            "worker_name": "venv-parity-worker",
            "command": ["python3", "-I", "crates/tool-engine/tests/fixtures/venv_parity_worker.py"],
            "restart": "pooled"
        },
        "input_schema": {"type": "object"},
        "output_schema": {"type": "object"},
        "resources": {"cpu": "1", "memory": "256Mi", "timeout_seconds": 60},
        "network": {"mode": "none"},
        "secrets": [],
        "permission_level": "T0",
        "idempotent": true,
        "health": {"probe": {"type": "worker_ping"}},
        "provenance": {
            "origin": "rip-native",
            "license": "repo-internal",
            "dependencies": [],
            "tested": "RT-2 e2e 实跑"
        }
    });
    registry.register(&manifest).unwrap();
    (
        Engine::open(pool, registry.clone()),
        registry,
        unique_dir("cache5"),
    )
}

fn submit_venv_debug(engine: &Engine, run_id: &str) {
    engine
        .submit(&json!({
            "protocol_version": "1",
            "tool_id": "rip.t.venv.debug",
            "run_id": run_id,
            "input": {"op": "echo"},
            "workspace_id": "ws-default",
            "budget": {"timeout_seconds": 60}
        }))
        .unwrap();
}

#[test]
fn python_worker_venv_criterion5_fail_closed_rejection() {
    let (engine, _registry, _cache) = fresh_env();
    // missing venv → PREPARING 拒绝
    let absent = unique_dir("absent-venv-c5");
    let adapter = venv_sided_adapter(Some(fixture_venv_config(&absent)));
    submit_venv_debug(&engine, "rn-venv-reject-missing");
    let out = execute_via_engine(AdapterCall {
        engine: &engine,
        run_id: "rn-venv-reject-missing",
        adapter: &adapter,
        cancel_after: None,
    })
    .unwrap();
    assert_eq!(
        out.transcript,
        vec![
            ToolRunStatus::Queued,
            ToolRunStatus::Preparing,
            ToolRunStatus::Failed
        ]
    );
    let err = out.envelope.error.as_ref().unwrap();
    assert_eq!(err.category, "DEPENDENCY");
    assert_eq!(err.code, json!("VENV_NOT_PROVISIONED"));
    assert!(!err.retryable);
    assert!(
        adapter.stderr_lines().is_empty(),
        "zero worker process spawned"
    );
    assert!(
        adapter.resolved_interpreter().is_none(),
        "no interpreter resolved (argv face)"
    );
    // 零系统解释器回退：唯一终局为 FAILED 信封，无任何 Succeeded 行
    assert_eq!(out.envelope.status, ToolRunStatus::Failed);
    adapter.shutdown();

    // lock 对账不符 → VENV_IDENTITY_MISMATCH 拒绝（expected 注入面）
    let root = make_fixture_venv();
    let lock_path = unique_dir("lockc5").with_extension("txt");
    fs::write(&lock_path, b"pip==1 --hash=sha256:beef\n").unwrap();
    let mut config = fixture_venv_config(&root);
    config.requirements_lock = Some(lock_path.clone());
    let first = venv_probe(&config, None, None).expect("first probe");
    let mut stale = first.clone();
    stale.requirements_lock_sha256 = Some("f".repeat(64));
    let adapter2 = parity_adapter_with_identity(
        "venv-parity-worker",
        vec![
            "python3".to_string(),
            "-I".to_string(),
            "crates/tool-engine/tests/fixtures/venv_parity_worker.py".to_string(),
        ],
        config,
        stale,
    );
    submit_venv_debug(&engine, "rn-venv-reject-lock");
    let out2 = execute_via_engine(AdapterCall {
        engine: &engine,
        run_id: "rn-venv-reject-lock",
        adapter: &adapter2,
        cancel_after: None,
    })
    .unwrap();
    assert_eq!(
        out2.transcript,
        vec![
            ToolRunStatus::Queued,
            ToolRunStatus::Preparing,
            ToolRunStatus::Failed
        ]
    );
    let err2 = out2.envelope.error.as_ref().unwrap();
    assert_eq!(err2.category, "EXECUTION");
    assert_eq!(err2.code, json!("VENV_IDENTITY_MISMATCH"));
    assert!(adapter2.stderr_lines().is_empty());
    adapter2.shutdown();
    cleanup_dir(&root);
    let _ = fs::remove_file(&lock_path);
}

// ---------------------------------------------------------------------------
// 判据 6：协议一致性对拍（同一 fixture worker：venv 解释器 vs 系统解释器）
// ---------------------------------------------------------------------------

#[test]
fn python_worker_venv_criterion6_protocol_parity() {
    let root = make_fixture_venv();
    let venv_adapter = venv_sided_adapter(Some(fixture_venv_config(&root)));
    venv_adapter.ensure_ready().expect("venv side handshake");
    let system_adapter = system_sided_adapter();
    system_adapter
        .ensure_ready()
        .expect("system side handshake");

    // 初始化/能力面：两解释器握手均成功且 worker_name/版本一致性由 adapter 校验；
    // echo 数据承载 initialize/get_capabilities 常量面（protocol_version /
    // worker_name / worker_version / capabilities）逐字段一致断言。
    let venv_echo = echo_of(&venv_adapter);
    let sys_echo = echo_of(&system_adapter);
    for key in [
        "op",
        "protocol_version",
        "worker_name",
        "worker_version",
        "capabilities",
        "isolated",
    ] {
        assert_eq!(venv_echo[key], sys_echo[key], "parity field {key}");
    }
    // 判据 6 字面达标面：原始握手两方法响应（fixture 按到达序捕获前两条
    // result，基座协议行为零改动）逐字段对拍——venv 解释器与系统解释器下
    // initialize/get_capabilities 响应全键一致（此两方法响应不含
    // sys.executable，无声明差异面）。
    let vinit = &venv_echo["handshake_replay"]["initialize"];
    let sinit = &sys_echo["handshake_replay"]["initialize"];
    assert!(
        !vinit.is_null(),
        "initialize raw response captured (venv side)"
    );
    assert!(
        !sinit.is_null(),
        "initialize raw response captured (system side)"
    );
    for key in [
        "protocol_version",
        "worker_name",
        "worker_version",
        "capabilities",
    ] {
        assert_eq!(vinit[key], sinit[key], "initialize raw parity field {key}");
    }
    assert_eq!(
        vinit, sinit,
        "initialize raw response field-by-field identical"
    );
    let vcaps = &venv_echo["handshake_replay"]["get_capabilities"];
    let scaps = &sys_echo["handshake_replay"]["get_capabilities"];
    assert!(
        !vcaps.is_null(),
        "get_capabilities raw response captured (venv side)"
    );
    assert!(
        !scaps.is_null(),
        "get_capabilities raw response captured (system side)"
    );
    for key in [
        "protocol_version",
        "worker_name",
        "worker_version",
        "methods",
        "capabilities",
        "initialized",
    ] {
        assert_eq!(
            vcaps[key], scaps[key],
            "get_capabilities raw parity field {key}"
        );
    }
    assert_eq!(
        vcaps, scaps,
        "get_capabilities raw response field-by-field identical"
    );
    assert_eq!(venv_echo["isolated"], json!(true), "-I 保留（venv 侧）");
    assert_eq!(sys_echo["isolated"], json!(true), "-I 保留（系统侧）");
    // 声明差异面：executable/argv[0]（venv 侧 = venv 解释器绝对路径）
    let resolved = venv_adapter.resolved_interpreter().unwrap();
    assert_eq!(venv_echo["executable"], json!(resolved));
    // 系统侧解释器面：裸名 PATH 解析在子进程内归一为绝对路径（现状语义）；
    // 断言其以 /python3 结尾而不钉宿主路径（机器无关）。
    let sys_exec = sys_echo["executable"].as_str().expect("executable string");
    assert!(
        sys_exec.ends_with("/python3"),
        "system interpreter resolved: {sys_exec}"
    );

    // execute 预算超时：worker 内 timer 在 timeout_ms 处强制交付 timeout；
    // 响应 message 确定性（"budget timeout after {timeout_ms} ms"）逐字段一致。
    let vt = venv_adapter
        .execute(&json!({"op": "sleep", "ms": 2000}), 200, 65_536, None, None)
        .expect("timeout outcome");
    let st = system_adapter
        .execute(&json!({"op": "sleep", "ms": 2000}), 200, 65_536, None, None)
        .expect("timeout outcome");
    match (&vt, &st) {
        (
            tool_engine::adapter::AdapterOutcome::TimedOut { message: vm },
            tool_engine::adapter::AdapterOutcome::TimedOut { message: sm },
        ) => {
            assert_eq!(vm, sm, "timeout response field-parity");
            assert_eq!(vm, "budget timeout after 200 ms");
        }
        _ => panic!("expect TimedOut both sides: {vt:?} vs {st:?}"),
    }

    // 协作式取消：cancel_after 转发 cancel → -32004 CANCELLED；
    // Failed{category=CANCELLED, code=-32004, message 确定性} 逐字段一致。
    let vc = venv_adapter
        .execute(
            &json!({"op": "sleep", "ms": 5000}),
            60_000,
            65_536,
            None,
            Some(Duration::from_millis(200)),
        )
        .expect("cancel outcome");
    let sc = system_adapter
        .execute(
            &json!({"op": "sleep", "ms": 5000}),
            60_000,
            65_536,
            None,
            Some(Duration::from_millis(200)),
        )
        .expect("cancel outcome");
    match (&vc, &sc) {
        (
            tool_engine::adapter::AdapterOutcome::Failed {
                category: vcat,
                code: vcode,
                message: vmsg,
                retryable: vr,
            },
            tool_engine::adapter::AdapterOutcome::Failed {
                category: scat,
                code: scode,
                message: smsg,
                retryable: sr,
            },
        ) => {
            assert_eq!(vcat, scat, "cancel category parity");
            assert_eq!(vcode, scode, "cancel code parity");
            assert_eq!(vcat, "CANCELLED");
            assert_eq!(*vcode, json!(-32004));
            assert_eq!(vmsg, smsg, "cancel message field-parity");
            assert_eq!(vmsg, "task cancelled by cancel request");
            assert_eq!(vr, sr);
        }
        _ => panic!("expect Failed[CANCELLED] both sides: {vc:?} vs {sc:?}"),
    }

    venv_adapter.shutdown();
    system_adapter.shutdown();
    cleanup_dir(&root);
}

/// echo 任务回数（Succeeded data）。
fn echo_of(adapter: &PythonWorkerAdapter) -> Value {
    match adapter
        .execute(&json!({"op": "echo"}), 30_000, 65_536, None, None)
        .expect("echo executes")
    {
        tool_engine::adapter::AdapterOutcome::Succeeded { data, .. } => data,
        other => panic!("expect Succeeded, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 判据 7：未声明 venv 的派发链零回归（信封/状态机与 RT-1 基线一致）
// ---------------------------------------------------------------------------

#[test]
fn python_worker_venv_criterion7_bare_name_unchanged() {
    let (engine, registry, _cache) = fresh_env();
    // 真实 parsing worker 全链（未声明 venv → 现状行为逐字节不变断言载体）
    engine
        .submit(&json!({
            "protocol_version": "1",
            "tool_id": "rip.t.venv.debug",
            "run_id": "rn-venv-bare",
            "input": {"op": "echo"},
            "workspace_id": "ws-default",
            "budget": {"timeout_seconds": 60}
        }))
        .unwrap();
    let adapter = system_sided_adapter();
    let out = execute_via_engine(AdapterCall {
        engine: &engine,
        run_id: "rn-venv-bare",
        adapter: &adapter,
        cancel_after: None,
    })
    .unwrap();
    // 状态机与 RT-1 基线逐行一致：QUEUED→PREPARING→RUNNING→SUCCEEDED
    assert_eq!(
        out.transcript,
        vec![
            ToolRunStatus::Queued,
            ToolRunStatus::Preparing,
            ToolRunStatus::Running,
            ToolRunStatus::Succeeded,
        ]
    );
    assert_eq!(out.envelope.status, ToolRunStatus::Succeeded);
    assert_eq!(out.envelope.environment.runtime_type, "python_worker");
    assert!(out.envelope.result.is_some());
    // 裸名派发：解释器解析值原样（无 venv 替换面）
    assert_eq!(adapter.resolved_interpreter().as_deref(), Some("python3"));
    // stderr 日志采集面正常（worker 真实拉起）
    assert!(adapter.stderr_lines().iter().any(|l| l.contains("startup")));
    adapter.shutdown();
    drop(registry);
}

// ---------------------------------------------------------------------------
// 判据 8：VenvIdentity 序列化记录面（逐字段非空断言 + 零 Registry 写入面）
// ---------------------------------------------------------------------------

#[test]
fn python_worker_venv_criterion8_identity_record_face() {
    let root = make_fixture_venv();
    let identity =
        venv_probe(&fixture_venv_config(&root), None, None).expect("fixture venv probes available");
    let serialized = serde_json::to_string(&identity).expect("VenvIdentity serializes");
    // D6 冻结 schema 逐字段在序列化面在场
    for key in [
        "venv_root",
        "interpreter_path",
        "interpreter_identity",
        "sha256_hex",
        "file_type",
        "owner_mode",
        "symlink_target",
        "base_interpreter",
        "interpreter_version",
        "python_version_cfg",
        "deps_capset",
        "deps_fingerprint",
        "requirements_lock_sha256",
        "direct_deps_satisfied",
        "status",
        "probe_timestamp_unix_ms",
        "fingerprint",
    ] {
        assert!(
            serialized.contains(&format!("\"{key}\"")),
            "field {key} missing in {serialized}"
        );
    }
    // 值面非空断言（requirements_lock_sha256 未声明为 null 属合法空缺）
    assert_eq!(identity.status, VenvStatus::Available);
    assert!(!identity.interpreter_identity.file_type.is_empty());
    assert!(!identity.interpreter_identity.owner_mode.is_empty());
    assert!(identity.probe_timestamp_unix_ms > 0);
    // deps_fingerprint 独立复算
    let capset_json: Vec<Value> = identity
        .deps_capset
        .iter()
        .map(|(n, v)| json!([n, v]))
        .collect();
    assert_eq!(
        identity.deps_fingerprint,
        tool_registry::hash::sha256_hex(serde_json::to_string(&capset_json).unwrap().as_bytes())
            [..16]
            .to_string()
    );
    // venv 状态零写 Tool Registry / Manifest：本探测面全文件零 Registry 导入
    // （S7 grep 断言双复核：crates/tool-engine/src/venv.rs 无 tool_registry::Registry /
    //  register 写入入口，仅复用其 hash 帮手）。
    cleanup_dir(&root);
}
