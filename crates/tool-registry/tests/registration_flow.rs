//! P1-4 验收（ROADMAP §2.3 P1-4 判据原句）：
//! 1. P1-1 两个示例 Manifest 在测试内存库走完登记、查询、取 Schema 全流程；
//! 2. 校验九条逐条有拒绝用例（必含 remote_runner 拒绝与 license=unknown 拒绝）。
//!
//! 示例文件来源：schemas/tool-manifest/examples/*.yaml（P1-1 落盘）经
//! yaml.safe_load 机械转出的同名 .json 副本（vendor 无 yaml crate，
//! Rust 侧读 JSON 副本，转换与 YAML 等价性已在落盘时 roundtrip 断言）。
//! 内存库每次迁移全链 001..005，测试互不共享状态。

use database::{migrate, DbPath, Pool, PoolConfig};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tool_registry::{content_hash, Options, Registry, RegistryError, ValidationRule};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn migrations_dir() -> PathBuf {
    repo_root().join("migrations")
}

fn examples_dir() -> PathBuf {
    repo_root().join("schemas/tool-manifest/examples")
}

fn fresh_pool() -> Pool {
    let pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(1),
    })
    .unwrap();
    migrate(&pool, &migrations_dir()).unwrap();
    pool
}

/// 验收流程用注册表：显式放行 `<digest>` 占位（镜像属 Phase 2/3 交付，
/// TOOL-CONTRACT 11.1），生产语义默认拒绝（默认开关另有拒绝用例覆盖）。
fn flow_registry() -> Registry {
    Registry::open(fresh_pool()).with_options(Options {
        allow_placeholder_digest: true,
    })
}

