//! P9-4 协议一致性载具（ROADMAP §10.4 验收第 2 条：`cargo test --offline -p
//! rp-mcp-parity`；§10.5 风险 1：MCP 面与 Web 逻辑漂移是最容易复发的债，
//! 载具必须是常驻 CI 门禁而不是一次性验收）。
//!
//! 五个业务操作对 REST 与 MCP 两通道断言同一输出形状与同一审计链：
//! 1. 列工具：REST GET /api/tools vs MCP rip.list_tools（同一合并读面，total
//!    与逐条核心字段一致）+ /api/mcp-catalog 目录计数与能力面词表对账；
//! 2. 查 Schema：REST GET /api/tools?tool_id= vs MCP rip.get_tool_schema
//!    （input/output Schema JSON 逐字节一致）；
//! 3. 跑工具：REST POST /api/workflow-runs（wf-p8-tool 样例，tool-engine
//!    同一受理链）vs MCP rip.run_tool（SUCCEEDED + result 形状一致 + 审计
//!    链锚各自在场：workflow trace_id = wfr- run_id，MCP run 自锚定
//!    trace_id = run_id，015/018「run 即链锚」同一惯例）；
//! 4. 查 Run：两通道互读对方 run（同一运行库）：REST /api/runs?run_id= 读
//!    MCP run（correlation_id / evidence / artifact 关联一致），MCP
//!    rip.get_run_status 读 REST 通道 run；
//! 5. 查 trace：REST /api/traces?id=<run_id> 与 MCP resources/read
//!    rip://runs/<run_id> 对同一 run 解析出同一链锚与同一 evidence 集，
//!    两通道审计链同源（REST /api/traces 的 015 词表 + MCP 资源面同一
//!    tool_run/artifact/evidence 数据面）。
//!
//! 通道实现口径：
//! - REST 通道 = 真实 rp_server.py HTTP 进程（Bearer 鉴权 + aux/运行库注入
//!   + RP_WORKFLOW_BIN 指向 target/debug/rp-workflow），HTTP 客户端为 std
//!     TcpStream 薄实现（vendor 无 HTTP client crate；rp_server 响应恒带
//!     Content-Length 且请求带 Connection: close，读至 EOF）；
//! - MCP 通道 = mcp-server [`McpServer::handle_line`]（bin rp-mcp stdio 循环
//!   同一入口；stdio 循环本身由 mcp-server tests 与 selftest_phase9_mcp.sh
//!   覆盖，载具不重复）；
//! - 两通道指向同一临时主库（001 水位，platform.db 同款水位）+ 同一临时
//!   运行库（全量迁移 + rp-tool register 同库面登记），生产 platform*.db
//!   零接触零写。
//!
//! 前置守卫（V-9）：python3 在位（worker 与 rp_server 都需要）；rp-workflow
//! 二进制缺位时按 Cargo 自动构建（cargo build --offline，vendor 在库）。

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use database::{rusqlite, DbPath, Pool, PoolConfig};
use serde_json::{json, Value};

use mcp_server::dispatch::RunChannel;
use mcp_server::{LineOutcome, McpServer};

const TOOL_ID: &str = "rip.kb.keyword_search";
const WORKFLOW_ID: &str = "wf-p8-tool";
const TOKEN: &str = "parity-token-p9-4";
/// rip.list_tools / 轮询共用输入（keyword_search Schema 要求 keyword）。
const KEYWORD: &str = "etl";

static FIXTURE: OnceLock<Fixture> = OnceLock::new();

/// 载具夹具：临时主库 + 临时运行库 + REST 进程 + MCP 协议面。
struct Fixture {
    run_db: PathBuf,
    port: u16,
    mcp: McpServer,
    _server: std::process::Child,
    server_stdout: PathBuf,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn tmp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rp-mcp-parity-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("fixture dir");
    dir
}

