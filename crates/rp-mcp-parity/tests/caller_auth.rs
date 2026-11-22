//! MA-1（docs/rip3/MA-1-TASK.md §2.1/§3 S5/§4 判据 2-5/8）Caller 身份与多密钥
//! 鉴权 e2e 载具。范式 = tests/parity.rs:170-181（真实 rp_server.py 进程 +
//! env 注入；HTTP 客户端 std TcpStream 薄实现，vendor 无 HTTP client crate）。
//!
//! 流程：临时库全量迁移 001..026 → rp-admin 预置双 caller 双 key（026 唯一
//! 写入口，D6）→ 真实 rp_server.py 起服（审计 JSONL + 限频关闭）→ 六项断言：
//! ①legacy 单令牌兼容并存（D3，审计 caller=default）；②双 caller 双 key 互
//! 不串扰（判据 3）；③未知 key 401 形状不变（判据 4）；④key-revoke 后下一
//! 请求 401 + 重复 revoke 幂等（判据 5/6）；⑤caller 停用 join 判定后其 key
//! 401（判据 4，停用写面 = 测试夹具直写临时库，红线 3 只约束 server）；⑥
//! 审计 JSONL caller 字段三态（D5）+ 多 key 校验 p95 < 5ms 计时粗断言（判据 8）。
//!
//! rp-admin 二进制解析顺序：RP_ADMIN_BIN env → `$CARGO_TARGET_DIR/debug/
//! rp-admin` → repo `target/debug/rp-admin` → 全缺位时按 parity.rs ensure_bin
//! :81-92 同范式进程内 `cargo build --offline -q` 补建（cargo test 进入测试
//! 执行段已释放构建锁；rp-workflow/rp-mcp 载具同路径 CI 已过门，ci.yml parity
//! job 同口径）。生产 platform*.db 零接触零写。

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

/// legacy 单令牌（D3：RP_AUTH_TOKEN 语义零变化，映射隐式 caller default）。
const TOKEN: &str = "ma1-caller-auth-legacy-token";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn tmp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ma1-caller-auth-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("fixture dir");
    dir
}

fn free_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("bind :0")
        .local_addr()
        .expect("local addr")
        .port()
}