fn load_example(name: &str) -> Value {
    let text = std::fs::read_to_string(examples_dir().join(format!("{name}.json"))).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn assert_rule(err: RegistryError, rule: ValidationRule, context: &str) {
    match err {
        RegistryError::Validation {
            rule: r, reason, ..
        } => {
            assert_eq!(r, rule, "{context}: wrong rule, reason={reason}");
        }
        other => panic!("{context}: expected Validation({rule:?}), got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 全流程：登记 → 查询 → 取 Schema（判据原句）
// ---------------------------------------------------------------------------

#[test]
fn two_examples_register_query_schema_full_flow() {
    let reg = flow_registry();
    let doc = load_example("rip.doc.parse");
    let http = load_example("rip.http.inspect");

    // 登记
    let r1 = reg.register(&doc).unwrap();
    assert_eq!(r1.tool_id, "rip.doc.parse");
    assert_eq!(r1.version, "0.1.0");
    assert!(!r1.superseded_previous);
    assert_eq!(r1.content_hash, content_hash(&doc));
    let r2 = reg.register(&http).unwrap();
    assert_eq!(r2.tool_id, "rip.http.inspect");
    assert_eq!(r2.version, "1.0.0");

    // 发现
    let found = reg.discover().unwrap();
    let ids: Vec<&str> = found.iter().map(|t| t.tool_id.as_str()).collect();
    assert_eq!(ids, vec!["rip.doc.parse", "rip.http.inspect"]);

    // 查询
    let d1 = reg.query("rip.doc.parse").unwrap();
    assert_eq!(d1.current_version.as_deref(), Some("0.1.0"));
    assert_eq!(d1.tool.lifecycle, "VALIDATED");
    assert_eq!(d1.tool.governance, "ACTIVE");
    assert_eq!(d1.tool.permission_level.as_deref(), Some("T0"));
    assert_eq!(d1.tool.timeout_s, Some(60));
    let d2 = reg.query("rip.http.inspect").unwrap();
    assert_eq!(d2.tool.permission_level.as_deref(), Some("T1"));
    assert!(matches!(
        reg.query("no.such.tool").unwrap_err(),
        RegistryError::NotFound { .. }
    ));

    // 取 Schema：与 Manifest 原文一致
    let (inp1, outp1) = reg.schemas("rip.doc.parse", None).unwrap();
    assert_eq!(inp1, doc["input_schema"]);
    assert_eq!(outp1, doc["output_schema"]);
    let (inp2, _) = reg.schemas("rip.http.inspect", Some("1.0.0")).unwrap();
    assert_eq!(inp2, http["input_schema"]);
    assert!(matches!(
        reg.schemas("rip.doc.parse", Some("9.9.9")).unwrap_err(),
        RegistryError::NotFound { .. }
    ));

    // 第 9 条落库对账：provenance_json / content_hash / validated_at
    let v1 = d1
        .versions
        .iter()
        .find(|v| v.version == "0.1.0")
        .expect("version row");
    assert_eq!(v1.status, "current");
    assert_eq!(v1.content_hash, content_hash(&doc));
    assert!(v1.validated_at.is_some());
    let provenance: Value = serde_json::from_str(v1.provenance_json.as_deref().unwrap()).unwrap();
    assert_eq!(provenance, doc["provenance"]);
}

#[test]
fn upgrade_supersedes_previous_current() {
    let reg = flow_registry();
    let mut doc = load_example("rip.doc.parse");
    reg.register(&doc).unwrap();
    doc["version"] = json!("0.2.0");
    let r = reg.register(&doc).unwrap();
    assert!(r.superseded_previous);
    let d = reg.query("rip.doc.parse").unwrap();
    assert_eq!(d.current_version.as_deref(), Some("0.2.0"));
    let old = d.versions.iter().find(|v| v.version == "0.1.0").unwrap();
    assert_eq!(old.status, "superseded");
    // content_hash 随版本分叉（0.2.0 的 hash 含新版本号）
    assert_ne!(old.content_hash, r.content_hash);
}

// ---------------------------------------------------------------------------
// 九条逐条拒绝用例
// ---------------------------------------------------------------------------

#[test]
fn rule1_rejects_missing_field_and_unsupported_schema_version() {
    let reg = flow_registry();
    let mut doc = load_example("rip.doc.parse");
    doc.as_object_mut().unwrap().remove("provenance");
    assert_rule(
        reg.register(&doc).unwrap_err(),
        ValidationRule::RequiredFields,
        "missing provenance",
    );

    let mut doc2 = load_example("rip.doc.parse");
    doc2["schema_version"] = json!("2");
    assert_rule(
        reg.register(&doc2).unwrap_err(),
        ValidationRule::RequiredFields,
        "schema_version 2 unsupported",
    );
}

#[test]
fn rule2_rejects_duplicate_and_downgrade_and_aborts_write() {
    let reg = flow_registry();
    let mut doc = load_example("rip.doc.parse");
    reg.register(&doc).unwrap();

    // 完全相同的 (id, version) 重复登记
    assert_rule(
        reg.register(&doc).unwrap_err(),
        ValidationRule::VersionComparable,
        "duplicate version",
    );
    // semver 回退
    doc["version"] = json!("0.0.9");
    assert_rule(
        reg.register(&doc).unwrap_err(),
        ValidationRule::VersionComparable,
        "downgrade",
    );
    // 失败不产生半登记：版本行数不变
    let d = reg.query("rip.doc.parse").unwrap();
    assert_eq!(d.versions.len(), 1);
}

#[test]
fn rule3_rejects_invalid_io_schema() {
    let reg = flow_registry();
    let mut doc = load_example("rip.doc.parse");
    doc["input_schema"] = json!({"type": "nosuchtype"});
    assert_rule(
        reg.register(&doc).unwrap_err(),
        ValidationRule::IoSchema,
        "unknown type keyword",
    );
    let mut doc2 = load_example("rip.doc.parse");
    doc2["output_schema"] = json!({"properties": "not-an-object"});
    assert_rule(
        reg.register(&doc2).unwrap_err(),
        ValidationRule::IoSchema,
        "properties not object",
    );
}

#[test]
fn rule4_rejects_remote_runner_tag_image_and_default_placeholder() {
    let reg = flow_registry();

    // remote_runner：D-TC-01 词表内但 Phase 1 不开放（必含用例）
    let mut http = load_example("rip.http.inspect");
    http["runtime"]["type"] = json!("remote_runner");
    assert_rule(
        reg.register(&http).unwrap_err(),
        ValidationRule::Runtime,
        "remote_runner",
    );

    // tag 引用镜像
    let mut http2 = load_example("rip.http.inspect");
    http2["runtime"]["image"] = json!("registry.local/rip/http-inspector:latest");
    assert_rule(
        reg.register(&http2).unwrap_err(),
        ValidationRule::Runtime,
        "tag reference",
    );

    // 默认开关（生产语义）下占位 digest 拒绝
    let strict = Registry::open(fresh_pool());
    assert_rule(
        strict
            .register(&load_example("rip.http.inspect"))
            .unwrap_err(),
        ValidationRule::Runtime,
        "placeholder digest by default",
    );
}

#[test]
fn rule5_rejects_license_unknown_and_digest_mismatch() {
    let reg = flow_registry();

    // license=unknown（必含用例，TOOL-INVENTORY TPscan 裁定）
    let mut doc = load_example("rip.doc.parse");
    doc["provenance"]["license"] = json!("unknown");
    assert_rule(
        reg.register(&doc).unwrap_err(),
        ValidationRule::Provenance,
        "license unknown",
    );

    // podman_vm 的 image_digest 与 runtime.image 不一致
    let mut http = load_example("rip.http.inspect");
    http["provenance"]["image_digest"] =
        json!("sha256:1111111111111111111111111111111111111111111111111111111111111111");
    assert_rule(
        reg.register(&http).unwrap_err(),
        ValidationRule::Provenance,
        "digest mismatch",
    );

    // podman_vm 缺 image_digest
    let mut http3 = load_example("rip.http.inspect");
    http3["provenance"]
        .as_object_mut()
        .unwrap()
        .remove("image_digest");
    assert_rule(
        reg.register(&http3).unwrap_err(),
        ValidationRule::Provenance,
        "podman_vm requires image_digest",
    );
}

#[test]
fn rule6_rejects_timeout_and_output_bytes_out_of_domain() {
    let reg = flow_registry();
    let mut doc = load_example("rip.doc.parse");
    doc["resources"]["timeout_seconds"] = json!(0);
    assert_rule(
        reg.register(&doc).unwrap_err(),
        ValidationRule::Resources,
        "timeout 0",
    );

    let mut doc2 = load_example("rip.doc.parse");
    doc2["resources"]["timeout_seconds"] = json!(601);
    assert_rule(
        reg.register(&doc2).unwrap_err(),
        ValidationRule::Resources,
        "timeout 601",
    );

    let mut doc3 = load_example("rip.doc.parse");
    doc3["resources"]["max_output_bytes"] = json!(100);
    assert_rule(
        reg.register(&doc3).unwrap_err(),
        ValidationRule::Resources,
        "max_output_bytes below 256",
    );
}

#[test]
fn rule7_rejects_required_secret_without_store_or_hit() {
    let mut doc = load_example("rip.doc.parse");
    doc["secrets"] = json!([{"name": "RIP_TEST_KEY", "inject_as": "env", "required": true}]);

    // 未配置 secrets 存储
    let reg = flow_registry();
    assert_rule(
        reg.register(&doc).unwrap_err(),
        ValidationRule::Secrets,
        "no store configured",
    );

    // 配置了存储但键不存在
    let reg2 = flow_registry().with_secrets_lookup(Arc::new(|_: &str| false));
    assert_rule(
        reg2.register(&doc).unwrap_err(),
        ValidationRule::Secrets,
        "key missing in store",
    );

    // 存储命中后通过
    let reg3 = flow_registry().with_secrets_lookup(Arc::new(|k: &str| k == "RIP_TEST_KEY"));
    assert!(reg3.register(&doc).is_ok());

    // required=false 的缺失键按宽告警放行
    let mut doc2 = load_example("rip.doc.parse");
    doc2["secrets"] = json!([{"name": "RIP_OPTIONAL_KEY", "inject_as": "env", "required": false}]);
    let reg4 = flow_registry().with_secrets_lookup(Arc::new(|_: &str| false));
    assert!(reg4.register(&doc2).is_ok());
}

#[test]
fn rule8_rejects_input_violation_and_hint_violation() {
    let reg = flow_registry();
    // input 违反 oneOf（text/path 皆缺）
    let mut doc = load_example("rip.doc.parse");
    doc["examples"] = json!([{"input": {"format": "md"}}]);
    assert_rule(
        reg.register(&doc).unwrap_err(),
        ValidationRule::Examples,
        "input fails oneOf",
    );
    // output_hint 的键违反 output_schema 对应属性（format 需字符串枚举）
    let mut doc2 = load_example("rip.doc.parse");
    doc2["examples"] = json!([{"input": {"text": "x"}, "output_hint": {"format": 123}}]);
    assert_rule(
        reg.register(&doc2).unwrap_err(),
        ValidationRule::Examples,
        "output_hint key violates property schema",
    );
}

#[test]
fn rule9_rejects_nothing_but_persists_verified_rows() {
    // 第 9 条为持久化语义条目（无拒绝面）：断言登记行完整且失败原子回滚。
    let reg = flow_registry();
    let doc = load_example("rip.doc.parse");
    let vm = tool_registry::ValidatedManifest {
        manifest: doc.clone(),
        id: doc["id"].as_str().unwrap().to_string(),
        version: doc["version"].as_str().unwrap().to_string(),
        content_hash: content_hash(&doc),
    };
    tool_registry::store::register(reg.pool(), &vm).unwrap();
    let pool = reg.pool();
    let (tv, tool_rows): (i64, i64) = pool
        .with_conn(|c| {
            let tv: i64 = c.query_row("SELECT COUNT(*) FROM tool_version", [], |r| r.get(0))?;
            let t: i64 = c.query_row("SELECT COUNT(*) FROM tool", [], |r| r.get(0))?;
            Ok::<(i64, i64), database::rusqlite::Error>((tv, t))
        })
        .unwrap();
    assert_eq!((tv, tool_rows), (1, 1));
    assert!(matches!(
        tool_registry::store::register(pool, &vm).unwrap_err(),
        RegistryError::Storage(_)
    ));
    let (tv2, tool_rows2): (i64, i64) = pool
        .with_conn(|c| {
            let tv: i64 = c.query_row("SELECT COUNT(*) FROM tool_version", [], |r| r.get(0))?;
            let t: i64 = c.query_row("SELECT COUNT(*) FROM tool", [], |r| r.get(0))?;
            Ok::<(i64, i64), database::rusqlite::Error>((tv, t))
        })
        .unwrap();
    assert_eq!(
        (tv2, tool_rows2),
        (1, 1),
        "UNIQUE(tool_id, version) 兜底后不得产生半登记"
    );
}
