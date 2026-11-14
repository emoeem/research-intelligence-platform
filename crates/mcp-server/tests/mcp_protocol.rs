//! P9-1 协议与目录集成测试：MCP 2024-11-05 子集三交互（initialize 握手 /
//! tools/list 分页 / rip.get_tool_schema 按需）+ 握手鉴权（无令牌被拒）+
//! 目录与 Tool Center 同源断言（aux 增量合并 / handle_tools 排序 / 摘要
//! 投影无完整说明）。临时库自建（不触碰生产 platform*.db）。

use std::path::PathBuf;

use database::rusqlite::Connection;
use serde_json::{json, Value};

use mcp_server::{LineOutcome, McpServer, CAPABILITY_TOOLS};

/// 临时库路径（每测试独立，退出清理）。
fn tmp_db(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir();
    let path = dir.join(format!(
        "rp_mcp_test_{}_{}.db",
        tag,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&path);
    path
}

/// 建最小 tool 表（列序与 005 迁移一致，读面只依赖 TOOL_LIST_COLUMNS 十二列
/// + 两个 Schema 列）。
fn create_tool_table(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE tool (
            tool_id          TEXT PRIMARY KEY,
            name             TEXT NOT NULL UNIQUE,
            kind             TEXT,
            lifecycle        TEXT,
            governance       TEXT NOT NULL DEFAULT 'ACTIVE',
            install_state    TEXT,
            health_state     TEXT,
            permission_level TEXT,
            timeout_s        INTEGER,
            source_system    TEXT,
            created_at       TEXT,
            updated_at       TEXT,
            input_schema     TEXT,
            output_schema    TEXT
        );",
    )
    .unwrap();
}

#[allow(clippy::too_many_arguments)]
fn insert_tool(
    conn: &Connection,
    tool_id: &str,
    name: &str,
    kind: &str,
    lifecycle: &str,
    governance: &str,
    updated_at: &str,
    input_schema: Option<&str>,
    output_schema: Option<&str>,
) {
    let or_empty = |o: Option<&str>| o.unwrap_or("").to_string();
    conn.execute(
        "INSERT INTO tool (tool_id, name, kind, lifecycle, governance, install_state,
            health_state, permission_level, timeout_s, source_system, created_at,
            updated_at, input_schema, output_schema)
         VALUES (?1, ?2, ?3, ?4, ?5, 'not_installed', 'unknown', 'T0', 60,
                 'tool-registry', ?6, ?6, ?7, ?8)",
        database::rusqlite::params![
            tool_id,
            name,
            kind,
            lifecycle,
            governance,
            updated_at,
            or_empty(input_schema),
            or_empty(output_schema)
        ],
    )
    .unwrap();
}

/// 主库：4 行（aaa/BBB 同刻测 NOCASE 排序，zzz 旧刻测 updated_at DESC，
/// dup-tool 与 aux 重叠测去重）。
fn make_main_db(path: &std::path::Path) {
    let conn = Connection::open(path).unwrap();
    create_tool_table(&conn);
    insert_tool(&conn, "aaa", "aaa", "cli", "REGISTERED", "ACTIVE", "2026-01-02T00:00:00Z", None, None);
    insert_tool(&conn, "BBB", "BBB", "cli", "REGISTERED", "ACTIVE", "2026-01-02T00:00:00Z", None, None);
    insert_tool(&conn, "zzz", "zzz", "cli", "VALIDATED", "QUARANTINED", "2026-01-01T00:00:00Z", None, None);
    insert_tool(&conn, "dup-tool", "dup-tool", "cli", "ENABLED", "ACTIVE", "2026-01-03T00:00:00Z", None, None);
    conn.close().unwrap();
}

