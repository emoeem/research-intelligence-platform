//! DI-1 判据 10：mcp-server native_rust 派发分支端到端（RT-1 批次报告 G
//! 遗留 #2 消费）。真实 run_tool 通道（与 bin 同一 RunChannel 装配路径）：
//! PDF 通道链派发（两条真实 run，trace_id = run_id 自锚定 → tool_run 表
//! 可反查 /api/traces 面）+ 嗅探门前置拒绝（无 run 行）+ 输入 64 MiB 帽
//! stat 前置拒绝（评审修订：fs::read 全量读入前判帽，D7 口径）+ 词表外
//! 声明 Resolver 闸硬拒（DI-3/RT-4 D3：mcp_client → RT3_RUNTIME_VOCAB）+
//! 通道表外 native assign 工具承载守卫（B 组正式登记件，词形零漂移）。
//!
//! 纪律：临时库自建（生产 platform*.db / rip-data 零触碰）；fixture 来自
//! 入库 crates/document-router/tests/fixtures/（评审修订：原 rip-data/
//! parse-smoke/in/ 不入库，CI fresh checkout 缺样本；钉值见
//! document-router tests/fixture_pins.rs）；能力缺失按目录 status +
//! executable_path 存在性二道检测 SKIP（红线 3，评审修订补 stat 面）。

use std::path::{Path, PathBuf};
use std::time::Duration;

use database::{migrate, DbPath, Pool, PoolConfig};
use mcp_server::dispatch::RunChannel;
use serde_json::{json, Value};
use tool_engine::capability::CapabilityProvider;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

struct Fixture {
    run_db: PathBuf,
    document_input_root: PathBuf,
    workspaces_root: PathBuf,
    manifest_dirs: Vec<PathBuf>,
    capability_seed: PathBuf,
    artifacts_dir: PathBuf,
    extra_manifest_dir: PathBuf,
}

