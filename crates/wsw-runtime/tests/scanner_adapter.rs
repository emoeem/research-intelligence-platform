//! WSW-7 三 Scanner Adapter 集成测试（docs/rip3/wsw/WSW-7-TASK.md S7）。
//!
//! - Nuclei 真实扫描 local_range 靶机（scripts/local_range.py loopback 面，
//!   禁模拟冒充——判据 1）：matched_count 对账 + result.jsonl artifact 在库
//!   + 五态投影 INCONCLUSIVE（命中 ≠ CONFIRMED）+ trace_id 落列；
//! - ZAP 真实扫描同口径（判据 2；镜像 digest 守卫——V-9 SKIP 计数面）；
//! - RTIS stub 委派矩阵 + 真实 poc get V-9 守卫（判据 3；零接触负例硬门
//!   ——scanner.rs 源面 poc add 零词形 grep 断言）；
//! - 预算负例（max_requests 超上限 → input_schema 结构化拒绝；timeout 超
//!   manifest 帽 → budget exceeds 结构化拒绝——判据 6）+ 族并发 1 断言；
//! - TIMED_OUT 看门狗负例（budget.timeout_seconds 压测——判据 5）。
//!
//! V-9 守卫预挂（D6）：podman bin / 镜像 digest / rt.py 真源任一缺 →
//! eprintln SKIP + 早返回（exit 0 计数，不 FAIL）；全离线可跑面（投影/
//! 负例/族帽/RTIS stub）零守卫常跑。`--test-threads=1`。

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use tool_engine::adapter::podman_host::{FamilyLimiter, PodmanHostAdapter, PodmanHostConfig};
use tool_engine::Engine;
use verification_engine::provider::RtisPocProvider;
use wsw_runtime::scanner::ScanError;
use wsw_runtime::scanner::{
    project_normalized, providers_projection, scan_nuclei, scan_rtis, scan_zap, NucleiScanParams,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn podman_available() -> bool {
    Command::new("podman")
        .arg("info")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn image_exists(image: &str) -> bool {
    Command::new("podman")
        .args(["image", "exists", image])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn rtis_real_db_available() -> bool {
    Path::new("/home/emo/.dsh/redteam/knowledge.db").exists()
        && repo_root().join("integrations/rtis/rt.py").exists()
}

/// 本地靶场进程束（spawn + readiness 探活 + Drop 停机——用完即停纪律）。
struct LocalRange {
    child: Child,
    #[allow(dead_code)]
    port: u16,
}

impl LocalRange {
    fn spawn(port: u16) -> Self {
        let child = Command::new("python3")
            .arg(repo_root().join("scripts/local_range.py"))
            .arg("--port")
            .arg(port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("local_range spawn");
        let me = Self { child, port };
        // readiness：/p59c-marker 200 探活（最多 5s）。
        let mut ok = false;
        for _ in 0..50 {
            if req_marker_ready(port) {
                ok = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(ok, "local_range {port} readiness 超时");
        me
    }
}

impl Drop for LocalRange {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn req_marker_ready(port: u16) -> bool {
    // 纯 std TCP 探活 + HTTP GET（零外部依赖——vendor 红线）。
    use std::io::Write;
    use std::net::TcpStream;
    let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) else {
        return false;
    };
    let _ = stream.write_all(
        format!("GET /p59c-marker HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\n\r\n").as_bytes(),
    );
    let mut buf = Vec::new();
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.read_to_end(&mut buf);
    String::from_utf8_lossy(&buf).contains("RIP3-P59C-LOCAL-RANGE-MARKER")
}

/// scan 运行库（临时文件库——tool_run/artifact 行落库对账载体，判据 1）。
struct ScanDb {
    path: PathBuf,
}

impl ScanDb {
    fn open(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("wsw7-scan-{tag}-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("scan-runs.db");
        let pool = database::Pool::open(database::PoolConfig {
            path: database::DbPath::File(path.clone()),
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(5),
        })
        .expect("scan db pool");
        database::migrate(&pool, &repo_root().join("migrations")).expect("scan db migrate");
        Self { path }
    }

    fn engine(&self) -> Engine {
        let pool = database::Pool::open(database::PoolConfig {
            path: database::DbPath::File(self.path.clone()),
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(5),
        })
        .expect("scan db pool 2");
        let registry = tool_registry::Registry::open(pool.clone());
        for (tool_id, manifest_rel) in [
            (
                "rip.security.nuclei",
                "manifests/tools/rip.security.nuclei.json",
            ),
            ("rip.security.zap", "manifests/tools/rip.security.zap.json"),
        ] {
            if registry.query(tool_id).is_ok() {
                continue; // query-first 幂等（rp-wsw scan_engine 同款）
            }
            let text = std::fs::read_to_string(repo_root().join(manifest_rel)).expect("manifest");
            let manifest: serde_json::Value = serde_json::from_str(&text).expect("manifest json");
            registry.register(&manifest).expect("register");
        }
        Engine::open(pool, registry)
    }
}

impl Drop for ScanDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.path.parent().unwrap());
    }
}

fn adapter() -> PodmanHostAdapter {
    PodmanHostAdapter::new(PodmanHostConfig {
        workspace_root: std::env::temp_dir().join(format!(
            "wsw7-ws-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_micros()
        )),
        ..PodmanHostConfig::default()
    })
}

/// tool_run/artifact 行对账（rusqlite 经 database 重导出只读面）。
fn db_rows(db: &ScanDb, run_id: &str) -> (Option<String>, usize, Option<String>) {
    let conn = database::rusqlite::Connection::open_with_flags(
        &db.path,
        database::rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("db open ro");
    let trace: Option<String> = conn
        .query_row(
            "SELECT trace_id FROM tool_run WHERE run_id = ?1",
            [run_id],
            |r| r.get(0),
        )
        .ok();
    let artifacts: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM artifact WHERE run_id = ?1 AND sha256 != ''",
            [run_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let status: Option<String> = conn
        .query_row(
            "SELECT status FROM tool_run WHERE run_id = ?1",
            [run_id],
            |r| r.get(0),
        )
        .ok();
    (trace, artifacts, status)
}

// ---------------------------------------------------------------------------
// 判据 1：Nuclei 真实扫描 local_range 靶机（禁模拟冒充）
// ---------------------------------------------------------------------------

#[test]
fn nuclei_real_scan_local_range_end_to_end() {
    if !podman_available() || !image_exists(verification_engine::provider::NUCLEI_IMAGE) {
        eprintln!("SKIP: podman/rip-nuclei 镜像不在位（V-9 守卫——D6；镜像重建为 e2e 前置步骤）");
        return;
    }
    let range = LocalRange::spawn(8080);
    let _ = &range;
    let db = ScanDb::open("nuclei");
    let engine = db.engine();
    let adapter = adapter();
    let run_id = format!("rn-wsw7-nuclei-e2e-{}", std::process::id());
    let trace_id = format!("tr-wsw7-nuclei-e2e-{}", std::process::id());
    let params = NucleiScanParams {
        target: "http://127.0.0.1:8080".to_owned(),
        template_ref: None,
        template_file: Some(repo_root().join("scripts/local_range_template.yaml")),
        max_requests: 10,
        timeout_s: 120,
    };
    let outcome = scan_nuclei(
        &engine,
        &adapter,
        &params,
        &run_id,
        Some("task-wsw7-e2e"),
        Some(&trace_id),
    )
    .expect("nuclei real dispatch");
    // Provider 面对账：SUCCEEDED + matched_count ≥1（marker 命中）+ 版本/关联锚。
    assert_eq!(outcome.status_word, "SUCCEEDED", "{}", outcome.summary);
    let matched = outcome
        .result
        .as_ref()
        .and_then(|r| r.get("matched_count"))
        .and_then(|v| v.as_i64())
        .expect("matched_count 在场");
    assert!(matched >= 1, "local_range marker 命中对账：{matched}");
    assert!(outcome.tool_version.is_some(), "工具版本恒在");
    assert_eq!(outcome.rule_ref, None, "无 template_ref 面 rule_ref 空");
    // 五态投影：命中 ≠ CONFIRMED → INCONCLUSIVE（A9）。
    let normalized = project_normalized(&outcome);
    assert_eq!(normalized.result_status, "INCONCLUSIVE");
    // 落库对账：tool_run.trace_id 落列 + result.jsonl artifact sha256 在库。
    let (trace, artifacts, status) = db_rows(&db, outcome.tool_run_id.as_deref().unwrap_or(""));
    assert_eq!(
        trace.as_deref(),
        Some(trace_id.as_str()),
        "trace_id 015 落列"
    );
    assert_eq!(status.as_deref(), Some("SUCCEEDED"));
    assert!(
        artifacts >= 1,
        "result.jsonl artifact 在库（sha256 登记面）"
    );
}

// ---------------------------------------------------------------------------
// 判据 2：ZAP 真实扫描同口径（镜像 digest 守卫——V-9 SKIP 计数面）
// ---------------------------------------------------------------------------

#[test]
fn zap_real_scan_local_range_end_to_end() {
    if !podman_available() || !image_exists(verification_engine::provider::ZAP_IMAGE) {
        eprintln!("SKIP: podman/rip-zap 镜像不在位（V-9 守卫——D6；选项 B 词形）");
        return;
    }
    let range = LocalRange::spawn(8080);
    let _ = &range;
    let db = ScanDb::open("zap");
    let engine = db.engine();
    let adapter = adapter();
    let run_id = format!("rn-wsw7-zap-e2e-{}", std::process::id());
    let trace_id = format!("tr-wsw7-zap-e2e-{}", std::process::id());
    let outcome = scan_zap(
        &engine,
        &adapter,
        &wsw_runtime::scanner::ZapScanParams {
            target: "http://127.0.0.1:8080".to_owned(),
            timeout_s: 420,
            alert_budget: 100,
        },
        &run_id,
        Some("task-wsw7-e2e"),
        Some(&trace_id),
    )
    .expect("zap real dispatch");
    // D-TC-03 帧对账：matched_count = 报告 alerts 数（wrapper python3 解析面）
    // + zap_version 输出 + alerts_excerpt 词形（判据 2）。
    assert_eq!(outcome.status_word, "SUCCEEDED", "{}", outcome.summary);
    let result = outcome.result.as_ref().expect("result object（信封守卫）");
    assert!(result
        .get("matched_count")
        .and_then(|v| v.as_i64())
        .is_some());
    assert!(result
        .get("alerts_excerpt")
        .and_then(|v| v.as_str())
        .is_some());
    let version = result
        .get("zap_version")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(!version.is_empty(), "zap_version 输出（版本恒在纪律）");
    // 五态投影：alerts 命中 ≠ CONFIRMED → INCONCLUSIVE。
    let normalized = project_normalized(&outcome);
    assert_eq!(normalized.result_status, "INCONCLUSIVE");
    // 落库对账：tool_run/artifact 双面。
    let (trace, artifacts, status) = db_rows(&db, outcome.tool_run_id.as_deref().unwrap_or(""));
    assert_eq!(trace.as_deref(), Some(trace_id.as_str()));
    assert_eq!(status.as_deref(), Some("SUCCEEDED"));
    assert!(artifacts >= 1, "zap-report.json artifact 在库");
}

// ---------------------------------------------------------------------------
// 判据 3：RTIS 委派矩阵（stub 零真实库依赖）+ 真实 poc get V-9 守卫
// ---------------------------------------------------------------------------

#[test]
fn rtis_stub_delegation_matrix() {
    let tmp = std::env::temp_dir().join(format!("wsw7-rtis-matrix-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let stub = tmp.join("rt.py");
    std::fs::write(
        &stub,
        r#"#!/usr/bin/env python3
import argparse, json, sys
p = argparse.ArgumentParser()
sub = p.add_subparsers(dest="cmd")
g = sub.add_parser("poc"); g.add_argument("action")
g.add_argument("--id", type=int)
g.add_argument("--on")
g.add_argument("--json", action="store_true")
args = p.parse_args()
if args.cmd == "poc" and args.action == "get":
    print(json.dumps({"id": args.id, "title": "stub poc", "category": "other", "kind": "script",
                      "language": "bash", "severity": "info", "verified": 0, "hit_count": 1,
                      "updated_at": "2026-01-01T00:00:00Z",
                      "content": "SECRET-POC-BODY-SHOULD-NOT-LEAK"}))
    sys.exit(0)
if args.cmd == "poc" and args.action == "use":
    print(json.dumps({"poc_id": args.id, "used_on": args.on, "recorded": True}))
    sys.exit(0)
print(json.dumps({"error": "unsupported"})); sys.exit(3)
"#,
    )
    .unwrap();
    let provider = RtisPocProvider {
        rt_py: stub,
        python: "python3".to_owned(),
    };
    // 正例矩阵：元数据锚 + 回填 + 五态投影 not_tested。
    let frame = scan_rtis(&provider, 900008, "http://127.0.0.1:8080").expect("stub 正例");
    assert_eq!(frame["metadata"]["id"], 900008);
    assert_eq!(frame["metadata"]["content_omitted"], true);
    assert_eq!(frame["use_recorded"]["recorded"], true);
    let projected = wsw_runtime::scanner::project_rtis(&frame);
    assert_eq!(projected["result_status"], "not_tested");
    // POC 正文零复制硬门（红线 5）。
    let text = serde_json::to_string(&frame).unwrap();
    assert!(!text.contains("SECRET-POC-BODY-SHOULD-NOT-LEAK"));
    // 负例：exit 非零结构化透出（非 panic 非 0）。
    let bad = tmp.join("rt_bad.py");
    std::fs::write(&bad, "#!/usr/bin/env python3\nimport sys\nsys.exit(4)\n").unwrap();
    let bad_provider = RtisPocProvider {
        rt_py: bad,
        python: "python3".to_owned(),
    };
    let err = scan_rtis(&bad_provider, 900008, "http://127.0.0.1:8080").expect_err("stub 负例");
    assert_eq!(err.category, "tool_unavailable");
    assert!(err.message.contains("poc get failed"));
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn rtis_real_poc_get_metadata_readback() {
    if !rtis_real_db_available() {
        eprintln!("SKIP: RTIS 数据真源不在位（V-9 gitignore 运行时数据词形——D6）");
        return;
    }
    let provider = RtisPocProvider::from_repo_root(&repo_root());
    // 真实 poc get 元数据回读（只读面；真实库 ID 空间锚 900008 在位）。
    let metadata = provider
        .resolve_metadata(900008)
        .expect("real poc get metadata");
    assert_eq!(metadata["id"], 900008);
}

/// 零接触负例硬门（判据 3）：scanner 源面零 poc add/update / 零 RTIS 库直写
/// 词形 grep 断言（RTIS 交互唯一路径 = poc get/use 两接口；禁词形 = rt.py
/// arg-vector 形——文档级提及不算派发词形）。
#[test]
fn rtis_zero_touch_source_grep() {
    let scanner_rs = repo_root().join("crates/wsw-runtime/src/scanner.rs");
    let text = std::fs::read_to_string(&scanner_rs).expect("scanner.rs 在场");
    for banned in ["\"poc\", \"add\"", "\"poc\", \"update\"", "knowledge.db"] {
        assert!(
            !text.contains(banned),
            "scanner.rs 派发禁词形在场：{banned}（RTIS 零接触负例硬门）"
        );
    }
    // 两接口词形在场（poc get / poc use 唯一路径）。
    assert!(text.contains("resolve_metadata"), "poc get 锚");
    assert!(text.contains("record_use"), "poc use 锚");
    // rp-wsw/verification-engine 面 RTIS 通道同口径反证（poc update 词形
    // 唯一在位面 = provider.rs record_verification（P5-7 通道）——scanner.rs
    // 零 update 调用；provider.rs 该调用为多行 arg-vector 形）。
    let provider_rs =
        std::fs::read_to_string(repo_root().join("crates/verification-engine/src/provider.rs"))
            .expect("provider.rs 在场");
    assert!(
        provider_rs.contains("record_verification") && provider_rs.contains("\"update\""),
        "poc update 唯一在位面 = provider.rs record_verification（P5-7 通道）"
    );
}

// ---------------------------------------------------------------------------
// 判据 6：预算双闸结构化拒绝 + 族并发 1 断言
// ---------------------------------------------------------------------------

#[test]
fn budget_negative_max_requests_and_timeout() {
    let db = ScanDb::open("neg");
    let engine = db.engine();
    let adapter = adapter();
    let run_id = format!("rn-wsw7-neg-{}", std::process::id());
    // max_requests > 1000 → Manifest input_schema 结构化拒绝（VALIDATION 词形）。
    let params = NucleiScanParams {
        target: "http://127.0.0.1:8080".to_owned(),
        template_ref: None,
        template_file: None,
        max_requests: 1001,
        timeout_s: 60,
    };
    let err: ScanError = scan_nuclei(&engine, &adapter, &params, &run_id, None, None)
        .expect_err("max_requests 1001 结构化拒绝");
    assert!(
        err.message.contains("input fails input_schema"),
        "VALIDATION 词形：{}",
        err.message
    );
    assert!(!err.message.is_empty());
    // timeout > manifest 帽（600）→ provider 装配面裁剪（clamp 1..600，
    // D3①「budget.timeout_s 同源 manifest」首闸）+ 闸面拒绝双闸在 engine
    // submit 承载（BudgetExceeds VALIDATION 词形——run budget > manifest
    // 上限结构化拒绝，mcp-server run_tool dispatch 同型）。
    let request = serde_json::json!({
        "protocol_version": "1",
        "tool_id": "rip.security.zap",
        "run_id": format!("rn-wsw7-budget-neg-{}", std::process::id()),
        "submit_key": "scan:budget:neg",
        "input": {"url": "http://127.0.0.1:8080", "target": "http://127.0.0.1:8080"},
        "workspace_id": "ws-verify",
        "budget": {"timeout_seconds": 601, "max_output_bytes": 8388608},
        "result_schema_version": "1"
    });
    let err = engine
        .submit(&request)
        .expect_err("budget.timeout_seconds 601 > manifest cap 600 结构化拒绝");
    assert!(
        err.to_string().contains("budget exceeds manifest cap"),
        "BudgetExceeds VALIDATION 词形：{err}"
    );
    // nuclei 同型（timeout_seconds > 300 cap）。
    let request = serde_json::json!({
        "protocol_version": "1",
        "tool_id": "rip.security.nuclei",
        "run_id": format!("rn-wsw7-budget-neg2-{}", std::process::id()),
        "submit_key": "scan:budget:neg2",
        "input": {"url": "http://127.0.0.1:8080", "target": "http://127.0.0.1:8080"},
        "workspace_id": "ws-verify",
        "budget": {"timeout_seconds": 301, "max_output_bytes": 8388608},
        "result_schema_version": "1"
    });
    let err = engine
        .submit(&request)
        .expect_err("budget.timeout_seconds 301 > manifest cap 300 结构化拒绝");
    assert!(
        err.to_string().contains("budget exceeds manifest cap"),
        "{err}"
    );
    // provider 装配面裁剪闸词形（D3①：601 → clamp 600 进 input；build_input
    // 原样消费面已在 scanner.rs 单测覆盖——此处断言 zap 装配非拒绝面词形）。
    let clamped =
        verification_engine::provider::ZapProvider::build_input("http://127.0.0.1:8080", 601, 100);
    assert_eq!(clamped["timeout_seconds"], 600, "clamp 裁剪闸（601→600）");
}

#[test]
fn scanner_family_concurrency_default_is_one() {
    // 族帽（判据 6）：scanner 族（network）默认并发 1——同族第二次 try_acquire
    // 拒绝（podman_host.rs FamilyLimiter :172 面；MA-2 per-caller 帽词形零改动）。
    assert_eq!(PodmanHostConfig::default().family_concurrency_default, 1);
    let limiter = FamilyLimiter::new(1);
    let _first = limiter
        .try_acquire("network")
        .expect("同族首占成功（并发 1）");
    assert!(
        limiter.try_acquire("network").is_none(),
        "同族第二次派发阻塞/拒绝（族并发 1 实测）"
    );
    // per-caller 顶层帽面在场（MA-2 D7 caller_cap 词形零改动——无 caller 直通；
    // guard 绑定持有——临时丢弃即释放槽位，FamilyGuard Drop 语义）。
    let limiter2 = FamilyLimiter::new(2);
    let g1 = limiter2.try_acquire("network");
    let g2 = limiter2.try_acquire("network");
    assert!(g1.is_some() && g2.is_some(), "帽 2 内两占成功");
    assert!(limiter2.try_acquire("network").is_none(), "帽满第三占拒绝");
    drop(g1);
    assert!(limiter2.try_acquire("network").is_some(), "Drop 释放后回占");
}

// ---------------------------------------------------------------------------
// 判据 5：TIMED_OUT 看门狗负例（budget.timeout_seconds 压测——镜像在位时）
// ---------------------------------------------------------------------------

#[test]
fn zap_watchdog_timeout_negative() {
    if !podman_available() || !image_exists(verification_engine::provider::ZAP_IMAGE) {
        eprintln!("SKIP: rip-zap 镜像不在位（V-9 守卫——看门狗压测随镜像面）");
        return;
    }
    let range = LocalRange::spawn(8080);
    let _ = &range;
    let db = ScanDb::open("timeout");
    let engine = db.engine();
    let adapter = adapter();
    let run_id = format!("rn-wsw7-zap-timeout-{}", std::process::id());
    // budget.timeout_seconds = 1s：ZAP JVM 启动面必超（TIMED_OUT 看门狗负例）。
    let outcome = scan_zap(
        &engine,
        &adapter,
        &wsw_runtime::scanner::ZapScanParams {
            target: "http://127.0.0.1:8080".to_owned(),
            timeout_s: 1,
            alert_budget: 100,
        },
        &run_id,
        None,
        None,
    )
    .expect("timeout dispatch surfaced（看门狗信封交付）");
    assert_eq!(outcome.status_word, "TIMED_OUT", "{}", outcome.summary);
    assert_eq!(outcome.error_category.as_deref(), Some("timeout"));
    // 五态：执行超时 ≠ NOT_REPRODUCED → INCONCLUSIVE（五态纪律规则 2）。
    let normalized = project_normalized(&outcome);
    assert_eq!(normalized.result_status, "INCONCLUSIVE");
    assert_eq!(normalized.error_category.as_deref(), Some("timeout"));
}

// ---------------------------------------------------------------------------
// D8：providers 投影与 manifest 逐字段一致（判据 4 子面；零守卫常跑）
// ---------------------------------------------------------------------------

#[test]
fn providers_projection_manifest_fields() {
    let projection = providers_projection(&repo_root()).expect("projection");
    let rows = projection["providers"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    for row in rows {
        let manifest_text = std::fs::read_to_string(
            repo_root().join(row["manifest"].as_str().unwrap().trim_start_matches("./")),
        )
        .expect("manifest 直读");
        let manifest: serde_json::Value = serde_json::from_str(&manifest_text).unwrap();
        assert_eq!(row["tool_id"], manifest["id"]);
        assert_eq!(row["runtime"], manifest["runtime"]["type"]);
        assert_eq!(row["family"], manifest["family"]);
        assert_eq!(row["permission_level"], manifest["permission_level"]);
        assert_eq!(row["idempotent"], manifest["idempotent"]);
        assert_eq!(row["version"], manifest["version"]);
        if manifest["runtime"].get("image").is_some() {
            assert_eq!(row["image"], manifest["runtime"]["image"]);
        }
    }
    let plugins = &projection["plugins"];
    assert!(plugins["catalog_entry_count"].as_u64().unwrap() >= 1);
    assert_eq!(plugins["source"], "manifests/plugins/catalog.json");
}
