//! P9-1 协议与目录集成测试（rmcp 迁移整改批次重写）：
//! spawn 真实 rp-mcp 二进制（[`common::McpProc`]），以裸 JSON-RPC 客户端
//! 驱动 stdio 协议面，断言标准一致性而非手写期望：
//! - 三态令牌授权边界（RP_MCP_TOKEN 缺失 / 空值 fail-closed exit 2，正确态
//!   正常服务；审计 §4.3 重定义：环境变量注入即授权，无协议认证字段）；
//! - initialize 版本协商（P1：2024-11-05 协商回显；rmcp 支持列表内版本
//!   2025-11-25 回显；列表外版本回落服务端声明版本 2025-11-25）；
//! - P0 标准 Tool 对象：tools/list 每条含必需 name 与 inputSchema，分页只
//!   控量不省必需 Schema（旧「断言 inputSchema 不得出现」的反规范测试已
//!   改写为标准一致性断言）；
//! - 目录与 Tool Center 同源（aux 增量合并 / handle_tools 排序，经 catalog
//!   直读面 + rip.list_tools 信封断言）；词表外方法 -32601；未知工具
//!   -32602；目录数据面不可读 -32603；坏 JSON 行静默跳过（rmcp transport
//!   实测口径，issue #938 同款，与 rtis_mcp.mjs 纪律一致）；
//! - stdout/stderr 分离：stdout 每行均为合法 JSON 协议消息。
//!
//! 临时库自建（不触碰生产 platform*.db）。

mod common;

use std::path::PathBuf;

use database::rusqlite::Connection;
use serde_json::{Value, json};

use common::McpProc;

use mcp_server::catalog::Catalog;

/// 建最小 tool 表（列序与 005 迁移一致，读面只依赖十二列 + 两个 Schema 列）。
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

/// 标准夹具：主库 + aux，run face 关停（RP_RUN_DB=none）。
fn fixture(tag: &str) -> (PathBuf, PathBuf, McpProc) {
    let main = common::tmp_db(&format!("main-{tag}"));
    let aux = common::tmp_db(&format!("aux-{tag}"));
    make_main_db(&main);
    make_aux_db(&aux);
    let proc = McpProc::spawn(&[
        ("RP_DB_PATH", main.to_string_lossy().as_ref()),
        ("RP_AUX_DB", aux.to_string_lossy().as_ref()),
    ]);
    (main, aux, proc)
}

/// 解析 tools/call result 的 content[0].text 载荷。
fn payload_of(reply: &Value) -> Value {
    serde_json::from_str(reply["result"]["content"][0]["text"].as_str().expect("text"))
        .expect("payload json")
}

/// 注册面预期顺序（handle_tools 口径）：主库段 updated_at DESC,
/// name COLLATE NOCASE ASC（同刻时 aaa < BBB 小写序），aux 段（不与主库
/// 重叠的行）按 aux 排序追加在尾。
fn expected_registry_ids() -> Vec<&'static str> {
    vec!["dup-tool", "aaa", "BBB", "zzz", "rip.kb.rtis"]
}

// ---------------------------------------------------------------------------
// 三态令牌授权边界（RP_MCP_TOKEN 缺失 / 错误形态 / 正确注入）
// ---------------------------------------------------------------------------

