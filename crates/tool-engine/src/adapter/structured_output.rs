//! 只读 CLI 脚本的结构化输出 Adapter（TOOL-CONTRACT §10.2 B2 行「输出需
//! 结构化 Adapter」，ROADMAP P1-7b）。
//!
//! B2 三工具（rip.kb.keyword_search/T05、rip.kb.inventory/T06、
//! rip.ops.etl_status/T07）本体为文本行输出的只读脚本（scripts/ 下，
//! 零修改），经 scripts/rip_cli_worker.py 协议薄包装返回原样 stdout；
//! 本模块把文本行解析为各工具 output_schema 形状（解析失败显式报错，
//! 不静默放行）。fixture 取自本仓脚本实跑输出（见模块测试）。

use serde_json::{json, Map, Value};

/// 请求侧任务整形：把面向工具的 input 映射为薄包装 worker 的 task
/// （`{"args": [...]}`）。无适配规则的工具恒等返回（B1 行为不变）。
pub fn shape_task(tool_id: &str, input: &Value) -> Result<Value, String> {
    let empty = Map::new();
    let obj = input.as_object().unwrap_or(&empty);
    // ---- B5：RTIS/ERI 只读 Adapter（形状含 stdin_payload / task_file，提前分支）----
    match tool_id {
        "rip.kb.rtis" | "rip.kb.eri" => return shape_task_b5(tool_id, obj),
        _ => {}
    }
    let args: Vec<String> = match tool_id {
        "rip.kb.keyword_search" => {
            let keyword = obj
                .get("keyword")
                .and_then(Value::as_str)
                .ok_or_else(|| "keyword must be a string".to_string())?;
            let mut a = vec![keyword.to_string()];
            if let Some(limit) = obj.get("per_group_limit").and_then(Value::as_u64) {
                a.push(limit.to_string());
            }
            a
        }
        "rip.kb.inventory" => {
            if let Some(grade) = obj.get("grade").and_then(Value::as_str) {
                vec![grade.to_string()]
            } else {
                Vec::new()
            }
        }
        "rip.ops.etl_status" => Vec::new(),
        // ---- B4：rtk 五子命令（前缀子命令名 + 旗标映射，详见各 Manifest）----
        "rip.evidence.rtk.auth" => {
            let mut a = vec!["auth".to_string(), "check".to_string()];
            if let Some(v) = obj.get("root").and_then(Value::as_str) {
                a.push("--root".into());
                a.push(v.to_string());
            }
            if let Some(v) = obj.get("today").and_then(Value::as_str) {
                a.push("--today".into());
                a.push(v.to_string());
            }
            a
        }
        "rip.evidence.rtk.ledger" => {
            let mut a = vec!["ledger".to_string(), "sync".to_string()];
            if let Some(v) = obj.get("root").and_then(Value::as_str) {
                a.push("--root".into());
                a.push(v.to_string());
            }
            if obj.get("check_mode").and_then(Value::as_bool) == Some(true) {
                a.push("--check".into());
            }
            if obj.get("skip_auth").and_then(Value::as_bool) == Some(true) {
                a.push("--skip-auth".into());
            }
            a
        }
        "rip.evidence.rtk.lint" => {
            let paths = obj
                .get("paths")
                .and_then(Value::as_array)
                .ok_or_else(|| "paths must be an array of strings".to_string())?;
            if paths.is_empty() {
                return Err("paths must not be empty".to_string());
            }
            let mut a = vec!["report".to_string(), "lint".to_string()];
            for p in paths {
                a.push(
                    p.as_str()
                        .ok_or_else(|| "paths items must be strings".to_string())?
                        .to_string(),
                );
            }
            for (key, flag) in [
                ("allow", "--allow"),
                ("require", "--require"),
                ("any_of", "--any-of"),
            ] {
                if let Some(v) = obj.get(key).and_then(Value::as_str) {
                    a.push(flag.to_string());
                    a.push(v.to_string());
                }
            }
            if obj.get("no_structure").and_then(Value::as_bool) == Some(true) {
                a.push("--no-structure".into());
            }
            if obj.get("json_output").and_then(Value::as_bool) == Some(true) {
                a.push("--json".into());
            }
            a
        }
        "rip.evidence.rtk.mcp" => {
            let op = obj
                .get("op")
                .and_then(Value::as_str)
                .ok_or_else(|| "op must be \"list\" or \"call\"".to_string())?;
            let mut a = vec!["mcp".to_string(), op.to_string()];
            match op {
                "list" => {}
                "call" => {
                    let name = obj
                        .get("name")
                        .and_then(Value::as_str)
                        .ok_or_else(|| "mcp call requires name".to_string())?;
                    a.push(name.to_string());
                    if let Some(v) = obj.get("args_json").and_then(Value::as_str) {
                        a.push(v.to_string());
                    }
                }
                other => return Err(format!("unknown mcp op {other:?}")),
            }
            if let Some(v) = obj.get("server").and_then(Value::as_str) {
                a.push("--server".into());
                a.push(v.to_string());
            }
            if let Some(t) = obj.get("timeout_seconds").and_then(Value::as_u64) {
                a.push("--timeout".into());
                a.push(t.to_string());
            }
            a
        }
        "rip.evidence.rtk.scaffold" => {
            let req = |key: &str| -> Result<String, String> {
                obj.get(key)
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| format!("{key} must be a string"))
            };
            let mut a = vec![
                "scaffold".to_string(),
                req("dir")?,
                "--engagement".into(),
                req("engagement")?,
                "--client".into(),
                req("client")?,
                "--auth-ref".into(),
                req("auth_ref")?,
                "--window".into(),
                req("window")?,
                "--granted-by".into(),
                req("granted_by")?,
            ];
            if let Some(v) = obj.get("report_to").and_then(Value::as_str) {
                a.push("--report-to".into());
                a.push(v.to_string());
            }
            if obj.get("force").and_then(Value::as_bool) == Some(true) {
                a.push("--force".into());
            }
            a
        }
        _ => {
            return Ok(input.clone());
        }
    };
    Ok(json!({ "args": args }))
}