/// aux 库：1 行与主库重叠（dup-tool，增量合并必须去重）+ 1 行新增
/// rip.kb.rtis（带真实 Schema，知识检索面）。
fn make_aux_db(path: &std::path::Path) {
    let conn = Connection::open(path).unwrap();
    create_tool_table(&conn);
    insert_tool(&conn, "dup-tool", "dup-tool", "cli", "DISABLED", "ACTIVE", "2026-01-05T00:00:00Z", None, None);
    insert_tool(
        &conn,
        "rip.kb.rtis",
        "RTIS KB Search",
        "tool",
        "VALIDATED",
        "ACTIVE",
        "2026-01-04T00:00:00Z",
        Some(r#"{"type":"object","properties":{"q":{"type":"string"}},"required":["q"]}"#),
        Some(r#"{"type":"object","properties":{"ok":{"type":"boolean"}}}"#),
    );
    conn.close().unwrap();
}

/// 组装 server（run_channel: None = 执行面未配置；执行面用例见
/// tests/run_tool_e2e.rs）。
fn server_for(main: &std::path::Path, aux: Option<&std::path::Path>, token: Option<&str>) -> McpServer {
    McpServer {
        main_db: main.to_path_buf(),
        aux_db: aux.map(PathBuf::from),
        plugin_db: None,
        token: token.map(str::to_string),
        run_channel: None,
    }
}

/// 单请求进出：返回解析后的响应 Value。
fn roundtrip(server: &McpServer, request: Value) -> Value {
    match server.handle_line(&request.to_string()) {
        LineOutcome::Reply(line) => serde_json::from_str(&line).unwrap(),
        other => panic!("expected reply, got {other:?}"),
    }
}

/// 主库 + aux 的标准组合。
fn fixture() -> (PathBuf, PathBuf, McpServer) {
    let main = tmp_db("main");
    let aux = tmp_db("aux");
    make_main_db(&main);
    make_aux_db(&aux);
    let server = server_for(&main, Some(&aux), Some("tok-123"));
    (main, aux, server)
}

/// 注册面预期顺序（handle_tools 口径）：主库段 updated_at DESC,
/// name COLLATE NOCASE ASC（同刻时 aaa < BBB 小写序），aux 段（不与主库
/// 重叠的行）按 aux 排序追加在尾。
fn expected_registry_ids() -> Vec<&'static str> {
    vec!["dup-tool", "aaa", "BBB", "zzz", "rip.kb.rtis"]
}

#[test]
fn initialize_echoes_protocol_and_carries_no_tool_text() {
    let (_m, _a, server) = fixture();
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":7,"method":"initialize","params":{
            "protocolVersion":"2024-11-05","capabilities":{},
            "clientInfo":{"name":"t","version":"0"}}}),
    );
    let result = &resp["result"];
    assert_eq!(result["protocolVersion"], "2024-11-05");
    assert_eq!(result["capabilities"]["tools"]["listChanged"], false);
    assert_eq!(result["serverInfo"]["name"], "rp-mcp");
    assert_eq!(resp["id"], 7);
    let text = resp.to_string();
    assert!(
        !text.contains("\"description\""),
        "initialize 响应不得携带工具说明字段: {text}"
    );
    assert!(
        !text.contains("\"inputSchema\""),
        "initialize 响应不得携带工具 Schema: {text}"
    );
    assert!(!text.contains("\"tier\""), "initialize 响应不得携带目录条目: {text}");
    // 一行导航 instructions（不含任何 rip.* 工具全文，只点名交互面）。
    let instructions = result["instructions"].as_str().unwrap();
    assert!(instructions.contains("tools/list") && instructions.contains("rip.get_tool_schema"));
    assert!(instructions.chars().count() < 200, "instructions 保持一行导航，不塞工具全文");
}

#[test]
fn no_token_handshake_is_rejected_with_32001() {
    let (main, _aux, _s) = fixture();
    let server = server_for(&main, None, None);
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}),
    );
    assert_eq!(resp["error"]["code"], -32001, "无令牌握手被拒: {resp}");
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{"cursor":""}}),
    );
    assert_eq!(resp["error"]["code"], -32001, "无令牌 tools/list 被拒: {resp}");
}

#[test]
fn presented_token_must_match() {
    let (_m, _a, server) = fixture();
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "_meta":{"rp.auth":"Bearer wrong"},"protocolVersion":"2024-11-05"}}),
    );
    assert_eq!(resp["error"]["code"], -32001);
    assert!(resp["error"]["message"].as_str().unwrap().contains("auth_mismatch"));
    // 匹配的出示放行。
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":2,"method":"initialize","params":{
            "_meta":{"rp.auth":"Bearer tok-123"},"protocolVersion":"2024-11-05"}}),
    );
    assert!(resp.get("result").is_some());
}

