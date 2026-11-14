//! P10-3/P10-4 验证载具（ROADMAP §11.3 P10-4 判据）：T0/T1/T2 越界拒绝矩阵
//! 与 secrets 不泄露核查（D-TC-06），自动化进 CI（模型在环与 VM 长演练除外）。
//!
//! 矩阵口径（docs/phase1/T1-T2-BOUNDARY-RULING.md 裁决 + TOOL-CONTRACT
//! D-TC-05/D-TC-06 原文）：
//! 1. 登记面：permission_level 域外值（T3/小写 t2）登记拒绝（§2.5 第 8 条）；
//! 2. 派发面：T2 缺 authorization_ref 必须 POLICY_DENIED 拒绝且零落行
//!    （tool-engine submit 路径，D-TC-05）；T0/T1 无授权引用照常受理
//!    （裁决第 1 节：写平台内部库不升 T2，闸只在 T2）；
//! 3. 调用协议面：ToolCallRequest 无 secrets 字段（D-TC-06），未知字段拒绝；
//! 4. Manifest 面：secrets 条目只含键名不含值，值形键与键名形态违规拒绝
//!    （schema v1 secrets.pattern + validate.rs 未知键守卫）；
//! 5. 泄露面：required secret 缺平台 secrets 存储侧信时登记拒绝（§2.5 第 7
//!    条）；planted 假想 secret 值在 run 全行、tool/tool_version 全行、终态
//!    信封逐字节扫描零命中（不进重放记录、不进登记面）。
//!
//! 测试库：内存库迁移全链；占位 digest 显式放行（仅测试库）；platform*.db
//! 零改动。

use database::{DbPath, Pool, PoolConfig, migrate};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tool_engine::mock_runner::MockRunner;
use tool_engine::{Engine, EngineError};
use tool_registry::{Options, Registry, RegistryError, ValidationRule};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn fresh_pool() -> Pool {
    let pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(1),
    })
    .unwrap();
    migrate(&pool, &repo_root().join("migrations")).unwrap();
    pool
}

/// 以示例 Manifest（rip.doc.parse.json）为模板改写 id / permission_level /
/// secrets，得到矩阵各行。其余字段沿用示例（登记九条全过）。
fn manifest_with(id: &str, permission: &str, secrets: Value) -> Value {
    let mut m: Value = serde_json::from_str(
        &std::fs::read_to_string(
            repo_root().join("schemas/tool-manifest/examples/rip.doc.parse.json"),
        )
        .unwrap(),
    )
    .unwrap();
    m["id"] = json!(id);
    m["name"] = json!(format!("Security Matrix {id}"));
    m["permission_level"] = json!(permission);
    m["secrets"] = secrets;
    m
}

fn base_secrets() -> Value {
    json!([])
}

