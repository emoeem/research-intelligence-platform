//! P1-7a 验收（ROADMAP §2.3 P1-7a 判据原句，TOOL-CONTRACT §10.2 B1 行）：
//! B1 三工具（rip.doc.parse/T02、rip.text.embed/T03、rip.kb.evaluate/T04）
//! python_worker Manifest 直登开发库副本后可发现、可查 Schema。
//!
//! 登记文件：manifests/tools/*.yaml（本批落点，ROADMAP 状态行授权的批次内部
//! 细节），Rust 侧读机械转出的同名 .json 副本（vendor 无 yaml crate，转换
//! 等价性在落盘时断言，先例见 registration_flow.rs 头注）。
//! 开发库副本：migrations/dev-b1.db（项目内，*.db 已 gitignore；每次运行
//! 先删旧副本重建，platform.db 与 platform-dryrun.db 不写）。
//! T03 的 V-1（bge 模型权重许可证）已裁决：ModelScope API 实测上游
//! BAAI/bge-small-zh-v1.5 License=mit（2026-11-09），裁决记录在
//! manifests/tools/rip.text.embed.yaml 头注与 provenance.model_license。

use database::{migrate, DbPath, Pool, PoolConfig};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tool_registry::{content_hash, Registry};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn migrations_dir() -> PathBuf {
    repo_root().join("migrations")
}

fn dev_copy_path() -> PathBuf {
    migrations_dir().join("dev-b1.db")
}

fn manifests_dir() -> PathBuf {
    repo_root().join("manifests/tools")
}

fn load_manifest(name: &str) -> Value {
    let text = std::fs::read_to_string(manifests_dir().join(format!("{name}.json"))).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn reset_dev_copy() {
    for suffix in ["", "-wal", "-shm"] {
        let mut p = dev_copy_path().clone().into_os_string();
        p.push(suffix);
        let p = PathBuf::from(p);
        if p.exists() {
            std::fs::remove_file(&p).unwrap_or_else(|e| panic!("remove {}: {e}", p.display()));
        }
    }
}

fn fresh_registry() -> Registry {
    reset_dev_copy();
    let pool = Pool::open(PoolConfig {
        path: DbPath::File(dev_copy_path()),
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .expect("open dev copy pool");
    migrate(&pool, &migrations_dir()).expect("migrations 001..005");
    // 生产语义（严格开关默认值）：三工具均为 python_worker，无镜像 digest 面。
    Registry::open(pool)
}

#[test]
fn b1_three_manifests_register_discover_query_schema() {
    let reg = fresh_registry();
    let doc = load_manifest("rip.doc.parse");
    let embed = load_manifest("rip.text.embed");
    let evaluate = load_manifest("rip.kb.evaluate");

    // 登记（生产语义直登，无开关放行）
    let r1 = reg.register(&doc).unwrap();
    assert_eq!(r1.tool_id, "rip.doc.parse");
    assert_eq!(r1.version, "0.1.0");
    assert_eq!(r1.content_hash, content_hash(&doc));
    let r2 = reg.register(&embed).unwrap();
    assert_eq!(r2.tool_id, "rip.text.embed");
    assert_eq!(r2.version, "0.2.0");
    let r3 = reg.register(&evaluate).unwrap();
    assert_eq!(r3.tool_id, "rip.kb.evaluate");
    assert_eq!(r3.version, "0.1.0");

    // 发现：恰好 B1 三件，id 升序
    let found = reg.discover().unwrap();
    let ids: Vec<&str> = found.iter().map(|t| t.tool_id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["rip.doc.parse", "rip.kb.evaluate", "rip.text.embed"],
        "discover must return exactly the B1 batch"
    );
    for t in &found {
        assert_eq!(t.lifecycle, "VALIDATED");
        assert_eq!(t.governance, "ACTIVE");
        assert_eq!(t.permission_level.as_deref(), Some("T0"));
    }

    // 查询：current 版本与 worker 事实一致（worker NAME/VERSION 常量）
    for (id, version) in [
        ("rip.doc.parse", "0.1.0"),
        ("rip.text.embed", "0.2.0"),
        ("rip.kb.evaluate", "0.1.0"),
    ] {
        let d = reg.query(id).unwrap();
        assert_eq!(d.current_version.as_deref(), Some(version), "{id}");
        assert_eq!(d.versions.len(), 1, "{id}");
        assert_eq!(d.versions[0].status, "current");
        assert!(d.versions[0].validated_at.is_some(), "{id}");
        assert!(
            !d.versions[0]
                .provenance_json
                .as_deref()
                .unwrap_or_default()
                .is_empty(),
            "{id} provenance_json must be non-empty"
        );
        assert_eq!(
            d.versions[0].content_hash,
            content_hash(&load_manifest(match id {
                "rip.doc.parse" => "rip.doc.parse",
                "rip.text.embed" => "rip.text.embed",
                _ => "rip.kb.evaluate",
            }))
        );
    }

    // 取 Schema：与 Manifest 原文一致
    let (doc_in, doc_out) = reg.schemas("rip.doc.parse", None).unwrap();
    assert_eq!(doc_in, doc["input_schema"]);
    assert_eq!(doc_out, doc["output_schema"]);
    let (emb_in, emb_out) = reg.schemas("rip.text.embed", Some("0.2.0")).unwrap();
    assert_eq!(emb_in, embed["input_schema"]);
    assert_eq!(emb_out, embed["output_schema"]);
    let (ev_in, ev_out) = reg.schemas("rip.kb.evaluate", None).unwrap();
    assert_eq!(ev_in, evaluate["input_schema"]);
    assert_eq!(ev_out, evaluate["output_schema"]);
}

#[test]
fn b1_t03_records_resolved_model_license_v1() {
    // V-1 关闭的登记面证据：T03 Manifest 必须携带模型许可证裁决记录，
    // 且 provenance.license（工具本体）非 unknown（校验九条第 5 条可过）。
    let embed = load_manifest("rip.text.embed");
    let provenance = embed["provenance"].as_object().unwrap();
    let model_license = provenance
        .get("model_license")
        .and_then(Value::as_str)
        .expect("T03 must record model_license (V-1 resolution)");
    assert!(
        model_license.contains("MIT") && model_license.contains("BAAI"),
        "model_license must cite the resolved license: {model_license}"
    );
    assert_eq!(provenance["license"], "repo-internal");
    let deps = provenance["dependencies"].as_array().unwrap();
    assert!(
        deps.iter()
            .any(|d| d.as_str().unwrap_or_default().contains("bge-small-zh-v1.5")),
        "dependencies must name the model weights"
    );
}
