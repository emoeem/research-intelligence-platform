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
use tool_registry::validate::{content_hash, validate_static};
use tool_registry::{Options, Registry};

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
    let text =
        std::fs::read_to_string(repo_root().join(format!("manifests/tools/{name}.yaml"))).unwrap();
    serde_yaml_ng::from_str(&text).unwrap()
}

fn load_json(name: &str) -> serde_json::Value {
    let text =
        std::fs::read_to_string(repo_root().join(format!("manifests/tools/{name}.json"))).unwrap();
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
            assert!(
                yaml["network"]["egress"]["ports"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|p| p.is_i64() || p.is_u64()),
                "{name}: egress.ports 必填"
            );
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
            let r = registry
                .register(&doc)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(r.tool_id, name);
            assert_eq!(r.version, "0.1.0");
        }
        let found = registry.discover().unwrap();
        for name in TOOLS {
            assert!(
                found.iter().any(|t| t.tool_id == name),
                "{name} discoverable"
            );
            let d = registry.query(name).unwrap();
            assert_eq!(d.current_version.as_deref(), Some("0.1.0"));
            let permission = match name {
                "rip.knowledge.catalog.extract" | "rip.security.extension.audit" => "T0",
                _ => "T1",
            };
            assert_eq!(
                d.tool.permission_level.as_deref(),
                Some(permission),
                "{name}"
            );
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

// ---------------------------------------------------------------------------
// RT-1 native_rust allowlist 形状（RT-1-ADR D3；S2 增强面正负例矩阵）
// ---------------------------------------------------------------------------

/// 以既有四份 Manifest 之一为底座换 runtime 面的构造器（其余字段走原
/// Manifest，保证 rule1/3/5/6/8 全链参与）。
fn native_rust_doc() -> serde_json::Value {
    let mut doc = load_json("rip.knowledge.catalog.extract");
    doc["input_schema"] = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["subcommand"],
        "properties": {
            "subcommand": {"type": "string", "enum": ["-layout", "-opw"]},
            "input_path": {"type": "string", "minLength": 1}
        }
    });
    doc["output_schema"] = serde_json::json!({"type": "object"});
    doc["examples"] = serde_json::json!([
        {"input": {"subcommand": "-layout", "input_path": "sample.pdf"}}
    ]);
    doc
}

#[test]
fn sk3_native_rust_allowlist_shapes_accept() {
    // 形态 A：binary 直呼 + 64hex 钉值 + capability_ref（无 subcommand，
    // argv_template 承接参数面）。
    let mut a = native_rust_doc();
    a["runtime"] = serde_json::json!({
        "type": "native_rust",
        "binary": "/usr/bin/pdftotext",
        "binary_sha256": "f85872f94fb67433a1b2c3d4e5f60112233445566778899aabbccddeeff00112",
        "capability_ref": "pdf.pdftotext"
    });
    validate_static(&a, &Options::default()).unwrap_or_else(|e| panic!("shape A rejected: {e}"));

    // 形态 A 段首数字 capability_ref：C 目录权威 id archive.7z（段首数字与
    // validate.rs capability_ref_ok / capability.rs CapabilityId::new /
    // schemas/tool-manifest/v1.json pattern 三面同规则，评审修订补测）。
    let mut d = native_rust_doc();
    d["runtime"] = serde_json::json!({
        "type": "native_rust",
        "binary": "/usr/bin/7z",
        "binary_sha256": "f802383f13b8331aabaf779def00a356c8d2b8148255170b75b03028f4d23ad4",
        "capability_ref": "archive.7z"
    });
    validate_static(&d, &Options::default())
        .unwrap_or_else(|e| panic!("digit-start capability_ref (archive.7z) rejected: {e}"));

    // 形态 B：subcommands 白名单 + argv_template（D3 形状）。
    let mut b = native_rust_doc();
    b["runtime"] = serde_json::json!({
        "type": "native_rust",
        "binary": "/usr/bin/pdftotext",
        "binary_sha256": "f85872f94fb67433a1b2c3d4e5f60112233445566778899aabbccddeeff00112",
        "subcommands": ["-layout", "-opw"],
        "argv_template": ["{{input_path}}", "-enc", "UTF-8"]
    });
    validate_static(&b, &Options::default()).unwrap_or_else(|e| panic!("shape B rejected: {e}"));

    // wrapper 形态：binary_target_sha256（C §5.1:270 wrapper 与 exec 目标双钉）。
    let mut w = native_rust_doc();
    w["runtime"] = serde_json::json!({
        "type": "native_rust",
        "binary": "/usr/bin/7z",
        "binary_sha256": "f802383f13b8331aabaf779def00a356c8d2b8148255170b75b03028f4d23ad4",
        "binary_target_sha256": "d48e622e9009acdb000000000000000000000000000000000000000000000000",
        "subcommands": ["l", "x"],
        "argv_template": ["{{out_dir}}"]
    });
    validate_static(&w, &Options::default())
        .unwrap_or_else(|e| panic!("wrapper shape rejected: {e}"));

    // 旧 subcommand 单值字段与新白名单并存仍接受（保留兼容）。
    let mut c = b.clone();
    c["runtime"]["subcommand"] = serde_json::json!("-layout");
    validate_static(&c, &Options::default())
        .unwrap_or_else(|e| panic!("subcommand+subcommands rejected: {e}"));
}