#[test]
fn token_boundary_three_states() {
    // 缺失态：进程 fail-closed 拒启（exit 2，stderr 显著告警），不进入
    // 服务循环（授权边界 = 宿主环境变量注入，非协议认证）。
    let main = common::tmp_db("tok-missing");
    make_main_db(&main);
    let mut proc = McpProc::spawn(&[
        ("RP_DB_PATH", main.to_string_lossy().as_ref()),
        ("RP_MCP_TOKEN", ""),
    ]);
    proc.send_raw(
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}})
            .to_string()
            .as_str(),
    );
    assert_eq!(
        proc.wait_exit(),
        2,
        "无令牌 spawn 必须 exit 2（fail-closed 授权边界）"
    );

    // 空值态（错误形态令牌）：同样 fail-closed（trim 后为空 = 未授权）。
    let main = common::tmp_db("tok-empty");
    make_main_db(&main);
    let mut proc = McpProc::spawn(&[
        ("RP_DB_PATH", main.to_string_lossy().as_ref()),
        ("RP_MCP_TOKEN", "   "),
    ]);
    proc.send_raw(
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}})
            .to_string()
            .as_str(),
    );
    assert_eq!(proc.wait_exit(), 2, "空白令牌 spawn 必须 exit 2");

    // 正确态：带令牌正常服务（握手成功 + tools/list 可用）。
    let (_m, _a, mut proc) = fixture("tok-ok");
    let reply = proc.handshake("2024-11-05");
    assert!(reply.get("result").is_some(), "正确令牌握手成功: {reply}");
    let reply = proc.request("tools/list", json!({"cursor": ""}));
    assert!(reply.get("result").is_some(), "正确令牌 tools/list 可用: {reply}");
    proc.wait_exit();
}

// ---------------------------------------------------------------------------
// initialize 版本协商（P1 整改：rmcp 承载；支持列表 = 2024-11-05 至
// 2025-11-25，列表外回落服务端声明版本）
// ---------------------------------------------------------------------------

#[test]
fn initialize_negotiates_protocol_versions() {
    let (_m, _a, mut proc) = fixture("negotiate");
    // 客户端请求 2024-11-05（平台基线）：支持列表内，按规范协商回显。
    let reply = proc.request(
        "initialize",
        json!({"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"t","version":"0"}}),
    );
    let result = &reply["result"];
    assert_eq!(result["protocolVersion"], "2024-11-05", "支持列表内版本协商回显: {reply}");
    assert_eq!(result["capabilities"]["tools"]["listChanged"], false);
    assert_eq!(result["capabilities"]["resources"]["subscribe"], false);
    assert_eq!(result["capabilities"]["prompts"]["listChanged"], false);
    assert_eq!(result["serverInfo"]["name"], "rp-mcp");
    assert_eq!(reply["jsonrpc"], "2.0");
    // initialize 响应不携带工具全文（计划 §11:658；instructions 为一行导航，
    // 可点名交互面但不携带目录条目 / Schema 字段）。
    let text = reply.to_string();
    assert!(
        !text.contains("\"tier\"")
            && !text.contains("\"inputSchema\"")
            && !text.contains("\"description\""),
        "initialize 响应不得携带目录条目或工具 Schema 字段: {text}"
    );
    let instructions = result["instructions"].as_str().unwrap();
    assert!(
        instructions.contains("tools/list") && instructions.contains("rip.get_tool_schema"),
        "一行导航点名交互面"
    );
    assert!(instructions.chars().count() < 200, "instructions 保持一行导航");

    // 客户端请求 rmcp 支持的最新握手版本（2025-11-25）：协商回显。
    let reply = proc.request(
        "initialize",
        json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"t","version":"0"}}),
    );
    assert_eq!(
        reply["result"]["protocolVersion"], "2025-11-25",
        "rmcp 支持列表内最新版本协商回显: {reply}"
    );

    // 客户端请求列表外版本：按规范回落服务端支持的版本（不回显未知版本）。
    let reply = proc.request(
        "initialize",
        json!({"protocolVersion":"1999-01-01","capabilities":{},"clientInfo":{"name":"t","version":"0"}}),
    );
    assert_eq!(
        reply["result"]["protocolVersion"], "2025-11-25",
        "列表外版本回落服务端声明版本（不得回显客户端任意版本）: {reply}"
    );
    proc.wait_exit();
}

// ---------------------------------------------------------------------------
// P0 标准 Tool 对象 + 分页控量（旧反规范断言已改写为标准一致性断言）
// ---------------------------------------------------------------------------

