//! DI-3/RT-4 判据 3：dispatch 派发 e2e 拒绝面（负例为硬性红线）——PolicyResolver
//! 接线（RT-3-ADR D9 缝合点）后的五负例硬拒矩阵，spawn 真实 rp-mcp 二进制
//! （run_tool_e2e 同一 rmcp stdio 通道）逐负例断言：
//!
//! 1. **词表外 runtime.type** → `RT3_RUNTIME_VOCAB`（VALIDATION；decide N10b，
//!    复杂度维 Simple 基线 = policy_resolver_matrix 词表外行同款形状）；
//! 2. **fixture venv 声明指缺省根** → `RT3_VENV_UNAVAILABLE`（DEPENDENCY）
//!    且零 worker 进程拉起（Resolver venv 真探测先于 submit/adapter spawn，
//!    受理拒绝零 run 行——D3① fail-closed 先于 adapter ensure_ready 双层纵深）；
//! 3. **rip.ops.worker-runtime 正式 manifest 经 run_tool** → VALIDATION 拒
//!    （not_applicable 元数据条目不派发面——R7 短路在最前不消费 venv 子事实，
//!    known_limits「登记即置 DISABLED」语义在派发面兑现，行为变化显性记账）；
//! 4. **合成 inconsistent fixture（声明↔目标冲突）** → POLICY_DENIED + gap
//!    清单随 payload 回显（N6 vm_local×原生档声明；迁移完成态 28 assign 下
//!    生产 unreachable，负例 fixture 承载防绕过语义）；
//! 5. **podman_vm 声明 fixture（network vm_local）** → `RT3_VM_NOT_READY`
//!    （RUNNER_UNAVAILABLE，零 gateway 派发零降级——floor=vm × vm_ready=false，
//!    接线前该声明走 container_gateway WaitingForRunner 非终态，接线后派发
//!    前硬拒）。
//!
//! V-9 守卫预挂（0f2922cb 教训口径）：bin/seed/载具缺 → SKIP 计数 + 零断言
//! 返回（不伪装 PASS 不误报 FAIL）。全离线零安装，`--test-threads=1`。

mod common;

use std::path::{Path, PathBuf};

use database::{DbPath, Pool, PoolConfig};
use serde_json::{json, Value};

use common::McpProc;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

static TMP_SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// V-9 守卫预挂：载具（capability 种子 / 正式 worker-runtime 登记件 / 迁移
/// 目录）缺 → SKIP（println 计数理由 + 零断言返回）。
fn v9_guard() -> bool {
    let root = repo_root();
    let carriers = [
        "manifests/capabilities/host-catalog-seed.json",
        "manifests/tools/rip.ops.worker-runtime.json",
        "migrations",
    ];
    match carriers.iter().find(|rel| !root.join(rel).exists()) {
        Some(rel) => {
            println!("SKIP dispatch_policy_gate: repo carrier missing ({rel})");
            false
        }
        None => true,
    }
}

/// 测试工作区（run_db / artifacts / 内联 Manifest 目录置首——fixture 同 id
/// 遮蔽仓库登记件，run_tool_e2e A8 同款口径）。
struct GateFixture {
    _dir: PathBuf,
    run_db: PathBuf,
    manifest_dir: PathBuf,
}