/// 响应侧结构化：把薄包装返回的 `{exit_code, stdout, stderr_tail}` 解析为
/// 工具 output_schema 形状。无适配规则的工具恒等返回（B1 行为不变）。
pub fn adapt(tool_id: &str, input: &Value, data: &Value) -> Result<Value, String> {
    match tool_id {
        "rip.kb.keyword_search" => {
            let keyword = input
                .get("keyword")
                .and_then(Value::as_str)
                .unwrap_or_default();
            parse_kb_search(keyword, data)
        }
        "rip.kb.inventory" => {
            let grade = input.get("grade").and_then(Value::as_str);
            parse_inventory(grade, data)
        }
        "rip.ops.etl_status" => parse_etl_status(data),
        _ => Ok(data.clone()),
    }
}

/// B5：RTIS/ERI 只读 Adapter 的任务整形。
/// - rip.kb.rtis：op=cli（rt.py poc search）产出 args；op=engine（rtis_kb.mjs）
///   产出 stdin_payload（{"cmd":"search",...}）。
/// - rip.kb.eri：task_json 校验为合法 JSON 后物化为 bridge-gate --task-file。
fn shape_task_b5(tool_id: &str, obj: &Map<String, Value>) -> Result<Value, String> {
    match tool_id {
        "rip.kb.rtis" => {
            let surface = obj
                .get("surface")
                .and_then(Value::as_str)
                .ok_or_else(|| "surface must be \"cli\" or \"engine\"".to_string())?;
            let q = obj
                .get("q")
                .and_then(Value::as_str)
                .ok_or_else(|| "q must be a string".to_string())?;
            let limit = obj.get("limit").and_then(Value::as_u64).unwrap_or(5);
            match surface {
                // rt.py poc search（检索路径 mode=ro，rt.py:557）
                "cli" => {
                    let a = vec![
                        "poc".to_string(),
                        "search".to_string(),
                        "--q".to_string(),
                        q.to_string(),
                        "--limit".to_string(),
                        limit.to_string(),
                    ];
                    Ok(json!({ "surface": "cli", "args": a }))
                }
                // rtis_kb.mjs 引擎面：stdin JSON {"cmd":"search","q":...,"limit":...}
                "engine" => {
                    let payload = json!({ "cmd": "search", "q": q, "limit": limit });
                    Ok(json!({
                        "surface": "engine",
                        "args": [],
                        "stdin_payload": payload.to_string()
                    }))
                }
                other => Err(format!("unknown rtis surface {other:?}")),
            }
        }
        "rip.kb.eri" => {
            let task_json = obj
                .get("task_json")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    "task_json must be a JSON string (bridge-gate task file)".to_string()
                })?;
            serde_json::from_str::<Value>(task_json)
                .map_err(|e| format!("task_json must be valid JSON: {e}"))?;
            let mode = obj.get("mode").and_then(Value::as_str).unwrap_or("auto");
            if !matches!(mode, "auto" | "rp" | "legacy") {
                return Err(format!("mode {mode:?} not in auto/rp/legacy"));
            }
            Ok(json!({
                "args": ["bridge-gate", "--task-file", "{TASK_FILE}", "--mode", mode],
                "task_file": task_json,
            }))
        }
        _ => Err(format!("no B5 shaping rule for {tool_id}")),
    }
}

