//! Phase 9 P9-2 rip.run_tool 端到端执行通道（ROADMAP §10.3 P9-2，计划
//! §11:670-679 外部 AI 八步的 5 到 7 步：run_tool → 派发 Tool Engine →
//! 结构化结果 + Artifact + Evidence 引用）。
//!
//! 派发纪律（ask 原文「与 Web Workflow Tool 节点同一引擎，不建第二套派发
//! 逻辑」）：本模块不复刻任何受理/状态机/信封逻辑，只做三件事：
//! 1. 把 MCP tools/call arguments 整形为 TOOL-CONTRACT §4.1 ToolCallRequest
//!    （protocol_version=1，run_id 本模块生成，与 workflow exec::node_tool
//!    的 new_id("rn") 同一形状）；
//! 2. 交给 [`tool_engine::Engine`]（crates/tool-engine/src/lib.rs:147 open、
//!    :157 submit）受理（D-TC-05：结构、Registry 存在性、版本解析、预算帽、
//!    input_schema、T2 authorization_ref，拒绝不落行），再按 Manifest
//!    runtime.type 走既有适配器驱动到终态：
//!    - python_worker：[`tool_engine::adapter::execute_via_engine`]（真实
//!      python 子进程，rp-workflow CLI 同款路径）；
//!    - podman_host / podman_vm：[`runtime_gateway::Gateway::dispatch`]
//!      （心跳探测，探测失败落 WAITING_FOR_RUNNER 不 FAILED，计划 §5.6:336
//!      语义）+ [`tool_engine::adapter::execute_container_via_engine`]
//!      （P3-3 全链同款，容器与镜像不在位时如实落 DEPENDENCY 准备失败）；
//! 3. 终态信封经 [`agent_engine::evidence::EvidenceRegistrar`]（P7-2 最小
//!    闭环同一实现，face=rp-mcp）登记 Artifact + Evidence（SUCCEEDED 取
//!    result，失败取 error 对象），evidence id 回填信封 evidence_refs。
//!
//! 数据面：tool_run / artifact / evidence / runtime_node 行全部落在运行时
//! 数据库（RP_RUN_DB，缺省 rip-data/tool-runs.db，gitignore 运行时数据），
//! 生产 migrations/platform.db 保持零写（P9-1 红线延续）。运行库按仓库
//! migrations 全量迁移（001 到 018，零新增迁移号），工具登记沿用既有
//! `rp-tool register --db <run_db>` 路径（SESSION-HANDOFF「添加新工具 =
//! 一条可复制路径」），本模块不做登记只做执行。
//!
//! P9-4 审计链自锚定（双通道一致性载具前提，015/018「run 即链锚」同款惯例）：
//! 受理走 `Engine::submit_correlated(&request, Some(run_id))`（tool_run.trace_id
//! = run_id，workflow tool 节点 submit_correlated 同一引擎入口），登记
//! `Registration.trace_id = Some(run_id)`（artifact/evidence 行同链，015 单锚
//! 传播）。此前 MCP 通道 trace_id 全 NULL，REST /api/traces 无法反查 MCP 通道
//! 运行；自锚定后两通道的运行都能经同一 trace 面回查（rp-mcp-parity 载具
//! 断言面）。T2 校验、词表、拒绝不落行等受理语义零变化（correlated 仅在
//! submit 成功后 UPDATE trace_id 列，与 workflow 同款）。
//!
//! 无状态模型（P9-1 §九延续）：每次 tools/call 独立打开运行库，执行完即
//! 释放；长会话仍不存在。

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::{json, Value};
use tool_engine::adapter::{
    execute_container_via_engine, execute_via_engine, AdapterCall, ContainerCall, NetworkPlan,
    PodmanHostAdapter, PodmanHostConfig, PythonWorkerAdapter, PythonWorkerConfig, RestartPolicy,
};
use tool_engine::{DriveOutcome, Engine, EngineError};
use tool_registry::Registry;

/// run_tool 入参词表外的键一律拒绝（D-TC-06 同款 fail-closed，不静默忽略）。
const RUN_TOOL_ARG_KEYS: [&str; 7] = [
    "tool_id",
    "input",
    "task_id",
    "authorization_ref",
    "tool_version",
    "workspace_id",
    "budget",
];