/// 二进制缺位时按 Cargo 自动构建（vendor 在库，--offline 红线不破坏）。
fn ensure_bin(name: &str, pkg: &str) -> PathBuf {
    let path = repo_root().join("target/debug").join(name);
    if !path.exists() {
        let status = Command::new("cargo")
            .args(["build", "--offline", "-q", "-p", pkg, "--bin", name])
            .status()
            .expect("cargo build spawn");
        assert!(status.success(), "cargo build {name} failed");
    }
    assert!(path.exists(), "bin missing after build: {}", path.display());
    path
}

fn python3_available() -> bool {
    Command::new("python3")
        .arg("-V")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 主库：001 水位（migrations/platform.db 同款：tool/knowledge/evidence/
/// audit_event/research_task 在场，tool_run/agent_run/workflow_run 不在）。
fn make_main_db(path: &Path) {
    let conn = rusqlite::Connection::open(path).expect("main db open");
    let sql = std::fs::read_to_string(repo_root().join("migrations/001_init.sql"))
        .expect("read 001_init.sql");
    conn.execute_batch(&sql).expect("apply 001_init.sql");
    conn.close().expect("close main db");
}

/// 运行库：全量迁移（001 到 019，零新增迁移号）+ rp-tool register 库面同款
/// 登记（run_tool_e2e.rs register_manifest 同一实现路径）。
fn make_run_db(path: &Path) {
    let pool = Pool::open(PoolConfig {
        path: DbPath::File(path.to_path_buf()),
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .expect("run db pool");
    database::migrate(&pool, &repo_root().join("migrations")).expect("run db migrate");
    let manifest_text =
        std::fs::read_to_string(repo_root().join("manifests/tools").join(format!("{TOOL_ID}.json")))
            .expect("read manifest");
    let manifest: Value = serde_json::from_str(&manifest_text).expect("parse manifest");
    tool_registry::Registry::open(pool)
        .register(&manifest)
        .expect("register manifest");
}

/// 空闲端口（bind :0 后放弃；进程启动竞态由就绪轮询兜底）。
fn free_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("bind :0")
        .local_addr()
        .expect("local addr")
        .port()
}

fn fixture() -> &'static Fixture {
    FIXTURE.get_or_init(|| {
        assert!(python3_available(), "python3 在位是载具前置（rp_server 与 worker 都依赖）");
        let dir = tmp_dir();
        let main_db = dir.join("main.db");
        let run_db = dir.join("run.db");
        make_main_db(&main_db);
        make_run_db(&run_db);

        // workflow 定义入运行库（rp-workflow put 同一 CLI 面，REST 与 MCP 都
        // 不做登记只做执行）。
        let rp_workflow = ensure_bin("rp-workflow", "workflow-engine");
        let sample = repo_root().join("docs/phase8/samples/p8-tool-node.json");
        let out = Command::new(&rp_workflow)
            .args(["put", sample.to_string_lossy().as_ref(), "--db", run_db.to_string_lossy().as_ref()])
            .current_dir(repo_root().as_os_str())
            .output()
            .expect("rp-workflow put spawn");
        assert!(
            out.status.success(),
            "rp-workflow put failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );

        // REST 通道：真实 rp_server.py 进程（Bearer + aux/运行库注入 +
        // 限频关闭避免轮询撞 429 + workflow 写面指向同一运行库）。
        let port = free_port();
        let server_stdout = dir.join("rp_server.log");
        let log = std::fs::File::create(&server_stdout).expect("server log");
        let server = Command::new("python3")
            .arg("apps/server/rp_server.py")
            .env("RP_PORT", port.to_string())
            .env("RP_AUTH_TOKEN", TOKEN)
            .env("RP_DB_PATH", &main_db)
            .env("RP_AUX_DB", &run_db)
            .env("RP_WORKFLOW_DB", &run_db)
            .env("RP_WORKFLOW_BIN", &rp_workflow)
            .env("RP_RATE_LIMIT_RPM", "0")
            .env("RP_WORKFLOW_LOG_DIR", dir.join("wf-logs"))
            .current_dir(repo_root().as_os_str())
            .stdout(log.try_clone().expect("log clone"))
            .stderr(log)
            .spawn()
            .expect("spawn rp_server.py");

        // MCP 通道：协议面（bin rp-mcp stdio 循环同一入口），同一主库 + 同一
        // aux + 同一运行库（run_tool 自锚定 trace_id 载具断言前提）。
        let mcp = McpServer {
            main_db: main_db.clone(),
            aux_db: Some(run_db.clone()),
            plugin_db: None,
            token: Some(TOKEN.to_string()),
            run_channel: Some(RunChannel {
                run_db: run_db.clone(),
                migrations_dir: repo_root().join("migrations"),
                manifest_dirs: vec![repo_root().join("manifests/tools")],
                artifacts_dir: dir.join("artifacts"),
                workspaces_root: dir.join("workspaces"),
                runtime_node_id: "parity-host".to_string(),
                working_dir: Some(repo_root()),
            }),
        };

        let fix = Fixture { run_db, port, mcp, _server: server, server_stdout };
        wait_rest_ready(&fix);
        fix
    })
}

fn wait_rest_ready(fix: &Fixture) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match http(fix.port, "GET", "/api/stats", Some(TOKEN), None) {
            Ok(resp) if resp.status == 200 => return,
            _ => {}
        }
        assert!(
            Instant::now() < deadline,
            "rp_server 未就绪（30s）：log 见 {}",
            fix.server_stdout.display()
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

// ---------------------------------------------------------------------------
// REST 通道薄客户端（std TcpStream；rp_server HTTP/1.0 + Content-Length +
// Connection: close，读至 EOF）
// ---------------------------------------------------------------------------

struct HttpResp {
    status: u16,
    headers: String,
    body: String,
}

fn http(port: u16, method: &str, target: &str, token: Option<&str>, body: Option<&str>) -> Result<HttpResp, String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .map_err(|e| format!("tcp connect: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("read timeout");
    let body_bytes = body.unwrap_or("").as_bytes();
    let mut request = format!(
        "{method} {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nContent-Length: {}\r\n",
        body_bytes.len()
    );
    if let Some(t) = token {
        request.push_str(&format!("Authorization: Bearer {t}\r\n"));
    }
    if body.is_some() {
        request.push_str("Content-Type: application/json\r\n");
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).map_err(|e| format!("write request: {e}"))?;
    stream.write_all(body_bytes).map_err(|e| format!("write body: {e}"))?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).map_err(|e| format!("read response: {e}"))?;
    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, payload) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| "response head/body split failed".to_string())?;
    let mut lines = head.lines();
    let status_line = lines.next().unwrap_or_default();
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    Ok(HttpResp { status, headers: head.to_string(), body: payload.to_string() })
}

fn rest_get(port: u16, target: &str) -> (u16, Value, String) {
    let resp = http(port, "GET", target, Some(TOKEN), None).expect("rest GET");
    let body = serde_json::from_str(&resp.body).unwrap_or(Value::Null);
    (resp.status, body, resp.headers)
}

fn rest_post(port: u16, target: &str, payload: &Value) -> (u16, Value) {
    let resp = http(port, "POST", target, Some(TOKEN), Some(&payload.to_string()))
        .expect("rest POST");
    let body = serde_json::from_str(&resp.body).unwrap_or(Value::Null);
    (resp.status, body)
}

fn header(headers: &str, name: &str) -> Option<String> {
    headers
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with(&format!("{}:", name.to_ascii_lowercase())))
        .map(|l| l.split_once(':').expect("header colon").1.trim().to_string())
}