#[test]
fn sk3_native_rust_allowlist_negative_matrix() {
    // 相对路径 + 钉值字段在场 → 登记面拒绝（判据 3）。
    let mut doc = native_rust_doc();
    doc["runtime"] = serde_json::json!({
        "type": "native_rust",
        "binary": "tools/bin/pdftotext",
        "binary_sha256": "f85872f94fb67433a1b2c3d4e5f60112233445566778899aabbccddeeff00112"
    });
    let err = validate_static(&doc, &Options::default()).unwrap_err();
    assert!(
        err.to_string().contains("must be an absolute path"),
        "relative path must be rejected: {err}"
    );

    // PATH 名称解析形态 + 钉值字段在场 → 拒绝（D3 否决项）。
    let mut doc = native_rust_doc();
    doc["runtime"] = serde_json::json!({
        "type": "native_rust",
        "binary": "pdftotext",
        "binary_sha256": "f85872f94fb67433a1b2c3d4e5f60112233445566778899aabbccddeeff00112"
    });
    let err = validate_static(&doc, &Options::default()).unwrap_err();
    assert!(
        err.to_string().contains("must be an absolute path"),
        "PATH name form must be rejected: {err}"
    );

    // 坏 hash（非 64 小写 hex）→ 拒绝。
    for bad in [
        "xyz",
        "ABCDEF0123ABCDEF0123ABCDEF0123ABCDEF0123ABCDEF0123ABCDEF0123ABCD",
    ] {
        let mut doc = native_rust_doc();
        doc["runtime"] = serde_json::json!({
            "type": "native_rust",
            "binary": "/usr/bin/pdftotext",
            "binary_sha256": bad
        });
        let err = validate_static(&doc, &Options::default()).unwrap_err();
        assert!(
            err.to_string().contains("64 lowercase hex"),
            "bad hash {bad:?} must be rejected: {err}"
        );
    }

    // subcommands 越界：空数组 / 非字符串项 / 空串项。
    for bad in [
        serde_json::json!([]),
        serde_json::json!(["-layout", 3]),
        serde_json::json!(["-layout", ""]),
    ] {
        let mut doc = native_rust_doc();
        doc["runtime"] = serde_json::json!({
            "type": "native_rust",
            "binary": "/usr/bin/pdftotext",
            "binary_sha256": "f85872f94fb67433a1b2c3d4e5f60112233445566778899aabbccddeeff00112",
            "subcommands": bad
        });
        let err = validate_static(&doc, &Options::default()).unwrap_err();
        assert!(
            err.to_string().contains("subcommands"),
            "bad subcommands {bad} must be rejected: {err}"
        );
    }

    // argv_template 注入面：`../` 路径穿越与 shell 元字符 → 拒绝（判据 3）。
    for bad in ["../escape", "a;rm", "a|b", "$(x)", "`id`", "a\nb"] {
        let mut doc = native_rust_doc();
        doc["runtime"] = serde_json::json!({
            "type": "native_rust",
            "binary": "/usr/bin/pdftotext",
            "binary_sha256": "f85872f94fb67433a1b2c3d4e5f60112233445566778899aabbccddeeff00112",
            "argv_template": ["{{input_path}}", bad]
        });
        let err = validate_static(&doc, &Options::default()).unwrap_err();
        assert!(
            err.to_string().contains("argv_template item"),
            "argv injection {bad:?} must be rejected: {err}"
        );
    }

    // capability_ref 越界（非两段小写）→ 拒绝。
    let mut doc = native_rust_doc();
    doc["runtime"] = serde_json::json!({
        "type": "native_rust",
        "binary": "/usr/bin/pdftotext",
        "binary_sha256": "f85872f94fb67433a1b2c3d4e5f60112233445566778899aabbccddeeff00112",
        "capability_ref": "pdftotext"
    });
    let err = validate_static(&doc, &Options::default()).unwrap_err();
    assert!(
        err.to_string()
            .contains("two-segment lowercase capability id"),
        "bad capability_ref must be rejected: {err}"
    );

    // 段首数字 capability_ref 正例（C 目录权威 id archive.7z；评审修订补测：
    // 与 v1.json pattern / validate.rs / capability.rs 三面同规则）。
    let mut doc = native_rust_doc();
    doc["runtime"] = serde_json::json!({
        "type": "native_rust",
        "binary": "/usr/bin/7z",
        "binary_sha256": "f802383f13b8331aabaf779def00a356c8d2b8148255170b75b03028f4d23ad4",
        "subcommands": ["l"],
        "capability_ref": "archive.7z"
    });
    validate_static(&doc, &Options::default())
        .unwrap_or_else(|e| panic!("digit-start capability_ref (archive.7z) rejected: {e}"));

    // D4 env 登记卫生（评审修订）：动态链接器注入类键（LD_PRELOAD/
    // LD_LIBRARY_PATH）登记拒绝——派发面 env_clear 白名单外结构性禁入面。
    for bad_key in ["LD_PRELOAD", "LD_LIBRARY_PATH"] {
        let mut doc = native_rust_doc();
        doc["runtime"] = serde_json::json!({
            "type": "native_rust",
            "binary": "/usr/bin/pdftotext",
            "binary_sha256": "f85872f94fb67433a1b2c3d4e5f60112233445566778899aabbccddeeff00112",
            "subcommands": ["-layout"],
            "env": {bad_key: "/tmp/evil.so"}
        });
        let err = validate_static(&doc, &Options::default()).unwrap_err();
        assert!(
            err.to_string().contains("dynamic-linker injection"),
            "env {bad_key} must be rejected at registration: {err}"
        );
    }

    // 旧形状收紧不破：无钉值时 subcommand 仍必填（缺省拒绝）。
    let mut doc = native_rust_doc();
    doc["runtime"] = serde_json::json!({"type": "native_rust", "binary": "rp-plugin"});
    let err = validate_static(&doc, &Options::default()).unwrap_err();
    assert!(
        err.to_string()
            .contains("`subcommand` must be a non-empty string"),
        "legacy shape without subcommand must be rejected: {err}"
    );
}