fn raw_of(data: &Value) -> Result<&str, String> {
    let obj = data
        .as_object()
        .ok_or_else(|| "wrapper payload must be an object".to_string())?;
    let exit_code = obj
        .get("exit_code")
        .and_then(Value::as_i64)
        .ok_or_else(|| "wrapper payload missing integer exit_code".to_string())?;
    let stdout = obj
        .get("stdout")
        .and_then(Value::as_str)
        .ok_or_else(|| "wrapper payload missing string stdout".to_string())?;
    if exit_code != 0 {
        let tail = obj.get("stderr_tail").and_then(Value::as_str).unwrap_or("");
        return Err(format!(
            "script exited with code {exit_code}; stderr_tail: {}",
            tail.chars()
                .rev()
                .take(400)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
        ));
    }
    Ok(stdout)
}

/// rip.kb.keyword_search：解析分组文本行
/// （`== 组名 ==`、缩进命中行、`(无命中)`、`共 N 条命中（每组最多 L 条）`）。
fn parse_kb_search(keyword: &str, data: &Value) -> Result<Value, String> {
    let stdout = raw_of(data)?;
    let mut groups: Vec<Value> = Vec::new();
    let mut total_hits: Option<u64> = None;
    let mut per_group_limit: Option<u64> = None;
    for line in stdout.lines() {
        if let Some(label) = line.strip_prefix("== ").and_then(|r| r.strip_suffix(" ==")) {
            groups.push(json!({ "label": label.trim(), "hits": [], "empty": true }));
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("共 ").and_then(|r| r.split_once(' ')) {
            let (n, tail) = rest;
            total_hits = n.parse::<u64>().ok().or(total_hits);
            if let Some(l) = tail
                .split_once("每组最多 ")
                .and_then(|(_, after)| after.split(' ').next())
            {
                per_group_limit = l.parse::<u64>().ok().or(per_group_limit);
            }
            continue;
        }
        if trimmed == "(无命中)" {
            continue;
        }
        if !line.starts_with(' ') {
            return Err(format!(
                "unrecognized line in kb_search output: {trimmed:?}"
            ));
        }
        let group = groups
            .last_mut()
            .ok_or_else(|| "hit line before any group header".to_string())?;
        group["hits"]
            .as_array_mut()
            .ok_or_else(|| "corrupt group hits".to_string())?
            .push(Value::String(trimmed.to_string()));
        group["empty"] = Value::Bool(false);
    }
    let total_hits =
        total_hits.ok_or_else(|| "missing total line in kb_search output".to_string())?;
    let per_group_limit =
        per_group_limit.ok_or_else(|| "missing per-group limit in kb_search output".to_string())?;
    Ok(json!({
        "keyword": keyword,
        "per_group_limit": per_group_limit,
        "total_hits": total_hits,
        "groups": groups,
    }))
}

/// rip.kb.inventory：解析统计行与可选的等级明细段
/// （`资源总量: N`、`按质量等级: {'A': 4, ...}`、`== 质量等级 X（N 条）==`）。
fn parse_inventory(grade: Option<&str>, data: &Value) -> Result<Value, String> {
    let stdout = raw_of(data)?;
    let mut total: Option<u64> = None;
    let mut by_grade: Option<Value> = None;
    let mut by_lifecycle: Option<Value> = None;
    let mut by_trust: Option<Value> = None;
    let mut grade_count: Option<u64> = None;
    let mut entries: Vec<Value> = Vec::new();
    let mut in_grade_section = false;
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("资源总量: ") {
            total = Some(
                rest.trim()
                    .parse::<u64>()
                    .map_err(|e| format!("bad total {rest:?}: {e}"))?,
            );
            in_grade_section = false;
            continue;
        }
        if let Some(rest) = line.strip_prefix("按质量等级: ") {
            by_grade = Some(parse_py_dict(rest)?);
            continue;
        }
        if let Some(rest) = line.strip_prefix("按生命周期: ") {
            by_lifecycle = Some(parse_py_dict(rest)?);
            continue;
        }
        if let Some(rest) = line.strip_prefix("按信任级: ") {
            by_trust = Some(parse_py_dict(rest)?);
            continue;
        }
        if line.starts_with("== 质量等级 ") {
            let inner = line
                .trim_start_matches("== 质量等级 ")
                .trim_end_matches("==")
                .trim();
            let n = inner
                .split_once("（")
                .and_then(|(_, after)| after.trim_end_matches("条）").trim().parse::<u64>().ok());
            grade_count = n;
            in_grade_section = true;
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if in_grade_section && line.starts_with("  ") && !line.starts_with("      ") {
            entries.push(entry_from_line(trimmed));
        }
        // reasons 缩进行（六个空格）并入上一条 raw，不做结构化。
    }
    Ok(json!({
        "total": total.ok_or_else(|| "missing total line".to_string())?,
        "by_grade": by_grade.ok_or_else(|| "missing by_grade line".to_string())?,
        "by_lifecycle": by_lifecycle.ok_or_else(|| "missing by_lifecycle line".to_string())?,
        "by_trust": by_trust.ok_or_else(|| "missing by_trust line".to_string())?,
        "grade_filter": grade.map(|g| Value::String(g.to_string())).unwrap_or(Value::Null),
        "grade_count": grade_count,
        "grade_entries": entries,
    }))
}

/// 单条明细行 `{id}  {name}  [{lifecycle}] 分 {score}` 的容错解析：
/// 结构字段尽力解析，原文始终保留在 raw（名称含双空格等场景不失败）。
fn entry_from_line(line: &str) -> Value {
    let mut entry = Map::new();
    entry.insert("raw".to_string(), Value::String(line.to_string()));
    // 尾部 `] 分 {score}`：切分时保留右括号供生命周期解析。
    let mut head = line;
    if let Some(idx) = head.rfind("] 分 ") {
        let score_part = head[idx + "] 分 ".len()..].trim();
        if let Ok(score) = score_part.parse::<f64>() {
            entry.insert("score".to_string(), json!(score));
        }
        head = &head[..idx + 1];
    }
    // `{name}  [{lifecycle}]`：剥右括号后按 `  [` 反切。
    if let Some(stripped) = head.strip_suffix(']') {
        if let Some((before, lc)) = stripped.rsplit_once("  [") {
            entry.insert("lifecycle".to_string(), Value::String(lc.to_string()));
            head = before;
        }
    }
    if let Some((id, name)) = head.split_once("  ") {
        entry.insert("resource_id".to_string(), Value::String(id.to_string()));
        entry.insert("name".to_string(), Value::String(name.to_string()));
    } else if !head.is_empty() {
        entry.insert("name".to_string(), Value::String(head.to_string()));
    }
    Value::Object(entry)
}

/// python dict repr（`{'A': 4, ...}`，键值为简单词与整数）到 JSON 的保守转换。
fn parse_py_dict(raw: &str) -> Result<Value, String> {
    let t = raw.trim();
    if !t.starts_with('{') || !t.ends_with('}') {
        return Err(format!("not a dict literal: {t:?}"));
    }
    let allowed = t.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || matches!(
                c,
                '{' | '}' | '\'' | ':' | ',' | ' ' | '_' | '-' | '?' | '.'
            )
    });
    if !allowed {
        return Err(format!("dict literal has unexpected chars: {t:?}"));
    }
    let jsonish = t.replace('\'', "\"");
    serde_json::from_str(&jsonish).map_err(|e| format!("bad dict literal {t:?}: {e}"))
}