/// 执行通道配置（bin 层由 RP_RUN_DB 等环境变量装配）。
#[derive(Debug, Clone)]
pub struct RunChannel {
    /// 运行库路径（可写，tool_run / artifact / evidence / runtime_node 行
    /// 与执行期 tool/tool_version 登记行所在）。
    pub run_db: PathBuf,
    /// 迁移目录（bin 缺省仓库根 migrations，测试注入 CARGO_MANIFEST_DIR
    /// 推导路径）。
    pub migrations_dir: PathBuf,
    /// Manifest 目录（runtime 块解析来源；tool 表不存 runtime 块，
    /// rp-workflow resolve_python_worker 同款口径）。
    pub manifest_dirs: Vec<PathBuf>,
    /// Artifact 落盘根（EvidenceRegistrar with_artifacts_dir；
    /// `<root>/<run_id>/<artifact_id>.json`）。
    pub artifacts_dir: PathBuf,
    /// 容器执行工作区根（PodmanHostConfig.workspace_root）。
    pub workspaces_root: PathBuf,
    /// 环境指纹 runtime_node_id（PodmanHostConfig.node_id）。
    pub runtime_node_id: String,
    /// python worker 进程 working_dir（rp-workflow 同款：bin 缺省当前目录，
    /// 即仓库根执行纪律；测试注入仓库根）。
    pub working_dir: Option<PathBuf>,
}

impl RunChannel {
    /// 打开运行库连接池并确保迁移到位（幂等：已迁移库只做版本核对）。
    fn open_pool(&self) -> Result<database::Pool, String> {
        let pool = database::Pool::open(database::PoolConfig {
            path: database::DbPath::File(self.run_db.clone()),
            readonly: false,
            max_connections: 2,
            busy_timeout: Duration::from_secs(5),
        })
        .map_err(|e| format!("run db open failed: {e}"))?;
        // WAL 显式开启（RIP3 P1-1，R2 §6.9）：运行库为高频写面；只读面
        // （open_pool_readonly）与生产 platform*.db 不受影响。
        pool.enable_wal().map_err(|e| format!("run db wal failed: {e}"))?;
        database::migrate(&pool, &self.migrations_dir)
            .map_err(|e| format!("run db migrate failed: {e}"))?;
        Ok(pool)
    }

    /// 只读打开运行库（get_run_status 读面；库不存在时不创建，如实报错）。
    fn open_pool_readonly(&self) -> Result<database::Pool, String> {
        if !self.run_db.exists() {
            return Err(format!(
                "run db 不存在：{}（先经 run_tool 或 rp-tool register --db 建立运行面）",
                self.run_db.display()
            ));
        }
        database::Pool::open(database::PoolConfig::readonly_file(&self.run_db))
            .map_err(|e| format!("run db open failed: {e}"))
    }