// ---------------------------------------------------------------------------
// MCP 通道（协议面 roundtrip）
// ---------------------------------------------------------------------------

fn mcp_roundtrip(server: &McpServer, request: Value) -> Value {
    match server.handle_line(&request.to_string()) {
        LineOutcome::Reply(line) => serde_json::from_str(&line).expect("jsonrpc reply"),
        other => panic!("expected reply, got {other:?}"),
    }
}

fn mcp_call(server: &McpServer, name: &str, args: Value, id: i64) -> Value {
    let reply = mcp_roundtrip(
        server,
        json!({"jsonrpc":"2.0","id":id,"method":"tools/call",
               "params":{"name":name,"arguments":args}}),
    );
    assert_eq!(reply["result"]["isError"], false, "mcp tools/call {name} 必须 isError=false: {reply}");
    serde_json::from_str(reply["result"]["content"][0]["text"].as_str().expect("text"))
        .expect("payload json")
}

fn mcp_read_resource(server: &McpServer, uri: &str, id: i64) -> Value {
    let reply = mcp_roundtrip(
        server,
        json!({"jsonrpc":"2.0","id":id,"method":"resources/read","params":{"uri":uri}}),
    );
    assert!(reply.get("error").is_none(), "resources/read {uri} 被拒: {reply}");
    let contents = reply["result"]["contents"].as_array().expect("contents");
    assert_eq!(contents.len(), 1, "单 contents 条目");
    assert_eq!(contents[0]["uri"], uri, "回填 uri");
    serde_json::from_str(contents[0]["text"].as_str().expect("text")).expect("resource json")
}