#[test]
fn tools_list_is_paginated_with_opaque_cursor() {
    let (_m, _a, server) = fixture();
    // 全量：能力面 5 + 注册面 5（4 主库行 + 1 aux 新行，dup 去重）。
    let mut names: Vec<String> = Vec::new();
    let mut cursor = String::new();
    loop {
        let resp = roundtrip(
            &server,
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"cursor":cursor}}),
        );
        let result = &resp["result"];
        for tool in result["tools"].as_array().unwrap() {
            names.push(tool["name"].as_str().unwrap().to_string());
        }
        match result.get("nextCursor").and_then(Value::as_str) {
            Some(c) => cursor = c.to_string(),
            None => break,
        }
        assert!(names.len() < 100, "分页游标失灵");
    }
    let mut expected: Vec<String> = CAPABILITY_TOOLS
        .iter()
        .map(|(n, _)| n.to_string())
        .collect();
    expected.extend(
        expected_registry_ids()
            .iter()
            .map(|s| format!("rip.{s}")),
    );
    assert_eq!(names, expected, "分页游标遍历必须不重不漏（能力面前段 + 注册面段）");
}

#[test]
fn tools_list_page_limit_bounds_are_enforced() {
    let (_m, _a, server) = fixture();
    for bad in [0u64, 201, 500] {
        let resp = roundtrip(
            &server,
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"limit":bad}}),
        );
        assert_eq!(resp["error"]["code"], -32602, "limit={bad} 越界必须 -32602");
    }
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"cursor":"v1-bad"}}),
    );
    assert_eq!(resp["error"]["code"], -32602, "非法游标必须 -32602");
    // 显式 limit=2 的页切片。
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"limit":2}}),
    );
    let tools = resp["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0]["name"], "rip.list_tools");
    assert_eq!(resp["result"]["nextCursor"], "v1-2");
}

#[test]
fn tools_list_entries_are_summary_projection_without_full_text() {
    let (_m, _a, server) = fixture();
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"limit":200}}),
    );
    for tool in resp["result"]["tools"].as_array().unwrap() {
        let obj = tool.as_object().unwrap();
        assert!(!obj.contains_key("description"), "目录条目不得携带说明: {obj:?}");
        assert!(!obj.contains_key("inputSchema"), "Schema 按需（rip.get_tool_schema），目录条目不携带: {obj:?}");
    }
    // 注册面条目带 Tool Center 摘要列（同源投影）。
    let row = resp["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "rip.aaa")
        .unwrap();
    assert_eq!(row["tier"], "registry");
    assert_eq!(row["tool_id"], "aaa");
    assert_eq!(row["lifecycle"], "REGISTERED");
    assert_eq!(row["governance"], "ACTIVE");
}

#[test]
fn registry_tier_is_same_source_with_handle_tools_semantics() {
    let (main, aux, _s) = fixture();
    // 直接经 Catalog 读面断言排序与合并（handle_tools 口径）。
    let catalog = mcp_server::catalog::Catalog::open(&main, Some(&aux)).unwrap();
    let rows = catalog.list_registry(&Default::default()).unwrap();
    let ids: Vec<&str> = rows.iter().map(|r| r.tool_id.as_str()).collect();
    assert_eq!(ids, expected_registry_ids(), "排序 = updated_at DESC, name COLLATE NOCASE ASC；aux 只补主库没有的行");
    // 与库直查 SQL 逐条一致（同源）。
    let conn =
        Connection::open_with_flags(&main, database::rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let db_ids: Vec<String> = conn
        .prepare("SELECT tool_id FROM tool ORDER BY updated_at DESC, name COLLATE NOCASE ASC")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(&ids[..4], &db_ids[..], "主库段与 handle_tools 同款 SQL 排序逐条一致");
}

#[test]
fn aux_merge_dedups_overlapping_ids() {
    let (_m, _a, server) = fixture();
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"limit":200}}),
    );
    let names: Vec<String> = resp["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        names.iter().filter(|n| *n == "rip.dup-tool").count(),
        1,
        "aux 与主库重叠的 tool_id 只出现一次（主库为准，aux 只补缺）: {names:?}"
    );
    assert!(
        names.contains(&"rip.rip.kb.rtis".to_string()),
        "aux 新行按 rip.<tool_id> 进入目录: {names:?}"
    );
}

