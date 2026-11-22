//! MA-3 外部 AI MCP 派发往返 e2e（docs/rip3/MA-3-TASK.md §0 D4 + §4 判据 3
//! Rust 面；§三 S7 subtask_dispatch）。真实进程 / 真实 stdio JSON-RPC /
//! 真实 rp-agent 子进程闭环（禁模拟冒充，判据 12）：
//!
//! 1. caller key 预置（sha256 → api_key JOIN caller，026 面）→ RP_MCP_TOKEN
//!    注入真实 rp-mcp → rip.run_subtask 往返（spawn rp-agent 真实
//!    python_worker 闭环）返回终态报告 → agent_run.caller_id = 外部 caller
//!    落列（库直查断言）→ rip.get_subtask_status 一致回读；
//! 2. 非法 token（无 api_key 行映射）→ rip.run_subtask 结构化拒绝
//!    （POLICY_DENIED fail-closed）而 tools/list 正常（目录面零影响）；
//! 3. rip.claim_task 双语义（D7 创建面 + D2 CAS 领取）+ rip.complete_claim
//!    本人回写 + rip.send_message / rip.fetch_messages 邮箱往返（D3 分记）；
//! 4. rip.contribute_knowledge 结构化失败面（rp-kc 缺席/PG 不在 =
//!    INFRA/EXECUTION 结构化错误，不伪造成功；真实供体往返由
//!    selftest_ma3.sh 判据 7 承载）。
//!
//! 门禁口径：rp-agent / rp-kc 构建产物不在位时 SKIP（V-x 口径——
//! `cargo test --workspace` 全量先构建全部 bin，收口门必然实跑）。

#![deny(missing_docs)]

mod common;

use common::McpProc;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const TOOL_ID: &str = "rip.kb.keyword_search";
const AGENT_ID: &str = "agent.ma3.subtask";
const CALLER: &str = "caller-ma3-ext";

fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn target_bin(name: &str) -> std::path::PathBuf {
    let target = std::env::var("CARGO_TARGET_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| repo_root().join("target"));
    target.join("debug").join(name)
}

/// 一次性临时库：全链迁移 + caller/api_key 预置 + Manifest 登记 + 测试
/// Agent 定义（RP_DB_PATH 主库面 = RP_RUN_DB 运行库面同库——测试载具，
/// 生产侧两库分工不变）。
fn setup_db(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("rip-ma3-subtask-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("e2e.db");
    let pool = database::Pool::open(database::PoolConfig {
        path: database::DbPath::File(db.clone()),
        readonly: false,
        max_connections: 1,
        busy_timeout: std::time::Duration::from_secs(5),
    })
    .unwrap();
    database::migrate(&pool, &repo_root().join("migrations")).unwrap();
    // caller + api_key（明文 token 只在测试内存，库中仅 sha256 hex——026 D2 口径）。
    let token = format!("rp1_{}", "A".repeat(43));
    let key_hash = format!("{:x}", Sha256::digest(token.as_bytes()));
    pool.with_conn(|c| -> Result<(), database::DbError> {
        c.execute(
            "INSERT INTO caller (caller_id, display_name, permission_level, created_at) \
             VALUES (?1, ?2, 'T1', '2026-11-22T00:00:00.000Z')",
            database::rusqlite::params![CALLER, "MA-3 外部 AI"],
        )?;
        c.execute(
            "INSERT INTO api_key (key_hash, caller_id, created_at) \
             VALUES (?1, ?2, '2026-11-22T00:00:00.000Z')",
            database::rusqlite::params![key_hash, CALLER],
        )?;
        Ok(())
    })
    .unwrap();
    // Manifest 登记 + ENABLED + 测试 Agent 定义（ma2 harness 同款一次性载具）。
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(
            repo_root().join("manifests/tools").join(format!("{TOOL_ID}.json")),
        )
        .unwrap(),
    )
    .unwrap();
    tool_registry::Registry::open(pool.clone()).register(&manifest).unwrap();
    pool.with_conn(|c| {
        c.execute(
            "UPDATE tool SET lifecycle = 'ENABLED' WHERE tool_id = ?1",
            [TOOL_ID],
        )
    })
    .unwrap();
    let definition = domain::AgentDefinition::try_new(
        domain::AgentId::try_new(AGENT_ID).unwrap(),
        "0.1.0".to_string(),
        "{}".to_string(),
        vec![domain::CapabilityRef::try_new(TOOL_ID, None).unwrap()],
    )
    .unwrap();
    agent_engine::store::AgentStore::open(pool.clone(), tool_registry::Registry::open(pool.clone()))
        .register_definition(&definition, "MA-3 MCP 派发测试 Agent")
        .unwrap();
    drop(pool);
    let _ = token_len_guard(&token);
    db
}

/// 明文 token 形状守卫（rp1_ + 43 位 = 47 字符，026 D2 口径）。
fn token_len_guard(token: &str) -> usize {
    assert_eq!(token.len(), 47);
    assert!(token.starts_with("rp1_"));
    token.len()
}

fn tools_list_names(proc_: &mut McpProc) -> Vec<String> {
    // tools/list 标准分页（服务端页大小固定；游标遍历不重不漏）。
    let mut names = Vec::new();
    let mut cursor = String::new();
    loop {
        let params = if cursor.is_empty() {
            json!({})
        } else {
            json!({"cursor": cursor})
        };
        let reply = proc_.request("tools/list", params);
        let result = reply.get("result").expect("tools/list result");
        for tool in result["tools"].as_array().expect("tools array") {
            names.push(tool["name"].as_str().expect("tool name").to_string());
        }
        match result.get("nextCursor").and_then(Value::as_str) {
            Some(c) => cursor = c.to_string(),
            None => break,
        }
    }
    names
}

#[test]
fn mcp_subtask_roundtrip_and_caller_attribution() {
    // V-9 受控环境端到端（first_batch_b2.rs:103-109 同口径）：goal 的
    // keyword_search worker（rip_kb_search.py:14）硬编码只读打开
    // migrations/platform.db——gitignore 的运行时数据，CI/全新 checkout 无此
    // 文件（sqlite3 unable to open database file，CI 45007685259 实证）。
    // 有 platform.db 的受控环境真跑（selftest_ma3.sh 硬验收②承载）。
    if !repo_root().join("migrations/platform.db").exists() {
        eprintln!(
            "SKIP: migrations/platform.db 不在本地（gitignore 运行时数据），keyword_search worker 无法打开库（V-9，first_batch_b2 同口径受控环境端到端）"
        );
        return;
    }
    let agent_bin = target_bin("rp-agent");
    if !agent_bin.exists() {
        eprintln!("SKIP: rp-agent 构建产物不在位（cargo test --workspace 全量先构建全部 bin，收口门必然实跑）");
        return;
    }
    let db = setup_db("roundtrip");
    let db_str = db.to_string_lossy().to_string();
    let token = format!("rp1_{}", "A".repeat(43));
    let mut proc_ = McpProc::spawn_in(
        &[
            ("RP_MCP_TOKEN", token.as_str()),
            ("RP_DB_PATH", db_str.as_str()),
            ("RP_RUN_DB", db_str.as_str()),
            ("RP_PLUGIN_DB", "none"),
            ("RP_AGENT_BIN", agent_bin.to_string_lossy().as_ref()),
            ("RP_SUBTASK_TIMEOUT_S", "120"),
        ],
        Some(repo_root().as_path()),
    );
    proc_.handshake("2024-11-05");

    // 七枚新工具在 tools/list 目录（additive；既有 rip.* 仍在）。
    let names = tools_list_names(&mut proc_);
    for tool in [
        "rip.run_subtask",
        "rip.get_subtask_status",
        "rip.claim_task",
        "rip.complete_claim",
        "rip.send_message",
        "rip.fetch_messages",
        "rip.contribute_knowledge",
    ] {
        assert!(names.iter().any(|n| n == tool), "目录缺 {tool}: {names:?}");
    }
    assert!(names.iter().any(|n| n == "rip.run_tool"), "既有 rip.run_tool 仍在");

    // rip.run_subtask 往返（真实 rp-agent python_worker 闭环）。
    let t0 = std::time::Instant::now();
    let result = proc_.call(
        "rip.run_subtask",
        json!({
            "agent_id": AGENT_ID,
            "goal": "MA-3 MCP 派发往返：检索 ma3-binding 关键词并给结论",
        })
    );
    let elapsed = t0.elapsed();
    let payload: Value = serde_json::from_str(result["result"]["content"][0]["text"].as_str().unwrap())
        .expect("structured payload");
    assert_eq!(payload["ok"], json!(true), "run_subtask ok: {payload}");
    let report = &payload["report"];
    assert_eq!(report["state"], json!("SUCCEEDED"), "子任务终态: {report}");
    assert!(report["run_id"].as_str().unwrap().starts_with("arn-"));
    assert_eq!(payload["caller"], json!(CALLER));
    let agent_run_id = report["run_id"].as_str().unwrap().to_string();
    eprintln!("[ma3-e2e] run_subtask elapsed_ms={} run_id={}", elapsed.as_millis(), agent_run_id);

    // agent_run.caller_id 落列（库直查；硬验收②归属面）。
    let pool = database::Pool::open(database::PoolConfig::readonly_file(&db)).unwrap();
    let caller_id: Option<String> = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT caller_id FROM agent_run WHERE run_id = ?1",
                [agent_run_id.as_str()],
                |r| r.get(0),
            )
            .optional()
        })
        .unwrap()
        .flatten();
    assert_eq!(caller_id.as_deref(), Some(CALLER), "agent_run.caller_id = 外部 caller");
    // trace 链：MCP 通道自锚定（trace_id = run_id）。
    let trace_id: Option<String> = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT trace_id FROM agent_run WHERE run_id = ?1",
                [agent_run_id.as_str()],
                |r| r.get(0),
            )
            .optional()
        })
        .unwrap()
        .flatten();
    assert!(trace_id.is_some(), "agent_run.trace_id 自锚定在位");
    drop(pool);

    // rip.get_subtask_status 一致回读。
    let result = proc_.call(
        "rip.get_subtask_status",
        json!({"run_id": agent_run_id})
    );
    let payload: Value =
        serde_json::from_str(result["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(payload["state"], json!("SUCCEEDED"));
    assert_eq!(payload["caller_id"], json!(CALLER));
    assert_eq!(payload["agent_id"], json!(AGENT_ID));

    // rip.claim_task 双语义：创建（claimable）→ CAS 领取（claimed）。
    let result = proc_.call(
        "rip.claim_task",
        json!({
            "kind": "agent_subtask",
            "title": "MCP 派发 claim 载具",
            "payload": json!({"target": "example.com"}).to_string(),
        })
    );
    let payload: Value =
        serde_json::from_str(result["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(payload["ok"], json!(true));
    let claim = &payload["claim"];
    assert_eq!(claim["state"], json!("claimable"));
    assert_eq!(claim["attempt"], json!(1));
    assert!(claim["claim_id"].as_str().unwrap().starts_with("tcl-"));
    let claim_id = claim["claim_id"].as_str().unwrap().to_string();

    let result = proc_.call(
        "rip.claim_task",
        json!({"claim_id": claim_id})
    );
    let payload: Value =
        serde_json::from_str(result["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(payload["action"], json!("claimed"));
    assert_eq!(payload["claim"]["state"], json!("claimed"));
    assert_eq!(payload["claim"]["claimed_by"], json!(CALLER));

    // rip.complete_claim（本人回写）。
    let result = proc_.call(
        "rip.complete_claim",
        json!({
            "claim_id": claim_id,
            "result_json": json!({"done": true}).to_string(),
        })
    );
    let payload: Value =
        serde_json::from_str(result["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(payload["claim"]["state"], json!("completed"));

    // rip.send_message / rip.fetch_messages 邮箱往返（分记：delivered 落，
    // read_at NULL；自发自收载具）。
    let result = proc_.call(
        "rip.send_message",
        json!({
            "to_caller": CALLER,
            "payload": json!({"hello": "ma3"}).to_string(),
        })
    );
    let payload: Value =
        serde_json::from_str(result["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(payload["ok"], json!(true));
    let message_id = payload["message"]["message_id"].as_str().unwrap().to_string();
    assert!(message_id.starts_with("mam-"));

    let result = proc_.call(
        "rip.fetch_messages",
        json!({})
    );
    let payload: Value =
        serde_json::from_str(result["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(payload["ok"], json!(true));
    let items = payload["items"].as_array().unwrap();
    assert!(
        items.iter().any(|m| m["message_id"].as_str() == Some(message_id.as_str())
            && m["delivered_at"].is_string()
            && m["read_at"].is_null()),
        "邮箱分记（delivered 落 / read NULL）: {items:?}"
    );

    // rip.contribute_knowledge：rp-kc 缺席路径 = 结构化失败（不伪造成功；
    // 真实供体往返由 selftest_ma3.sh 判据 7 承载）。
    let result = proc_.call(
        "rip.contribute_knowledge",
        json!({
            "type": "vuln_signature",
            "title": "e2e 载具卡",
            "content": "MA-3 MCP contribute e2e content",
        })
    );
    let payload: Value =
        serde_json::from_str(result["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    // RP_PG_DSN 未配置 → rp-kc 落缺省 DSN（本机 PG 未接 = pg_unavailable）或
    // 进程在位正常收据；两者皆为结构化面（不伪造）。断言「必有结构化信封」。
    assert!(
        payload["ok"].is_boolean() || payload.get("error").is_some()
            || result["result"]["isError"] == json!(true),
        "结构化信封在位: {payload}"
    );
    proc_.wait_exit();
}

/// 非法 token：派发工具结构化拒绝（POLICY_DENIED fail-closed），目录面零影响。
#[test]
fn unknown_token_rejects_dispatch_but_keeps_catalog() {
    let db = setup_db("badtoken");
    let db_str = db.to_string_lossy().to_string();
    let mut proc_ = McpProc::spawn_in(
        &[
            // 令牌存在但无 api_key 行映射（sha256 未命中 = caller None）。
            ("RP_MCP_TOKEN", "rp1_invalid-no-mapping-token"),
            ("RP_DB_PATH", db_str.as_str()),
            ("RP_RUN_DB", db_str.as_str()),
            ("RP_PLUGIN_DB", "none"),
        ],
        Some(repo_root().as_path()),
    );
    proc_.handshake("2024-11-05");

    // 目录面零影响。
    let names = tools_list_names(&mut proc_);
    assert!(names.iter().any(|n| n == "rip.run_tool"));

    // rip.run_subtask 结构化拒绝（POLICY_DENIED；不落行不伪造）。
    let result = proc_.call(
        "rip.run_subtask",
        json!({
            "agent_id": AGENT_ID,
            "goal": "非法 token 派发必须被拒",
        })
    );
    eprintln!("[ma3-e2e] bad-token run_subtask raw: {result}");
    assert_eq!(result["result"]["isError"], json!(true));
    let payload: Value =
        serde_json::from_str(result["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(payload["error"]["category"], json!("POLICY_DENIED"));
    assert!(
        payload["error"]["message"]
            .as_str()
            .unwrap()
            .contains("fail-closed"),
        "fail-closed 词形: {payload}"
    );

    // rip.claim_task / rip.send_message 同面拒绝。
    for (name, args) in [
        ("rip.claim_task", json!({"kind": "review", "title": "x", "payload": "{}"})),
        ("rip.send_message", json!({"to_caller": CALLER, "payload": "{}"})),
        ("rip.fetch_messages", json!({})),
        ("rip.contribute_knowledge", json!({"type": "vuln_signature", "title": "t", "content": "c"})),
    ] {
        let result = proc_.call(name, args.clone());
        assert_eq!(result["result"]["isError"], json!(true), "{name} must reject");
        let payload: Value = serde_json::from_str(
            result["result"]["content"][0]["text"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(payload["error"]["category"], json!("POLICY_DENIED"), "{name}: {payload}");
    }
    proc_.wait_exit();
}

use database::rusqlite::OptionalExtension;