/// 轮询 workflow run 至终态（REST 通道异步受理，P8-3 契约）。
fn rest_wait_workflow(port: u16, run_id: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let (status, body, _h) = rest_get(port, &format!("/api/workflow-runs?id={run_id}"));
        assert_eq!(status, 200, "workflow run 详情 200: {body}");
        let state = body["run"]["state"].as_str().unwrap_or_default().to_string();
        if matches!(state.as_str(), "succeeded" | "failed" | "cancelled") {
            assert_eq!(state, "succeeded", "REST 通道 workflow run 终态: {body}");
            return body;
        }
        assert!(Instant::now() < deadline, "workflow run 轮询超时（run {run_id}）");
        std::thread::sleep(Duration::from_millis(400));
    }
}

/// 运行库直查（审计链对账面）。
fn run_db_row(run_db: &Path, sql: &str, param: &str) -> Value {
    let pool = Pool::open(PoolConfig::readonly_file(run_db)).expect("run db ro pool");
    pool.with_conn(|conn| -> rusqlite::Result<Value> {
        let mut stmt = conn.prepare(sql)?;
        let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let rows = stmt.query_map([param], |row| {
            let mut map = serde_json::Map::new();
            for (idx, name) in names.iter().enumerate() {
                let value = match row.get_ref(idx)? {
                    rusqlite::types::ValueRef::Null => Value::Null,
                    rusqlite::types::ValueRef::Integer(i) => json!(i),
                    rusqlite::types::ValueRef::Real(x) => json!(x),
                    rusqlite::types::ValueRef::Text(t) => {
                        json!(String::from_utf8_lossy(t).to_string())
                    }
                    rusqlite::types::ValueRef::Blob(_) => Value::Null,
                };
                map.insert(name.clone(), value);
            }
            Ok(Value::Object(map))
        })?;
        let first: Option<Value> = rows.filter_map(|x| x.ok()).next();
        Ok(first.unwrap_or(Value::Null))
    })
    .expect("run db query")
}

// ---------------------------------------------------------------------------
// 判据一：列工具（REST /api/tools vs MCP rip.list_tools + /api/mcp-catalog
// 对账）
// ---------------------------------------------------------------------------

