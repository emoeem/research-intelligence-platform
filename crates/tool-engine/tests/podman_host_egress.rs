//! P3-2 验证载具：podman_host 出站 allowlist 执行面（egress_proxy + 分派）。
//!
//! 判据（批次自验清单）：
//! 1. allowlist 内 https://example.com/ 经注入代理 CONNECT 通（SUCCEEDED +
//!    代理 TunnelAccepted 事件 + body.bin Artifact sha256 宿主对账）；
//! 2. 非 allowlist host 的 CONNECT 被拒并落结构化错误（run FAILED/EXECUTION，
//!    错误消息含 tunnel/403；代理 Rejected 事件落账），不静默换通道；
//! 3. 注入面实测冻结：容器 Config.Env 含 http(s)_proxy=http://127.0.0.1:<port>，
//!    NetworkMode=pasta:-T（本机 podman 6.1.4：pasta 在位、slirp4netns 不在，
//!    宿主接口 IP 直连变体被 pasta 拒，Connection refused，本测试即冻结证据）；
//! 4. network.mode=none 分派守卫：维持 --network none，url 工具无网失败；
//! 5. 代理生命周期随 run 结束退出（egress_proxy.rs 单测覆盖端口关闭）。
//!
//! 边界声明（ADR §8 已知代价的宿主面版本）：过渡期 allowlist 为代理层策略
//! 执行，只对遵守 http(s)_proxy env 的客户端（自有可信工具面）有效；pasta
//! 默认出站未在网络层硬隔离，不受信工具与硬隔离属 podman_vm 目标拓扑。
//!
//! 门禁：容器端到端保持本地执行（V-9 同款）：podman 不可用或 webresearch
//! 镜像不可构建时打印跳过说明直接返回，CI（无 podman）保持全绿。

use database::{migrate, DbPath, Pool, PoolConfig};
use domain::ToolRunStatus;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tool_engine::adapter::{
    execute_container_via_engine, proxy_events, ContainerCall, NetworkPlan, PodmanHostAdapter,
    PodmanHostConfig, ProxyEvent,
};
use tool_engine::Engine;
use tool_registry::{Options, Registry};

const IMAGE_TAG: &str = "localhost/rip-tool-webresearch:latest";
/// P3-1 构建产物 digest（containerfiles/README.md 台账同步值；P3-2 批次内
/// 工具脚本补 stderr 镜像后重建更新）。
const IMAGE_DIGEST: &str = "085b4ef304e990be18d8f6aa917ec0c04c33223486d2f971544493176e288c58";