    /// 按 tool_id 扫 Manifest 目录取 runtime 块（json + yaml 都认；
    /// tls_inspect 系工具只有 yaml 登记件）。找不到如实报错。
    fn manifest_for(&self, tool_id: &str) -> Result<Value, String> {
        for dir in &self.manifest_dirs {
            let entries = match std::fs::read_dir(dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            let mut paths: Vec<PathBuf> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.extension()
                        .map(|x| x == "json" || x == "yaml")
                        .unwrap_or(false)
                })
                .collect();
            paths.sort();
            for path in paths {
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let parsed: Result<Value, String> = if text.trim_start().starts_with('{') {
                    serde_json::from_str::<Value>(&text)
                        .map_err(|e| e.to_string())
                } else {
                    serde_yaml_ng::from_str::<Value>(&text)
                        .map_err(|e| e.to_string())
                };
                let Ok(value) = parsed else { continue };
                if value.get("id").and_then(Value::as_str) == Some(tool_id) {
                    return Ok(value);
                }
            }
        }
        Err(format!(
            "manifest for {tool_id} not found in {:?}（runtime 块解析来源；登记走 rp-tool register）",
            self.manifest_dirs
        ))
    }

    /// rip.run_tool：受理 + 派发 + 登记（返回 MCP tools/call 载荷；
    /// Err 载荷 = isError=true 的结构化结果，不伪造成功）。
    pub fn run_tool(&self, args: &Value) -> Result<Value, Value> {
        let obj = args.as_object().ok_or_else(|| {
            err_payload("run_tool arguments must be a JSON object", "VALIDATION")
        })?;
        for key in obj.keys() {
            if !RUN_TOOL_ARG_KEYS.contains(&key.as_str()) {
                return Err(err_payload(
                    &format!("unknown run_tool argument {key:?}（词表：{RUN_TOOL_ARG_KEYS:?}）"),
                    "VALIDATION",
                ));
            }
        }
        let Some(tool_id) = obj.get("tool_id").and_then(Value::as_str) else {
            return Err(err_payload("run_tool 需要 tool_id", "VALIDATION"));
        };
        let Some(input) = obj.get("input").filter(|v| v.is_object()) else {
            return Err(err_payload("run_tool 需要 object 形态的 input", "VALIDATION"));
        };
        let task_id = obj.get("task_id").and_then(Value::as_str).map(str::to_string);
        let authorization_ref = obj
            .get("authorization_ref")
            .and_then(Value::as_str)
            .map(str::to_string);
        let tool_version = obj
            .get("tool_version")
            .and_then(Value::as_str)
            .map(str::to_string);
        let workspace_id = obj
            .get("workspace_id")
            .and_then(Value::as_str)
            .unwrap_or("ws-mcp")
            .to_string();
        let budget = obj.get("budget").filter(|b| b.is_object()).cloned();

        let pool = self.open_pool().map_err(|e| err_payload(&e, "INFRA"))?;
        let engine = Engine::open(pool.clone(), Registry::open(pool.clone()));
        let manifest = self.manifest_for(tool_id).map_err(|e| {
            err_payload(&e, "VALIDATION")
        })?;
        let runtime_type = manifest
            .get("runtime")
            .and_then(|r| r.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        let request = build_request(
            tool_id,
            input,
            &workspace_id,
            task_id.as_deref(),
            authorization_ref.as_deref(),
            tool_version.as_deref(),
            budget.as_ref(),
        );
        let submitted_run_id = request
            .get("run_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        // 派发：runtime.type 分流到两条既有驱动路径（本模块不写第三条）。
        // P9-4：受理走 submit_correlated 自锚定（trace_id = run_id，015/018
        // run 即链锚惯例；workflow tool 节点同一入口），使 MCP 通道运行可经
        // REST /api/traces 反查（双通道同一审计链载具前提）。
        let outcome = match runtime_type.as_str() {
            "python_worker" => {
                let adapter = build_python_adapter(&manifest, self.working_dir.clone())
                    .map_err(|e| err_payload(&e, "INFRA"))?;
                let submitted = engine
                    .submit_correlated(&request, Some(&submitted_run_id))
                    .map_err(rejection_payload)?;
                execute_via_engine(AdapterCall {
                    engine: &engine,
                    run_id: &submitted.run_id,
                    adapter: &adapter,
                    cancel_after: None,
                })
                .map_err(|e| err_payload(&format!("tool drive failed: {e}"), "INFRA"))?
            }
            "podman_host" | "podman_vm" => {
                let gateway = self.container_gateway(&engine, &pool)?;
                let dispatched = gateway.dispatch(&request).map_err(|e| match e {
                    runtime_gateway::GatewayError::Engine(engine_err) => rejection_payload(engine_err),
                    other => err_payload(&format!("gateway dispatch failed: {other}"), "INFRA"),
                })?;
                match dispatched {
                    runtime_gateway::DispatchOutcome::WaitingForRunner {
                        node_id, health, ..
                    } => {
                        // 探测失败不产生 FAILED（计划 §5.6:336 原文语义）：
                        // run 行已在库（WAITING_FOR_RUNNER），如实返回状态，
                        // 不伪造结果不登记证据（非终态）。
                        return Ok(json!({
                            "ok": false,
                            "run_id": submitted_run_id,
                            "tool_id": tool_id,
                            "status": "WAITING_FOR_RUNNER",
                            "node_id": node_id,
                            "health": health.as_str(),
                            "note": "容器 Runner 探测未通过（本机无 podman 或探针命令失败）；run 行已落库等待 Runner，恢复后经 Gateway 心跳恢复路径续跑（计划 §5.6:336，不伪装失败）"
                        }));
                    }
                    runtime_gateway::DispatchOutcome::Queued { run_id, .. } => {
                        self.drive_container(&engine, &manifest, &run_id)?
                    }
                }
            }
            other => {
                return Err(err_payload(
                    &format!(
                        "tool {tool_id} runtime type {other:?} 不在本面承载范围（python_worker / podman_host / podman_vm）"
                    ),
                    "VALIDATION",
                ));
            }
        };

        self.terminal_payload(&pool, &engine, outcome, task_id.as_deref())
    }

    /// rip.get_run_status：运行库只读投影（run 行在场即真实状态；不经过
    /// 引擎状态机，纯读面，rp_server /api/runs 同款只读语义）。
    pub fn run_status(&self, run_id: &str) -> Result<Value, Value> {
        let pool = self.open_pool_readonly().map_err(|e| {
            err_payload(&e, "INFRA")
        })?;
        let conn = pool
            .with_conn(|c| {
                let mut stmt = c.prepare(
                    "SELECT run_id, tool_id, tool_version, task_id, status, error_category,
                            created_at, started_at, finished_at, updated_at, duration_ms,
                            cancel_requested_at, trace_id
                     FROM tool_run WHERE run_id = ?1",
                )?;
                let mut rows = stmt.query_map([run_id], |r| {
                    Ok(json!({
                        "run_id": r.get::<_, String>(0)?,
                        "tool_id": r.get::<_, String>(1)?,
                        "tool_version": r.get::<_, String>(2)?,
                        "task_id": r.get::<_, Option<String>>(3)?,
                        "status": r.get::<_, String>(4)?,
                        "error_category": r.get::<_, Option<String>>(5)?,
                        "created_at": r.get::<_, String>(6)?,
                        "started_at": r.get::<_, Option<String>>(7)?,
                        "finished_at": r.get::<_, Option<String>>(8)?,
                        "updated_at": r.get::<_, String>(9)?,
                        "duration_ms": r.get::<_, Option<i64>>(10)?,
                        "cancel_requested_at": r.get::<_, Option<String>>(11)?,
                        "trace_id": r.get::<_, Option<String>>(12)?,
                    }))
                })?;
                match rows.next() {
                    Some(row) => Ok::<Option<Value>, database::rusqlite::Error>(Some(row?)),
                    None => Ok(None),
                }
            })
            .map_err(|e| err_payload(&format!("run db read failed: {e}"), "INFRA"))?;
        let Some(mut row) = conn else {
            return Err(err_payload(&format!("run {run_id:?} not found"), "VALIDATION"));
        };
        // Artifact / Evidence 引用计数与 id 列表（引用面，供外部 AI 持引用取件）。
        let conn = pool.with_conn(|c| {
            let mut arts = c
                .prepare("SELECT artifact_id, role, uri, sha256, size_bytes FROM artifact WHERE run_id = ?1 ORDER BY created_at")?;
            let list: Vec<Value> = arts
                .query_map([run_id], |r| {
                    Ok(json!({
                        "artifact_id": r.get::<_, String>(0)?,
                        "role": r.get::<_, String>(1)?,
                        "uri": r.get::<_, String>(2)?,
                        "sha256": r.get::<_, String>(3)?,
                        "size_bytes": r.get::<_, i64>(4)?,
                    }))
                })?
                .filter_map(|x| x.ok())
                .collect();
            let mut evs = c.prepare("SELECT evidence_id, evidence_type, storage_ref FROM evidence WHERE source_pk = ?1 ORDER BY created_at")?;
            let ev_list: Vec<Value> = evs
                .query_map([run_id], |r| {
                    Ok(json!({
                        "evidence_id": r.get::<_, String>(0)?,
                        "evidence_type": r.get::<_, String>(1)?,
                        "storage_ref": r.get::<_, Option<String>>(2)?,
                    }))
                })?
                .filter_map(|x| x.ok())
                .collect();
            Ok::<(Vec<Value>, Vec<Value>), database::rusqlite::Error>((list, ev_list))
        }).map_err(|e| err_payload(&format!("run db read failed: {e}"), "INFRA"))?;
        row["artifacts"] = json!(conn.0);
        row["evidence_refs"] = json!(
            conn.1
                .iter()
                .map(|e| e["evidence_id"].clone())
                .collect::<Vec<_>>()
        );
        row["evidence"] = json!(conn.1);
        Ok(row)
    }

    /// rip.cancel_run：协作式取消请求（D-TC-04），引擎同一
    /// request_cancel 实现（RUNNING / WAITING_FOR_RUNNER 可发起）。
    pub fn cancel_run(&self, run_id: &str) -> Result<Value, Value> {
        if !self.run_db.exists() {
            return Err(err_payload(
                &format!("run db 不存在：{}", self.run_db.display()),
                "INFRA",
            ));
        }
        let pool = self.open_pool().map_err(|e| err_payload(&e, "INFRA"))?;
        let engine = Engine::open(pool.clone(), Registry::open(pool.clone()));
        let accepted = engine.request_cancel(run_id).map_err(|e| match e {
            EngineError::NotFound(_) => {
                err_payload(&format!("run {run_id:?} not found"), "VALIDATION")
            }
            other => err_payload(&format!("cancel failed: {other}"), "INFRA"),
        })?;
        Ok(json!({
            "ok": true,
            "run_id": run_id,
            "accepted": accepted,
            "note": if accepted {
                "cancel_requested_at 已落库（协作式取消，RUNNING/WAITING_FOR_RUNNER 期受理）"
            } else {
                "当前状态不可发起取消（仅 RUNNING 与 WAITING_FOR_RUNNER 可请求，D-TC-04）"
            }
        }))
    }

    /// 容器路径 Gateway（podman_host 节点 + 真实 CommandProbe::podman 探针，
    /// P3-3 全链同款装配）。
    fn container_gateway(
        &self,
        engine: &Engine,
        pool: &database::Pool,
    ) -> Result<runtime_gateway::Gateway, Value> {
        let store = runtime_gateway::HealthStore::open(pool.clone());
        let gateway = runtime_gateway::Gateway::new(
            engine.clone(),
            store,
            runtime_gateway::GatewayConfig::default(),
        );
        gateway
            .register_node(
                domain::RuntimeNode {
                    node_id: self.runtime_node_id.clone(),
                    kind: domain::RuntimeNodeKind::PodmanHost,
                    endpoint: None,
                    health: domain::NodeHealth::Unknown,
                    version: None,
                },
                std::sync::Arc::new(runtime_gateway::CommandProbe::podman()),
            )
            .map_err(|e| err_payload(&format!("register runtime node failed: {e}"), "INFRA"))?;
        Ok(gateway)
    }

    /// 容器路径驱动（image / entrypoint / family / egress 端口全部来自
    /// Manifest 登记件，不猜测；P3-3 execute_container_via_engine 全链）。
    fn drive_container(
        &self,
        engine: &Engine,
        manifest: &Value,
        run_id: &str,
    ) -> Result<DriveOutcome, Value> {
        let runtime = manifest
            .get("runtime")
            .ok_or_else(|| err_payload("manifest has no runtime block", "VALIDATION"))?;
        let image = runtime
            .get("image")
            .and_then(Value::as_str)
            .ok_or_else(|| err_payload("manifest runtime.image missing", "VALIDATION"))?
            .to_string();
        let entrypoint: Vec<String> = runtime
            .get("entrypoint")
            .and_then(Value::as_array)
            .ok_or_else(|| err_payload("manifest runtime.entrypoint missing", "VALIDATION"))?
            .iter()
            .map(|v| v.as_str().unwrap_or_default().to_string())
            .collect();
        let family = manifest
            .get("family")
            .and_then(Value::as_str)
            .unwrap_or("default")
            .to_string();
        let network = match manifest
            .get("network")
            .and_then(|n| n.get("mode"))
            .and_then(Value::as_str)
        {
            Some("egress_allowlist") => {
                let ports: Vec<u16> = manifest
                    .get("network")
                    .and_then(|n| n.get("egress"))
                    .and_then(|e| e.get("ports"))
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|p| p.as_u64().map(|x| x as u16))
                            .collect()
                    })
                    .unwrap_or_else(|| vec![80, 443]);
                NetworkPlan::EgressAllowlist { ports }
            }
            _ => NetworkPlan::None,
        };
        let adapter = PodmanHostAdapter::new(PodmanHostConfig {
            workspace_root: self.workspaces_root.clone(),
            node_id: self.runtime_node_id.clone(),
            ..PodmanHostConfig::default()
        });
        let call = ContainerCall {
            engine,
            run_id,
            adapter: &adapter,
            image: &image,
            entrypoint: &entrypoint,
            family: &family,
            network,
        };
        execute_container_via_engine(call)
            .map_err(|e| err_payload(&format!("container drive failed: {e}"), "INFRA"))
    }

    /// 终态载荷：信封投影 + Artifact/Evidence 登记（P7-2 同一实现，
    /// face=rp-mcp；evidence id 回填信封 evidence_refs）。
    fn terminal_payload(
        &self,
        pool: &database::Pool,
        _engine: &Engine,
        mut outcome: DriveOutcome,
        task_id: Option<&str>,
    ) -> Result<Value, Value> {
        let envelope = &mut outcome.envelope;
        let payload_for_ledger = envelope
            .result
            .clone()
            .or_else(|| {
                envelope.error.as_ref().map(|e| {
                    json!({
                        "category": e.category,
                        "code": e.code,
                        "message": e.message,
                        "retryable": e.retryable,
                    })
                })
            })
            .unwrap_or_else(|| json!({}));
        let registrar = agent_engine::evidence::EvidenceRegistrar::new(pool.clone())
            .with_face("rp-mcp")
            .with_artifacts_dir(self.artifacts_dir.clone());
        let registered = registrar
            .register_tool_result(&agent_engine::evidence::Registration {
                agent_id: "run_tool".to_string(),
                task_id: task_id.map(str::to_string),
                tool_run_id: envelope.run_id.clone(),
                tool_id: envelope.tool_id.clone(),
                tool_version: envelope.tool_version.clone(),
                payload: payload_for_ledger,
                // P9-4：artifact/evidence 行同链落 trace_id = run_id（015 单锚
                // 传播，/api/traces 经 evidence 探针也可反查本 run）。
                trace_id: Some(envelope.run_id.clone()),
            })
            .map_err(|e| err_payload(&format!("artifact/evidence registration failed: {e}"), "INFRA"))?;
        envelope.evidence_refs = vec![registered.evidence_id.clone()];

        let succeeded = envelope.status == domain::ToolRunStatus::Succeeded;
        let projection = tool_engine::envelope::to_value(envelope);
        Ok(json!({
            "ok": succeeded,
            "run_id": envelope.run_id,
            "tool_id": envelope.tool_id,
            "tool_version": envelope.tool_version,
            "status": envelope.status.as_str(),
            "result": envelope.result,
            "summary": envelope.summary,
            "error": envelope.error.as_ref().map(|e| json!({
                "category": e.category,
                "code": e.code,
                "message": e.message,
                "retryable": e.retryable,
            })),
            "artifacts": projection.get("artifacts").cloned().unwrap_or(json!([])),
            "artifact_id": registered.artifact_id,
            "artifact_uri": registered.artifact_uri,
            "evidence_refs": [registered.evidence_id],
            "evidence_id": registered.evidence_id,
            "content_hash": registered.content_hash,
            "usage": projection.get("usage").cloned().unwrap_or(json!({})),
            "environment": projection.get("environment").cloned().unwrap_or(json!({})),
            "transcript": outcome.transcript.iter().map(|s| json!(s.as_str())).collect::<Vec<_>>(),
        }))
    }
}