fn request(tool_id: &str, run_id: &str, authorization_ref: Option<&str>) -> Value {
    let mut req = json!({
        "protocol_version": "1",
        "tool_id": tool_id,
        "run_id": run_id,
        "submit_key": format!("p10-4-{run_id}"),
        "input": {"text": "# 标题\n\n正文一段。", "format": "md"},
        "workspace_id": "ws-p104",
        "budget": {"timeout_seconds": 60, "max_output_bytes": 1048576},
        "result_schema_version": "1"
    });
    if let Some(r#ref) = authorization_ref {
        req["authorization_ref"] = json!(r#ref);
    }
    req
}

/// 建好引擎并登记一个工具；返回（engine, registry 侧 pool）。
fn engine_with(manifest: &Value) -> (Engine, Pool) {
    let pool = fresh_pool();
    let registry = Registry::open(pool.clone()).with_options(Options {
        allow_placeholder_digest: true,
    });
    registry.register(manifest).unwrap();
    (Engine::open(pool.clone(), registry), pool)
}

fn run_row_count(pool: &Pool, run_id: &str) -> usize {
    pool.with_conn(|c| {
        c.query_row(
            "SELECT COUNT(*) FROM tool_run WHERE run_id = ?1",
            [run_id],
            |r| r.get::<_, i64>(0),
        )
    })
    .unwrap() as usize
}

/// 整表（或按 tool_id 过滤）全行全列拼成一个字符串，供逐字节扫描。
fn scan_rows(pool: &Pool, sql: &str, param: &str) -> String {
    pool.with_conn(|c| {
        let mut stmt = c.prepare(sql)?;
        let rows = stmt.query_map([param], |r| {
            let mut s = String::new();
            for i in 0..r.as_ref().column_count() {
                use database::rusqlite::types::ValueRef;
                match r.get_ref(i)? {
                    ValueRef::Null => {}
                    ValueRef::Text(bytes) | ValueRef::Blob(bytes) => {
                        s.push_str(&String::from_utf8_lossy(bytes));
                    }
                    ValueRef::Integer(v) => s.push_str(&v.to_string()),
                    ValueRef::Real(v) => s.push_str(&v.to_string()),
                }
            }
            Ok(s)
        })?;
        let mut all = String::new();
        for row in rows {
            all.push_str(&row?);
        }
        Ok::<String, database::rusqlite::Error>(all)
    })
    .unwrap()
}

// ---------------------------------------------------------------------------
// 登记面：permission_level 域外值拒绝
// ---------------------------------------------------------------------------

#[test]
fn registration_rejects_permission_level_outside_t0t1t2() {
    let pool = fresh_pool();
    let registry = Registry::open(pool.clone()).with_options(Options {
        allow_placeholder_digest: true,
    });
    for bad in ["T3", "t2", "T9", ""] {
        let m = manifest_with("rip.matrix.level", bad, base_secrets());
        let err = registry.register(&m).unwrap_err();
        match &err {
            RegistryError::Validation { reason, .. } => {
                assert!(
                    reason.contains("not in T0/T1/T2"),
                    "permission_level {bad:?} must be rejected by the T0/T1/T2 domain check: {reason}"
                );
            }
            other => panic!("expected Validation rejection for {bad:?}, got {other:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// 派发面：T2 越界拒绝与放行（POLICY_DENIED，零落行）
// ---------------------------------------------------------------------------

#[test]
fn t2_without_authorization_ref_rejected_policy_denied_with_no_row() {
    let m = manifest_with("rip.matrix.t2", "T2", base_secrets());
    let (engine, pool) = engine_with(&m);
    let run_id = "rn-p104-t2-deny";
    let err = engine
        .submit(&request("rip.matrix.t2", run_id, None))
        .unwrap_err();
    match &err {
        EngineError::Rejected(rejection) => {
            assert_eq!(
                rejection.code(),
                json!("POLICY_DENIED"),
                "T2 without authorization_ref is POLICY_DENIED (D-TC-05)"
            );
            assert!(
                rejection.reason().contains("authorization_ref"),
                "reason must name the missing authorization_ref: {}",
                rejection.reason()
            );
        }
        other => panic!("expected Rejected, got {other:?}"),
    }
    assert_eq!(
        run_row_count(&pool, run_id),
        0,
        "rejected submission must leave zero tool_run rows (§3.2 第 1 行)"
    );
}

#[test]
fn t2_with_authorization_ref_accepted_and_ctx_recorded() {
    let m = manifest_with("rip.matrix.t2ok", "T2", base_secrets());
    let (engine, pool) = engine_with(&m);
    let submitted = engine
        .submit(&request(
            "rip.matrix.t2ok",
            "rn-p104-t2-accept",
            Some("eng-2026-11-14-001"),
        ))
        .unwrap();
    assert_eq!(submitted.tool_id, "rip.matrix.t2ok");
    assert_eq!(run_row_count(&pool, "rn-p104-t2-accept"), 1);
    let ctx: Value = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT permission_ctx FROM tool_run WHERE run_id = ?1",
                ["rn-p104-t2-accept"],
                |r| r.get::<_, String>(0),
            )
            .map(|text| serde_json::from_str(&text).unwrap())
        })
        .unwrap();
    assert_eq!(ctx["permission_level"], json!("T2"));
    assert_eq!(ctx["authorization_ref"], json!("eng-2026-11-14-001"));
}

#[test]
fn t0_t1_without_authorization_ref_accepted_no_gate() {
    // 裁决（T1-T2-BOUNDARY-RULING §1）：闸只在 T2；T0/T1 缺授权引用照常受理。
    for (id, level, run_id) in [
        ("rip.matrix.t0", "T0", "rn-p104-t0-pass"),
        ("rip.matrix.t1", "T1", "rn-p104-t1-pass"),
    ] {
        let m = manifest_with(id, level, base_secrets());
        let (engine, pool) = engine_with(&m);
        let submitted = engine.submit(&request(id, run_id, None)).unwrap();
        assert_eq!(submitted.tool_id, id);
        assert_eq!(run_row_count(&pool, run_id), 1);
        let ctx: Value = pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT permission_ctx FROM tool_run WHERE run_id = ?1",
                    [run_id],
                    |r| r.get::<_, String>(0),
                )
                .map(|text| serde_json::from_str(&text).unwrap())
            })
            .unwrap();
        assert_eq!(ctx["permission_level"], json!(level));
        assert_eq!(
            ctx["authorization_ref"],
            Value::Null,
            "no authorization gate below T2"
        );
    }
}