#[test]
fn tools_list_entries_are_standard_tool_objects() {
    let (_m, _a, mut proc) = fixture("std-tool");
    proc.handshake("2024-11-05");
    let mut names: Vec<String> = Vec::new();
    let mut cursor = String::new();
    let mut pages = 0;
    loop {
        let params = if cursor.is_empty() {
            json!({})
        } else {
            json!({"cursor": cursor})
        };
        let reply = proc.request("tools/list", params);
        let result = reply.get("result").expect("tools/list result");
        for tool in result["tools"].as_array().expect("tools array") {
            // P0：每条含必需 name（非空字符串）与 inputSchema（object 形态，
            // 含 type）。
            let name = tool["name"].as_str().expect("tool name string");
            assert!(!name.is_empty(), "tool name 非空: {tool}");
            assert!(
                tool["inputSchema"].is_object() && tool["inputSchema"]["type"].is_string(),
                "每条 Tool 对象必须含合法 inputSchema（P0 标准必需字段）: {tool}"
            );
            names.push(name.to_string());
        }
        match result.get("nextCursor").and_then(Value::as_str) {
            Some(c) => cursor = c.to_string(),
            None => break,
        }
        pages += 1;
        assert!(pages < 50, "分页游标失灵");
    }
    // 组合目录 = 能力面前段 6 条（固定次序）+ 注册面 5 行（4 主库 + 1 aux 新行，
    // dup 去重）；注册面 Schema 缺省 = 宽松 object 形态（不省略必需 Schema）。
    let mut expected: Vec<String> = mcp_server::CAPABILITY_TOOLS
        .iter()
        .map(|(n, _)| n.to_string())
        .collect();
    expected.extend(expected_registry_ids().iter().map(|s| format!("rip.{s}")));
    assert_eq!(names, expected, "分页游标遍历不重不漏（能力面前段 + 注册面段）");

    // 能力面 description（一行导航）+ 注册面零 description（Token 纪律，
    // 完整元数据经 rip.get_tool_schema）。
    let reply = proc.request("tools/list", json!({}));
    for tool in reply["result"]["tools"].as_array().unwrap() {
        let is_capability = tool["name"].as_str().unwrap().starts_with("rip.")
            && mcp_server::CAPABILITY_TOOLS
                .iter()
                .any(|(n, _)| *n == tool["name"].as_str().unwrap());
        assert_eq!(
            tool.get("description").is_some(),
            is_capability,
            "能力面有 description，注册面无（{}）", tool["name"]
        );
    }
    // aux 行的真实 Schema 列直读进 tools/list（与 tool 表 input_schema 同源）。
    let reply = proc.request("tools/list", json!({}));
    let rtis = reply["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "rip.rip.kb.rtis")
        .expect("aux schema row");
    assert_eq!(
        rtis["inputSchema"]["required"],
        json!(["q"]),
        "aux 行 input_schema 列直读（同源）: {rtis}"
    );
    proc.wait_exit();
}

#[test]
fn tools_list_pagination_is_server_paged_with_opaque_cursor() {
    let (_m, _a, mut proc) = fixture("pagination");
    proc.handshake("2024-11-05");
    // 服务端固定页大小（MCP 标准分页只含 cursor；limit 参数不在标准面），
    // 首页 = 6 能力面 + 14 注册面中的 14 行：11 总条数 → 首页 11 行无下一页。
    let reply = proc.request("tools/list", json!({}));
    let tools = reply["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 11, "夹具 11 条全量（6 能力面 + 5 注册面）: {}", tools.len());
    assert!(
        reply["result"].get("nextCursor").is_none(),
        "全量一页内无 nextCursor"
    );
    // 非法游标 -32602。
    let reply = proc.request("tools/list", json!({"cursor": "v1-bad"}));
    assert_eq!(reply["error"]["code"], -32602, "非法游标必须 -32602: {reply}");
    // 多页遍历：临时库再造 30 行（> 页大小 20）验证真分页。
    drop(proc);
    let main = common::tmp_db("many");
    {
        let conn = Connection::open(&main).unwrap();
        create_tool_table(&conn);
        for i in 0..30 {
            insert_tool(&conn, &format!("t{i:02}"), &format!("t{i:02}"), "cli", "REGISTERED", "ACTIVE", "2026-01-02T00:00:00Z", None, None);
        }
        conn.close().unwrap();
    }
    let mut proc = McpProc::spawn(&[("RP_DB_PATH", main.to_string_lossy().as_ref())]);
    proc.handshake("2024-11-05");
    let mut seen: Vec<String> = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let params = cursor
            .as_deref()
            .map(|c| json!({"cursor": c}))
            .unwrap_or_else(|| json!({}));
        let reply = proc.request("tools/list", params);
        let result = reply["result"].as_object().expect("result");
        for tool in result["tools"].as_array().unwrap() {
            seen.push(tool["name"].as_str().unwrap().to_string());
            // 分页控量不省必需 Schema（每页每条都带 inputSchema）。
            assert!(tool["inputSchema"].is_object(), "分页不省 inputSchema: {tool}");
        }
        match result.get("nextCursor") {
            Some(Value::String(c)) => {
                cursor = Some(c.clone());
                assert_eq!(result["tools"].as_array().unwrap().len(), 20, "服务端固定页大小 20");
            }
            _ => break,
        }
    }
    assert_eq!(seen.len(), 36, "30 注册面 + 6 能力面全量遍历");
    let mut uniq = seen.clone();
    uniq.sort();
    uniq.dedup();
    assert_eq!(uniq.len(), seen.len(), "遍历无重复");
    proc.wait_exit();
}

// ---------------------------------------------------------------------------
// 目录同源（catalog 直读面 + rip.list_tools 信封）
// ---------------------------------------------------------------------------

#[test]
fn registry_tier_is_same_source_with_handle_tools_semantics() {
    let (main, aux, _proc) = fixture("same-source");
    // catalog 直读面断言排序与合并（handle_tools 口径；lib API 直测）。
    let catalog = Catalog::open(&main, Some(&aux)).unwrap();
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
fn list_tools_call_envelope_is_handle_tools_shape() {
    let (_m, _a, mut proc) = fixture("list-envelope");
    proc.handshake("2024-11-05");
    // rip.list_tools：handle_tools 口径信封（total = 注册面行数，能力面不计入）。
    let reply = proc.call("rip.list_tools", json!({"limit": 2}));
    assert_eq!(reply["result"]["isError"], false);
    let payload = payload_of(&reply);
    assert_eq!(payload["total"], 5, "total = 注册面行数（能力面不计入）");
    assert_eq!(payload["returned"], 2);
    assert_eq!(payload["limit"], 2);
    assert_eq!(payload["items"][0]["tool_id"], "dup-tool");
    assert_eq!(payload["items"][0]["tier"], "registry");
    assert_eq!(payload["nextCursor"], "v1-2");
    // 关键字过滤（LIKE 字面匹配）。
    let reply = proc.call("rip.list_tools", json!({"keyword": "kb"}));
    let payload = payload_of(&reply);
    assert_eq!(payload["total"], 1);
    assert_eq!(payload["items"][0]["tool_id"], "rip.kb.rtis");
    // aux 重叠去重（dup-tool 只出现一次，主库为准）。
    let reply = proc.call("rip.list_tools", json!({}));
    let payload = payload_of(&reply);
    let ids: Vec<&str> = payload["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["tool_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, expected_registry_ids(), "注册面全量顺序 = handle_tools 口径");
    // 词表外 lifecycle = -32602（BadFilter）。
    let reply = proc.call("rip.list_tools", json!({"lifecycle": "NOPE"}));
    assert_eq!(reply["error"]["code"], -32602, "词表外过滤必须 -32602: {reply}");
    // limit 越界 = -32602。
    let reply = proc.call("rip.list_tools", json!({"limit": 201}));
    assert_eq!(reply["error"]["code"], -32602);
    proc.wait_exit();
}

// ---------------------------------------------------------------------------
// 按需 Schema（rip.get_tool_schema 平台自定义详情工具，P0 保留面）
// ---------------------------------------------------------------------------

#[test]
fn get_tool_schema_serves_registry_and_capability_on_demand() {
    let (_m, _a, mut proc) = fixture("schema");
    proc.handshake("2024-11-05");
    // 注册面：aux 行的真实 Schema（与 tool 表 input_schema 列同源）。
    let reply = proc.call("rip.get_tool_schema", json!({"tool_id": "rip.kb.rtis"}));
    assert_eq!(reply["result"]["isError"], false);
    let payload = payload_of(&reply);
    assert_eq!(payload["source"], "aux");
    assert_eq!(payload["input_schema"]["required"][0], "q");
    assert_eq!(payload["output_schema"]["properties"]["ok"]["type"], "boolean");
    // 无 Schema 的主库行 = 双 null（真实列值，不造假）。
    let reply = proc.call("rip.get_tool_schema", json!({"tool_id": "aaa"}));
    let payload = payload_of(&reply);
    assert_eq!(payload["source"], "main");
    assert_eq!(payload["input_schema"], Value::Null);
    assert_eq!(payload["output_schema"], Value::Null);
    // 能力面：内置入参 Schema。
    let reply = proc.call("rip.get_tool_schema", json!({"tool_id": "rip.run_tool"}));
    let payload = payload_of(&reply);
    assert_eq!(payload["source"], "capability");
    assert_eq!(payload["input_schema"]["required"], json!(["tool_id", "input"]));
    // 工具不存在 = -32602。
    let reply = proc.call("rip.get_tool_schema", json!({"tool_id": "nope"}));
    assert_eq!(reply["error"]["code"], -32602);
    // 缺 tool_id = -32602。
    let reply = proc.call("rip.get_tool_schema", json!({}));
    assert_eq!(reply["error"]["code"], -32602);
    proc.wait_exit();
}

// ---------------------------------------------------------------------------
// tools/call 派发面（未接线结构化 isError / 注册面直呼 / 未知工具）
// ---------------------------------------------------------------------------

#[test]
fn unwired_tools_return_structured_is_error() {
    let (_m, _a, mut proc) = fixture("unwired");
    proc.handshake("2024-11-05");
    // 尚未接线的能力面（search_knowledge）：结构化 isError，不伪造成功。
    let reply = proc.call("rip.search_knowledge", json!({}));
    assert_eq!(reply["result"]["isError"], true, "search_knowledge 未接线必须 isError=true");
    let text = reply["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("\"wired_in\""), "search_knowledge 指明接线归属: {text}");
    // run face 未配置（RP_RUN_DB=none）：如实返回未配置说明。
    for (name, arguments) in [
        ("rip.run_tool", json!({"tool_id": "rip.doc.parse", "input": {"text": "x"}})),
        ("rip.get_run_status", json!({"run_id": "rn-x"})),
        ("rip.cancel_run", json!({"run_id": "rn-x"})),
    ] {
        let reply = proc.call(name, arguments);
        assert_eq!(reply["result"]["isError"], true, "{name} 未配置执行面必须 isError=true");
        assert!(
            reply["result"]["content"][0]["text"].as_str().unwrap().contains("run face"),
            "{name} 说明 run face 未配置"
        );
    }
    // 缺 run_id = -32602（协议参数面）。
    for name in ["rip.get_run_status", "rip.cancel_run"] {
        let reply = proc.call(name, json!({}));
        assert_eq!(reply["error"]["code"], -32602, "{name} 缺 run_id = -32602");
    }
    // 注册面直呼指向 rip.run_tool 通道（执行面统一走该通道）。
    let reply = proc.call("rip.aaa", json!({}));
    assert_eq!(reply["result"]["isError"], true);
    let text = reply["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("rip.run_tool"), "注册面直呼指明执行通道: {text}");
    // 目录外工具 = -32602 unknown tool。
    let reply = proc.call("rip.nope", json!({}));
    assert_eq!(reply["error"]["code"], -32602);
    let msg = reply["error"]["message"].as_str().unwrap();
    assert!(msg.contains("unknown tool"), "未知工具报 unknown tool: {msg}");
    proc.wait_exit();
}

// ---------------------------------------------------------------------------
// 方法面与 conformance（ping / resources / prompts / 未知方法 / 通知 / 坏行）
// ---------------------------------------------------------------------------

#[test]
fn method_dispatch_follows_subset_surface() {
    let (_m, _a, mut proc) = fixture("dispatch");
    proc.handshake("2024-11-05");
    // ping（rmcp 内建，空 result）。
    let reply = proc.request("ping", json!({}));
    assert_eq!(reply["result"], json!({}), "ping 空结果: {reply}");
    // P9-4 resources / prompts 面（七 resource = 静态四 + 模板三；prompt 三条）。
    let reply = proc.request("resources/list", json!({}));
    let uris: Vec<&str> = reply["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["uri"].as_str().unwrap())
        .collect();
    assert_eq!(uris.len(), 4, "静态资源四件: {uris:?}");
    let reply = proc.request("resources/templates/list", json!({}));
    let templates: Vec<&str> = reply["result"]["resourceTemplates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["uriTemplate"].as_str().unwrap())
        .collect();
    assert_eq!(templates.len(), 3, "模板资源三件: {templates:?}");
    let reply = proc.request("prompts/list", json!({}));
    let prompts: Vec<&str> = reply["result"]["prompts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        prompts,
        vec!["rip.tool_run_playbook", "rip.run_audit_review", "rip.knowledge_brief"]
    );
    let reply = proc.request(
        "prompts/get",
        json!({"name": "rip.tool_run_playbook", "arguments": {"tool_id": "rip.doc.parse"}}),
    );
    assert_eq!(reply["result"]["messages"][0]["role"], "user");
    assert!(reply["result"]["messages"][0]["content"]["text"].is_string());
    // 词表外资源与 prompt = -32602。
    let reply = proc.request("resources/read", json!({"uri": "rip://nope"}));
    assert_eq!(reply["error"]["code"], -32602);
    let reply = proc.request("prompts/get", json!({"name": "rip.nope"}));
    assert_eq!(reply["error"]["code"], -32602);
    // 词表外方法 = -32601（rmcp 兜底 method_not_found）。
    let reply = proc.request("resources/subscribe", json!({}));
    assert_eq!(reply["error"]["code"], -32601, "词表外方法 -32601: {reply}");
    // notifications/initialized 静默（无响应行）：后接请求仍正常应答。
    proc.send_raw(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
    let reply = proc.request("ping", json!({}));
    assert_eq!(reply["result"], json!({}));
    // 坏 JSON 行静默跳过（rmcp transport 实测口径，issue #938；不回 -32700），
    // 后续合法请求照常应答。
    proc.send_raw("this is not json");
    let reply = proc.request("ping", json!({}));
    assert_eq!(reply["result"], json!({}));
    assert!(
        proc.sideband.is_empty(),
        "坏行与通知不产生任何 stdout 响应行: {:?}",
        proc.sideband
    );
    proc.wait_exit();
}

#[test]
fn catalog_unavailable_reports_real_empty_state() {
    let empty = common::tmp_db("empty");
    let conn = Connection::open(&empty).unwrap();
    conn.execute_batch("CREATE TABLE marker (id INTEGER);").unwrap();
    conn.close().unwrap();
    let mut proc = McpProc::spawn(&[("RP_DB_PATH", empty.to_string_lossy().as_ref())]);
    proc.handshake("2024-11-05");
    let reply = proc.request("tools/list", json!({}));
    assert_eq!(reply["error"]["code"], -32603, "数据面不可读 = -32603: {reply}");
    assert_eq!(
        reply["error"]["data"]["reason"], "aux_db_not_configured",
        "真实空态 reason 词表"
    );
    proc.wait_exit();
}

#[test]
fn stdout_lines_are_pure_protocol_messages_with_token() {
    // stdout 纪律：握手 + 三个请求，stdout 读到的每一行都是合法 JSON-RPC
    // 消息（日志走 stderr；本驱动逐行 JSON 解析即隐式断言）。令牌常量与
    // 三态令牌用例共用（TOKEN 正确态）。
    let (_m, _a, mut proc) = fixture("stdout-pure");
    proc.handshake("2024-11-05");
    let _ = proc.request("tools/list", json!({}));
    let _ = proc.call("rip.get_tool_schema", json!({"tool_id": "aaa"}));
    let _ = proc.request("ping", json!({}));
    assert_eq!(proc.sideband.len(), 0, "无带外残余行（stdout 全部为配对响应）");
    proc.wait_exit();
}