#[test]
fn sk3_native_rust_allowlist_registers_end_to_end() {
    // 新形状正例走完整登记链（fresh DB，version comparable 无冲突）。
    let mut doc = native_rust_doc();
    doc["runtime"] = serde_json::json!({
        "type": "native_rust",
        "binary": "/usr/bin/pdftotext",
        "binary_sha256": "f85872f94fb67433a1b2c3d4e5f60112233445566778899aabbccddeeff00112",
        "subcommands": ["-layout", "-opw"],
        "argv_template": ["{{input_path}}"]
    });
    doc["id"] = serde_json::json!("rip.doc.pdf.extract");
    doc["name"] = serde_json::json!("PDF Text Extract (native)");
    doc["version"] = serde_json::json!("0.1.0");
    doc["provenance"]["source_path"] =
        serde_json::json!("manifests/tools/rip.doc.pdf.extract.json");

    let db = std::env::temp_dir().join(format!("sk3-native-{}.db", std::process::id()));
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
        let r = registry
            .register(&doc)
            .unwrap_or_else(|e| panic!("register: {e}"));
        assert_eq!(r.tool_id, "rip.doc.pdf.extract");
    })
    .unwrap_or_else(|e| {
        let _ = std::fs::remove_file(&db);
        std::panic::resume_unwind(e);
    });
    let _ = std::fs::remove_file(&db);
}