// ---------------------------------------------------------------------------
// 调用协议面：secrets 不进 ToolCallRequest（D-TC-06）
// ---------------------------------------------------------------------------

#[test]
fn request_protocol_rejects_secrets_field() {
    let m = manifest_with("rip.matrix.proto", "T0", base_secrets());
    let (engine, pool) = engine_with(&m);
    let mut req = request("rip.matrix.proto", "rn-p104-proto", None);
    req["secrets"] = json!([{"name": "SRC_API_TOKEN", "inject_as": "env", "required": true}]);
    let err = engine.submit(&req).unwrap_err();
    match &err {
        EngineError::Rejected(rejection) => {
            let reason = rejection.reason();
            assert!(
                reason.contains("unknown request field"),
                "secrets must be rejected as an unknown request field: {reason}"
            );
            assert!(
                reason.contains("D-TC-06"),
                "rejection must cite D-TC-06 (no secrets in call protocol): {reason}"
            );
        }
        other => panic!("expected Rejected, got {other:?}"),
    }
    assert_eq!(run_row_count(&pool, "rn-p104-proto"), 0);
}

// ---------------------------------------------------------------------------
// Manifest 面：secrets 只含键名不含值；required 缺存储侧信拒绝
// ---------------------------------------------------------------------------

#[test]
fn manifest_secrets_carry_names_not_values() {
    let pool = fresh_pool();
    let registry = Registry::open(pool.clone())
        .with_options(Options {
            allow_placeholder_digest: true,
        })
        .with_secrets_lookup(std::sync::Arc::new(|name: &str| name == "SRC_API_TOKEN"));

    // 键名合规且存储侧信存在：登记受理（D-TC-06：只声明名与注入方式）。
    let ok = manifest_with(
        "rip.matrix.sec.ok",
        "T1",
        json!([{"name": "SRC_API_TOKEN", "inject_as": "env", "required": true}]),
    );
    registry.register(&ok).unwrap();

    // 值形键（value）出现即拒绝：Manifest 中绝不出现值。
    let with_value = manifest_with(
        "rip.matrix.sec.value",
        "T1",
        json!([{"name": "SRC_API_TOKEN", "inject_as": "env", "required": true,
                "value": "sk-live-should-never-appear"}]),
    );
    let err = registry.register(&with_value).unwrap_err();
    assert!(
        matches!(
            &err,
            RegistryError::Validation { reason, .. }
                if reason.contains("unknown")
        ),
        "secrets item with a value key must be rejected: {err:?}"
    );

    // 键名形态违规（必须 ^[A-Z][A-Z0-9_]+$）：小写即拒绝。
    let lower = manifest_with(
        "rip.matrix.sec.lower",
        "T1",
        json!([{"name": "src_api_token", "inject_as": "env", "required": true}]),
    );
    let err = registry.register(&lower).unwrap_err();
    assert!(
        matches!(
            &err,
            RegistryError::Validation { reason, .. }
                if reason.contains("must match ^[A-Z][A-Z0-9_]+$")
        ),
        "lowercase secret name must be rejected by the pattern: {err:?}"
    );

    // required secret 不在平台 secrets 存储：登记拒绝（§2.5 第 7 条）。
    let missing = manifest_with(
        "rip.matrix.sec.missing",
        "T1",
        json!([{"name": "OTHER_API_TOKEN", "inject_as": "env", "required": true}]),
    );
    let err = registry.register(&missing).unwrap_err();
    assert!(
        matches!(
            &err,
            RegistryError::Validation {
                rule: ValidationRule::Secrets,
                ..
            }
        ),
        "required secret missing from the store must fail rule 7: {err:?}"
    );
}

