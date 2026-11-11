//! P6-4 五类来源契约测试（ROADMAP P6-4 完成判据）：
//! 每类来源各一个最小插件走完 discover → verify → fetch → validate →
//! install，plugin.lock 落盘并含 §6 七要素 + 协商快照；git commit 存在性
//! 校验通过、浮动分支在 install 侧被拒（草案 D8）；archive sha256 不匹配
//! 进 INVALID 且产出机器可读报告；mcp_server 能力协商记录进 lock。
//!
//! 测试纪律：全部本地闭环（git 用 file:// 临时仓、mcp 用 python3 -I fixture
//! 脚本、http 用 std TcpListener 回环），不触网络与生产库。

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use plugin_registry::{
    hash::sha256_hex, parse_document, read_lock, source_default_trust, source_fetch, source_verify,
    PluginState, StaticChain, TrustLevel,
};
use serde_json::{json, Value};

fn unique(tag: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("p6src-{tag}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn fixture(name: &str) -> String {
    fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}

fn copy_tree(src: &Path, dst: &Path) {
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            fs::create_dir_all(&target).unwrap();
            copy_tree(&path, &target);
        } else {
            fs::copy(&path, &target).unwrap();
        }
    }
}

fn git(args: &[&str]) -> String {
    let out = Command::new("git").args(args).output().unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// 编辑 fixture Manifest 的 source 块占位符并写盘（安装链读取与 lock 对账）。
fn write_manifest(dir: &Path, doc: &Value) -> PathBuf {
    let path = dir.join("manifest.yaml");
    fs::write(&path, serde_json::to_string_pretty(doc).unwrap()).unwrap();
    path
}

/// 完整静态链：discover → verify → fetch → validate → install → lock 断言。
fn run_chain(
    chain: &mut StaticChain,
    doc: &str,
    manifest: &Value,
    staged: &Path,
    trust: TrustLevel,
) -> plugin_registry::PluginLock {
    let id = chain
        .discover(doc)
        .unwrap_or_else(|e| panic!("discover: {e}"));
    source_verify(manifest).unwrap_or_else(|e| panic!("source verify: {e}"));
    let outcome = source_fetch(manifest, staged).unwrap_or_else(|e| panic!("source fetch: {e}"));
    let _ = outcome;
    let report = chain.validate(&id, Some(staged)).unwrap();
    assert!(
        report.ok,
        "validate: {}",
        plugin_registry::render_report_text(&report)
    );
    chain.install(&id, staged, trust).unwrap()
}

fn assert_lock_seven_elements(lock: &plugin_registry::PluginLock, id: &str, trust: TrustLevel) {
    assert_eq!(lock.plugin_id, id);
    assert_eq!(lock.resolved_version, "0.1.0");
    assert_eq!(lock.sha256.len(), 64);
    assert_eq!(lock.license, "MIT");
    assert!(lock.installed_at_unix > 0);
    assert_eq!(lock.trust_level, trust);
    assert_eq!(
        lock.trust_level,
        source_default_trust(lock.source_type.as_str())
    );
}

// ---------------------------------------------------------------------------
// local_dir：非工作树复制 + git 工作树 pinned HEAD 复核
// ---------------------------------------------------------------------------

#[test]
fn local_dir_source_full_chain() {
    let tmp = unique("localdir");
    // 来源目录（非 git 工作树）。
    let source_dir = tmp.join("source");
    fs::create_dir_all(source_dir.join("capabilities")).unwrap();
    fs::write(
        source_dir.join("capabilities/knowledge_source.yaml"),
        "corpus_id: fixture-local-dir\n",
    )
    .unwrap();

    let mut manifest = parse_document(&fixture("local_dir/manifest.yaml")).unwrap();
    manifest.as_object_mut().unwrap()["source"]["path"] =
        json!(source_dir.to_string_lossy().to_string());

    let staged = tmp.join("installed/knowledge.fixture.localdir/0.1.0");
    let manifest_path = write_manifest(&tmp, &manifest);
    fs::create_dir_all(&staged).unwrap();
    fs::copy(&manifest_path, staged.join("manifest.yaml")).unwrap();

    let mut chain = StaticChain::new();
    let doc = fs::read_to_string(&manifest_path).unwrap();
    let lock = run_chain(&mut chain, &doc, &manifest, &staged, TrustLevel::Local);
    assert_lock_seven_elements(&lock, "knowledge.fixture.localdir", TrustLevel::Local);
    assert!(staged.join("capabilities/knowledge_source.yaml").is_file());
    assert!(staged.join("checksums.sha256").is_file());
    assert!(staged.join("plugin.lock").is_file());
    let reread = read_lock(&staged).unwrap();
    assert_eq!(reread, lock);
}

#[test]
fn local_dir_git_worktree_pinned_head_verified() {
    let tmp = unique("localdir-wt");
    // 来源目录做成 git 工作树。
    let source_dir = tmp.join("source");
    fs::create_dir_all(source_dir.join("capabilities")).unwrap();
    fs::write(
        source_dir.join("capabilities/knowledge_source.yaml"),
        "corpus_id: wt\n",
    )
    .unwrap();
    git(&["init", "--quiet", &source_dir.to_string_lossy()]);
    git(&[
        "-C",
        &source_dir.to_string_lossy(),
        "config",
        "user.email",
        "t@t",
    ]);
    git(&[
        "-C",
        &source_dir.to_string_lossy(),
        "config",
        "user.name",
        "t",
    ]);
    git(&["-C", &source_dir.to_string_lossy(), "add", "."]);
    git(&[
        "-C",
        &source_dir.to_string_lossy(),
        "commit",
        "--quiet",
        "-m",
        "init",
    ]);
    let head = git(&["-C", &source_dir.to_string_lossy(), "rev-parse", "HEAD"]);

    let mut manifest = parse_document(&fixture("local_dir/manifest.yaml")).unwrap();
    manifest.as_object_mut().unwrap()["source"]["path"] =
        json!(source_dir.to_string_lossy().to_string());
    manifest.as_object_mut().unwrap()["source"]["pinned"]["value"] = json!(head);
    let doc = serde_json::to_string_pretty(&manifest).unwrap();

    let staged = tmp.join("installed");
    fs::create_dir_all(&staged).unwrap();
    let manifest_path = write_manifest(&tmp, &manifest);
    fs::copy(&manifest_path, staged.join("manifest.yaml")).unwrap();

    let mut chain = StaticChain::new();
    let id = chain.discover(&doc).unwrap();
    source_verify(&manifest).unwrap();
    let outcome = plugin_registry::source_fetch(&manifest, &staged).unwrap();
    // local_dir 协商快照回填 resolved HEAD（§6 pinned 取 HEAD sha）。
    assert_eq!(
        outcome.negotiation.as_ref().unwrap()["resolved_head"],
        json!(head)
    );
    let report = chain.validate(&id, Some(&staged)).unwrap();
    assert!(report.ok);
    let lock = chain.install(&id, &staged, TrustLevel::Local).unwrap();
    // lock.negotiation 携带 resolved HEAD（P6-4 协商进 lock）。
    assert_eq!(
        lock.negotiation.as_ref().unwrap()["resolved_head"],
        json!(head)
    );

    // pinned 与 HEAD 不一致 → verify 拒绝。
    let mut bad = manifest.clone();
    bad.as_object_mut().unwrap()["source"]["pinned"]["value"] =
        json!("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef");
    let err = plugin_registry::source_verify(&bad).unwrap_err();
    assert_eq!(err.code, "pin_mismatch");
}

// ---------------------------------------------------------------------------
// git：file:// 临时仓 commit 存在性 + 浮动分支 install 侧拒绝（D8）
// ---------------------------------------------------------------------------

#[test]
fn git_source_full_chain_and_floating_branch() {
    let tmp = unique("git");
    // 构建本地 git 仓（file:// URL 闭环）。
    let repo = tmp.join("repo");
    copy_tree(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/git/content"),
        &repo,
    );
    git(&["init", "--quiet", &repo.to_string_lossy()]);
    git(&["-C", &repo.to_string_lossy(), "config", "user.email", "t@t"]);
    git(&["-C", &repo.to_string_lossy(), "config", "user.name", "t"]);
    git(&["-C", &repo.to_string_lossy(), "add", "."]);
    git(&[
        "-C",
        &repo.to_string_lossy(),
        "commit",
        "--quiet",
        "-m",
        "init",
    ]);
    let head = git(&["-C", &repo.to_string_lossy(), "rev-parse", "HEAD"]);

    let mut manifest = parse_document(&fixture("git/manifest.yaml")).unwrap();
    manifest.as_object_mut().unwrap()["source"]["url"] =
        json!(format!("file://{}", repo.display()));
    manifest.as_object_mut().unwrap()["source"]["pinned"]["value"] = json!(head);
    let doc = serde_json::to_string_pretty(&manifest).unwrap();
    let manifest_path = write_manifest(&tmp, &manifest);

    // commit 存在性：verify 通过；不存在的 commit 拒绝。
    source_verify(&manifest).unwrap();
    let mut missing = manifest.clone();
    missing.as_object_mut().unwrap()["source"]["pinned"]["value"] =
        json!("0000000000000000000000000000000000000000");
    let err = source_verify(&missing).unwrap_err();
    assert_eq!(err.code, "commit_not_found");

    let staged = tmp.join("installed/knowledge.fixture.git/0.1.0");
    fs::create_dir_all(&staged).unwrap();
    fs::copy(&manifest_path, staged.join("manifest.yaml")).unwrap();

    let mut chain = StaticChain::new();
    let lock = run_chain(&mut chain, &doc, &manifest, &staged, TrustLevel::Community);
    assert_lock_seven_elements(&lock, "knowledge.fixture.git", TrustLevel::Community);
    assert_eq!(
        lock.negotiation.as_ref().unwrap()["resolved_commit"],
        json!(head)
    );
    assert!(staged.join("capabilities/knowledge_source.yaml").is_file());
    assert!(staged.join("docs/usage.md").is_file());

    // 浮动分支（草案 D8）：validate rule 5 与 install 侧 verify 双重拒绝。
    let mut floating = manifest.clone();
    floating.as_object_mut().unwrap()["source"]["pinned"] =
        json!({"kind": "branch", "value": "main"});
    let doc_floating = serde_json::to_string_pretty(&floating).unwrap();
    let mut chain2 = StaticChain::new();
    chain2.discover(&doc_floating).unwrap();
    let report = chain2.validate("knowledge.fixture.git", None).unwrap();
    assert!(!report.ok);
    assert_eq!(report.errors[0].rule, 5);
    assert_eq!(report.errors[0].code, "floating_branch_rejected");
    let err = source_verify(&floating).unwrap_err();
    assert_eq!(err.code, "floating_branch_rejected");
    assert!(err.to_json_string().contains("D8"));
}

// ---------------------------------------------------------------------------
// archive：sha256 对账 + 不匹配进 INVALID（机器可读报告）
// ---------------------------------------------------------------------------

#[test]
fn archive_source_full_chain_and_hash_mismatch_invalid() {
    let tmp = unique("archive");
    // 打包 fixture 内容为无压缩 tar。
    let content = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/archive/content");
    let archive_path = tmp.join("pkg.tar");
    {
        let file = fs::File::create(&archive_path).unwrap();
        let mut builder = tar::Builder::new(file);
        builder.append_dir_all(".", &content).unwrap();
        builder.finish().unwrap();
    }
    let bytes = fs::read(&archive_path).unwrap();
    let sha = sha256_hex(&bytes);

    let mut manifest = parse_document(&fixture("archive/manifest.yaml")).unwrap();
    manifest.as_object_mut().unwrap()["source"]["url"] =
        json!(archive_path.to_string_lossy().to_string());
    manifest.as_object_mut().unwrap()["source"]["sha256"] = json!(sha);
    manifest.as_object_mut().unwrap()["source"]["size"] = json!(bytes.len() as i64);
    let doc = serde_json::to_string_pretty(&manifest).unwrap();
    let manifest_path = write_manifest(&tmp, &manifest);

    let staged = tmp.join("installed/knowledge.fixture.archive/0.1.0");
    fs::create_dir_all(&staged).unwrap();
    fs::copy(&manifest_path, staged.join("manifest.yaml")).unwrap();

    let mut chain = StaticChain::new();
    let lock = run_chain(&mut chain, &doc, &manifest, &staged, TrustLevel::Community);
    assert_lock_seven_elements(&lock, "knowledge.fixture.archive", TrustLevel::Community);
    assert!(staged.join("capabilities/knowledge_source.yaml").is_file());

    // sha256 不匹配：fetch 拒绝（机器可读）→ fetch_reject 进 INVALID。
    let mut bad = manifest.clone();
    bad.as_object_mut().unwrap()["source"]["sha256"] =
        json!("1111111111111111111111111111111111111111111111111111111111111111");
    let err = source_verify(&bad).unwrap_err();
    assert_eq!(err.code, "archive_hash_mismatch");
    let report_json = err.to_json_string();
    assert!(report_json.contains("archive_hash_mismatch"));

    let mut chain2 = StaticChain::new();
    let bad_doc = serde_json::to_string_pretty(&bad).unwrap();
    chain2.discover(&bad_doc).unwrap();
    chain2.fetch_reject("knowledge.fixture.archive").unwrap();
    assert_eq!(
        chain2.state_of("knowledge.fixture.archive"),
        Some(PluginState::Invalid),
        "fetch failure lands INVALID (§5.2 install failure semantics)"
    );
    // 跃迁流水机器可读：install 动作落到 INVALID。
    let jsonl = chain2.records_jsonl();
    assert!(jsonl.contains("\"action\":\"install\""));
    assert!(jsonl.contains("\"to\":\"INVALID\""));
    // INVALID 处置面：plugin-registry 侧报告即可机读展示（Plugin Center 列）。
    let report = chain2
        .validate("knowledge.fixture.archive", None)
        .unwrap_err()
        .to_json_string();
    assert!(report.contains("illegal_transition"));
}

// ---------------------------------------------------------------------------
// mcp_server：stdio 握手 + 工具列表快照哈希进 lock
// ---------------------------------------------------------------------------

#[test]
fn mcp_server_source_handshake_snapshot_into_lock() {
    let tmp = unique("mcp");
    let fixture_py = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mcp_server/mcp_server_fixture.py");
    let endpoint = format!("[\"python3\", \"-I\", \"{}\"]", fixture_py.display());

    let mut manifest = parse_document(&fixture("mcp_server/manifest.yaml")).unwrap();
    manifest.as_object_mut().unwrap()["source"]["endpoint"] = json!(endpoint);
    let doc = serde_json::to_string_pretty(&manifest).unwrap();
    let manifest_path = write_manifest(&tmp, &manifest);

    let staged = tmp.join("installed/knowledge.fixture.mcp/0.1.0");
    fs::create_dir_all(&staged).unwrap();
    fs::copy(&manifest_path, staged.join("manifest.yaml")).unwrap();

    let mut chain = StaticChain::new();
    let lock = run_chain(&mut chain, &doc, &manifest, &staged, TrustLevel::Remote);
    assert_lock_seven_elements(&lock, "knowledge.fixture.mcp", TrustLevel::Remote);
    // 能力协商记录进 lock（§6）：协议版本 + serverInfo + tools 快照哈希。
    let negotiation = lock.negotiation.as_ref().expect("negotiation recorded");
    assert_eq!(negotiation["protocol_version"], "2024-11-05");
    assert_eq!(negotiation["server_info"]["name"], "fixture-mcp");
    let tools_sha = negotiation["tools_sha256"].as_str().unwrap();
    assert_eq!(tools_sha.len(), 64);
    // 快照哈希与工具明细一致性：tools 明细重算哈希对账。
    let expected = sha256_hex(negotiation["tools"].to_string().as_bytes());
    assert_eq!(tools_sha, expected);
    assert!(staged.join("negotiation.json").is_file());
    assert!(staged.join("capabilities/mcp-tools.yaml").is_file());

    // 协议版本不一致 → 拒绝。
    let mut mismatch = manifest.clone();
    mismatch.as_object_mut().unwrap()["source"]["protocol_version"] = json!("9999-01-01");
    let err = plugin_registry::source_fetch(&mismatch, &tmp.join("bad")).unwrap_err();
    assert_eq!(err.code, "protocol_version_mismatch");
}

// ---------------------------------------------------------------------------
// http_api：std TcpListener 回环契约 + 契约哈希进 lock + 版本不一致拒绝
// ---------------------------------------------------------------------------

#[test]
fn http_api_source_contract_hash_into_lock() {
    let tmp = unique("http");
    let contract = json!({
        "api_version": "v1",
        "description": "fixture contract",
        "endpoints": [{"path": "/search", "method": "GET"}]
    });
    let body = serde_json::to_string_pretty(&contract).unwrap();
    let body_len = body.len();
    let body_sha = sha256_hex(body.as_bytes());

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        // 应答两轮（正常 fetch + 复核请求），随后退出。
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = Vec::new();
            let mut byte = [0u8; 1];
            loop {
                match stream.read(&mut byte) {
                    Ok(1) if byte[0] != 0 => buf.push(byte[0]),
                    _ => break,
                }
                if buf.windows(4).rev().any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let _ = stream.write_all(
                format!("HTTP/1.0 200 OK\r\nContent-Length: {body_len}\r\n\r\n{body}").as_bytes(),
            );
            let _ = stream.flush();
        }
    });

    let mut manifest = parse_document(&fixture("http_api/manifest.yaml")).unwrap();
    manifest.as_object_mut().unwrap()["source"]["base_url"] =
        json!(format!("http://127.0.0.1:{port}/"));
    let doc = serde_json::to_string_pretty(&manifest).unwrap();
    let manifest_path = write_manifest(&tmp, &manifest);

    let staged = tmp.join("installed/knowledge.fixture.http/0.1.0");
    fs::create_dir_all(&staged).unwrap();
    fs::copy(&manifest_path, staged.join("manifest.yaml")).unwrap();

    let mut chain = StaticChain::new();
    let lock = run_chain(&mut chain, &doc, &manifest, &staged, TrustLevel::Remote);
    assert_lock_seven_elements(&lock, "knowledge.fixture.http", TrustLevel::Remote);
    let negotiation = lock.negotiation.as_ref().expect("negotiation recorded");
    assert_eq!(negotiation["api_version"], "v1");
    let contract_sha = negotiation["contract_sha256"].as_str().unwrap();
    assert_eq!(contract_sha.len(), 64);
    // 契约哈希对账：原始响应体重算一致。
    assert_eq!(contract_sha, body_sha);
    assert!(staged.join("contract.json").is_file());
    assert!(staged.join("capabilities/http-contract.yaml").is_file());

    // api_version 不一致 → 拒绝（机器可读；第二次请求让服务线程自然退出）。
    let mut mismatch = manifest.clone();
    mismatch.as_object_mut().unwrap()["source"]["api_version"] = json!("v9");
    let _ = write_manifest(&tmp, &mismatch);
    let err = plugin_registry::source_fetch(&mismatch, &tmp.join("bad")).unwrap_err();
    assert_eq!(err.code, "api_version_mismatch");
    server.join().unwrap();
}