fn fixture(tag: &str) -> GateFixture {
    let dir = std::env::temp_dir().join(format!(
        "rp-mcp-policy-gate-{}-{}-{}",
        tag,
        std::process::id(),
        TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).expect("fixture dir");
    let manifest_dir = dir.join("manifests");
    std::fs::create_dir_all(&manifest_dir).expect("manifest dir");
    let run_db = dir.join("run.db");
    GateFixture {
        _dir: dir,
        run_db,
        manifest_dir,
    }
}

impl GateFixture {
    fn server(&self) -> McpProc {
        McpProc::spawn_in(
            &[
                ("RP_MCP_TOKEN", "tok"),
                ("RP_RUN_DB", self.run_db.to_string_lossy().as_ref()),
                (
                    "RP_MIGRATIONS_DIR",
                    repo_root().join("migrations").to_string_lossy().as_ref(),
                ),
                (
                    "RP_MANIFEST_DIRS",
                    self.manifest_dir.to_string_lossy().as_ref(),
                ),
                (
                    "RP_CAPABILITY_SEED",
                    repo_root()
                        .join("manifests/capabilities/host-catalog-seed.json")
                        .to_string_lossy()
                        .as_ref(),
                ),
            ],
            Some(&repo_root()),
        )
    }
}

/// 夹具 Manifest 基座（登记面 + 派发面同源；词形对齐正式登记件骨架）。
fn gate_manifest(tool_id: &str, description: &str, runtime: Value, network: Value) -> Value {
    json!({
        "schema_version": "1",
        "id": tool_id,
        "name": format!("DI-3/RT-4 gate fixture ({tool_id})"),
        "version": "0.1.0",
        "kind": "tool",
        "family": "utility",
        "description": description,
        "runtime": runtime,
        "input_schema": {"type": "object"},
        "output_schema": {"type": "object"},
        "resources": {"cpu": "1", "memory": "64Mi", "timeout_seconds": 30, "max_output_bytes": 65536},
        "network": network,
        "secrets": [],
        "permission_level": "T0",
        "idempotent": true,
        "health": {"probe": {"type": "worker_ping"}},
        "provenance": {
            "origin": "rip-native",
            "license": "repo-internal",
            "dependencies": [],
            "tested": "DI-3/RT-4 dispatch_policy_gate fixture"
        }
    })
}

fn write_fixture(fix: &GateFixture, manifest: &Value) {
    let id = manifest["id"].as_str().expect("fixture id");
    let path = fix.manifest_dir.join(format!("{id}.json"));
    std::fs::write(&path, serde_json::to_vec_pretty(manifest).unwrap()).expect("write manifest");
}

/// 运行库直连（断言面）。
fn run_row_count(fix: &GateFixture) -> i64 {
    let pool = Pool::open(PoolConfig {
        path: DbPath::File(fix.run_db.clone()),
        readonly: true,
        max_connections: 1,
        busy_timeout: std::time::Duration::from_secs(5),
    })
    .expect("run db pool");
    pool.with_conn(|c| {
        c.query_row("SELECT COUNT(*) FROM tool_run WHERE 1=1", [], |r| {
            r.get::<_, i64>(0)
        })
    })
    .expect("count")
}

fn payload_of(reply: &Value) -> Value {
    serde_json::from_str(
        reply["result"]["content"][0]["text"]
            .as_str()
            .expect("text"),
    )
    .expect("payload json")
}

/// 负例 1：词表外 runtime.type → RT3_RUNTIME_VOCAB（VALIDATION）。
#[test]
fn gate_vocab_outside_runtime_type_rejected() {
    if !v9_guard() {
        return;
    }
    let fix = fixture("vocab");
    let manifest = gate_manifest(
        "rip.t.gate.vocab",
        "判据 3 负例①：词表外 runtime.type（mcp_client）",
        json!({"type": "mcp_client"}),
        json!({"mode": "none"}),
    );
    write_fixture(&fix, &manifest);
    let mut proc = fix.server();
    proc.handshake("2024-11-05");
    let reply = proc.call(
        "rip.run_tool",
        json!({"tool_id": "rip.t.gate.vocab", "input": {}}),
    );
    assert_eq!(
        reply["result"]["isError"], true,
        "词表外必须 isError: {reply}"
    );
    let payload = payload_of(&reply);
    assert_eq!(payload["error"]["category"], "VALIDATION", "{payload}");
    assert_eq!(payload["error"]["code"], "RT3_RUNTIME_VOCAB", "{payload}");
    assert_eq!(run_row_count(&fix), 0, "受理拒绝零 run 行");
    proc.wait_exit();
}

/// 负例 2：fixture venv 声明指缺省根 → RT3_VENV_UNAVAILABLE（DEPENDENCY）
/// 零 worker 进程拉起（探测拒绝先于 spawn）。
#[test]
fn gate_venv_absent_root_rejected_before_spawn() {
    if !v9_guard() {
        return;
    }
    let fix = fixture("venv-absent");
    let absent_root = std::env::temp_dir().join(format!(
        "rt4-gate-absent-venv-{}-{}",
        std::process::id(),
        TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    let manifest = gate_manifest(
        "rip.t.gate.venv",
        "判据 3 负例②：venv 声明指缺省根（探测 Missing）",
        json!({
            "type": "python_worker",
            "protocol_version": "1",
            "worker_name": "parsing-worker",
            "command": ["python3", "-I", "workers/python/parsing_worker.py"],
            "working_dir": "repo://research-platform",
            "venv": {"path": absent_root.to_string_lossy(), "direct_deps": ["pip"]}
        }),
        json!({"mode": "none"}),
    );
    write_fixture(&fix, &manifest);
    let mut proc = fix.server();
    proc.handshake("2024-11-05");
    let reply = proc.call(
        "rip.run_tool",
        json!({"tool_id": "rip.t.gate.venv", "input": {}}),
    );
    assert_eq!(reply["result"]["isError"], true, "{reply}");
    let payload = payload_of(&reply);
    assert_eq!(payload["error"]["category"], "DEPENDENCY", "{payload}");
    assert_eq!(
        payload["error"]["code"], "RT3_VENV_UNAVAILABLE",
        "{payload}"
    );
    // 探测拒绝先于 spawn：零 run 行（提交前受理拒绝）+ 拒绝消息为 venv 五态
    // 语义（非 worker 执行错误）。
    assert_eq!(run_row_count(&fix), 0, "venv 探测拒绝零 run 行");
    let message = payload["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("venv") || message.contains("RT3_VENV_UNAVAILABLE"),
        "拒绝消息点名 venv 探测面: {payload}"
    );
    proc.wait_exit();
}

/// 负例 3：rip.ops.worker-runtime 正式 manifest 经 run_tool → VALIDATION 拒
/// （not_applicable 不派发面）。
#[test]
fn gate_metadata_entry_not_dispatchable() {
    if !v9_guard() {
        return;
    }
    let fix = fixture("metadata");
    // 正式登记件原文入夹具（R7 短路在最前不消费 venv 子事实——CI 无
    // workers/python/.venv 时 probe Missing 亦不改变 not_applicable 语义）。
    let official =
        std::fs::read_to_string(repo_root().join("manifests/tools/rip.ops.worker-runtime.json"))
            .expect("official manifest readable");
    let manifest: Value = serde_json::from_str(&official).expect("official manifest json");
    write_fixture(&fix, &manifest);
    let mut proc = fix.server();
    proc.handshake("2024-11-05");
    let reply = proc.call(
        "rip.run_tool",
        json!({"tool_id": "rip.ops.worker-runtime", "input": {}}),
    );
    assert_eq!(reply["result"]["isError"], true, "{reply}");
    let payload = payload_of(&reply);
    assert_eq!(payload["error"]["category"], "VALIDATION", "{payload}");
    let message = payload["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("not_applicable") || message.contains("元数据条目"),
        "拒绝点名元数据条目不派发面: {payload}"
    );
    assert_eq!(run_row_count(&fix), 0, "not_applicable 拒绝零 run 行");
    proc.wait_exit();
}

/// 负例 4：合成 inconsistent fixture（声明↔目标冲突）→ POLICY_DENIED +
/// gap 清单随 payload 回显。
#[test]
fn gate_inconsistent_declaration_rejected_with_gap_echo() {
    if !v9_guard() {
        return;
    }
    let fix = fixture("inconsistent");
    // 声明 python_worker × network vm_local → N6 声明冲突语境（gap =
    // vm_local→kernel→vm 迁移清单）；迁移完成态 28 assign 下生产 unreachable，
    // 负例 fixture 承载「未来登记声明↔决策不一致即拒」防绕过语义。
    let manifest = gate_manifest(
        "rip.t.gate.inconsistent",
        "判据 3 负例④：声明与派生目标冲突（vm_local × python_worker）",
        json!({
            "type": "python_worker",
            "protocol_version": "1",
            "worker_name": "parsing-worker",
            "command": ["python3", "-I", "workers/python/parsing_worker.py"],
            "working_dir": "repo://research-platform"
        }),
        json!({"mode": "vm_local"}),
    );
    write_fixture(&fix, &manifest);
    let mut proc = fix.server();
    proc.handshake("2024-11-05");
    let reply = proc.call(
        "rip.run_tool",
        json!({"tool_id": "rip.t.gate.inconsistent", "input": {}}),
    );
    assert_eq!(reply["result"]["isError"], true, "{reply}");
    let payload = payload_of(&reply);
    assert_eq!(payload["error"]["category"], "POLICY_DENIED", "{payload}");
    let gap = payload["error"]["gap"].as_array().expect("gap echo");
    assert!(!gap.is_empty(), "gap 清单随 payload 回显: {payload}");
    let gap_text = gap
        .iter()
        .filter_map(|g| g.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        gap_text.contains("vm_local")
            && gap_text.contains("kernel")
            && gap_text.contains("podman_vm"),
        "gap = vm_local→kernel→vm 迁移清单: {payload}"
    );
    assert_eq!(run_row_count(&fix), 0, "inconsistent 拒绝零 run 行");
    proc.wait_exit();
}

/// 负例 5：podman_vm 声明 fixture（network vm_local）→ RT3_VM_NOT_READY
/// （RUNNER_UNAVAILABLE，零 gateway 派发零降级）。
#[test]
fn gate_podman_vm_declaration_rejected_no_gateway_dispatch() {
    if !v9_guard() {
        return;
    }
    let fix = fixture("podman-vm");
    let manifest = gate_manifest(
        "rip.t.gate.podmanvm",
        "判据 3 负例⑤：podman_vm 声明（VM 未就绪 floor=vm）",
        json!({
            "type": "podman_vm",
            "image": "@sha256:1111111111111111111111111111111111111111111111111111111111111111",
            "entrypoint": ["/bin/true"]
        }),
        json!({"mode": "vm_local"}),
    );
    write_fixture(&fix, &manifest);
    let mut proc = fix.server();
    proc.handshake("2024-11-05");
    let reply = proc.call(
        "rip.run_tool",
        json!({"tool_id": "rip.t.gate.podmanvm", "input": {}}),
    );
    assert_eq!(reply["result"]["isError"], true, "{reply}");
    let payload = payload_of(&reply);
    assert_eq!(
        payload["error"]["category"], "RUNNER_UNAVAILABLE",
        "{payload}"
    );
    assert_eq!(payload["error"]["code"], "RT3_VM_NOT_READY", "{payload}");
    // 零 gateway 派发零降级：非 WAITING_FOR_RUNNER 非终态（接线前该声明走
    // container_gateway 探测面），run 行零落（受理拒绝不落行）。
    assert_eq!(run_row_count(&fix), 0, "VM 未就绪拒绝零 run 行");
    let message = payload["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("VM") || message.contains("RT3_VM_NOT_READY"),
        "拒绝消息点名 VM 闸: {payload}"
    );
    proc.wait_exit();
}