#[test]
fn filters_apply_to_registry_segment() {
    let (_m, _a, server) = fixture();
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"lifecycle":"VALIDATED","limit":200}}),
    );
    let names: Vec<String> = resp["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        names,
        [
            "rip.list_tools",
            "rip.get_tool_schema",
            "rip.run_tool",
            "rip.search_knowledge",
            "rip.get_run_status",
            "rip.cancel_run",
            "rip.zzz",
            "rip.rip.kb.rtis"
        ],
        "lifecycle 过滤命中主库段 zzz 与 aux 段 rip.kb.rtis（能力面恒在前段，主库段在 aux 段前）: {names:?}"
    );
    // 词表外 lifecycle = -32602。
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"lifecycle":"NOPE"}}),
    );
    assert_eq!(resp["error"]["code"], -32602);
}

#[test]
fn get_tool_schema_serves_registry_and_capability_on_demand() {
    let (_m, _a, server) = fixture();
    // 注册面：aux 行的真实 Schema（与 tool 表 input_schema 列同源）。
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
            "name":"rip.get_tool_schema","arguments":{"tool_id":"rip.kb.rtis"}}}),
    );
    assert_eq!(resp["result"]["isError"], false);
    let payload: Value =
        serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(payload["source"], "aux");
    assert_eq!(payload["input_schema"]["required"][0], "q");
    assert_eq!(payload["output_schema"]["properties"]["ok"]["type"], "boolean");
    // 无 Schema 的主库行 = 双 null（真实列值，不造假）。
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"rip.get_tool_schema","arguments":{"tool_id":"aaa"}}}),
    );
    let payload: Value =
        serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(payload["source"], "main");
    assert_eq!(payload["input_schema"], Value::Null);
    assert_eq!(payload["output_schema"], Value::Null);
    // 能力面：内置入参 Schema。
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{
            "name":"rip.get_tool_schema","arguments":{"tool_id":"rip.run_tool"}}}),
    );
    let payload: Value =
        serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(payload["source"], "capability");
    assert_eq!(payload["input_schema"]["required"], json!(["tool_id", "input"]));
    // 工具不存在 = -32602。
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{
            "name":"rip.get_tool_schema","arguments":{"tool_id":"nope"}}}),
    );
    assert_eq!(resp["error"]["code"], -32602);
    // 缺 tool_id = -32602。
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{
            "name":"rip.get_tool_schema","arguments":{}}}),
    );
    assert_eq!(resp["error"]["code"], -32602);
}

#[test]
fn unwired_tools_return_structured_is_error() {
    let (_m, _a, server) = fixture();
    // P9-2 后仍未接线的能力面只剩 search_knowledge（后续批次，不伪造成功）。
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
            "name":"rip.search_knowledge","arguments":{}}}),
    );
    assert_eq!(resp["result"]["isError"], true, "search_knowledge 未接线必须 isError=true");
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("\"wired_in\""), "search_knowledge 指明接线归属: {text}");
    // run/get_run_status/cancel_run 已接线：RunChannel 未配置时如实返回
    // 未配置说明（不伪造执行），测试面 run_channel=None 即此态。
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
            "name":"rip.run_tool","arguments":{"tool_id":"rip.doc.parse","input":{"text":"x"}}}}),
    );
    assert_eq!(resp["result"]["isError"], true, "未配置执行面必须 isError=true");
    assert!(
        resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("run face"),
        "run_tool 说明 run face 未配置: {}",
        resp["result"]["content"][0]["text"]
    );
    for (name, arguments) in [
        ("rip.get_run_status", json!({"run_id": "rn-x"})),
        ("rip.cancel_run", json!({"run_id": "rn-x"})),
    ] {
        let resp = roundtrip(
            &server,
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
                "name":name,"arguments":arguments}}),
        );
        assert_eq!(resp["result"]["isError"], true, "{name} 未配置执行面必须 isError=true");
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("run face"),
            "{name} 说明 run face 未配置: {text}"
        );
    }
    // get_run_status / cancel_run 缺 run_id = -32602（协议参数面）。
    for name in ["rip.get_run_status", "rip.cancel_run"] {
        let resp = roundtrip(
            &server,
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
                "name":name,"arguments":{}}}),
        );
        assert_eq!(resp["error"]["code"], -32602, "{name} 缺 run_id = -32602");
    }
    // 注册面直呼指向 rip.run_tool 通道（P9-2 起执行面统一走该通道）。
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"rip.aaa","arguments":{}}}),
    );
    assert_eq!(resp["result"]["isError"], true);
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("rip.run_tool"), "注册面直呼指明执行通道: {text}");
    // 目录外工具 = -32602 unknown tool。
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{
            "name":"rip.nope","arguments":{}}}),
    );
    assert_eq!(resp["error"]["code"], -32602);
}