/// ToolCallRequest 整形（TOOL-CONTRACT §4.1；run_id 本面生成，形状同
/// workflow exec::node_tool 的 new_id("rn")）。
#[allow(clippy::too_many_arguments)]
fn build_request(
    tool_id: &str,
    input: &Value,
    workspace_id: &str,
    task_id: Option<&str>,
    authorization_ref: Option<&str>,
    tool_version: Option<&str>,
    budget: Option<&Value>,
) -> Value {
    let mut request = json!({
        "protocol_version": "1",
        "run_id": new_run_id(),
        "tool_id": tool_id,
        "input": input,
        "workspace_id": workspace_id,
    });
    if let Some(obj) = request.as_object_mut() {
        if let Some(v) = task_id {
            obj.insert("task_id".to_string(), json!(v));
        }
        if let Some(v) = authorization_ref {
            obj.insert("authorization_ref".to_string(), json!(v));
        }
        if let Some(v) = tool_version {
            obj.insert("tool_version".to_string(), json!(v));
        }
        if let Some(b) = budget {
            obj.insert("budget".to_string(), b.clone());
        }
    }
    request
}

/// run id 生成（rn-<hash16>-<seq8>，workflow exec::new_id 同款形状）。
fn new_run_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    static SALT: std::sync::OnceLock<(u64, u64)> = std::sync::OnceLock::new();
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let (salt_a, salt_b) = *SALT.get_or_init(|| {
        (
            std::collections::hash_map::RandomState::new()
                .build_hasher()
                .finish(),
            std::collections::hash_map::RandomState::new()
                .build_hasher()
                .finish(),
        )
    });
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write(format!("{nanos}|{salt_a:x}{salt_b:x}|{seq}|{}|mcp-server", std::process::id()).as_bytes());
    let digest = hasher.finish();
    format!("rn-{digest:016x}{seq:08x}")
}

