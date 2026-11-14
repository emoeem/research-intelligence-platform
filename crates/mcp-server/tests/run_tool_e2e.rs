//! P9-2 rip.run_tool 端到端集成测试（ROADMAP §10.3 P9-2 + ask 自验判据）：
//! - 无令牌 tools/call 被拒（-32001，禁止匿名）；
//! - T2 authorization_ref 校验保留（D-TC-05 POLICY_DENIED，拒绝不落行）；
//! - 真实 python_worker 全链：run_tool → Engine 受理 → execute_via_engine →
//!   SUCCEEDED 结构化结果 + Artifact + Evidence 引用（P7-2 同一登记实现，
//!   face=rp-mcp），tool_run / artifact / evidence 行对账；
//! - rip.get_run_status 运行投影与 rip.cancel_run 协作式取消（D-TC-04）；
//! - 容器工具（rip.http.inspect）在本机无 podman 时如实 WAITING_FOR_RUNNER
//!   （计划 §5.6:336，探测失败不 FAILED，run 行在库；V-9：podman 在位的
//!   环境跳过本用例，容器全链由 tool-engine web_research_e2e 覆盖）。
//!
//! 纪律：临时库自建（不触碰生产 platform*.db 与 rip-data）；python worker
//! 为真实子进程（python3 缺席按 V-9 跳过）；tool-engine / runtime-gateway
//! 为 dev-dep（测试装配与守卫用，执行面与 bin 同一 RunChannel 路径）。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use database::{migrate, DbPath, Pool, PoolConfig};
use serde_json::{json, Value};

use mcp_server::dispatch::RunChannel;
use mcp_server::{LineOutcome, McpServer};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

static TMP_SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// 测试工作区（run_db / artifacts / workspaces / 内联 Manifest 目录）。
struct Fixture {
    _dir: PathBuf,
    run_db: PathBuf,
    artifacts_dir: PathBuf,
    workspaces_root: PathBuf,
    manifest_dirs: Vec<PathBuf>,
    extra_manifest_dir: PathBuf,
}

fn fixture(tag: &str) -> Fixture {
    let dir = std::env::temp_dir().join(format!(
        "rp-mcp-run-e2e-{}-{}-{}",
        tag,
        std::process::id(),
        TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).expect("fixture dir");
    let run_db = dir.join("run.db");
    let artifacts_dir = dir.join("artifacts");
    let workspaces_root = dir.join("workspaces");
    let extra_manifest_dir = dir.join("manifests");
    std::fs::create_dir_all(&extra_manifest_dir).expect("manifest dir");
    Fixture {
        _dir: dir,
        run_db,
        artifacts_dir,
        workspaces_root,
        manifest_dirs: vec![
            repo_root().join("manifests/tools"),
            repo_root().join("containerfiles/rip-tool-webresearch"),
            extra_manifest_dir.clone(),
        ],
        extra_manifest_dir,
    }
}

impl Fixture {
    fn channel(&self) -> RunChannel {
        RunChannel {
            run_db: self.run_db.clone(),
            migrations_dir: repo_root().join("migrations"),
            manifest_dirs: self.manifest_dirs.clone(),
            artifacts_dir: self.artifacts_dir.clone(),
            workspaces_root: self.workspaces_root.clone(),
            runtime_node_id: "host-e2e".to_string(),
            working_dir: Some(repo_root()),
        }
    }

    fn server(&self, token: Option<&str>) -> McpServer {
        McpServer {
            main_db: repo_root().join("migrations/platform.db"),
            aux_db: None,
            plugin_db: None,
            token: token.map(str::to_string),
            run_channel: Some(self.channel()),
        }
    }
}

/// 单请求进出（tools/call → 解析信封）。
fn roundtrip(server: &McpServer, name: &str, arguments: Value, id: i64) -> Value {
    let request = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": { "name": name, "arguments": arguments },
    });
    match server.handle_line(&request.to_string()) {
        LineOutcome::Reply(line) => serde_json::from_str(&line).expect("jsonrpc reply"),
        other => panic!("expected reply, got {other:?}"),
    }
}

/// 解析 tools/call result 的 content[0].text 载荷。
fn payload_of(reply: &Value) -> Value {
    serde_json::from_str(reply["result"]["content"][0]["text"].as_str().expect("text"))
        .expect("payload json")
}