#[test]
fn list_tools_call_returns_handle_tools_envelope() {
    let (_m, _a, server) = fixture();
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
            "name":"rip.list_tools","arguments":{"limit":2}}}),
    );
    let payload: Value =
        serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(payload["total"], 5, "total = 注册面行数（能力面不计入）");
    assert_eq!(payload["returned"], 2);
    assert_eq!(payload["limit"], 2);
    assert_eq!(payload["items"][0]["tool_id"], "dup-tool");
    assert_eq!(payload["nextCursor"], "v1-2");
    // 关键字过滤（LIKE 字面匹配）。
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"rip.list_tools","arguments":{"keyword":"kb"}}}),
    );
    let payload: Value =
        serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(payload["total"], 1);
    assert_eq!(payload["items"][0]["tool_id"], "rip.kb.rtis");
}

#[test]
fn method_dispatch_follows_subset_surface() {
    let (_m, _a, server) = fixture();
    // ping。
    let resp = roundtrip(&server, json!({"jsonrpc":"2.0","id":1,"method":"ping"}));
    assert_eq!(resp["result"], json!({}));
    // P9-4 resources / prompts 面（七 resource = 静态四 + 模板三；prompt 三条）。
    let resp = roundtrip(&server, json!({"jsonrpc":"2.0","id":2,"method":"resources/list"}));
    let uris: Vec<&str> = resp["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["uri"].as_str().unwrap())
        .collect();
    assert_eq!(uris.len(), 4, "静态资源四件: {uris:?}");
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":3,"method":"resources/templates/list"}),
    );
    let templates: Vec<&str> = resp["result"]["resourceTemplates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["uriTemplate"].as_str().unwrap())
        .collect();
    assert_eq!(templates.len(), 3, "模板资源三件: {templates:?}");
    let resp = roundtrip(&server, json!({"jsonrpc":"2.0","id":4,"method":"prompts/list"}));
    let prompts: Vec<&str> = resp["result"]["prompts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        prompts,
        vec!["rip.tool_run_playbook", "rip.run_audit_review", "rip.knowledge_brief"]
    );
    // 词表外资源与 prompt = -32602。
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":5,"method":"resources/read","params":{"uri":"rip://nope"}}),
    );
    assert_eq!(resp["error"]["code"], -32602);
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":6,"method":"prompts/get","params":{"name":"rip.nope"}}),
    );
    assert_eq!(resp["error"]["code"], -32602);
    // 子集外方法 = -32601（resources/subscribe 未纳入子集）。
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":7,"method":"resources/subscribe"}),
    );
    assert_eq!(resp["error"]["code"], -32601);
    // notifications/initialized 静默。
    let out = server.handle_line(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}).to_string());
    assert!(matches!(out, LineOutcome::Silent));
    // 坏 JSON 行跳过（不回 -32700，rtis_mcp.mjs 口径）。
    let out = server.handle_line("this is not json");
    assert!(matches!(out, LineOutcome::Skip));
}

#[test]
fn catalog_unavailable_reports_real_empty_state() {
    let empty = tmp_db("empty");
    let conn = Connection::open(&empty).unwrap();
    conn.execute_batch("CREATE TABLE marker (id INTEGER);").unwrap();
    conn.close().unwrap();
    let server = server_for(&empty, None, Some("tok"));
    let resp = roundtrip(
        &server,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}),
    );
    assert_eq!(resp["error"]["code"], -32603);
    assert_eq!(resp["error"]["data"]["reason"], "aux_db_not_configured");
    let _ = std::fs::remove_file(&empty);
}