/// python_worker 适配器装配（worker_name / command 取自 Manifest runtime
/// 块，rp-workflow resolve_python_worker 同款口径；repo:// 前缀按当前
/// 目录落地，由调用方保证仓库根执行纪律）。
fn build_python_adapter(
    manifest: &Value,
    working_dir: Option<PathBuf>,
) -> Result<PythonWorkerAdapter, String> {
    let runtime = manifest
        .get("runtime")
        .ok_or_else(|| "manifest has no runtime block".to_string())?;
    let worker_name = runtime
        .get("worker_name")
        .and_then(Value::as_str)
        .unwrap_or("rip-cli-worker")
        .to_string();
    let command: Vec<String> = runtime
        .get("command")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if command.is_empty() {
        return Err("manifest runtime.command is empty".to_string());
    }
    Ok(PythonWorkerAdapter::new(PythonWorkerConfig {
        worker_name,
        command,
        working_dir,
        restart: RestartPolicy::Pooled,
        extra_env: Vec::new(),
        handshake_timeout: Duration::from_secs(10),
    }))
}

/// isError=true 的结构化载荷（category 用 §5.3 词表：VALIDATION /
/// POLICY_DENIED / INFRA / EXECUTION）。
fn err_payload(message: &str, category: &str) -> Value {
    json!({
        "ok": false,
        "error": {
            "category": category,
            "message": message,
        }
    })
}