/// rip.ops.etl_status：解析 `etl_*_last_run.json: {json|截断文本|缺失}` 与
/// `platform.db: quick_check=ok | resources=N evidence=N tasks=N`。
fn parse_etl_status(data: &Value) -> Result<Value, String> {
    let stdout = raw_of(data)?;
    let mut files: Vec<Value> = Vec::new();
    let mut platform: Option<Value> = None;
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("platform.db: ") {
            let mut obj = Map::new();
            for part in rest.split(" | ") {
                for token in part.split_whitespace() {
                    if let Some((k, v)) = token.split_once('=') {
                        let key = k.trim().to_string();
                        let value = match key.as_str() {
                            "quick_check" => Value::String(v.trim().to_string()),
                            _ => v
                                .trim()
                                .parse::<i64>()
                                .map(|n| json!(n))
                                .unwrap_or_else(|_| Value::String(v.trim().to_string())),
                        };
                        obj.insert(key, value);
                    }
                }
            }
            platform = Some(Value::Object(obj));
            continue;
        }
        if let Some((file, payload)) = trimmed.split_once(": ") {
            let file = file.to_string();
            if payload == "缺失" {
                files.push(json!({ "file": file, "status": "missing" }));
                continue;
            }
            if let Some(err) = payload.strip_prefix("解析失败 ") {
                files.push(json!({ "file": file, "status": "error", "error": err }));
                continue;
            }
            // json.dumps(ensure_ascii=False)[:200]：截断时可能不是合法 JSON。
            match serde_json::from_str::<Value>(payload) {
                Ok(v) => files.push(json!({ "file": file, "status": "ok", "summary": v })),
                Err(_) => files.push(json!({
                    "file": file,
                    "status": "truncated",
                    "summary_raw": payload,
                })),
            }
            continue;
        }
        return Err(format!(
            "unrecognized line in etl_status output: {trimmed:?}"
        ));
    }
    Ok(json!({
        "platform": platform.ok_or_else(|| "missing platform.db line".to_string())?,
        "etl_files": files,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const KB_SEARCH_HITS: &str = "== 工具 ==\n   nmap  nmap  [健康:ok]\n== 技能 ==\n   active-scan  active-scan  主动端口与服务扫描（nmap/masscan），严格遵守授权范围\n== 资源 ==\n   (无命中)\n== 知识 ==\n   (无命中)\n== 研究任务 ==\n   (无命中)\n\n共 2 条命中（每组最多 2 条）。只读查询。\n";

    const INVENTORY_ALL: &str = "资源总量: 81\n按质量等级: {'A': 4, 'B': 28, 'C': 44, 'D': 5}\n按生命周期: {'INDEXED': 81}\n按信任级: {'A': 4, 'B': 28, 'C': 44, 'D': 5}\n";

    const INVENTORY_GRADE_A: &str = "资源总量: 81\n按质量等级: {'A': 4, 'B': 28, 'C': 44, 'D': 5}\n按生命周期: {'INDEXED': 81}\n按信任级: {'A': 4, 'B': 28, 'C': 44, 'D': 5}\n\n== 质量等级 A（4 条）==\n  gh:rmusser01/infosec_reference  Infosec Reference  [INDEXED] 分 89.0\n      {\"relevance\": {\"score\": 22, \"term_hits\": [\"pentest\", \"penetration\"]}}\n";

    const ETL_STATUS: &str = "etl_last_run.json: {\"etl_version\": \"1.1.1\", \"finished_at\": \"2026-11-06T16:53:06Z\", \"fk_violations\": 0}\netl_evidence_last_run.json: {\"etl_script\": \"etl_evidence.py\", \"etl_version\": \"2.0.0\", \"finished_at\": \"2026-11-0\netl_tasks_last_run.json: {\"etl_script\": \"etl_tasks.py\", \"etl_version\": \"2.0.0\", \"fk_violations\": 0, \"quarantine_count\": 0, \"quarant\nplatform.db: quick_check=ok | resources=81 evidence=700 tasks=42\n";

    fn wrapper(stdout: &str) -> Value {
        json!({ "exit_code": 0, "stdout": stdout, "stderr_tail": "" })
    }

    #[test]
    fn kb_search_fixture_parses_groups_and_totals() {
        let out = parse_kb_search("nmap", &wrapper(KB_SEARCH_HITS)).unwrap();
        assert_eq!(out["keyword"], "nmap");
        assert_eq!(out["per_group_limit"], 2);
        assert_eq!(out["total_hits"], 2);
        let groups = out["groups"].as_array().unwrap();
        assert_eq!(groups.len(), 5);
        assert_eq!(groups[0]["label"], "工具");
        assert_eq!(groups[0]["hits"][0], "nmap  nmap  [健康:ok]");
        assert_eq!(groups[0]["empty"], false);
        assert_eq!(groups[2]["label"], "资源");
        assert_eq!(groups[2]["empty"], true);
    }

    #[test]
    fn inventory_fixture_parses_stats_and_grade_entries() {
        let all = parse_inventory(None, &wrapper(INVENTORY_ALL)).unwrap();
        assert_eq!(all["total"], 81);
        assert_eq!(all["by_grade"]["A"], 4);
        assert_eq!(all["grade_filter"], Value::Null);

        let grade = parse_inventory(Some("A"), &wrapper(INVENTORY_GRADE_A)).unwrap();
        assert_eq!(grade["grade_filter"], "A");
        assert_eq!(grade["grade_count"], 4);
        let e0 = &grade["grade_entries"][0];
        assert_eq!(e0["resource_id"], "gh:rmusser01/infosec_reference");
        assert_eq!(e0["lifecycle"], "INDEXED");
        assert_eq!(e0["score"], 89.0);
        assert!(e0["raw"].as_str().unwrap().contains("Infosec"));
    }

    #[test]
    fn etl_status_fixture_parses_platform_and_files() {
        let out = parse_etl_status(&wrapper(ETL_STATUS)).unwrap();
        let platform = &out["platform"];
        assert_eq!(platform["quick_check"], "ok");
        assert_eq!(platform["resources"], 81);
        assert_eq!(platform["evidence"], 700);
        assert_eq!(platform["tasks"], 42);
        let files = out["etl_files"].as_array().unwrap();
        assert_eq!(files.len(), 3);
        assert_eq!(files[0]["status"], "ok");
        assert_eq!(files[0]["summary"]["etl_version"], "1.1.1");
        assert_eq!(files[1]["status"], "truncated");
        assert!(files[1]["summary_raw"].as_str().unwrap().starts_with('{'));
    }

    #[test]
    fn nonzero_exit_and_malformed_payload_fail_closed() {
        let bad = json!({ "exit_code": 64, "stdout": "usage", "stderr_tail": "" });
        assert!(parse_etl_status(&bad).is_err());
        assert!(parse_kb_search("x", &json!({"exit_code": 0, "stdout": "garbage"})).is_err());
        assert!(adapt(
            "rip.kb.keyword_search",
            &json!({"keyword": "x"}),
            &wrapper(KB_SEARCH_HITS)
        )
        .is_ok());
        // 无适配规则的工具恒等返回。
        let passthrough = json!({"anything": 1});
        assert_eq!(
            adapt("rip.doc.parse", &json!({}), &passthrough).unwrap(),
            passthrough
        );
    }

    #[test]
    fn shape_task_maps_b2_inputs_and_passes_others_through() {
        let t = shape_task(
            "rip.kb.keyword_search",
            &json!({"keyword": "nmap", "per_group_limit": 2}),
        )
        .unwrap();
        assert_eq!(t, json!({"args": ["nmap", "2"]}));

        let t = shape_task("rip.kb.inventory", &json!({"grade": "A"})).unwrap();
        assert_eq!(t, json!({"args": ["A"]}));

        let t = shape_task("rip.ops.etl_status", &json!({})).unwrap();
        assert_eq!(t, json!({"args": []}));

        let t = shape_task("rip.kb.inventory", &json!({})).unwrap();
        assert_eq!(t, json!({"args": []}));

        let raw = json!({"text": "x"});
        assert_eq!(shape_task("rip.doc.parse", &raw).unwrap(), raw);

        assert!(shape_task("rip.kb.keyword_search", &json!({})).is_err());
    }

    #[test]
    fn shape_task_maps_b4_rtk_subcommands() {
        let t = shape_task("rip.evidence.rtk.auth", &json!({"root": "runs/x"})).unwrap();
        assert_eq!(t, json!({"args": ["auth", "check", "--root", "runs/x"]}));

        let t = shape_task(
            "rip.evidence.rtk.ledger",
            &json!({"check_mode": true, "skip_auth": false}),
        )
        .unwrap();
        assert_eq!(t, json!({"args": ["ledger", "sync", "--check"]}));

        let t = shape_task(
            "rip.evidence.rtk.lint",
            &json!({"paths": ["reports/a.md"], "allow": "local\\.test", "json_output": true}),
        )
        .unwrap();
        assert_eq!(
            t,
            json!({"args": ["report", "lint", "reports/a.md", "--allow", "local\\.test", "--json"]})
        );
        assert!(shape_task("rip.evidence.rtk.lint", &json!({"paths": []})).is_err());
        assert!(shape_task("rip.evidence.rtk.lint", &json!({})).is_err());

        let t = shape_task("rip.evidence.rtk.mcp", &json!({"op": "list"})).unwrap();
        assert_eq!(t, json!({"args": ["mcp", "list"]}));

        let t = shape_task(
            "rip.evidence.rtk.mcp",
            &json!({"op": "call", "name": "t", "args_json": "{\"x\":1}"}),
        )
        .unwrap();
        assert_eq!(t, json!({"args": ["mcp", "call", "t", "{\"x\":1}"]}));
        assert!(shape_task("rip.evidence.rtk.mcp", &json!({"op": "call"})).is_err());
        assert!(shape_task("rip.evidence.rtk.mcp", &json!({"op": "nope"})).is_err());

        let t = shape_task(
            "rip.evidence.rtk.scaffold",
            &json!({
                "dir": "runs/e1", "engagement": "e1", "client": "c",
                "auth_ref": "AUTH-1", "window": "2026-01-01~2026-02-01",
                "granted_by": "g", "force": true
            }),
        )
        .unwrap();
        assert_eq!(
            t,
            json!({"args": [
                "scaffold", "runs/e1", "--engagement", "e1", "--client", "c",
                "--auth-ref", "AUTH-1", "--window", "2026-01-01~2026-02-01",
                "--granted-by", "g", "--force"
            ]})
        );
        assert!(shape_task("rip.evidence.rtk.scaffold", &json!({"dir": "runs/e1"})).is_err());
    }

    #[test]
    fn shape_task_maps_b5_rtis_and_eri() {
        let t = shape_task(
            "rip.kb.rtis",
            &json!({"surface": "cli", "q": "kingdee", "limit": 2}),
        )
        .unwrap();
        assert_eq!(
            t,
            json!({"surface": "cli", "args": ["poc", "search", "--q", "kingdee", "--limit", "2"]})
        );

        let t = shape_task("rip.kb.rtis", &json!({"surface": "engine", "q": "kingdee"})).unwrap();
        assert_eq!(t["surface"], "engine");
        let payload: Value = serde_json::from_str(t["stdin_payload"].as_str().unwrap()).unwrap();
        assert_eq!(payload["cmd"], "search");
        assert_eq!(payload["q"], "kingdee");
        assert_eq!(payload["limit"], 5);
        assert!(t["args"].as_array().unwrap().is_empty());

        assert!(shape_task("rip.kb.rtis", &json!({"surface": "cli"})).is_err());
        assert!(shape_task("rip.kb.rtis", &json!({"surface": "nope", "q": "x"})).is_err());

        let t = shape_task(
            "rip.kb.eri",
            &json!({"task_json": "{\"objective\":\"probe\",\"keywords\":[\"nmap\"]}"}),
        )
        .unwrap();
        assert_eq!(
            t["args"],
            json!([
                "bridge-gate",
                "--task-file",
                "{TASK_FILE}",
                "--mode",
                "auto"
            ])
        );
        assert!(t["task_file"].as_str().unwrap().contains("probe"));

        assert!(shape_task("rip.kb.eri", &json!({"task_json": "not-json"})).is_err());
        assert!(shape_task("rip.kb.eri", &json!({"task_json": "{}", "mode": "bad"})).is_err());
    }
}