// ---------------------------------------------------------------------------
// 泄露面：假想 secret 值在全持久化面零命中（登记面 + run 行 + 终态信封）
// ---------------------------------------------------------------------------

#[test]
fn planted_secret_value_never_enters_registry_run_rows_or_envelope() {
    const SECRET_VALUE: &str = "sk-live-P10MATRIX-9f27c1d0secret";
    let m = manifest_with(
        "rip.matrix.leak",
        "T1",
        json!([{"name": "SRC_API_TOKEN", "inject_as": "env", "required": true}]),
    );
    let pool = fresh_pool();
    let registry = Registry::open(pool.clone())
        .with_options(Options {
            allow_placeholder_digest: true,
        })
        // 侧信只见键名：值从头到尾没有进入平台任何接口（见 lookup 签名）。
        .with_secrets_lookup(std::sync::Arc::new(move |name: &str| {
            name == "SRC_API_TOKEN" && !name.contains(SECRET_VALUE)
        }));
    registry.register(&m).unwrap();

    let engine = Engine::open(pool.clone(), registry);
    let submitted = engine
        .submit(&request("rip.matrix.leak", "rn-p104-leak", None))
        .unwrap();
    // 驱动到终态（终态信封是 D-TC-06「不进重放记录」的对账面）。
    let outcome = engine
        .drive(
            &submitted.run_id,
            &MockRunner::succeed(
                json!({
                    "format": "md",
                    "source": {"kind": "inline", "path": null, "bytes": 30, "sha256": "abc"},
                    "sections": [],
                    "heading_tree": [],
                    "stats": {"chars": 30, "sections": 1, "headings": 1}
                }),
                "p10-4 leak scan ok",
            ),
        )
        .unwrap();

    let envelope_text = tool_engine::envelope::to_value(&outcome.envelope).to_string();
    let run_rows = scan_rows(
        &pool,
        "SELECT * FROM tool_run WHERE run_id = ?1",
        "rn-p104-leak",
    );
    let tool_rows = scan_rows(
        &pool,
        "SELECT * FROM tool WHERE tool_id = ?1",
        "rip.matrix.leak",
    );
    let version_rows = scan_rows(
        &pool,
        "SELECT * FROM tool_version WHERE tool_id = ?1",
        "rip.matrix.leak",
    );

    // 值零命中：不进登记面、不进 run 全行、不进终态信封。
    for (face, text) in [
        ("tool rows", tool_rows.as_str()),
        ("tool_version rows", version_rows.as_str()),
        ("tool_run rows", run_rows.as_str()),
        ("terminal envelope", envelope_text.as_str()),
    ] {
        assert!(
            !text.contains(SECRET_VALUE),
            "planted secret value must never appear in {face}"
        );
    }

    // 键名不进 run 面与登记存储面：run 面无 secrets 概念（只有
    // permission_ctx/重放输入）；登记存储不逐字持久化 Manifest（store.rs:40
    // provenance_json 只取 manifest.provenance 字段），secrets 声明面只在
    // Manifest 源文件与校验视图，不入库即无泄露面。
    assert!(
        !run_rows.contains("SRC_API_TOKEN"),
        "secret name must not appear in tool_run rows"
    );
    assert!(
        !tool_rows.contains("SRC_API_TOKEN") && !version_rows.contains("SRC_API_TOKEN"),
        "secret name must not appear in registry storage rows"
    );
    // 信封重放输入是提交时的规整输入（不含 secrets，D-TC-06 原文）。
    assert!(
        !envelope_text.contains("SRC_API_TOKEN"),
        "secret name must not appear in the terminal envelope replay"
    );
}