/// python3 在位守卫（V-9：真实 worker 用例保持本地可执行环境）。
fn python3_available() -> bool {
    Command::new("python3")
        .arg("-V")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn podman_available() -> bool {
    Command::new("podman")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 运行库直连（断言面；库由 RunChannel 首次调用时迁移建立）。
fn run_pool(fix: &Fixture) -> Pool {
    Pool::open(PoolConfig {
        path: DbPath::File(fix.run_db.clone()),
        readonly: true,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .expect("run db pool")
}

fn count(fix: &Fixture, table: &str, where_clause: &str) -> i64 {
    let pool = run_pool(fix);
    pool.with_conn(|c| {
        c.query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE {where_clause}"),
            [],
            |r| r.get::<_, i64>(0),
        )
    })
    .expect("count")
}

/// 在运行库登记 Manifest（rp-tool register 的库面同款：Registry::register）。
fn register_manifest(fix: &Fixture, manifest: &Value) {
    let pool = Pool::open(PoolConfig {
        path: DbPath::File(fix.run_db.clone()),
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .expect("run db pool");
    migrate(&pool, &repo_root().join("migrations")).expect("migrate");
    tool_registry::Registry::open(pool)
        .register(manifest)
        .expect("register");
}

const T2_DEMO_MANIFEST_ID: &str = "rip.t2.demo";

/// T2 演示工具（parsing_worker 本体 + T2 权限面）：授权引用校验的正负两路
/// 都打真实 worker（input/输出形状与 rip.doc.parse 同族，output_schema
/// 取宽 object（rip.t.debug 先例），引擎侧仍按 output_schema 强校验）。
fn write_t2_demo_manifest(fix: &Fixture) -> Value {
    let manifest = json!({
        "schema_version": "1",
        "id": T2_DEMO_MANIFEST_ID,
        "name": "T2 Demo Parser",
        "version": "0.1.0",
        "kind": "tool",
        "family": "document",
        "description": "P9-2 e2e 专用 T2 权限面演示工具（parsing_worker 本体，验 authorization_ref 校验保留）",
        "runtime": {
            "type": "python_worker",
            "protocol_version": "1",
            "worker_name": "parsing-worker",
            "command": ["python3", "-I", "workers/python/parsing_worker.py"],
            "working_dir": "repo://research-platform",
            "restart": "pooled"
        },
        "input_schema": {
            "type": "object",
            "additionalProperties": false,
            "required": ["text"],
            "properties": { "text": { "type": "string" } }
        },
        "output_schema": { "type": "object" },
        "resources": { "cpu": "1", "memory": "256Mi", "timeout_seconds": 60 },
        "network": { "mode": "none" },
        "secrets": [],
        "permission_level": "T2",
        "idempotent": false,
        "health": { "probe": { "type": "worker_ping" } },
        "provenance": {
            "origin": "rip-native",
            "license": "repo-internal",
            "dependencies": [],
            "tested": "P9-2 e2e 实跑"
        }
    });
    let path = fix
        .extra_manifest_dir
        .join(format!("{T2_DEMO_MANIFEST_ID}.json"));
    std::fs::write(&path, serde_json::to_vec_pretty(&manifest).unwrap()).expect("write manifest");
    manifest
}

// ---------------------------------------------------------------------------
// 判据一：无令牌 tools/call 被拒（禁止匿名）
// ---------------------------------------------------------------------------

#[test]
fn run_tool_without_token_is_rejected_32001() {
    let fix = fixture("no-token");
    let server = fix.server(None);
    let reply = roundtrip(
        &server,
        "rip.run_tool",
        json!({"tool_id": "rip.doc.parse", "input": {"text": "x"}}),
        1,
    );
    assert_eq!(reply["error"]["code"], -32001, "无令牌 run_tool 被拒: {reply}");
    // 运行库零副作用（未建库未落行）。
    assert!(!fix.run_db.exists(), "无令牌调用不得创建运行库");
}

// ---------------------------------------------------------------------------
// 判据二：T2 authorization_ref 校验保留（D-TC-05 POLICY_DENIED，拒绝不落行）
// ---------------------------------------------------------------------------

#[test]
fn t2_without_authorization_ref_is_policy_denied_and_lands_no_run_row() {
    if !python3_available() {
        eprintln!("SKIP: python3 unavailable（V-9）");
        return;
    }
    let fix = fixture("t2-denied");
    let manifest = write_t2_demo_manifest(&fix);
    register_manifest(&fix, &manifest);
    let server = fix.server(Some("tok"));
    let reply = roundtrip(
        &server,
        "rip.run_tool",
        json!({"tool_id": T2_DEMO_MANIFEST_ID, "input": {"text": "# 标题\n\n正文。"}}),
        1,
    );
    assert_eq!(reply["result"]["isError"], true, "T2 缺授权引用必须 isError=true: {reply}");
    let payload = payload_of(&reply);
    assert_eq!(payload["error"]["category"], "POLICY_DENIED", "D-TC-05 类别: {payload}");
    assert!(
        payload["error"]["message"]
            .as_str()
            .unwrap()
            .contains("authorization_ref"),
        "拒绝原因点名授权引用: {payload}"
    );
    // D-TC-05：拒绝不落行（tool_run 零行，运行库已建但无 run）。
    assert_eq!(count(&fix, "tool_run", "1=1"), 0, "拒绝不得产生 run 行");
}

// ---------------------------------------------------------------------------
// 判据三：T2 带授权引用走通真实 worker 全链
// ---------------------------------------------------------------------------

#[test]
fn t2_with_authorization_ref_executes_real_worker() {
    if !python3_available() {
        eprintln!("SKIP: python3 unavailable（V-9）");
        return;
    }
    let fix = fixture("t2-granted");
    let manifest = write_t2_demo_manifest(&fix);
    register_manifest(&fix, &manifest);
    let server = fix.server(Some("tok"));
    let reply = roundtrip(
        &server,
        "rip.run_tool",
        json!({
            "tool_id": T2_DEMO_MANIFEST_ID,
            "input": {"text": "# P9-2\n\n正文一段。"},
            "authorization_ref": "authz-e2e-p92",
            "task_id": "task-p92-t2"
        }),
        1,
    );
    assert_eq!(reply["result"]["isError"], false, "带授权引用应执行成功: {reply}");
    let payload = payload_of(&reply);
    assert_eq!(payload["status"], "SUCCEEDED");
    assert_eq!(payload["result"]["stats"]["sections"], json!(1));
    assert_eq!(payload["transcript"][0], "QUEUED");
    assert_eq!(payload["transcript"].as_array().unwrap().last().unwrap(), "SUCCEEDED");
    // Artifact + Evidence 引用与库行对账（face=rp-mcp，P7-2 同一登记实现）。
    let run_id = payload["run_id"].as_str().unwrap();
    assert!(!payload["artifact_id"].as_str().unwrap().is_empty());
    assert_eq!(payload["evidence_refs"].as_array().unwrap().len(), 1);
    assert_eq!(count(&fix, "tool_run", &format!("run_id = '{run_id}' AND status = 'SUCCEEDED'")), 1);
    assert_eq!(count(&fix, "artifact", &format!("run_id = '{run_id}'")), 1);
    assert_eq!(
        count(&fix, "evidence", &format!("source_pk = '{run_id}' AND captured_by = 'rp-mcp:run_tool'")),
        1
    );
}

// ---------------------------------------------------------------------------
// 判据四：T0 python_worker 全链（rip.doc.parse 真实执行 + 引用对账 +
// get_run_status 投影 + 终态 cancel 拒绝）
// ---------------------------------------------------------------------------

#[test]
fn run_tool_doc_parse_full_channel_with_status_and_cancel() {
    if !python3_available() {
        eprintln!("SKIP: python3 unavailable（V-9）");
        return;
    }
    // 同源目录读面（Tool Center 同源）依赖 migrations/platform.db（gitignore
    // 运行时数据，B2/B5 同款）：CI 与新机器不在时按 V-9 跳过；受控环境实跑。
    if !repo_root().join("migrations/platform.db").exists() {
        eprintln!(
            "SKIP: migrations/platform.db 不在本地（gitignore 运行时数据），run_tool 同源回退全链无法实测（V-9，受控环境端到端）"
        );
        return;
    }
    let fix = fixture("doc-parse");
    register_manifest(&fix, &read_repo_manifest("manifests/tools/rip.doc.parse.json"));
    let server = fix.server(Some("tok"));
    let reply = roundtrip(
        &server,
        "rip.run_tool",
        json!({
            "tool_id": "rip.doc.parse",
            "input": {"text": "# 标题\n\n正文一段。", "format": "md"}
        }),
        1,
    );
    assert_eq!(reply["result"]["isError"], false, "doc.parse 全链应成功: {reply}");
    let payload = payload_of(&reply);
    let run_id = payload["run_id"].as_str().unwrap().to_string();
    assert_eq!(payload["tool_id"], "rip.doc.parse");
    assert_eq!(payload["result"]["source"]["kind"], "inline");
    assert!(payload["content_hash"].as_str().unwrap().len() == 64, "同哈希快照: {payload}");
    assert_eq!(count(&fix, "tool_run", &format!("run_id = '{run_id}' AND status = 'SUCCEEDED'")), 1);

    // rip.get_run_status：只读投影 + Artifact/Evidence 引用面。
    let reply = roundtrip(&server, "rip.get_run_status", json!({"run_id": run_id}), 2);
    assert_eq!(reply["result"]["isError"], false, "get_run_status: {reply}");
    let status = payload_of(&reply);
    assert_eq!(status["status"], "SUCCEEDED");
    assert_eq!(status["tool_id"], "rip.doc.parse");
    assert_eq!(status["artifacts"].as_array().unwrap().len(), 1);
    assert_eq!(status["evidence_refs"].as_array().unwrap().len(), 1);

    // rip.cancel_run：终态 run 不可发起取消（D-TC-04，accepted=false）。
    let reply = roundtrip(&server, "rip.cancel_run", json!({"run_id": run_id}), 3);
    let payload = payload_of(&reply);
    assert_eq!(payload["accepted"], false, "终态 run 取消必须拒绝: {payload}");

    // 未知 run 的状态查询 = 结构化 isError。
    let reply = roundtrip(&server, "rip.get_run_status", json!({"run_id": "rn-none"}), 4);
    assert_eq!(reply["result"]["isError"], true);
    let payload = payload_of(&reply);
    assert!(payload["error"]["message"].as_str().unwrap().contains("not found"), "{payload}");

    // rip.get_tool_schema 的 run_db 回退段：目录两库（主库无此行 + aux 未
    // 挂载）没有的行，运行库已登记则按同一形状返回（source=run_db）。
    let reply = roundtrip(
        &server,
        "rip.get_tool_schema",
        json!({"tool_id": "rip.doc.parse"}),
        5,
    );
    assert_eq!(reply["result"]["isError"], false, "run_db schema fallback: {reply}");
    let schema = payload_of(&reply);
    assert_eq!(schema["source"], "run_db");
    assert_eq!(
        schema["input_schema"]["oneOf"][0]["required"],
        json!(["text"]),
        "运行库登记件的 input_schema 同源（oneOf 必填形）: {schema}"
    );
}

// ---------------------------------------------------------------------------
// 判据五：未知工具与目录外参数（拒绝不落行）
// ---------------------------------------------------------------------------

#[test]
fn run_tool_unknown_tool_is_rejected_and_lands_no_run_row() {
    let fix = fixture("unknown-tool");
    let server = fix.server(Some("tok"));
    let reply = roundtrip(
        &server,
        "rip.run_tool",
        json!({"tool_id": "rip.nope", "input": {}}),
        1,
    );
    assert_eq!(reply["result"]["isError"], true);
    let payload = payload_of(&reply);
    assert!(
        payload["error"]["message"].as_str().unwrap().contains("manifest for rip.nope not found"),
        "未登记工具如实报 Manifest 缺失: {payload}"
    );
    assert_eq!(count(&fix, "tool_run", "1=1"), 0, "未知工具不落 run 行");
}

// ---------------------------------------------------------------------------
// 判据六：QUEUED run 的协作式取消（D-TC-04 受理面）
// ---------------------------------------------------------------------------

#[test]
fn cancel_run_marks_cancel_requested_at_for_waiting_run() {
    let fix = fixture("cancel-queued");
    register_manifest(&fix, &read_repo_manifest("manifests/tools/rip.doc.parse.json"));
    // 直接经引擎受理并落 WAITING_FOR_RUNNER（不经 run_tool，隔离取消语义；
    // D-TC-04：仅 RUNNING 与 WAITING_FOR_RUNNER 受理协作式取消）。
    let pool = Pool::open(PoolConfig {
        path: DbPath::File(fix.run_db.clone()),
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .expect("run db pool");
    migrate(&pool, &repo_root().join("migrations")).expect("migrate");
    let engine = tool_engine::Engine::open(pool.clone(), tool_registry::Registry::open(pool));
    engine
        .submit(&json!({
            "protocol_version": "1",
            "tool_id": "rip.doc.parse",
            "run_id": "rn-e2e-cancel",
            "input": {"text": "x"},
            "workspace_id": "ws-e2e"
        }))
        .expect("submit queued run");
    let server = fix.server(Some("tok"));
    // QUEUED 不可发起取消（accepted=false，无副作用）。
    let reply = roundtrip(&server, "rip.cancel_run", json!({"run_id": "rn-e2e-cancel"}), 1);
    let payload = payload_of(&reply);
    assert_eq!(payload["accepted"], false, "QUEUED 不受理取消: {payload}");
    // 第 3 行跃迁落 WAITING_FOR_RUNNER 后取消受理（D-TC-04）。
    engine
        .mark_waiting_for_runner("rn-e2e-cancel")
        .expect("waiting transition");
    let reply = roundtrip(&server, "rip.cancel_run", json!({"run_id": "rn-e2e-cancel"}), 1);
    let payload = payload_of(&reply);
    assert_eq!(payload["accepted"], true, "WAITING_FOR_RUNNER 受理取消: {payload}");
    let reply = roundtrip(&server, "rip.get_run_status", json!({"run_id": "rn-e2e-cancel"}), 2);
    let status = payload_of(&reply);
    assert!(status["cancel_requested_at"].is_string(), "取消时刻落行: {status}");
    // 未知 run 取消 = 结构化 isError。
    let reply = roundtrip(&server, "rip.cancel_run", json!({"run_id": "rn-none"}), 3);
    assert_eq!(reply["result"]["isError"], true);
}

// ---------------------------------------------------------------------------
// 判据七：容器工具（rip.http.inspect）无 podman 时如实 WAITING_FOR_RUNNER
// （run 行在库；探测失败不 FAILED，计划 §5.6:336；V-9：podman 在位环境跳过）
// ---------------------------------------------------------------------------

#[test]
fn container_tool_without_podman_waits_with_run_row_present() {
    if podman_available() {
        eprintln!("SKIP: podman available（容器全链由 web_research_e2e 覆盖，本用例只断言无 Runner 态）");
        return;
    }
    let fix = fixture("container-wait");
    register_manifest(
        &fix,
        &read_repo_manifest("containerfiles/rip-tool-webresearch/tool-manifest.json"),
    );
    let server = fix.server(Some("tok"));
    let reply = roundtrip(
        &server,
        "rip.run_tool",
        json!({"tool_id": "rip.http.inspect", "input": {"url": "https://example.com/"}}),
        1,
    );
    // Gateway 心跳判定：podman 探针失败 → WAITING_FOR_RUNNER（不 FAILED，
    // 不伪造结果；run 行在库对账）。
    assert_eq!(reply["result"]["isError"], false, "等待态不是调用失败: {reply}");
    let payload = payload_of(&reply);
    assert_eq!(payload["status"], "WAITING_FOR_RUNNER", "{payload}");
    assert_eq!(payload["ok"], false, "无结果产出（不伪造）");
    let run_id = payload["run_id"].as_str().unwrap();
    assert_eq!(
        count(&fix, "tool_run", &format!("run_id = '{run_id}' AND status = 'WAITING_FOR_RUNNER'")),
        1,
        "rip.http.inspect run 行在库（WAITING_FOR_RUNNER）"
    );
    // 状态查询同见等待态与节点健康说明。
    let reply = roundtrip(&server, "rip.get_run_status", json!({"run_id": run_id}), 2);
    let status = payload_of(&reply);
    assert_eq!(status["status"], "WAITING_FOR_RUNNER");
    assert_eq!(status["artifacts"].as_array().unwrap().len(), 0, "非终态无 Artifact");
}

/// 读仓库登记件（json 形态；yaml 件由 serde_yaml_ng 解析的路径在
/// dispatch::manifest_for 覆盖）。
fn read_repo_manifest(rel: &str) -> Value {
    let path = repo_root().join(rel);
    serde_json::from_str(&std::fs::read_to_string(path).expect("manifest file")).expect("manifest")
}
