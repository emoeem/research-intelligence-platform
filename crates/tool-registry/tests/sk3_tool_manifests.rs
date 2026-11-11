//! SK-3 工具 Manifest 静态链测试（P6-11 第 1/2 步）：四份双份 Manifest
//! （yaml+json）经仓库自身 parse + validate_static + Registry 登记逐键相等：
//! - 双份等价：serde_yaml_ng 解析 == serde_json 解析（逐键相等）；
//! - validate_static 全过（rule 1/3/4/5/6/8；native_rust 分支
//!   binary+subcommand 必填、egress_allowlist 分支 egress.ports 必填）；
//! - 登记面：四件全部可 register + discover/query 返回（T0/T1 权限词表如
//!   Manifest 所载）；
//! - license=unknown 复验（rule 5）：登记被拒（拒绝矩阵 license 类）。

use std::path::PathBuf;

use database::{DbPath, Pool, PoolConfig};
use tool_registry::{Options, Registry};
use tool_registry::validate::{content_hash, validate_static};

fn repo_root() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

const TOOLS: [&str; 4] = [
    "rip.knowledge.catalog.extract",
    "rip.security.extension.audit",
    "rip.knowledge.document.fetch",
    "rip.knowledge.source.sync",
];

fn load_yaml(name: &str) -> serde_json::Value {
    let text = std::fs::read_to_string(
        repo_root().join(format!("manifests/tools/{name}.yaml")),
    )
    .unwrap();
    serde_yaml_ng::from_str(&text).unwrap()
}

fn load_json(name: &str) -> serde_json::Value {
    let text = std::fs::read_to_string(
        repo_root().join(format!("manifests/tools/{name}.json")),
    )
    .unwrap();
    serde_json::from_str(&text).unwrap()
}

#[test]
fn sk3_dual_manifest_forms_are_key_equal_and_static_valid() {
    for name in TOOLS {
        let yaml = load_yaml(name);
        let json = load_json(name);
        assert_eq!(yaml, json, "{name}: yaml 与 json 双份必须逐键相等");
        // validate_static 全过（生产语义：占位 digest 拒绝开关默认关）
        let vm = validate_static(&yaml, &Options::default())
            .unwrap_or_else(|e| panic!("{name}: validate_static failed: {e}"));
        assert_eq!(vm.id, name);
        assert_eq!(vm.content_hash, content_hash(&yaml));

        // T0/T1 权限词表如载（计划 §6.2：A/B T0，C/D T1）
        let permission = yaml["permission_level"].as_str().unwrap();
        match name {
            "rip.knowledge.catalog.extract" | "rip.security.extension.audit" => {
                assert_eq!(permission, "T0", "{name}");
            }
            _ => assert_eq!(permission, "T1", "{name}"),
        }
        // 网络面分级：T0 不得声明出网，T1 的 fetch/sync 是 egress_allowlist
        let mode = yaml["network"]["mode"].as_str().unwrap();
        if permission == "T0" {
            assert_eq!(mode, "none", "{name}: T0 必须无网络");
        } else {
            assert_eq!(mode, "egress_allowlist", "{name}: T1 白名单出网");
            assert!(yaml["network"]["egress"]["ports"].as_array().unwrap().iter().all(|p| p.is_i64() || p.is_u64()),
                    "{name}: egress.ports 必填");
        }
    }
}

#[test]
fn sk3_four_tools_register_discover_query() {
    let db = std::env::temp_dir().join(format!("sk3-tools-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&db);
    let pool = Pool::open(PoolConfig {
        path: DbPath::File(db.clone()),
        readonly: false,
        max_connections: 1,
        busy_timeout: std::time::Duration::from_secs(5),
    })
    .unwrap();
    std::panic::catch_unwind(|| {
        database::migrate(&pool, &repo_root().join("migrations")).unwrap();
        let registry = Registry::open(pool.clone());
        for name in TOOLS {
            let doc = load_json(name);
            let r = registry.register(&doc).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(r.tool_id, name);
            assert_eq!(r.version, "0.1.0");
        }
        let found = registry.discover().unwrap();
        for name in TOOLS {
            assert!(found.iter().any(|t| t.tool_id == name), "{name} discoverable");
            let d = registry.query(name).unwrap();
            assert_eq!(d.current_version.as_deref(), Some("0.1.0"));
            let permission = match name {
                "rip.knowledge.catalog.extract" | "rip.security.extension.audit" => "T0",
                _ => "T1",
            };
            assert_eq!(d.tool.permission_level.as_deref(), Some(permission), "{name}");
        }
    })
    .unwrap_or_else(|e| {
        let _ = std::fs::remove_file(&db);
        std::panic::resume_unwind(e);
    });
    let _ = std::fs::remove_file(&db);
}

#[test]
fn sk3_license_unknown_rejected_at_registration() {
    // 拒绝矩阵 license 类复验：license=unknown 按 rule 5 拒登（2026-11-12
    // TOOL-CONTRACT rule 5 既有语义，SK-3 在新工具上再断言一次）。
    let mut doc = load_json("rip.knowledge.catalog.extract");
    doc["provenance"]["license"] = serde_json::json!("unknown");
    let err = validate_static(&doc, &Options::default()).unwrap_err();
    match err {
        tool_registry::RegistryError::Validation { rule, reason, .. } => {
            assert!(matches!(rule, tool_registry::ValidationRule::Provenance));
            assert!(reason.contains("license=unknown"), "reason: {reason}");
        }
        other => panic!("expect Validation, got {other:?}"),
    }
}