/// 引擎提交拒绝转载荷（D-TC-05：拒绝不落行；T2 缺授权引用为 POLICY_DENIED，
/// 其余为 VALIDATION，词表同 Rejection::category）。
fn rejection_payload(e: EngineError) -> Value {
    match e {
        EngineError::Rejected(r) => err_payload(&r.reason(), r.category()),
        other => err_payload(&format!("tool engine error: {other}"), "INFRA"),
    }
}

impl RunChannel {
    /// 运行库 tool 表 Schema 读面（get_tool_schema 的 run_db 回退段）：
    /// 目录读面（主库/aux）没有的行，若运行库已登记则按同一投影返回
    /// （tool 表同一形状，只读，不发明条目）。
    pub fn tool_schema(&self, tool_id: &str) -> Result<Option<Value>, String> {
        if !self.run_db.exists() {
            return Ok(None);
        }
        let pool = self
            .open_pool_readonly()
            .map_err(|e| format!("run db open failed: {e}"))?;
        pool.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT input_schema, output_schema FROM tool WHERE tool_id = ?1",
            )?;
            let mut rows = stmt.query_map([tool_id], |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, Option<String>>(1)?,
                ))
            })?;
            match rows.next() {
                None => Ok::<Option<Value>, database::rusqlite::Error>(None),
                Some(row) => {
                    let (input_raw, output_raw) = row?;
                    let parse = |raw: Option<String>| -> Value {
                        raw.and_then(|t| serde_json::from_str::<Value>(&t).ok())
                            .unwrap_or(Value::Null)
                    };
                    Ok::<Option<Value>, database::rusqlite::Error>(Some(json!({
                        "input_schema": parse(input_raw),
                        "output_schema": parse(output_raw),
                    })))
                }
            }
        })
        .map_err(|e| format!("run db read failed: {e}"))
    }
}