fn fixture(tag: &str) -> Fixture {
    let dir = std::env::temp_dir().join(format!(
        "di1-dispatch-{tag}-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let doc_root = dir.join("documents");
    std::fs::create_dir_all(&doc_root).unwrap();
    let ws = dir.join("workspaces");
    std::fs::create_dir_all(&ws).unwrap();
    // fixture：入库样本（document-router tests/fixtures/，sha256 钉值见
    // fixture_pins.rs）复制进受信文档根（零下载）。
    for name in ["sample.pdf", "evil.zip", "text-ocr.png", "tone.wav"] {
        std::fs::copy(
            repo_root().join(format!("crates/document-router/tests/fixtures/{name}")),
            doc_root.join(name),
        )
        .unwrap();
    }
    let extra = dir.join("manifests");
    std::fs::create_dir_all(&extra).unwrap();
    Fixture {
        run_db: dir.join("run.db"),
        document_input_root: doc_root,
        workspaces_root: ws,
        manifest_dirs: vec![repo_root().join("manifests/tools"), extra.clone()],
        capability_seed: repo_root().join("manifests/capabilities/host-catalog-seed.json"),
        artifacts_dir: dir.join("artifacts"),
        extra_manifest_dir: extra,
    }
}

impl Fixture {
    fn channel(&self) -> RunChannel {
        RunChannel {
            run_db: self.run_db.clone(),
            migrations_dir: repo_root().join("migrations"),
            manifest_dirs: self.manifest_dirs.clone(),
            artifacts_dir: self.artifacts_dir.clone(),
            workspaces_root: self.workspaces_root.clone(),
            runtime_node_id: "host-di1-dispatch-test".to_string(),
            working_dir: Some(repo_root()),
            document_input_root: self.document_input_root.clone(),
            capability_seed_path: self.capability_seed.clone(),
            // MA-3（D4）协作面字段（本测试不触协作工具；caller None =
            // fail-closed 缺省形态）。
            collab_caller: None,
            agent_bin: std::path::PathBuf::from("target/debug/rp-agent"),
            kc_bin: std::path::PathBuf::from("target/debug/rp-kc"),
            ingest_bin: std::path::PathBuf::from("target/debug/rp-ingest"),
            subtask_timeout_s: 600,
        }
    }
}

/// 九份通道表 Manifest 登记进运行库（DI-1 四份 + DI-2 五份；run_tool 受理
/// 面 D-TC-05 Registry 查询来源；rp-tool register --db 同款库面，不触生产库）。
fn register_di1_manifests(fix: &Fixture) {
    let pool = Pool::open(PoolConfig {
        path: DbPath::File(fix.run_db.clone()),
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();
    migrate(&pool, &repo_root().join("migrations")).unwrap();
    let registry = tool_registry::Registry::open(pool);
    for id in [
        "rip.doc.pdf.info",
        "rip.doc.pdf.text",
        "rip.doc.office.to_pdf",
        "rip.doc.image.meta",
        "rip.doc.image.ocr",
        "rip.media.probe",
        "rip.media.frames",
        "rip.archive.list",
        "rip.archive.extract",
    ] {
        let text = std::fs::read_to_string(repo_root().join(format!("manifests/tools/{id}.json")))
            .unwrap();
        registry
            .register(&serde_json::from_str(&text).unwrap())
            .unwrap_or_else(|e| panic!("register {id}: {e}"));
    }
}

/// 测试私有 Manifest 落 extra 目录（manifest_for 文件面）+ 运行库登记。
fn register_probe_manifest(fix: &Fixture, doc: &Value) {
    let id = doc["id"].as_str().unwrap().replace('.', "-");
    std::fs::write(
        fix.extra_manifest_dir.join(format!("{id}.json")),
        serde_json::to_string_pretty(doc).unwrap(),
    )
    .unwrap();
    let pool = Pool::open(PoolConfig {
        path: DbPath::File(fix.run_db.clone()),
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();
    migrate(&pool, &repo_root().join("migrations")).unwrap();
    tool_registry::Registry::open(pool)
        .register(doc)
        .expect("probe manifest registers");
}

/// 运行库直连断言面。
fn run_pool(fix: &Fixture) -> Pool {
    Pool::open(PoolConfig {
        path: DbPath::File(fix.run_db.clone()),
        readonly: true,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap()
}

/// 能力可用守卫（红线 3：缺失 SKIP + 显式标注，不伪造失败）。格式
/// `SKIP <test>: capability <id> status=<status>`（任务书 §6）；二道检测
/// 同 three_channel_e2e::require_capability——status=available 但
/// executable_path 缺席（异机）同样 SKIP，静态种子不做存活检测。
fn capability_available(test: &str) -> bool {
    let provider = tool_engine::capability::StaticCatalogProvider::load(
        &repo_root().join("manifests/capabilities/host-catalog-seed.json"),
    )
    .unwrap();
    for id in ["pdf.pdfinfo", "pdf.pdftotext"] {
        match provider.get(&tool_engine::capability::CapabilityId::new(id).unwrap()) {
            Some(e) if e.entry.status.as_str() == "available" => match &e.entry.executable_path {
                Some(path) if path.exists() => {}
                Some(path) => {
                    println!(
                        "SKIP {test}: capability {id} status=available-but-executable-missing ({})",
                        path.display()
                    );
                    return false;
                }
                None => {
                    println!(
                        "SKIP {test}: capability {id} status=available-but-executable-path-absent"
                    );
                    return false;
                }
            },
            Some(e) => {
                println!(
                    "SKIP {test}: capability {id} status={}",
                    e.entry.status.as_str()
                );
                return false;
            }
            None => {
                println!("SKIP {test}: capability {id} status=absent-from-catalog");
                return false;
            }
        }
    }
    true
}

/// RT-1 host_pin_guard SKIP 先例（commit 12258e86 同款）：宿主二进制全量
/// sha256 前缀对照 manifest 双钉。异机构建宿主（CI runner p7zip ≠ 本机
/// Arch 构建）时 NATIVE_IDENTITY_MISMATCH fail-closed 在归档闸序之前触发，
/// 闸面归因证据属本机宿主面——该用例 SKIP（不降级断言不伪造失败，
/// fail-closed 拒绝行为本身即 adapter 身份校验语义的实证）。
fn host_binary_pin_guard(test: &str, path: &str, sha256_prefix: &str) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        println!("SKIP {test}: host binary missing ({path})");
        return false;
    };
    let measured = tool_registry::hash::sha256_hex(&bytes);
    if !measured.starts_with(sha256_prefix) {
        println!(
            "SKIP {test}: host binary sha256 prefix mismatch at {path} expected {sha256_prefix} measured {measured}"
        );
        return false;
    }
    true
}

/// 判据 10：native_rust 分支派发 PDF 通道链（run_tool 全链：受理 → Router
/// 编排 → 统一信封载荷），两条子 run trace_id = run_id 自锚定可反查。
#[test]
fn native_rust_dispatch_runs_pdf_chain_with_self_anchored_traces() {
    if !capability_available("native_rust_dispatch_runs_pdf_chain_with_self_anchored_traces") {
        return;
    }
    let fix = fixture("pdf");
    register_di1_manifests(&fix);
    let channel = fix.channel();
    let payload = channel
        .run_tool(&json!({
            "tool_id": "rip.doc.pdf.info",
            "input": {"input_path": "sample.pdf"},
        }))
        .expect("native_rust dispatch succeeds");
    assert_eq!(payload["ok"], json!(true));
    assert_eq!(payload["router"], json!("document-router"));
    assert_eq!(payload["channel"], json!("pdf"));
    let unified = &payload["result"];
    assert_eq!(unified["channel"], json!("pdf"));
    assert_eq!(unified["source"]["name"], json!("sample.pdf"));
    assert_eq!(unified["metadata"]["pages"], json!(2));
    let runs = payload["runs"].as_array().unwrap();
    assert_eq!(
        runs.len(),
        2,
        "pdfinfo + pdftotext 两条子 run（真实 run_tool 派发）"
    );
    // trace_id = run_id 自锚定（REST /api/traces 可反查的承载面）。
    let pool = run_pool(&fix);
    for run in runs {
        let run_id = run["run_id"].as_str().unwrap();
        let (status, trace): (String, Option<String>) = pool
            .with_conn(|c| {
                c.query_row(
                    "SELECT status, trace_id FROM tool_run WHERE run_id = ?1",
                    [run_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
            })
            .unwrap();
        assert_eq!(status, "SUCCEEDED", "子 run {run_id} 真实终态");
        assert_eq!(
            trace.as_deref(),
            Some(run_id),
            "trace_id = run_id 自锚定（判据 10 /api/traces 反查面）"
        );
    }
}

/// 嗅探门前置拒绝（DI-2 ADR D1 迁移后形态）：gzip 魔数（1f8b 不入嗅探表）
/// → INPUT ROUTER_UNSUPPORTED_TYPE，无 run 行（D7 前置门语义）。evil.zip
/// 自 DI-2 起路由归档通道——其链级拒绝面迁移至
/// native_rust_dispatch_zip_routes_to_archive_gate（list 行留证）。
#[test]
fn native_rust_dispatch_gate_rejects_unsniffed_magic_without_rows() {
    let fix = fixture("gate");
    // gzip 魔数测试字节（1F 8B + 非 UTF-8 体；魔数权威，扩展名仅提示）。
    std::fs::write(
        fix.document_input_root.join("archive.gz"),
        [0x1Fu8, 0x8B, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0x02],
    )
    .unwrap();
    let channel = fix.channel();
    let err = channel
        .run_tool(&json!({
            "tool_id": "rip.doc.pdf.info",
            "input": {"input_path": "archive.gz"},
        }))
        .expect_err("gzip magic rejected at sniff gate");
    assert_eq!(err["ok"], json!(false));
    assert_eq!(err["error"]["category"], json!("INPUT"));
    assert_eq!(err["error"]["code"], json!("ROUTER_UNSUPPORTED_TYPE"));
    // 无 run 行（前置门先于任何提交）。
    let pool = run_pool(&fix);
    let count: i64 = pool
        .with_conn(|c| c.query_row("SELECT count(*) FROM tool_run", [], |r| r.get::<_, i64>(0)))
        .unwrap();
    assert_eq!(count, 0, "嗅探门拒绝零 run 行");
}

/// DI-2 判据 1 dispatch 面：input.ocr 请求位透传 → route_ocr → channel
/// "ocr" 统一输出（identify + tesseract 两子 run trace 自锚定）。
#[test]
fn native_rust_dispatch_ocr_request_bit_routes_ocr_channel() {
    let test = "native_rust_dispatch_ocr_request_bit_routes_ocr_channel";
    let provider = tool_engine::capability::StaticCatalogProvider::load(
        &repo_root().join("manifests/capabilities/host-catalog-seed.json"),
    )
    .unwrap();
    for id in ["image.magick", "ocr.tesseract"] {
        match provider.get(&tool_engine::capability::CapabilityId::new(id).unwrap()) {
            Some(e) if e.entry.status.as_str() == "available" => match &e.entry.executable_path {
                Some(path) if path.exists() => {}
                _ => {
                    println!("SKIP {test}: capability {id} executable missing");
                    return;
                }
            },
            _ => {
                println!("SKIP {test}: capability {id} status unavailable");
                return;
            }
        }
    }
    // The host binary must match the catalog pin before this success-path test
    // invokes it; a different package build is an environment-specific SKIP.
    if !host_binary_pin_guard(test, "/usr/bin/tesseract", "44e4c41d9986e8b2") {
        return;
    }
    let fix = fixture("ocr");
    register_di1_manifests(&fix);
    let channel = fix.channel();
    let payload = channel
        .run_tool(&json!({
            "tool_id": "rip.doc.image.meta",
            "input": {
                "input_path": "text-ocr.png",
                "ocr": {"language": "en"},
            },
        }))
        .expect("ocr request bit dispatches route_ocr");
    assert_eq!(payload["ok"], json!(true));
    assert_eq!(payload["channel"], json!("ocr"), "请求位路由 OCR 通道");
    let unified = &payload["result"];
    assert_eq!(unified["channel"], json!("ocr"));
    assert_eq!(unified["text"], json!("HELLO OCR 123"));
    assert_eq!(unified["metadata"]["ocr"]["language"], json!("eng"));
    assert_eq!(
        unified["metadata"]["ocr"]["requested_language"],
        json!("en")
    );
    let runs = payload["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 2, "identify + tesseract 两子 run");
}

/// DI-2 判据 3 dispatch 面：chi_tra 请求 → ROUTER_OCR_LANG_MISSING 前置门
/// 拒绝（DEPENDENCY），零 run 行。
#[test]
fn native_rust_dispatch_ocr_chi_tra_rejected_without_rows() {
    let test = "native_rust_dispatch_ocr_chi_tra_rejected_without_rows";
    let provider = tool_engine::capability::StaticCatalogProvider::load(
        &repo_root().join("manifests/capabilities/host-catalog-seed.json"),
    )
    .unwrap();
    for id in ["image.magick", "ocr.tesseract"] {
        match provider.get(&tool_engine::capability::CapabilityId::new(id).unwrap()) {
            Some(e) if e.entry.status.as_str() == "available" => match &e.entry.executable_path {
                Some(path) if path.exists() => {}
                _ => {
                    println!("SKIP {test}: capability {id} executable missing");
                    return;
                }
            },
            _ => {
                println!("SKIP {test}: capability {id} status unavailable");
                return;
            }
        }
    }
    let fix = fixture("chitra");
    register_di1_manifests(&fix);
    let channel = fix.channel();
    let err = channel
        .run_tool(&json!({
            "tool_id": "rip.doc.image.meta",
            "input": {
                "input_path": "text-ocr.png",
                "ocr": {"language": "zh-Hant"},
            },
        }))
        .expect_err("chi_tra request rejected at language gate");
    assert_eq!(err["ok"], json!(false));
    assert_eq!(err["error"]["category"], json!("DEPENDENCY"));
    assert_eq!(err["error"]["code"], json!("ROUTER_OCR_LANG_MISSING"));
    let pool = run_pool(&fix);
    let count: i64 = pool
        .with_conn(|c| c.query_row("SELECT count(*) FROM tool_run", [], |r| r.get::<_, i64>(0)))
        .unwrap();
    assert_eq!(count, 0, "语言前置门拒绝零 run 行");
}

/// DI-2 ADR D1 行为变更迁移面：evil.zip（DI-1 zip_other 嗅探门负例）改
/// 路由归档通道 → 闸 1 穿越名链级拒绝，错误载荷 + list 子 run 行留证
/// （解包阶段零提交）。
#[test]
fn native_rust_dispatch_zip_routes_to_archive_gate() {
    let test = "native_rust_dispatch_zip_routes_to_archive_gate";
    // manifest 双钉两段（rip.archive.list.json runtime.binary_sha256 /
    // 执行目标），任一段异构即整测 SKIP（capability_catalog.rs:236-237
    // 与 document-router three_channel_e2e 同口径）。
    if !host_binary_pin_guard(test, "/usr/bin/7z", "f802383f13b8331a")
        || !host_binary_pin_guard(test, "/usr/lib/7zip/7z", "d48e622e9009acdb")
    {
        return;
    }
    let fix = fixture("zipgate");
    register_di1_manifests(&fix);
    let channel = fix.channel();
    let err = channel
        .run_tool(&json!({
            "tool_id": "rip.doc.pdf.info",
            "input": {"input_path": "evil.zip"},
        }))
        .expect_err("evil.zip traversal entry rejected at archive gate 1");
    assert_eq!(err["ok"], json!(false));
    assert_eq!(err["error"]["category"], json!("INPUT"));
    assert_eq!(
        err["error"]["code"],
        json!("ROUTER_ARCHIVE_ENTRY_INVALID"),
        "闸 1 穿越名拒绝（DI-2 ADR D4）：{:?}",
        err["error"]
    );
    // list 子 run 行已在库保持真实终态；解包阶段零提交（D8 表口径）。
    let runs = err["runs"].as_array().expect("run rows present");
    assert_eq!(runs.len(), 1, "仅 list 行留证");
    let pool = run_pool(&fix);
    let (count, status): (i64, String) = pool
        .with_conn(|c| {
            c.query_row(
                "SELECT count(*), coalesce((SELECT status FROM tool_run LIMIT 1), '')",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
        })
        .unwrap();
    assert_eq!(count, 1, "list 单行（解包零提交）");
    assert_eq!(status, "SUCCEEDED", "list 行真实终态留证");
}

/// 评审修订 2026-11-19：输入 64 MiB 帽 stat 前置——fs::read 全量读入之前
/// 判帽（D7「Router 嗅探前 stat 检查」的派发面承载点；此前实现先
/// `std::fs::read` 再由 route() 对 bytes_len 判帽，帽生效前全量内存读入
/// 已完成，构成受信输入根内的内存放大缝）。超帽不读入、零 run 行；码/
/// 帽值与 route() 门同源（RouterReject::input_too_large +
/// channel::MAX_INPUT_FILE_BYTES）。稀疏文件（set_len）不实际写盘，帽在
/// stat 判定即拒绝——本测同时证明读面从未触达。
#[test]
fn native_rust_dispatch_input_too_large_rejected_before_read() {
    let fix = fixture("toobig");
    let big = fix.document_input_root.join("huge.pdf");
    let f = std::fs::File::create(&big).unwrap();
    f.set_len(document_router::channel::MAX_INPUT_FILE_BYTES + 1)
        .unwrap();
    drop(f);
    let channel = fix.channel();
    let err = channel
        .run_tool(&json!({
            "tool_id": "rip.doc.pdf.info",
            "input": {"input_path": "huge.pdf"},
        }))
        .expect_err("over-cap input rejected at dispatch stat gate");
    assert_eq!(err["ok"], json!(false));
    assert_eq!(err["error"]["category"], json!("INPUT"));
    assert_eq!(err["error"]["code"], json!("ROUTER_INPUT_TOO_LARGE"));
    assert!(
        err["error"]["message"]
            .as_str()
            .unwrap()
            .contains("exceeds cap"),
        "帽语义如实入消息：{}",
        err["error"]["message"]
    );
    let pool = run_pool(&fix);
    let count: i64 = pool
        .with_conn(|c| c.query_row("SELECT count(*) FROM tool_run", [], |r| r.get::<_, i64>(0)))
        .unwrap();
    assert_eq!(count, 0, "stat 帽拒绝零 run 行（读面未触达）");
}

/// 词表外 runtime.type 拒绝面（DI-3/RT-4 D3 行为变化显性记账）：mcp_client
/// runtime 工具经 Resolver 接线后由 decide() N10b 前置硬拒（RT3_RUNTIME_VOCAB /
/// VALIDATION）——原 `other` 臂「不在本面承载范围」词形被 Resolver 词表闸取代
/// （词表外声明不再到达派发 match；承载语义不变，拒绝点前移 + 码表化）。
#[test]
fn dispatch_vocab_outside_rejected_by_resolver_gate() {
    let fix = fixture("other");
    // 登记 mcp_client runtime 的非 DI-1 工具（登记面合法形态 + manifest_for
    // 文件面）。
    register_probe_manifest(
        &fix,
        &json!({
            "schema_version": "1",
            "id": "rip.dispatch.otherface",
            "name": "Dispatch Other-Face Probe",
            "version": "0.1.0",
            "kind": "tool",
            "family": "utility",
            "description": "词表外声明探针（mcp_client runtime 经 Resolver 词表闸硬拒）",
            "runtime": {"type": "mcp_client", "endpoint": "stdio:/bin/true", "tool_name": "probe"},
            "input_schema": {"type": "object"},
            "output_schema": {"type": "object"},
            "resources": {"cpu": "1", "memory": "256Mi", "timeout_seconds": 10},
            "network": {"mode": "none"},
            "secrets": [],
            "permission_level": "T0",
            "idempotent": false,
            "health": {"probe": {"type": "none"}, "on_run_recover": false},
            "provenance": {"origin": "rip-native", "license": "repo-internal", "dependencies": [], "tested": "dispatch other-face"}
        }),
    );
    let channel = fix.channel();
    let err = channel
        .run_tool(&json!({
            "tool_id": "rip.dispatch.otherface",
            "input": {},
        }))
        .expect_err("mcp_client runtime stays outside dispatch scope");
    assert_eq!(
        err["error"]["category"], "VALIDATION",
        "N10b 词表闸 category 保持 VALIDATION: {err}"
    );
    assert_eq!(
        err["error"]["code"], "RT3_RUNTIME_VOCAB",
        "Resolver 词表闸拒绝码（D3②）: {err}"
    );
}

/// 通道表外 native_rust assign 工具（B 组准入钉值齐备：rip.knowledge.document.fetch
/// 正式登记件）→ 承载范围守卫拒绝（DI-3/RT-4 迁移后 assign 决策到达派发面，
/// 通道表守卫词形零漂移——D3 判据 4 派发行为面断言之一）。
#[test]
fn native_rust_tool_outside_channel_table_rejected() {
    let fix = fixture("scope");
    // B 组正式登记件（binary 绝对路径 + 64hex 钉值 → Resolver assign）；
    // 通道表零扩展（RT-3-TASK §1/A2：dedicated dispatch comes with its own
    // batch）→ run_tool 承载范围守卫拒绝，消息词形与迁移前一致。
    let official = std::fs::read_to_string(
        repo_root().join("manifests/tools/rip.knowledge.document.fetch.json"),
    )
    .expect("official B-group manifest readable");
    let doc: Value = serde_json::from_str(&official).expect("official manifest json");
    register_probe_manifest(&fix, &doc);
    let channel = fix.channel();
    let err = channel
        .run_tool(&json!({
            "tool_id": "rip.knowledge.document.fetch",
            "input": {"url": "http://example.com/", "allow_domain": ["example.com"]},
        }))
        .expect_err("non-channel-table native tool rejected");
    let message = err["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("outside the DI-1/DI-2 document channel table"),
        "承载范围守卫语义（DI-2 九工具清单更新）：{message}"
    );
}