/// rp-admin 二进制解析（见模块头注；候选全缺位时按 ensure_bin 范式补建）。
fn rp_admin_bin() -> PathBuf {
    if let Ok(p) = std::env::var("RP_ADMIN_BIN") {
        let path = PathBuf::from(p);
        assert!(path.exists(), "RP_ADMIN_BIN 指向的二进制不存在：{}", path.display());
        return path;
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(td) = std::env::var("CARGO_TARGET_DIR") {
        candidates.push(PathBuf::from(td).join("debug/rp-admin"));
    }
    candidates.push(repo_root().join("target/debug/rp-admin"));
    for candidate in &candidates {
        if candidate.exists() {
            return candidate.clone();
        }
    }
    // parity.rs ensure_bin 同范式补建（--offline，vendored 面一致；测试执行段
    // 无构建锁冲突——rp-workflow 载具同路径已在 CI parity job 过门）。
    let status = Command::new("cargo")
        .args(["build", "--offline", "-q", "-p", "database", "--bin", "rp-admin"])
        .status()
        .expect("cargo build rp-admin spawn");
    assert!(status.success(), "cargo build rp-admin failed");
    for candidate in &candidates {
        if candidate.exists() {
            return candidate.clone();
        }
    }
    panic!(
        "rp-admin 补建后仍缺位（candidates: {:?}）：可用 RP_ADMIN_BIN 指向现成二进制",
        candidates
    );
}

fn run_admin(admin: &Path, args: &[&str]) -> String {
    let out = Command::new(admin)
        .args(args)
        .output()
        .expect("rp-admin spawn");
    assert!(
        out.status.success(),
        "rp-admin {:?} failed (exit {:?}): {}",
        args,
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

// ---------------------------------------------------------------------------
// REST 薄客户端（parity.rs 同款：rp_server 响应恒带 Content-Length +
// Connection: close，读至 EOF）
// ---------------------------------------------------------------------------

struct HttpResp {
    status: u16,
    headers: String,
    body: String,
}

fn http_get(port: u16, target: &str, token: Option<&str>) -> Result<HttpResp, String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .map_err(|e| format!("tcp connect: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("read timeout");
    let mut request = format!(
        "GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nContent-Length: 0\r\n"
    );
    if let Some(t) = token {
        request.push_str(&format!("Authorization: Bearer {t}\r\n"));
    }
    request.push_str("\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("write request: {e}"))?;
    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .map_err(|e| format!("read response: {e}"))?;
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
        .ok_or_else(|| format!("bad status line: {status_line}"))?;
    Ok(HttpResp {
        status,
        headers: head.to_string(),
        body: payload.to_string(),
    })
}

fn must_get(port: u16, token: Option<&str>) -> HttpResp {
    match http_get(port, "/api/stats", token) {
        Ok(resp) => resp,
        Err(err) => panic!("GET /api/stats failed: {err}"),
    }
}

fn wait_ready(port: u16, log_path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(resp) = http_get(port, "/api/stats", Some(TOKEN)) {
            if resp.status == 200 {
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "rp_server 未就绪（30s）：log 见 {}",
            log_path.display()
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// 判据 8 计时样：单请求毫秒（含完整 HTTP 往返；本地无网络噪声）。
fn timed_ms(port: u16, token: &str) -> f64 {
    let t0 = Instant::now();
    let resp = must_get(port, Some(token));
    assert_eq!(resp.status, 200, "timed request must succeed");
    t0.elapsed().as_secs_f64() * 1000.0
}

#[test]
fn caller_multi_key_auth_e2e() {
    // 前置守卫（V-9 同款）：python3 在位。
    assert!(
        Command::new("python3")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false),
        "python3 在位是载具前置（rp_server 依赖）"
    );

    let dir = tmp_dir();
    let main_db = dir.join("main.db");
    let admin = rp_admin_bin();

    // 临时库全量迁移 001..027（判据 1 载体：链尾 027 = MA-2 caller 归属列与 caller_budget）。
    {
        let pool = database::Pool::open(database::PoolConfig::writable_file(&main_db))
            .expect("open temp db");
        let applied =
            database::migrate(&pool, &repo_root().join("migrations")).expect("migrate temp db");
        assert_eq!(
            applied.last().map(|m| m.version),
            Some(30),
            "真实迁移链必须应用到当前链尾（027 MA-2 caller 归属列与 caller_budget 之后已随链演进至 030）"
        );
    }

    // rp-admin 预置双 caller 双 key（026 唯一写入口，D6；caller-add/key-gen
    // 自带幂等迁移，此处库已到位）。
    run_admin(
        &admin,
        &[
            "caller-add",
            "--db",
            main_db.to_string_lossy().as_ref(),
            "--name",
            "e2e-caller-a",
            "--level",
            "T1",
            "--caller-id",
            "caller-e2e-a",
        ],
    );
    run_admin(
        &admin,
        &[
            "caller-add",
            "--db",
            main_db.to_string_lossy().as_ref(),
            "--name",
            "e2e-caller-b",
            "--level",
            "T0",
            "--caller-id",
            "caller-e2e-b",
        ],
    );
    let key_a: Value = serde_json::from_str(
        &run_admin(
            &admin,
            &[
                "key-gen",
                "--db",
                main_db.to_string_lossy().as_ref(),
                "--caller-id",
                "caller-e2e-a",
                "--label",
                "e2e-a",
            ],
        ),
    )
    .expect("key-gen a json");
    let key_b: Value = serde_json::from_str(
        &run_admin(
            &admin,
            &[
                "key-gen",
                "--db",
                main_db.to_string_lossy().as_ref(),
                "--caller-id",
                "caller-e2e-b",
                "--label",
                "e2e-b",
            ],
        ),
    )
    .expect("key-gen b json");
    let hash_a = key_a["key_hash"].as_str().expect("key a hash").to_string();
    let key_a = key_a["key"].as_str().expect("key a plaintext").to_string();
    let key_b = key_b["key"].as_str().expect("key b plaintext").to_string();
    // 判据 6 形状（与 rp-admin 单测同源双保险）：rp1_ + 43 位 base64url。
    assert_eq!(key_a.len(), 47, "key 明文 47 字符（D2）");
    assert!(key_a.starts_with("rp1_"));
    assert_eq!(key_b.len(), 47);
    assert_ne!(key_a, key_b, "双 key 必须互异");

    // 起服：真实 rp_server.py（审计 JSONL + 限频关闭；parity.rs:170 范式）。
    let audit = dir.join("audit.jsonl");
    let port = free_port();
    let server_log = dir.join("rp_server.log");
    let log = std::fs::File::create(&server_log).expect("server log");
    let mut server = Command::new("python3")
        .arg("apps/server/rp_server.py")
        .env("RP_PORT", port.to_string())
        .env("RP_AUTH_TOKEN", TOKEN)
        .env("RP_DB_PATH", &main_db)
        .env("RP_API_AUDIT_LOG", &audit)
        .env("RP_RATE_LIMIT_RPM", "0")
        .current_dir(repo_root().as_os_str())
        .stdout(log.try_clone().expect("log clone"))
        .stderr(log)
        .spawn()
        .expect("spawn rp_server.py");
    wait_ready(port, &server_log);

    // ①判据 2：legacy 单令牌兼容并存（server 已带多 key 面）。
    let resp = must_get(port, Some(TOKEN));
    assert_eq!(resp.status, 200, "legacy 单令牌必须继续通过（D3）");

    // ②判据 3：双 caller 双 key 各自 200。
    let resp = must_get(port, Some(&key_a));
    assert_eq!(resp.status, 200, "caller-e2e-a key 必须通过");
    let resp = must_get(port, Some(&key_b));
    assert_eq!(resp.status, 200, "caller-e2e-b key 必须通过");

    // ③判据 8：多 key 校验计时（吊销前测；40 样本 warmup 5，p95 < 5ms）。
    for _ in 0..5 {
        must_get(port, Some(&key_a));
    }
    let mut samples: Vec<f64> = (0..40).map(|_| timed_ms(port, &key_a)).collect();
    samples.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    let p50 = samples[samples.len() / 2];
    let p95 = samples[(samples.len() as f64 * 0.95).floor() as usize % samples.len()];
    eprintln!("MA-1 e2e 多 key 校验计时：p50={p50:.2}ms p95={p95:.2}ms（40 样本，判据 8）");
    assert!(
        p95 < 5.0,
        "多 key 校验 p95={p95:.2}ms ≥ 5ms（判据 8：sha256 + 主键单行查询增量成本）"
    );
    let legacy_samples: Vec<f64> = (0..20)
        .map(|_| {
            let t0 = Instant::now();
            let resp = must_get(port, Some(TOKEN));
            assert_eq!(resp.status, 200);
            t0.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    let mut legacy_sorted = legacy_samples.clone();
    legacy_sorted.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    eprintln!(
        "MA-1 e2e legacy 对照：p50={:.2}ms（增量参考差 {:+.2}ms）",
        legacy_sorted[legacy_sorted.len() / 2],
        p50 - legacy_sorted[legacy_sorted.len() / 2]
    );

    // ④判据 4：未知 key 401 形状不变（401 + WWW-Authenticate + JSON error）。
    let resp = must_get(port, Some("rp1_zzz-not-a-real-key"));
    assert_eq!(resp.status, 401, "未知 key 必须 401");
    assert!(
        resp.headers.to_ascii_lowercase().contains("www-authenticate: bearer"),
        "401 必须携带 WWW-Authenticate: Bearer"
    );
    let err: Value = serde_json::from_str(&resp.body).expect("401 json body");
    assert_eq!(err["error"]["code"], "unauthorized", "401 形状同 A4");

    // ⑤判据 5：key-revoke 后下一请求 401（无缓存直查）；B 不受影响。
    run_admin(
        &admin,
        &[
            "key-revoke",
            "--db",
            main_db.to_string_lossy().as_ref(),
            "--key-hash",
            &hash_a,
        ],
    );
    let resp = must_get(port, Some(&key_a));
    assert_eq!(resp.status, 401, "revoke 后下一请求必须 401（判据 5）");
    let resp = must_get(port, Some(&key_b));
    assert_eq!(resp.status, 200, "revoke 只影响 A，B 仍 200（互不串扰）");
    // 判据 6：重复 revoke 幂等（exit 0 + already_revoked=true）。
    let rev2 = run_admin(
        &admin,
        &[
            "key-revoke",
            "--db",
            main_db.to_string_lossy().as_ref(),
            "--key-hash",
            &hash_a,
        ],
    );
    let rev2: Value = serde_json::from_str(&rev2).expect("re-revoke json");
    assert_eq!(rev2["already_revoked"], true, "重复 revoke 非致命（判据 6）");

    // ⑥判据 4/5：caller 停用（join 判定）后其 key 下一请求 401；legacy 不受
    // 影响。停用写面 = 测试夹具直写临时库（红线 3 只约束 server 校验路径，
    // parity.rs 测试面直写临时库同款惯例；运行期正式停用面留 MA-2 CLI）。
    {
        let pool = database::Pool::open(database::PoolConfig::writable_file(&main_db))
            .expect("reopen temp db for fixture write");
        pool.with_conn(|c| {
            c.execute(
                "UPDATE caller SET revoked_at = '2026-11-22T00:00:00.000Z' WHERE caller_id = 'caller-e2e-b'",
                [],
            )
        })
        .expect("disable caller-e2e-b");
    }
    let resp = must_get(port, Some(&key_b));
    assert_eq!(resp.status, 401, "caller 停用后其 key 必须 401（join 判定）");
    let resp = must_get(port, Some(TOKEN));
    assert_eq!(resp.status, 200, "caller 停用不影响 legacy default");

    // ⑦D5/判据 2/3：审计 JSONL caller 字段三态归位。
    let audit_text = std::fs::read_to_string(&audit).expect("audit jsonl");
    let rows: Vec<Value> = audit_text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Value>(l).expect("audit row json"))
        .collect();
    assert!(
        !rows.is_empty(),
        "审计 JSONL 必须有行（RP_API_AUDIT_LOG 已开启）"
    );
    let has = |caller: &str, status: u64| {
        rows.iter()
            .any(|r| r["caller"] == caller && r["status"] == status)
    };
    assert!(has("default", 200), "legacy 请求审计 caller=default（判据 2）");
    assert!(has("caller-e2e-a", 200), "A key 请求审计归 caller-e2e-a（判据 3）");
    assert!(has("caller-e2e-b", 200), "B key 请求审计归 caller-e2e-b（判据 3）");
    // 401 行（未知 key / 吊销 key / 停用 caller）= 匿名未过鉴权 → caller="-"
    //（D5 三态；身份只在过鉴权行出现）。
    assert!(
        rows.iter().any(|r| r["caller"] == "-" && r["status"] == 401),
        "401 行必须记匿名 \"-\""
    );
    assert!(
        rows.iter()
            .all(|r| r["status"] != 401 || r["caller"] == "-"),
        "401 行不允许携带任何身份（default/caller_id 只出现在过鉴权行）"
    );

    server.kill().expect("kill server");
    let _ = server.wait();
    let _ = std::fs::remove_dir_all(&dir);
}