static IMAGE_LOCK: Mutex<()> = Mutex::new(());
/// 容器生成类用例串行执行（容器面清零断言依赖独占）。
static E2E_LOCK: Mutex<()> = Mutex::new(());

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn podman_available() -> bool {
    Command::new("podman")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// webresearch 镜像就绪（缺失时按 P3-1 Containerfile 构建，宿主零安装）。
fn ensure_image() -> String {
    let _guard = IMAGE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let exists = Command::new("podman")
        .args(["image", "exists", IMAGE_TAG])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !exists {
        let dir = repo_root().join("containerfiles/rip-tool-webresearch");
        let out = Command::new("podman")
            .args([
                "build",
                "-t",
                IMAGE_TAG,
                "-f",
                &dir.join("Containerfile").to_string_lossy(),
                &dir.to_string_lossy(),
            ])
            .output()
            .expect("podman build spawn");
        assert!(
            out.status.success(),
            "podman build failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let digest = Command::new("podman")
        .args(["inspect", "--format", "{{.Digest}}", IMAGE_TAG])
        .output()
        .expect("podman inspect spawn");
    let text = String::from_utf8_lossy(&digest.stdout).trim().to_string();
    assert_eq!(
        text,
        format!("sha256:{IMAGE_DIGEST}"),
        "local image digest must match containerfiles/README.md digest ledger"
    );
    format!("{IMAGE_TAG}@sha256:{IMAGE_DIGEST}")
}

/// rip.http.inspect 测试 Manifest（镜像/入口与 P3-1 交付物一致；正式
/// manifests/tools/ 双份登记属 P3-3/P3-4 批次）。
fn http_inspect_manifest(image_ref: &str) -> Value {
    json!({
        "schema_version": "1",
        "id": "rip.http.inspect",
        "name": "RIP HTTP Inspect",
        "version": "0.1.0",
        "kind": "tool",
        "family": "web_http",
        "description": "T1 只读 GET 检查：url 派生临时 allowlist，出站经本机 CONNECT 白名单代理（P3-2）",
        "runtime": {
            "type": "podman_host",
            "image": image_ref,
            "entrypoint": ["python3", "/app/http_inspector.py"],
            "readonly_rootfs": true,
            "mounts": [
                {"role": "input", "path": "/task/in", "mode": "ro"},
                {"role": "output", "path": "/task/out", "mode": "rw"}
            ]
        },
        "input_schema": {
            "type": "object",
            "required": ["url"],
            "properties": {
                "url": {"type": "string", "description": "http/https GET 目标（allowlist host 派生源）"},
                "method": {"type": "string", "description": "仅 GET（T1）"},
                "timeout_seconds": {"type": "integer", "minimum": 1, "maximum": 60},
                "max_body_bytes": {"type": "integer", "minimum": 1},
                "headers": {"type": "object"}
            }
        },
        "output_schema": {
            "type": "object",
            "required": ["request_url", "final_url", "status_code", "body_bytes", "body_sha256"],
            "properties": {
                "request_url": {"type": "string"},
                "final_url": {"type": "string"},
                "status_code": {"type": "integer"},
                "reason": {"type": "string"},
                "redirects": {"type": "integer"},
                "elapsed_ms": {"type": "integer"},
                "body_bytes": {"type": "integer"},
                "body_sha256": {"type": "string"},
                "body_cap_bytes": {"type": "integer"},
                "content_type": {"type": "string"},
                "headers_selected": {"type": "object"},
                "header_count": {"type": "integer"},
                "out_files": {"type": "array"},
                "uid": {"type": "integer"}
            }
        },
        "resources": {
            "cpu": "2",
            "memory": "2Gi",
            "pid_limit": 256,
            "disk_mb": 256,
            "timeout_seconds": 120,
            "max_output_bytes": 1048576
        },
        "network": {
            "mode": "egress_allowlist",
            "egress": {
                "ports": [80, 443],
                "domains": ["example.com（静态声明为空：动态目标由派发时 host 派生临时 allowlist）"],
                "note": "P3-2：allowlist 为代理层策略执行，域名级网络阻断属已知限制（PHASE3-DESIGN 5.3）"
            }
        },
        "secrets": [],
        "permission_level": "T1",
        "idempotent": true,
        "health": {
            "probe": {
                "type": "exec",
                "command": ["python3", "-c", "import json; print(json.dumps({'ok': True}))"],
                "timeout_ms": 5000,
                "interval_seconds": 600,
                "degraded_after": 1,
                "broken_after": 3
            },
            "on_run_recover": true
        },
        "provenance": {
            "origin": "rip-native",
            "source_repo": "containerfiles/rip-tool-webresearch（本仓库，自研纯标准库包装）",
            "license": "repo-internal",
            "dependencies": [],
            "image_digest": format!("sha256:{IMAGE_DIGEST}"),
            "known_limits": [
                "T1 仅 GET；出站 allowlist 为代理层执行（遵守代理 env 的客户端），网络层硬隔离属 podman_vm 目标拓扑"
            ],
            "tested": "P3-2 批次端到端实测（crates/tool-engine/tests/podman_host_egress.rs）"
        },
        "context": {
            "summary": "Web Research 首工具：只读 GET",
            "tags": ["web_http", "podman_host", "p3-2"]
        },
        "examples": [
            {
                "input": {"url": "https://example.com/"},
                "output_hint": {"status": 200, "body_sha256": "..."}
            }
        ]
    })
}

/// env 注入冻结探针 Manifest（同镜像不同入口：睡眠后回显代理 env）。
fn env_probe_manifest(image_ref: &str) -> Value {
    let mut manifest = http_inspect_manifest(image_ref);
    manifest["id"] = json!("rip.http.envprobe");
    manifest["name"] = json!("RIP HTTP Env Probe");
    manifest["description"] =
        json!("P3-2 注入面冻结探针：睡眠 10s 内可 inspect，回显容器内代理 env");
    manifest["runtime"]["entrypoint"] = json!([
        "python3",
        "-c",
        "import os, sys, time; time.sleep(10); sys.stdout.write(__import__('json').dumps({'status': 'result', 'data': {'http_proxy': os.environ.get('http_proxy', ''), 'https_proxy': os.environ.get('https_proxy', ''), 'uid': os.getuid()}}) + '\\n')"
    ]);
    manifest["input_schema"] = json!({
        "type": "object",
        "required": ["url"],
        "properties": {"url": {"type": "string"}}
    });
    manifest["output_schema"] = json!({
        "type": "object",
        "required": ["https_proxy"],
        "properties": {
            "http_proxy": {"type": "string"},
            "https_proxy": {"type": "string"},
            "uid": {"type": "integer"}
        }
    });
    manifest["examples"] = json!([
        {"input": {"url": "https://example.com/"}, "output_hint": {"https_proxy": "http://127.0.0.1:0"}}
    ]);
    manifest
}

fn fresh_env(tag: &str, manifest: Value) -> (Pool, Engine, PathBuf) {
    let pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();
    migrate(&pool, &repo_root().join("migrations")).unwrap();
    let registry = Registry::open(pool.clone()).with_options(Options {
        allow_placeholder_digest: false,
    });
    registry.register(&manifest).unwrap();
    let engine = Engine::open(pool.clone(), registry);
    let ws_root = std::env::temp_dir().join(format!("rip-p32-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&ws_root);
    std::fs::create_dir_all(&ws_root).unwrap();
    (pool, engine, ws_root)
}

fn submit(engine: &Engine, run_id: &str, input: Value, timeout_seconds: u64) {
    engine
        .submit(&json!({
            "protocol_version": "1",
            "tool_id": "rip.http.inspect",
            "run_id": run_id,
            "submit_key": format!("p32-{run_id}"),
            "input": input,
            "workspace_id": "ws-p32",
            "budget": {"timeout_seconds": timeout_seconds, "max_output_bytes": 1048576},
            "result_schema_version": "1"
        }))
        .expect("submit accepted");
}

fn submit_probe(engine: &Engine, run_id: &str, input: Value, timeout_seconds: u64) {
    engine
        .submit(&json!({
            "protocol_version": "1",
            "tool_id": "rip.http.envprobe",
            "run_id": run_id,
            "submit_key": format!("p32-{run_id}"),
            "input": input,
            "workspace_id": "ws-p32",
            "budget": {"timeout_seconds": timeout_seconds, "max_output_bytes": 1048576},
            "result_schema_version": "1"
        }))
        .expect("submit accepted");
}

fn rip_containers() -> Vec<String> {
    if !podman_available() {
        // V-9：podman 不可用时用例在入口 SKIP，这里返回空集避免
        // 守卫前的清理调用先 panic（2026-11-12 门禁修复）。
        return Vec::new();
    }
    let out = Command::new("podman")
        .args([
            "ps",
            "-a",
            "--filter",
            "label=rip.runtime=rip-tool",
            "--format",
            "{{.Names}}",
        ])
        .output()
        .expect("podman ps spawn");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| l.starts_with("rip-tool-rn-p32-"))
        .collect()
}

fn force_cleanup_rip_containers() {
    if !podman_available() {
        // V-9：podman 不可用时空操作，SKIP 判定留给用例入口。
        return;
    }
    let out = Command::new("podman")
        .args([
            "ps",
            "-aq",
            "--filter",
            "label=rip.runtime=rip-tool",
            "--format",
            "{{.Names}}",
        ])
        .output()
        .expect("podman ps spawn");
    for name in String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| l.starts_with("rip-tool-rn-p32-"))
    {
        let _ = Command::new("podman").args(["rm", "-f", &name]).status();
    }
}

fn wait_container_visible(expected: &str, timeout: Duration) -> Option<String> {
    let deadline = Instant::now() + timeout;
    loop {
        if rip_containers().iter().any(|n| n == expected) {
            return Some(expected.to_string());
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// allowlist 内目标：经代理 CONNECT 通到 example.com（T1 单请求）。
#[test]
fn egress_allowlisted_get_succeeds_through_proxy() {
    let _e2e = E2E_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    force_cleanup_rip_containers();
    if !podman_available() {
        eprintln!("SKIP: podman unavailable (V-9，容器端到端保持本地执行)");
        return;
    }
    let image = ensure_image();
    let (_pool, engine, ws_root) = fresh_env("ok", http_inspect_manifest(&image));
    let adapter = PodmanHostAdapter::new(PodmanHostConfig {
        workspace_root: ws_root.clone(),
        ..PodmanHostConfig::default()
    });
    // 外网 TLS 握手瞬断重试（P5 flaky 修复，实测根因）：上游 edge 偶发在
    // CONNECT 隧道建立 ~400ms 后发 FIN，容器内 urlopen 落
    // SSL: UNEXPECTED_EOF_WHILE_READING。宿主出站路径经透明代理面（本机
    // dae0 eBPF 代理接口实测在位），外网瞬态与套件负载相关、可连续数秒；
    // 属外网瞬态而非 allowlist/代理语义失败（代理就绪探活与事件时序已另行
    // 修复）。仅当错误消息命中该特征时换 run_id 重跑（3 次尝试 + 2s 退避，
    // 给透明代理的节点健康切换留收敛窗口）；其余失败原样断言（不掩盖真实
    // 回归）。proxy_events 台账按 run_id 隔离，重跑无串扰。
    const OK_RUN_IDS: [&str; 3] = ["rn-p32-ok", "rn-p32-ok-retry2", "rn-p32-ok-retry3"];
    let mut envelope = None;
    let mut final_run_id = OK_RUN_IDS[0];
    for (attempt, run_id) in OK_RUN_IDS.iter().enumerate() {
        submit(&engine, run_id, json!({"url": "https://example.com/"}), 120);
        let outcome = execute_container_via_engine(ContainerCall {
            engine: &engine,
            run_id,
            adapter: &adapter,
            image: &image,
            entrypoint: &["python3".to_string(), "/app/http_inspector.py".to_string()],
            family: "web_http",
            network: NetworkPlan::EgressAllowlist {
                ports: vec![80, 443],
            },
        })
        .expect("egress e2e run");
        let env = outcome.envelope;
        let transient = env.status == ToolRunStatus::Failed
            && env
                .error
                .as_ref()
                .is_some_and(|e| e.message.contains("UNEXPECTED_EOF_WHILE_READING"));
        final_run_id = run_id;
        envelope = Some(env);
        if !transient || attempt + 1 == OK_RUN_IDS.len() {
            break;
        }
        eprintln!("transient upstream TLS EOF (attempt {attempt}), retrying with backoff");
        force_cleanup_rip_containers();
        std::thread::sleep(Duration::from_secs(2));
    }
    let env = &envelope.expect("egress e2e envelope");
    assert_eq!(
        env.status,
        ToolRunStatus::Succeeded,
        "allowlisted GET must succeed through proxy: {:?}",
        env.error
    );
    let data = env.result.as_ref().expect("result data");
    assert_eq!(data["status_code"].as_i64(), Some(200));
    assert_eq!(data["final_url"].as_str(), Some("https://example.com/"));
    assert!(data["body_bytes"].as_u64().unwrap() > 0);

    // 代理事件留痕：隧道确实经过代理（env 注入生效的直接证据，非直连）。
    let events = proxy_events(final_run_id);
    assert!(
        events.iter().any(
            |e| matches!(e, ProxyEvent::TunnelAccepted { host, port: 443 } if host == "example.com")
        ),
        "proxy must record TunnelAccepted example.com:443, got: {events:?}"
    );

    // body.bin Artifact sha256 与宿主文件对账一致（P3-5 判据同款口径）。
    let body_path = ws_root.join(format!("{final_run_id}/out/body.bin"));
    assert!(body_path.exists(), "body.bin artifact must exist");
    let bytes = std::fs::read(&body_path).unwrap();
    assert_eq!(
        data["body_sha256"].as_str().unwrap(),
        tool_registry::hash::sha256_hex(&bytes),
        "container-reported body sha256 must match host file"
    );
    let artifact = env
        .artifacts
        .iter()
        .find(|a| a.uri.ends_with("body.bin"))
        .expect("body.bin artifact registered");
    assert_eq!(artifact.sha256, tool_registry::hash::sha256_hex(&bytes));

    // run 结束后容器清零 + tmp 删除。
    assert!(
        rip_containers().is_empty(),
        "no rip-tool containers may remain: {:?}",
        rip_containers()
    );
    assert!(!ws_root.join(format!("{final_run_id}/tmp")).exists());
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// 非 allowlist host：代理 403，工具落结构化错误帧，不静默换通道。
///
/// 触发面说明：allowlist host 从 input.url 派生，工具对自身 url 的请求必然
/// 在名单内；代理拒绝的真实触发面是跨 host 重定向。本用例用稳定公共重定向
/// npmjs.com（301 到 www.npmjs.com，本会话 curl -sI 实测）：派发时冻结的
/// allowlist 为 {npmjs.com}，重定向目标的 CONNECT www.npmjs.com:443 被
/// 代理拒绝（403），urllib 落结构化错误，不回退直连（若绕过代理，重定向
/// 目标将直接抓取成功，run 会 SUCCEEDED，故本断言同时是「代理在链路上」
/// 的反证）。
#[test]
fn non_allowlisted_host_connect_rejected_structured() {
    let _e2e = E2E_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    force_cleanup_rip_containers();
    if !podman_available() {
        eprintln!("SKIP: podman unavailable (V-9，容器端到端保持本地执行)");
        return;
    }
    let image = ensure_image();
    let (_pool, engine, ws_root) = fresh_env("deny", http_inspect_manifest(&image));
    let adapter = PodmanHostAdapter::new(PodmanHostConfig {
        workspace_root: ws_root.clone(),
        ..PodmanHostConfig::default()
    });
    // 首个 CONNECT（本尊 url 派生 host）同样走外网 TLS 握手，瞬断重试口径
    // 与 ok 用例一致（仅 UNEXPECTED_EOF 特征命中才重跑，403 语义失败不重试）。
    const DENY_RUN_IDS: [&str; 3] = ["rn-p32-deny", "rn-p32-deny-retry2", "rn-p32-deny-retry3"];
    let mut envelope = None;
    let mut final_run_id = DENY_RUN_IDS[0];
    for (attempt, run_id) in DENY_RUN_IDS.iter().enumerate() {
        submit(&engine, run_id, json!({"url": "https://npmjs.com/"}), 120);
        let outcome = execute_container_via_engine(ContainerCall {
            engine: &engine,
            run_id,
            adapter: &adapter,
            image: &image,
            entrypoint: &["python3".to_string(), "/app/http_inspector.py".to_string()],
            family: "web_http",
            network: NetworkPlan::EgressAllowlist {
                ports: vec![80, 443],
            },
        })
        .expect("deny e2e run handled, not panic");
        let env = outcome.envelope;
        let transient = env.status == ToolRunStatus::Failed
            && env
                .error
                .as_ref()
                .is_some_and(|e| e.message.contains("UNEXPECTED_EOF_WHILE_READING"));
        final_run_id = run_id;
        envelope = Some(env);
        if !transient || attempt + 1 == DENY_RUN_IDS.len() {
            break;
        }
        eprintln!("transient upstream TLS EOF (attempt {attempt}), retrying with backoff");
        force_cleanup_rip_containers();
        std::thread::sleep(Duration::from_secs(2));
    }
    let env = &envelope.expect("deny e2e envelope");
    assert_eq!(
        env.status,
        ToolRunStatus::Failed,
        "redirect to non-allowlisted host must fail at the proxy, not succeed via direct egress"
    );
    let error = env.error.as_ref().expect("structured error");
    assert_eq!(error.category, "EXECUTION");
    assert!(
        error
            .message
            .to_lowercase()
            .contains("tunnel connection failed")
            && error.message.contains("403"),
        "error must expose the proxy 403 (no silent channel switch): {}",
        error.message
    );

    // 代理侧拒绝落账：重定向目标 host_not_allowlisted；首次 CONNECT（本尊
    // url 派生 host）正常放行进隧道。
    let events = proxy_events(final_run_id);
    assert!(
        events.iter().any(|e| matches!(
            e,
            ProxyEvent::Rejected { host, reason, .. }
                if host == "www.npmjs.com" && reason.contains("host_not_allowlisted")
        )),
        "proxy must record the structured rejection, got: {events:?}"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            ProxyEvent::TunnelAccepted { host, port: 443 } if host == "npmjs.com"
        )),
        "first CONNECT to the derived allowlist host must be accepted, got: {events:?}"
    );
    assert!(
        rip_containers().is_empty(),
        "container must be reclaimed after failed run"
    );
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// 注入面实测冻结：活容器 inspect 断言代理 env 与 pasta 网络；容器内 env
/// 回显与 inspect 一致。
#[test]
fn egress_env_injection_frozen_via_live_inspect() {
    let _e2e = E2E_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    force_cleanup_rip_containers();
    if !podman_available() {
        eprintln!("SKIP: podman unavailable (V-9，容器端到端保持本地执行)");
        return;
    }
    let image = ensure_image();
    let (_pool, engine, ws_root) = fresh_env("env", env_probe_manifest(&image));
    let adapter = PodmanHostAdapter::new(PodmanHostConfig {
        workspace_root: ws_root.clone(),
        ..PodmanHostConfig::default()
    });
    submit_probe(
        &engine,
        "rn-p32-env",
        json!({"url": "https://example.com/"}),
        120,
    );

    std::thread::scope(|s| {
        let runner = s.spawn(|| {
            execute_container_via_engine(ContainerCall {
                engine: &engine,
                run_id: "rn-p32-env",
                adapter: &adapter,
                image: &image,
                entrypoint: &[
                    "python3".to_string(),
                    "-c".to_string(),
                    "import os, sys, time; time.sleep(10); sys.stdout.write(__import__('json').dumps({'status': 'result', 'data': {'http_proxy': os.environ.get('http_proxy', ''), 'https_proxy': os.environ.get('https_proxy', ''), 'uid': os.getuid()}}) + '\\n')".to_string(),
                ],
                family: "web_http",
                network: NetworkPlan::EgressAllowlist { ports: vec![443] },
            })
        });

        let name = wait_container_visible("rip-tool-rn-p32-env", Duration::from_secs(30))
            .expect("container must become visible");
        // inspect 重试 + 诊断：容器若瞬退需看到退出原因（pasta 起网失败等）。
        let mut inspect = None;
        for attempt in 0..5 {
            match adapter.inspect(&name) {
                Ok(value) => {
                    inspect = Some(value);
                    break;
                }
                Err(e) => {
                    eprintln!("inspect attempt {attempt} failed: {e}");
                    let ps = Command::new("podman")
                        .args(["ps", "-a", "--format", "{{.Names}} {{.Status}}"])
                        .output()
                        .expect("podman ps spawn");
                    eprintln!("ps -a: {}", String::from_utf8_lossy(&ps.stdout).trim());
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
        }
        let inspect = match inspect {
            Some(value) => value,
            None => {
                // 诊断路径：容器瞬退时取 runner 的完整 outcome（含 podman
                // run 的 stderr 摘要），如实暴露根因后失败。
                let joined = runner.join().expect("runner thread");
                match joined {
                    Ok(outcome) => panic!(
                        "container vanished but run completed: status={:?} error={:?}",
                        outcome.envelope.status, outcome.envelope.error
                    ),
                    Err(e) => panic!("container vanished and run failed: {e}"),
                }
            }
        };
        let env_list = inspect["Config"]["Env"]
            .as_array()
            .expect("Config.Env")
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>();
        let proxies: Vec<&str> = env_list
            .iter()
            .copied()
            .filter(|e| {
                e.starts_with("http_proxy=http://127.0.0.1:")
                    || e.starts_with("https_proxy=http://127.0.0.1:")
                    || e.starts_with("HTTP_PROXY=http://127.0.0.1:")
                    || e.starts_with("HTTPS_PROXY=http://127.0.0.1:")
            })
            .collect();
        assert!(
            proxies.len() >= 2,
            "container must carry injected proxy env (lowercase + uppercase), got: {proxies:?}"
        );
        let mode = inspect["HostConfig"]["NetworkMode"]
            .as_str()
            .expect("NetworkMode");
        assert!(
            mode.starts_with("pasta"),
            "egress_allowlist must run on pasta backend (frozen this batch): {mode}"
        );
        assert!(
            !mode.contains("none"),
            "egress_allowlist must not be dispatched to network none: {mode}"
        );

        let outcome = match runner.join().expect("runner thread") {
            Ok(outcome) => outcome,
            Err(e) => panic!("probe run failed: {e}"),
        };
        assert_eq!(outcome.envelope.status, ToolRunStatus::Succeeded);
        let data = outcome.envelope.result.as_ref().expect("probe data");
        let in_container = data["https_proxy"].as_str().expect("https_proxy");
        assert!(
            in_container.starts_with("http://127.0.0.1:"),
            "container-side env view must point at the loopback proxy: {in_container}"
        );
        assert_eq!(
            in_container,
            proxies[0].split_once('=').unwrap().1,
            "inspect env and container-visible env must agree"
        );
    });

    assert!(rip_containers().is_empty());
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// none 分派守卫：NetworkPlan::None 维持 --network none，url 工具无网失败，
/// 且该 run 不产生任何代理事件。
#[test]
fn network_none_dispatch_stays_isolated() {
    let _e2e = E2E_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    force_cleanup_rip_containers();
    if !podman_available() {
        eprintln!("SKIP: podman unavailable (V-9，容器端到端保持本地执行)");
        return;
    }
    let image = ensure_image();
    let (_pool, engine, ws_root) = fresh_env("none", http_inspect_manifest(&image));
    let adapter = PodmanHostAdapter::new(PodmanHostConfig {
        workspace_root: ws_root.clone(),
        ..PodmanHostConfig::default()
    });
    submit(
        &engine,
        "rn-p32-none",
        json!({"url": "https://example.com/"}),
        60,
    );

    let outcome = execute_container_via_engine(ContainerCall {
        engine: &engine,
        run_id: "rn-p32-none",
        adapter: &adapter,
        image: &image,
        entrypoint: &["python3".to_string(), "/app/http_inspector.py".to_string()],
        family: "web_http",
        network: NetworkPlan::None,
    })
    .expect("none-mode run handled");
    assert_eq!(
        outcome.envelope.status,
        ToolRunStatus::Failed,
        "url tool under network none must fail (no route to target)"
    );
    let error = outcome.envelope.error.as_ref().expect("structured error");
    assert_eq!(error.category, "EXECUTION");
    assert!(
        error.message.contains("request failed"),
        "tool must see connection-level failure: {}",
        error.message
    );
    assert!(
        proxy_events("rn-p32-none").is_empty(),
        "none-mode must not spawn any egress proxy"
    );
    assert!(rip_containers().is_empty());
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// host 派生单元面（无需 podman）：POLICY 路径的输入守卫。
#[test]
fn url_host_derivation_guards() {
    // 直接复用 adapter 模块内的派生函数（POLICY 失败路径的输入面）。
    use tool_engine::adapter::host_from_url;
    assert_eq!(
        host_from_url("https://example.com/x").unwrap(),
        "example.com"
    );
    assert_eq!(
        host_from_url("http://user:pw@Example.COM:8080").unwrap(),
        "example.com"
    );
    assert!(host_from_url("ftp://example.com/").is_err());
    assert!(host_from_url("not-a-url").is_err());
    assert!(host_from_url("https://*").is_err());
}