#[test]
fn op1_list_tools_same_read_face_both_channels() {
    let fix = fixture();
    let port = fix.port;

    // REST 通道（main+aux 合并读面）。
    let (status, rest, _h) = rest_get(port, "/api/tools?limit=200");
    assert_eq!(status, 200);
    assert_eq!(rest["available"], true);

    // MCP 通道（同一读面 handle_tools 口径）。
    let mcp = mcp_call(&fix.mcp, "rip.list_tools", json!({}), 101);

    // 同一输出形状：total / returned / 工具 id 序列（同排序）/ 逐条核心字段。
    assert_eq!(rest["total"], mcp["total"], "目录 total 双通道一致");
    assert_eq!(rest["returned"], mcp["returned"], "returned 双通道一致");
    let rest_ids: Vec<&str> =
        rest["items"].as_array().unwrap().iter().map(|r| r["tool_id"].as_str().unwrap()).collect();
    let mcp_ids: Vec<&str> =
        mcp["items"].as_array().unwrap().iter().map(|r| r["tool_id"].as_str().unwrap()).collect();
    assert_eq!(rest_ids, mcp_ids, "工具 id 序列（排序同源）双通道一致");
    for field in ["kind", "lifecycle", "governance", "permission_level", "source_system", "updated_at"] {
        for (rest_row, mcp_row) in rest["items"].as_array().unwrap().iter().zip(mcp["items"].as_array().unwrap()) {
            assert_eq!(rest_row[field], mcp_row[field],
                "列工具字段 {field} 漂移（tool_id={}）", rest_row["tool_id"]);
        }
    }

    // /api/mcp-catalog 对账面：目录计数 == MCP 注册面 total；能力面词表 ==
    // MCP tools/list 能力面（wire 级断言在这里闭合）。
    let (status, mcp_cat, _h) = rest_get(port, "/api/mcp-catalog");
    assert_eq!(status, 200);
    assert_eq!(mcp_cat["catalog_count"], mcp["total"], "mcp-catalog 计数 == MCP 注册面 total");
    assert_eq!(mcp_cat["reconciliation"]["ok"], true, "读时对账 ok: {mcp_cat}");
    let capability_names: Vec<String> = mcp_cat["capability_tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let combined = mcp_roundtrip(
        &fix.mcp,
        json!({"jsonrpc":"2.0","id":102,"method":"tools/list","params":{"limit":200}}),
    );
    let wire_capability: Vec<String> = combined["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["tier"] == "capability")
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(capability_names, wire_capability, "能力面词表双通道一致");
}

// ---------------------------------------------------------------------------
// 判据二：查 Schema（REST ?tool_id= vs MCP rip.get_tool_schema）
// ---------------------------------------------------------------------------

#[test]
fn op2_get_tool_schema_identical_both_channels() {
    let fix = fixture();

    let (status, rest, _h) = rest_get(fix.port, &format!("/api/tools?tool_id={TOOL_ID}"));
    assert_eq!(status, 200);
    let mcp = mcp_call(&fix.mcp, "rip.get_tool_schema", json!({"tool_id": TOOL_ID}), 201);

    // 同一输出形状：input/output Schema 逐字节一致（同一 tool 表列直读）。
    assert_eq!(rest["input_schema"], mcp["input_schema"], "input Schema 双通道一致");
    assert_eq!(rest["output_schema"], mcp["output_schema"], "output Schema 双通道一致");
    assert_eq!(mcp["tool_id"], TOOL_ID);
    assert_eq!(rest["source"], mcp["source"], "Schema 读面（aux 回退）一致");
    assert!(rest["input_schema"].is_object(), "fixture 工具带真实 input Schema");
}

// ---------------------------------------------------------------------------
// 判据三：跑工具（REST workflow 受理 vs MCP rip.run_tool，同一 tool-engine）
// ---------------------------------------------------------------------------

#[test]
fn op3_run_tool_same_engine_both_channels() {
    let fix = fixture();
    let port = fix.port;

    // REST 通道：POST 受理 → 轮询终态 → tool_run_id。
    let (status, accepted) = rest_post(
        port,
        "/api/workflow-runs",
        &json!({"workflow_id": WORKFLOW_ID, "input": {"target": KEYWORD}}),
    );
    assert_eq!(status, 200, "REST 受理 200: {accepted}");
    assert_eq!(accepted["result"], "accepted");
    let wfr_id = accepted["run_id"].as_str().expect("run_id").to_string();
    assert_eq!(accepted["correlation_id"], json!(wfr_id), "受理响应 correlation_id = run_id");
    let detail = rest_wait_workflow(port, &wfr_id);
    let rest_run_id = detail["correlation"]["tool_run_id"].as_str().expect("tool_run_id").to_string();

    // MCP 通道：rip.run_tool 同一引擎受理链（submit_correlated 自锚定）。
    let payload = mcp_call(
        &fix.mcp,
        "rip.run_tool",
        json!({"tool_id": TOOL_ID, "input": {"keyword": KEYWORD, "per_group_limit": 2}}),
        301,
    );
    assert_eq!(payload["ok"], true, "MCP run_tool 成功: {payload}");
    assert_eq!(payload["status"], "SUCCEEDED");
    let mcp_run_id = payload["run_id"].as_str().expect("mcp run_id").to_string();

    // 同一输出形状：result 关键字段一致（同一工具同输入，确定性输出）。
    let result = &payload["result"];
    assert!(result.is_object(), "结构化 result 为 object");
    assert_eq!(result["keyword"], json!(KEYWORD));
    assert_eq!(result["per_group_limit"], json!(2));
    assert!(result["total_hits"].is_i64(), "total_hits 数值");
    assert_eq!(result["groups"].as_array().map(Vec::len), Some(5), "五组检索面");
    // REST 侧：workflow tool 节点输出 = 同一引擎信封 result（node_tool 返回
    // 结构化 result 合入 run state），与 MCP result 逐字段全等对照。
    let tool_node = detail["nodes"]
        .as_array()
        .expect("workflow nodes 数组")
        .iter()
        .find(|n| n["node_kind"] == "tool")
        .expect("tool 节点在 run 详情 nodes 中");
    assert_eq!(
        tool_node["output"],
        payload["result"],
        "REST tool 节点输出 == MCP result（同一引擎同一输入确定性输出）"
    );

    // 同一审计链：两 run 行落同一运行库 tool_run 表，链锚惯例一致
    // （workflow run trace_id = wfr-id；MCP run 自锚定 trace_id = run-id，
    // 015/018「run 即链锚」同一惯例），工作区面名按通道身份区分。
    let rest_row = run_db_row(&fix.run_db, "SELECT run_id, tool_id, tool_version, status, trace_id, workspace_id FROM tool_run WHERE run_id = ?1", &rest_run_id);
    let mcp_row = run_db_row(&fix.run_db, "SELECT run_id, tool_id, tool_version, status, trace_id, workspace_id FROM tool_run WHERE run_id = ?1", &mcp_run_id);
    assert_eq!(rest_row["tool_id"], json!(TOOL_ID));
    assert_eq!(mcp_row["tool_id"], json!(TOOL_ID));
    assert_eq!(rest_row["tool_version"], mcp_row["tool_version"], "版本解析同源");
    assert_eq!(rest_row["status"], json!("SUCCEEDED"));
    assert_eq!(mcp_row["status"], json!("SUCCEEDED"));
    assert_eq!(rest_row["trace_id"], json!(wfr_id), "workflow 通道链锚 = wfr run_id（018）");
    assert_eq!(mcp_row["trace_id"], json!(mcp_run_id), "MCP 通道自锚定 trace_id = run_id（015 同惯例）");
    assert_eq!(rest_row["workspace_id"], json!("ws-workflow"));
    assert_eq!(mcp_row["workspace_id"], json!("ws-mcp"));
}

// ---------------------------------------------------------------------------
// 判据四：查 Run（两通道互读对方 run：同一运行库，同一关联面）
// ---------------------------------------------------------------------------

#[test]
fn op4_get_run_cross_channel_same_correlation() {
    let fix = fixture();
    let port = fix.port;
    let (wfr_id, rest_run_id, mcp_run_id, mcp_payload) = run_both_channels(401);

    // MCP 通道读 REST 通道 run（同一运行库只读投影）。
    let mcp_read_rest = mcp_call(&fix.mcp, "rip.get_run_status", json!({"run_id": rest_run_id}), 402);
    assert_eq!(mcp_read_rest["status"], "SUCCEEDED");
    assert_eq!(mcp_read_rest["trace_id"], json!(wfr_id), "MCP 读 REST run 得同一链锚");

    // REST 通道读 MCP 通道 run：correlation_id + evidence/artifact 关联与
    // MCP 信封引用集一致。
    let target = format!("/api/runs?run_id={mcp_run_id}");
    let (status, rest, headers) = rest_get(port, &target);
    assert_eq!(status, 200);
    assert_eq!(rest["correlation_id"], json!(mcp_run_id), "P9-3 correlation_id = run_id");
    let evidence_ids: Vec<&str> = rest["correlation"]["evidence_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let mcp_evidence: Vec<&str> = mcp_payload["evidence_refs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(evidence_ids, mcp_evidence, "evidence 关联集双通道一致");
    let artifact_ids: Vec<&str> = rest["correlation"]["artifact_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        artifact_ids.iter().any(|a| *a == mcp_payload["artifact_id"].as_str().unwrap()),
        "REST artifact 关联含 MCP 信封 artifact_id"
    );
    // P9-3 关联头：REST run 详情对 MCP run 一样发 X-Trace-Id/X-Correlation-Id。
    assert_eq!(header(&headers, "X-Correlation-Id").as_deref(), Some(mcp_run_id.as_str()));
    assert_eq!(header(&headers, "X-Trace-Id").as_deref(), mcp_payload["run_id"].as_str());

    // 未知 run：两通道同一「不存在」语义（REST 404 no_run / MCP VALIDATION）。
    let (status, rest_err, _h) = rest_get(port, "/api/runs?run_id=rn-parity-unknown");
    assert_eq!(status, 404);
    assert_eq!(rest_err["error"]["code"], "no_run");
    let reply = mcp_roundtrip(
        &fix.mcp,
        json!({"jsonrpc":"2.0","id":403,"method":"tools/call",
               "params":{"name":"rip.get_run_status","arguments":{"run_id":"rn-parity-unknown"}}}),
    );
    assert_eq!(reply["result"]["isError"], true);
    let unknown_payload: Value =
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(unknown_payload["error"]["category"], "VALIDATION", "未知 run VALIDATION 语义一致");
}

// ---------------------------------------------------------------------------
// 判据五：查 trace（REST /api/traces vs MCP rip://runs/<run_id>，同一链锚）
// ---------------------------------------------------------------------------

#[test]
fn op5_trace_same_audit_chain_both_channels() {
    let fix = fixture();
    let port = fix.port;
    let (wfr_id, rest_run_id, mcp_run_id, _mcp_payload) = run_both_channels(501);

    // REST /api/traces：两通道 run 各自解析出链锚（workflow = wfr-id 经
    // tool_run 探针；MCP = 自锚定 run_id）。
    let (status, rest_trace_rest, _h) = rest_get(port, &format!("/api/traces?id={rest_run_id}"));
    assert_eq!(status, 200);
    assert_eq!(rest_trace_rest["trace_id"], json!(wfr_id), "REST run 链锚 = wfr-id");
    let (status, rest_trace_mcp, _h) = rest_get(port, &format!("/api/traces?id={mcp_run_id}"));
    assert_eq!(status, 200);
    assert_eq!(rest_trace_mcp["trace_id"], json!(mcp_run_id), "MCP run 经 REST trace 面反查 = 自锚定 id");

    // MCP rip://runs/<run_id>：与 rip.get_run_status 同一投影（run 行 +
    // evidence + artifact + trace 锚），两 run 的链锚与 REST /api/traces
    // 解析一致（同一审计链语义，不因通道而异）。
    let mcp_resource_rest = mcp_read_resource(&fix.mcp, &format!("rip://runs/{rest_run_id}"), 502);
    assert_eq!(mcp_resource_rest["trace_id"], json!(wfr_id), "资源面 REST run 链锚一致");
    let mcp_resource_mcp = mcp_read_resource(&fix.mcp, &format!("rip://runs/{mcp_run_id}"), 503);
    assert_eq!(mcp_resource_mcp["trace_id"], json!(mcp_run_id), "资源面 MCP run 链锚一致");

    // evidence 集一致（资源面 evidence 列表 == REST correlation.evidence_ids）。
    let mcp_resource_evidence: Vec<&str> = mcp_resource_mcp["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["evidence_id"].as_str().unwrap())
        .collect();
    let (_s, rest_run_detail, _h) = rest_get(port, &format!("/api/runs?run_id={mcp_run_id}"));
    let rest_evidence: Vec<&str> = rest_run_detail["correlation"]["evidence_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(mcp_resource_evidence, rest_evidence, "evidence 集双通道一致");

    // rip://evidence/<id> 实读：evidence 行 + Artifact 快照文本在位
    // （captured_by=rp-mcp:run_tool，storage_ref 指向 artifact 文件）。
    let evidence_id = mcp_resource_evidence[0].to_string();
    let evidence = mcp_read_resource(&fix.mcp, &format!("rip://evidence/{evidence_id}"), 504);
    assert_eq!(evidence["evidence_id"], json!(evidence_id));
    assert_eq!(evidence["source_pk"], json!(mcp_run_id), "evidence 回链 run_id");
    assert_eq!(evidence["trace_id"], json!(mcp_run_id), "evidence 同链 trace_id");
    assert!(evidence["artifact_text"].is_string(), "storage_ref 快照文本内联在位");
}

// ---------------------------------------------------------------------------
// 共用：两通道各跑一次工具（op4 / op5 复用同一执行，测试内独立执行保证
// 用例独立可跑）
// ---------------------------------------------------------------------------

fn run_both_channels(id_base: i64) -> (String, String, String, Value) {
    let fix = fixture();
    let port = fix.port;
    let (status, accepted) = rest_post(
        port,
        "/api/workflow-runs",
        &json!({"workflow_id": WORKFLOW_ID, "input": {"target": KEYWORD}}),
    );
    assert_eq!(status, 200, "REST 受理: {accepted}");
    let wfr_id = accepted["run_id"].as_str().expect("wfr id").to_string();
    let detail = rest_wait_workflow(port, &wfr_id);
    let rest_run_id =
        detail["correlation"]["tool_run_id"].as_str().expect("tool_run_id").to_string();
    let payload = mcp_call(
        &fix.mcp,
        "rip.run_tool",
        json!({"tool_id": TOOL_ID, "input": {"keyword": KEYWORD, "per_group_limit": 2}}),
        id_base,
    );
    assert_eq!(payload["ok"], true, "MCP run_tool 成功: {payload}");
    let mcp_run_id = payload["run_id"].as_str().expect("mcp run_id").to_string();
    (wfr_id, rest_run_id, mcp_run_id, payload)
}

/// 测试收尾：进程由测试进程退出时回收（rp_server 为子进程，Cargo 测试进程
/// 终止即孤儿化；显式 kill 交给 Drop 语义不可靠，落一次性 kill 守卫）。
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self._server.kill();
        let _ = self._server.wait();
        let _ = std::fs::remove_file(&self.server_stdout);
    }
}
