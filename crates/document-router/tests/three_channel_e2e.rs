//! DI-1 三通道真实 run e2e（DI-1-TASK §5 判据 1/2/3/4/5/6/7/8/9 逐项 +
//! 评审修订补测：D7 #7 页数帽 / D7 #8 像素帽行为级负例、页数帽定案输入
//! 缺席如实告警；§6 收口命令整套 `--test-threads=1`——office 通道 soffice
//! 隔离 profile 单线程承载，红线 6）。
//!
//! fixture 纪律（红线 2；评审修订 2026-11-19）：语料入库
//! `crates/document-router/tests/fixtures/`（此前取自
//! rip-data/parse-smoke/in/，该目录被根 .gitignore 整体排除不入库，CI
//! fresh checkout 必红），逐个 sha256 钉值断言见 `fixture_pins.rs`；
//! 另有 magick 本地生成 JPEG（xc: 内置画布，零下载）+ soffice 本地
//! txt→pdf 造超帽样本（判据 6）+ 测试内构造 2001 页最小合法 PDF（页数帽
//! 负例载具，纯代码构造零入库）；零网络。
//!
//! 能力缺失纪律（红线 3 / §6）：任一通道 CLI 缺失（capability 目录 status
//! != available，或目录 status=available 但 executable_path 存在性二道检测
//! 失败——静态种子不做存活检测，异机缺席在此层真实 SKIP）时该用例按
//! StaticCatalogProvider 判定 SKIP 并打印显式标注行（`SKIP <test>:
//! capability <id> status=<status>`，RT-1 host_pin_guard SKIP 先例
//! commit 12258e86 同款），不伪造失败不空跑。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use database::{migrate, DbPath, Pool, PoolConfig};
use document_router::channel::MAX_PAGES;
use serde_json::{json, Value};
use tool_engine::adapter::{NativeRustAdapter, NativeRustConfig};
use tool_engine::capability::{CapabilityId, CapabilityProvider, StaticCatalogProvider};
use tool_engine::Engine;
use tool_registry::{Options, Registry};

/// soffice 互斥锁（红线 6 office 单线程纪律的并行退守面：收口命令
/// --test-threads=1 下无竞争；默认并行模式下本套件内多个 soffice 用例
/// 互斥串行——本机实测并发 soffice 实例转换 exit 1，E §4.2 隔离 profile
/// 之外仍有实例级互斥）。RT-1 native_rust_e2e OBSERVATION_LOCK 同款。
static SOFFICE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

/// 入库 fixture 目录（byte 级同源副本来自 rip-data/parse-smoke/in/，sha256
/// 钉值见 fixture_pins.rs；缺文件即 panic——语料属仓库资产）。
fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn smoke(name: &str) -> Vec<u8> {
    std::fs::read(fixtures_dir().join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

static WS_SEQ: AtomicU32 = AtomicU32::new(0);

/// 独立临时工作区根（每用例独立；红线 6 office 隔离的结构前提）。
fn workspace_root(tag: &str) -> PathBuf {
    let ws = std::env::temp_dir().join(format!(
        "di1-router-e2e-{tag}-{}-{}",
        std::process::id(),
        WS_SEQ.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&ws);
    std::fs::create_dir_all(&ws).unwrap();
    ws
}

/// 引擎 + 九份通道表 Manifest 登记（DI-1 四份 + DI-2 五份；内存库；生产库
/// 零触碰，红线 4）。`extra` 为追加登记件（判据 5 harness 注入面）。
fn fresh_engine(extra: &[Value]) -> Engine {
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
    for doc in extra {
        registry
            .register(doc)
            .unwrap_or_else(|e| panic!("register extra {}: {e}", doc["id"]));
    }
    Engine::open(pool, registry)
}

/// capability 目录（种子四条 sha256_full 已回填；F §6 二道校验活面）。
fn provider() -> StaticCatalogProvider {
    StaticCatalogProvider::load(&repo_root().join("manifests/capabilities/host-catalog-seed.json"))
        .expect("seed loads")
}

/// 能力可用性守卫（红线 3）：缺失即 SKIP + 显式标注（`SKIP <test>:
/// capability <id> status=<status>`，任务书 §6 格式；评审修订 2026-11-19
/// 补测试名，多能力守卫的标注行可归属用例）。
///
/// 二道检测（评审修订 2026-11-19）：`provider.get` 是静态种子的纯缓存读
/// （不做存活检测），seed status=available 只如实反映本机快照——异机目录
/// 在案但可执行文件缺席时守卫照样放行，随后 adapter 身份校验失败落为
/// 测试失败而非 SKIP（红线 3 在异机不成立）。此处对 executable_path 做
/// stat 存在性复核，缺席按 SKIP 处理（缺失能力只报告不安装，红线 3）。
fn require_capability(provider: &StaticCatalogProvider, test: &str, id: &str) -> bool {
    match provider.get(&CapabilityId::new(id).expect("capability id")) {
        Some(entry) if entry.entry.status.as_str() == "available" => {
            match &entry.entry.executable_path {
                Some(path) if path.exists() => true,
                Some(path) => {
                    println!(
                        "SKIP {test}: capability {id} status=available-but-executable-missing ({})",
                        path.display()
                    );
                    false
                }
                None => {
                    println!(
                        "SKIP {test}: capability {id} status=available-but-executable-path-absent"
                    );
                    false
                }
            }
        }
        Some(entry) => {
            println!(
                "SKIP {test}: capability {id} status={}",
                entry.entry.status.as_str()
            );
            false
        }
        None => {
            println!("SKIP {test}: capability {id} status=absent-from-catalog");
            false
        }
    }
}

fn router_for(ws_root: &Path) -> document_router::Router {
    document_router::Router::new(
        document_router::RouterConfig {
            workspace_root: ws_root.to_path_buf(),
        },
        document_router::RouterManifests::from_dirs(&[repo_root().join("manifests/tools")])
            .expect("channel-table manifests load"),
    )
}

fn adapter_for(ws_root: &Path) -> NativeRustAdapter {
    // Auto 档（生产口径；本机落 prlimit 兜底，RT-1 报告 G 遗留 1）。
    NativeRustAdapter::new(NativeRustConfig {
        workspace_root: ws_root.to_path_buf(),
        ..NativeRustConfig::default()
    })
}

/// run 行状态查询（判据 1/2 断言面）。
fn run_row_status(engine: &Engine, run_id: &str) -> Option<String> {
    engine
        .pool()
        .with_conn(|c| {
            c.query_row(
                "SELECT status FROM tool_run WHERE run_id = ?1",
                [run_id],
                |r| r.get::<_, String>(0),
            )
            .map(Some)
        })
        .unwrap_or(None)
}

/// tool_run 行总数（判据 4 零 run 行断言面）。
fn run_row_count(engine: &Engine) -> i64 {
    engine
        .pool()
        .with_conn(|c| c.query_row("SELECT count(*) FROM tool_run", [], |r| r.get::<_, i64>(0)))
        .unwrap()
}

/// 子 run stdout 留档（Router 回读同路径）。
fn stdout_log(ws_root: &Path, run_id: &str) -> String {
    String::from_utf8_lossy(&std::fs::read(ws_root.join(run_id).join("out/stdout.log")).unwrap())
        .to_string()
}

/// magick 本地生成 JPEG（xc: 内置画布；magick 缺席按 SKIP）。
fn generated_jpeg(dir: &Path) -> Option<Vec<u8>> {
    let out = dir.join("generated.jpg");
    let ok = std::process::Command::new("/usr/bin/magick")
        .args([
            "-size",
            "64x32",
            "xc:white",
            "-colorspace",
            "sRGB",
            out.to_str().unwrap(),
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !ok {
        println!("SKIP: /usr/bin/magick unavailable for local JPEG fixture generation");
        return None;
    }
    Some(std::fs::read(&out).unwrap())
}

// ---------------------------------------------------------------------------
// 判据 1：PDF 通道端到端
// ---------------------------------------------------------------------------

#[test]
fn pdf_channel_end_to_end_two_runs_unified_output() {
    let provider = provider();
    if !require_capability(
        &provider,
        "pdf_channel_end_to_end_two_runs_unified_output",
        "pdf.pdfinfo",
    ) || !require_capability(
        &provider,
        "pdf_channel_end_to_end_two_runs_unified_output",
        "pdf.pdftotext",
    ) {
        return;
    }
    let engine = fresh_engine(&[]);
    let ws_root = workspace_root("pdf");
    let router = router_for(&ws_root);
    let adapter = adapter_for(&ws_root);
    let bytes = smoke("sample.pdf");
    let outcome = router
        .route(
            &engine,
            &adapter,
            Some(&provider),
            &bytes,
            "sample.pdf",
            "pdf",
        )
        .expect("pdf channel accepted");
    assert_eq!(outcome.status, document_router::RouteStatus::Succeeded);
    assert_eq!(outcome.runs.len(), 2, "pdfinfo + pdftotext 两条子 run");
    for run in &outcome.runs {
        assert_eq!(
            run_row_status(&engine, &run.run_id).as_deref(),
            Some("SUCCEEDED"),
            "子 run {} 行真实终态",
            run.run_id
        );
        assert_eq!(run.outcome.transcript.len(), 4, "[Q,P,RUNNING,SUCCEEDED]");
    }
    let info_run = &outcome.runs[0];
    let report = stdout_log(&ws_root, &info_run.run_id);
    assert!(
        report.lines().any(|l| l.trim_start().starts_with("Pages:")),
        "stdout.log 含 Pages 行（pdfinfo 报告形态）：{report}"
    );
    // 统一输出（D3 Schema）。
    let unified = outcome.unified.as_ref().unwrap();
    assert_eq!(unified["channel"], json!("pdf"));
    assert_eq!(unified["source"]["kind"], json!("file"));
    assert_eq!(unified["source"]["name"], json!("sample.pdf"));
    assert_eq!(
        unified["source"]["sha256"],
        json!(tool_registry::hash::sha256_hex(&bytes)),
        "source sha256 与源文件对账"
    );
    assert_eq!(unified["metadata"]["pages"], json!(2));
    assert_eq!(unified["metadata"]["page_size"]["width"], json!(595));
    assert_eq!(unified["metadata"]["page_size"]["height"], json!(842));
    assert_eq!(unified["metadata"]["page_size"]["unit"], json!("pts"));
    assert_eq!(
        unified["page_anchors"].as_array().unwrap().len(),
        2,
        "页锚 2 段（\\f ×2）"
    );
    assert!(
        !unified["text"].as_str().unwrap().is_empty(),
        "抽取文本非空"
    );
    // Artifact sha256 对账：子 run 信封 artifacts 与 file-mode data.artifacts
    // 同 sha256（D8：stdout 留档 = Artifact 行）。
    let data = info_run.outcome.envelope.result.as_ref().unwrap();
    let listed = data["artifacts"].as_array().unwrap();
    let stdout_entry = listed
        .iter()
        .find(|a| a["name"] == json!("stdout.log"))
        .expect("stdout.log listed");
    let row = info_run
        .outcome
        .envelope
        .artifacts
        .iter()
        .find(|a| a.uri.ends_with("/out/stdout.log"))
        .expect("stdout.log registered");
    assert_eq!(
        stdout_entry["sha256"],
        json!(row.sha256),
        "data ↔ Artifact 对账齐"
    );
    // trace 自锚定（判据 10 的 Router 面）。
    let trace = engine.trace_id_of(&info_run.run_id).unwrap();
    assert_eq!(trace.as_deref(), Some(info_run.run_id.as_str()));
    let _ = std::fs::remove_dir_all(&ws_root);
}

// ---------------------------------------------------------------------------
// 判据 2：Office 通道端到端（soffice 隔离 profile；套件 --test-threads=1）
// ---------------------------------------------------------------------------

#[test]
fn office_channel_end_to_end_cascade_three_runs() {
    let _guard = SOFFICE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let provider = provider();
    if !require_capability(
        &provider,
        "office_channel_end_to_end_cascade_three_runs",
        "document.libreoffice",
    ) || !require_capability(
        &provider,
        "office_channel_end_to_end_cascade_three_runs",
        "pdf.pdfinfo",
    ) || !require_capability(
        &provider,
        "office_channel_end_to_end_cascade_three_runs",
        "pdf.pdftotext",
    ) {
        return;
    }
    let engine = fresh_engine(&[]);
    let ws_root = workspace_root("office");
    let router = router_for(&ws_root);
    let adapter = adapter_for(&ws_root);
    let bytes = smoke("sample.docx");
    let outcome = router
        .route(
            &engine,
            &adapter,
            Some(&provider),
            &bytes,
            "sample.docx",
            "docx",
        )
        .expect("office channel accepted");
    assert_eq!(
        outcome.status,
        document_router::RouteStatus::Succeeded,
        "office chain failed: {:?}",
        outcome.reject
    );
    assert_eq!(
        outcome.runs.len(),
        3,
        "soffice + pdfinfo + pdftotext 三条子 run"
    );
    let office_run = &outcome.runs[0];
    assert_eq!(office_run.tool_id, "rip.doc.office.to_pdf");
    assert_eq!(
        run_row_status(&engine, &office_run.run_id).as_deref(),
        Some("SUCCEEDED")
    );
    // 转换产物登记（out/sample.pdf artifact 行）。
    let pdf_row = office_run
        .outcome
        .envelope
        .artifacts
        .iter()
        .find(|a| a.uri.ends_with("/out/sample.pdf"))
        .expect("converted pdf registered as artifact");
    assert_eq!(pdf_row.role, "primary");
    assert!(pdf_row.size_bytes > 0);
    // 统一输出：channel=office + 转换面声明 warnings + office 附加 metadata。
    let unified = outcome.unified.as_ref().unwrap();
    assert_eq!(unified["channel"], json!("office"));
    assert_eq!(unified["metadata"]["office_subtype"], json!("docx"));
    assert!(
        unified["metadata"]["converted_pdf_bytes"].as_u64().unwrap() > 0,
        "converted_pdf_bytes = 转换产物字节数"
    );
    let warnings = unified["warnings"].as_array().unwrap();
    assert!(
        warnings.iter().any(|w| w
            .as_str()
            .unwrap()
            .contains("office→pdf conversion-face declaration")),
        "warnings 携带 office→pdf 转换面声明（D3）：{warnings:?}"
    );
    assert!(
        !unified["text"].as_str().unwrap().is_empty(),
        "级联抽取文本非空"
    );
    assert!(
        !unified["page_anchors"].as_array().unwrap().is_empty(),
        "转换 PDF 页锚非空"
    );
    // 判据 7：隔离 profile——run_id 派生 tmp 目录用后即焚（cleanup_tmp）。
    assert!(
        !ws_root.join(&office_run.run_id).join("tmp").exists(),
        "office run tmp/（含 lo-profile）用后即焚"
    );
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// 判据 7：两次 office run 的隔离 profile 互不相同 + tmp 双清理（E §4.2
/// 禁共享 profile；单线程套件内串行两跑）。
#[test]
fn office_profile_isolation_across_two_runs() {
    let _guard = SOFFICE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let provider = provider();
    if !require_capability(
        &provider,
        "office_profile_isolation_across_two_runs",
        "document.libreoffice",
    ) || !require_capability(
        &provider,
        "office_profile_isolation_across_two_runs",
        "pdf.pdfinfo",
    ) || !require_capability(
        &provider,
        "office_profile_isolation_across_two_runs",
        "pdf.pdftotext",
    ) {
        return;
    }
    let engine = fresh_engine(&[]);
    let ws_root = workspace_root("office-iso");
    let router = router_for(&ws_root);
    let adapter = adapter_for(&ws_root);
    let bytes = smoke("sample.docx");
    let first = router
        .route(
            &engine,
            &adapter,
            Some(&provider),
            &bytes,
            "sample.docx",
            "docx",
        )
        .expect("first office run");
    let second = router
        .route(
            &engine,
            &adapter,
            Some(&provider),
            &bytes,
            "sample.docx",
            "docx",
        )
        .expect("second office run");
    let first_office = &first.runs[0];
    let second_office = &second.runs[0];
    assert_ne!(
        first_office.run_id, second_office.run_id,
        "两 run 的 run_id（→ profile 路径）互不相同"
    );
    assert!(
        !ws_root.join(&first_office.run_id).join("tmp").exists()
            && !ws_root.join(&second_office.run_id).join("tmp").exists(),
        "两 run 的 tmp/（含各自 profile）均用后即焚，无共享 profile 目录"
    );
    let _ = std::fs::remove_dir_all(&ws_root);
}

// ---------------------------------------------------------------------------
// 判据 3：图像通道端到端（PNG 样本 + magick 本地生成 JPEG）
// ---------------------------------------------------------------------------

#[test]
fn image_channel_end_to_end_png_and_generated_jpeg() {
    let provider = provider();
    if !require_capability(
        &provider,
        "image_channel_end_to_end_png_and_generated_jpeg",
        "image.magick",
    ) {
        return;
    }
    let engine = fresh_engine(&[]);
    let ws_root = workspace_root("image");
    let router = router_for(&ws_root);
    let adapter = adapter_for(&ws_root);
    let fixture_dir = std::env::temp_dir().join(format!("di1-jpeg-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&fixture_dir);

    // PNG 样本。
    let bytes = smoke("sample.png");
    let outcome = router
        .route(
            &engine,
            &adapter,
            Some(&provider),
            &bytes,
            "sample.png",
            "png",
        )
        .expect("image channel accepted");
    assert_eq!(outcome.status, document_router::RouteStatus::Succeeded);
    assert_eq!(outcome.runs.len(), 1);
    let unified = outcome.unified.as_ref().unwrap();
    assert_eq!(unified["channel"], json!("image"));
    assert_eq!(unified["metadata"]["format"], json!("PNG"));
    assert_eq!(unified["metadata"]["dimensions"]["width"], json!(834));
    assert_eq!(unified["metadata"]["dimensions"]["height"], json!(250));
    assert_eq!(unified["metadata"]["colorspace"], json!("sRGB"));
    assert_eq!(unified["metadata"]["depth"], json!(8));
    assert_eq!(unified["text"], json!(""), "image 通道 text 为空串（D3）");

    // magick 本地生成 JPEG（零下载）。
    let Some(jpeg) = generated_jpeg(&fixture_dir) else {
        return;
    };
    let outcome = router
        .route(
            &engine,
            &adapter,
            Some(&provider),
            &jpeg,
            "generated.jpg",
            "jpg",
        )
        .expect("jpeg accepted");
    assert_eq!(outcome.status, document_router::RouteStatus::Succeeded);
    let unified = outcome.unified.as_ref().unwrap();
    assert_eq!(unified["metadata"]["format"], json!("JPEG"));
    assert_eq!(unified["metadata"]["dimensions"]["width"], json!(64));
    assert_eq!(unified["metadata"]["dimensions"]["height"], json!(32));
    let _ = std::fs::remove_dir_all(&ws_root);
    let _ = std::fs::remove_dir_all(&fixture_dir);
}

// ---------------------------------------------------------------------------
// 判据 4（DI-2 ADR D1 迁移后形态）：不支持类型拒绝（嗅探门，零子进程零
// run 行）。zip_other 三负例（epub/zip/evil.zip）自 DI-2 起路由归档通道
// （迁移见 archive 四闸负例与本文件 archive e2e 正例），嗅探门零 run 行
// 负例集改由 tar/gzip/unknown 二进制承载（D1：fail-closed INPUT 拒绝）。
// ---------------------------------------------------------------------------

#[test]
fn unsupported_types_rejected_at_gate_with_zero_rows() {
    let provider = provider();
    let engine = fresh_engine(&[]);
    let ws_root = workspace_root("gate");
    let router = router_for(&ws_root);
    let adapter = adapter_for(&ws_root);

    // gzip 魔数（1F 8B，D1 实测可判不入表）→ Unknown → INPUT 拒绝。
    let gzip = [0x1Fu8, 0x8B, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0x02];
    let err = router
        .route(
            &engine,
            &adapter,
            Some(&provider),
            &gzip,
            "archive.gz",
            "gz",
        )
        .expect_err("gzip magic must stay fail-closed at sniff gate");
    assert_eq!(err.category, "INPUT");
    assert_eq!(err.code, "ROUTER_UNSUPPORTED_TYPE");
    // 非 UTF-8 二进制 blob → Unknown → INPUT 拒绝。
    let blob = [0xDEu8, 0xAD, 0xBE, 0xEF, 0xFF, 0x00, 0x01];
    let err = router
        .route(&engine, &adapter, Some(&provider), &blob, "blob.bin", "bin")
        .expect_err("unknown magic must be rejected at sniff gate");
    assert_eq!(err.code, "ROUTER_UNSUPPORTED_TYPE");
    // 伪造 .pdf 扩展名纯文本 → text 签名拒绝并注明走 rip.doc.parse。
    let fake_pdf = b"this is definitely not a pdf document\n".to_vec();
    let err = router
        .route(
            &engine,
            &adapter,
            Some(&provider),
            &fake_pdf,
            "fake.pdf",
            "pdf",
        )
        .expect_err("text signature must be rejected");
    assert_eq!(err.code, "ROUTER_UNSUPPORTED_TYPE");
    assert!(
        err.message.contains("rip.doc.parse"),
        "text 拒绝注明走 rip.doc.parse（判据 4）：{}",
        err.message
    );
    // 零子进程拉起 + 零 run 行（D7 前置门语义）。
    assert_eq!(run_row_count(&engine), 0, "嗅探门拒绝零 run 行");
    let _ = std::fs::remove_dir_all(&ws_root);
}

// ---------------------------------------------------------------------------
// 判据 5：能力 missing 显式错（pdf.mutool 种子条目形态注入）
// ---------------------------------------------------------------------------

#[test]
fn missing_capability_explicit_dependency_rejection() {
    let provider = provider();
    // harness 注入（pdf.mutool 种子条目形态：executable_path null / status
    // missing）：identity 校验过（钉值自持 stub）但 capability_ref 指向目录
    // missing 条目 → PREPARING 拒绝 DEPENDENCY 显式信封（F §6 二道校验）。
    let mut doc: Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("manifests/tools/rip.doc.pdf.info.json"))
            .unwrap(),
    )
    .unwrap();
    doc["runtime"]["capability_ref"] = json!("pdf.mutool");
    doc["id"] = json!("rip.doc.pdf.info.missingcap");
    doc["name"] = json!("PDF Info Probe (missingcap harness)");
    let ws_root = workspace_root("missingcap");
    // 钉值自持 identity stub（CI 修复批教训落实：钉值前提不得依赖宿主单机
    // 快照——原 harness 沿用 Manifest 的 /usr/bin/pdfinfo 钉值，CI runner
    // 无 poppler 时 identity 钉值校验先拒 NATIVE_BINARY_MISSING（message
    // "pinned binary … not present on host"，不含 missing），capability
    // missing 路径在异构宿主永远测不到）：测试内造最小可执行脚本（/bin/sh
    // 解释器跨宿主稳定在位）并实时 sha256 写入 harness Manifest，identity
    // 校验对照实测钉值恒过 → 二道校验按本测意图触发 capability missing
    // 显式 DEPENDENCY。宿主完整性由 probe stale/missing 语义承载。
    let stub = ws_root.join("missingcap-identity-stub.sh");
    std::fs::write(&stub, "#!/bin/sh\nexit 0\n").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    doc["runtime"]["binary"] = json!(stub.to_str().unwrap());
    doc["runtime"]["binary_sha256"] = json!(tool_registry::hash::sha256_hex(
        &std::fs::read(&stub).unwrap()
    ));
    let engine = fresh_engine(std::slice::from_ref(&doc));
    let adapter = adapter_for(&ws_root);
    let run_id = "rn-missingcap-di1";
    // 预写面照常（Router 驱动形态同 D5；PREPARING 拒绝发生在 identity 之后）。
    let in_dir = ws_root.join(run_id).join("in");
    std::fs::create_dir_all(&in_dir).unwrap();
    std::fs::write(in_dir.join("sample.pdf"), smoke("sample.pdf")).unwrap();
    engine
        .submit(&json!({
            "protocol_version": "1",
            "run_id": run_id,
            "tool_id": "rip.doc.pdf.info.missingcap",
            "input": {"input_path": "sample.pdf"},
            "workspace_id": "ws-missingcap",
        }))
        .unwrap();
    let outcome = tool_engine::adapter::execute_native_rust_via_engine(
        tool_engine::adapter::NativeRustCall {
            engine: &engine,
            run_id,
            adapter: &adapter,
            manifest: &doc,
            capability: Some(&provider),
        },
    )
    .expect("drive ok");
    assert_eq!(outcome.envelope.status, domain::ToolRunStatus::Failed);
    let error = outcome
        .envelope
        .error
        .as_ref()
        .expect("explicit error envelope");
    assert_eq!(
        error.category, "DEPENDENCY",
        "能力 missing → DEPENDENCY（D8 表）"
    );
    assert!(
        error.message.to_lowercase().contains("missing"),
        "显式 missing 语义：{}",
        error.message
    );
    assert_eq!(run_row_status(&engine, run_id).as_deref(), Some("FAILED"));
    let _ = std::fs::remove_dir_all(&ws_root);
}

// ---------------------------------------------------------------------------
// 判据 6：超帽截断（子 run stdout 超 max_output_bytes → NATIVE_OUTPUT_CAP_
// EXCEEDED FAILED；不静默丢字节）
// ---------------------------------------------------------------------------

#[test]
fn oversized_stdout_cap_exceeded_chain_fails_honestly() {
    let _guard = SOFFICE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let provider = provider();
    if !require_capability(
        &provider,
        "oversized_stdout_cap_exceeded_chain_fails_honestly",
        "document.libreoffice",
    ) || !require_capability(
        &provider,
        "oversized_stdout_cap_exceeded_chain_fails_honestly",
        "pdf.pdfinfo",
    ) || !require_capability(
        &provider,
        "oversized_stdout_cap_exceeded_chain_fails_honestly",
        "pdf.pdftotext",
    ) {
        return;
    }
    // soffice 本地造超帽样本：2 MiB 文本 → PDF（零下载；txt 输入非禁面——
    // D3 禁的是 `--convert-to txt:Text` 输出过滤器）。
    let big_dir = std::env::temp_dir().join(format!("di1-bigpdf-{}", std::process::id()));
    std::fs::create_dir_all(&big_dir).unwrap();
    let big_text = big_dir.join("big.txt");
    std::fs::write(&big_text, "word ".repeat(420_000)).unwrap(); // ~2.1 MB
    let converted = big_dir.join("big.pdf");
    let ok = std::process::Command::new("/usr/bin/soffice")
        .args([
            "--headless",
            "--norestore",
            &format!(
                "-env:UserInstallation=file://{}/lo-profile",
                big_dir.display()
            ),
            "--convert-to",
            "pdf",
            "--outdir",
            big_dir.to_str().unwrap(),
            big_text.to_str().unwrap(),
        ])
        .env_clear()
        .env("PATH", "/usr/bin")
        .env("TMPDIR", &big_dir)
        .env("LANG", "C.UTF-8")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !ok || !converted.exists() {
        println!("SKIP: local soffice could not generate the oversized PDF fixture");
        let _ = std::fs::remove_dir_all(&big_dir);
        return;
    }
    let bytes = std::fs::read(&converted).unwrap();
    let engine = fresh_engine(&[]);
    let ws_root = workspace_root("cap");
    let router = router_for(&ws_root);
    let adapter = adapter_for(&ws_root);
    let outcome = router
        .route(&engine, &adapter, Some(&provider), &bytes, "big.pdf", "pdf")
        .expect("pdf channel accepted");
    assert_eq!(outcome.status, document_router::RouteStatus::Failed);
    let reject = outcome.reject.as_ref().unwrap();
    assert_eq!(reject.category, "EXECUTION");
    assert_eq!(
        reject.code, "NATIVE_OUTPUT_CAP_EXCEEDED",
        "读帽触顶显式失败（不静默丢字节）"
    );
    // 链级失败 + 子 run 行留证（D8 表）：pdfinfo 成功、pdftotext 帽触顶 FAILED。
    assert_eq!(outcome.runs.len(), 2, "pdfinfo + pdftotext 两步都留证");
    assert_eq!(
        run_row_status(&engine, &outcome.runs[0].run_id).as_deref(),
        Some("SUCCEEDED"),
        "pdfinfo 行真实终态"
    );
    assert_eq!(
        run_row_status(&engine, &outcome.runs[1].run_id).as_deref(),
        Some("FAILED"),
        "pdftotext 帽触顶行真实终态"
    );
    assert_eq!(outcome.runs[1].tool_id, "rip.doc.pdf.text");
    let _ = std::fs::remove_dir_all(&ws_root);
    let _ = std::fs::remove_dir_all(&big_dir);
}

// ---------------------------------------------------------------------------
// 判据 8：env 白名单断言（env_echo fixture 形状复用 + file 模式）
// ---------------------------------------------------------------------------

#[test]
fn native_subprocess_env_is_exact_whitelist_file_mode() {
    // env_echo fixture（帧形态；白名单断言面与 result_mode 无关——D4 双模
    // 共享同一 adapter env 装配面）：断言四工具共享的 D4 白名单
    // （PATH/TMPDIR/LANG 精确集 + Manifest env；本批四 Manifest env 零追加
    // → 键集恰为白名单基集），零宿主键继承。
    let fixture = repo_root().join("crates/tool-engine/tests/fixtures/native_rust/env_echo.sh");
    let sha = tool_registry::hash::sha256_hex(&std::fs::read(&fixture).unwrap());
    assert_eq!(
        sha, "282af314ee118066dc64b6ccb59146e094287776a7053ced8873077241d73aa3",
        "fixture 钉值漂移（fixture 与钉值一并入库纪律）"
    );
    let mut manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("manifests/tools/rip.doc.pdf.info.json"))
            .unwrap(),
    )
    .unwrap();
    manifest["id"] = json!("rip.rt1.envfile");
    manifest["runtime"]["binary"] = json!(fixture.display().to_string());
    manifest["runtime"]["binary_sha256"] = json!(sha);
    manifest["runtime"]
        .as_object_mut()
        .unwrap()
        .remove("capability_ref");
    manifest["runtime"]
        .as_object_mut()
        .unwrap()
        .remove("result_mode");
    manifest.as_object_mut().unwrap().remove("examples");
    manifest["output_schema"] = json!({"type": "object"});
    manifest["runtime"]["argv_template"] = json!(["{{out_dir}}", "{{input_path}}"]);
    manifest["runtime"]["subcommands"] = json!(["env"]);
    manifest["input_schema"] = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["subcommand", "input_path"],
        "properties": {
            "subcommand": {"type": "string", "enum": ["env"]},
            "input_path": {"type": "string", "minLength": 1}
        }
    });
    let pool = Pool::open(PoolConfig {
        path: DbPath::Memory,
        readonly: false,
        max_connections: 1,
        busy_timeout: Duration::from_secs(5),
    })
    .unwrap();
    migrate(&pool, &repo_root().join("migrations")).unwrap();
    let registry = Registry::open(pool.clone());
    registry.register(&manifest).unwrap();
    let engine = Engine::open(pool, registry);
    let ws_root = workspace_root("envfile");
    let adapter = NativeRustAdapter::new(NativeRustConfig {
        workspace_root: ws_root.clone(),
        ..NativeRustConfig::default()
    });
    let run_id = "rn-envfile-di1";
    engine
        .submit(&json!({
            "protocol_version": "1",
            "run_id": run_id,
            "tool_id": "rip.rt1.envfile",
            "input": {"subcommand": "env", "input_path": "probe.txt"},
            "workspace_id": "ws-di1",
        }))
        .unwrap();
    let outcome = tool_engine::adapter::execute_native_rust_via_engine(
        tool_engine::adapter::NativeRustCall {
            engine: &engine,
            run_id,
            adapter: &adapter,
            manifest: &manifest,
            capability: None,
        },
    )
    .unwrap();
    assert_eq!(
        outcome.envelope.status,
        domain::ToolRunStatus::Succeeded,
        "env echo run failed: {:?}",
        outcome.envelope.error
    );
    let data = outcome.envelope.result.as_ref().unwrap();
    let env: Vec<(String, String)> = data["env"]
        .as_array()
        .unwrap()
        .iter()
        .map(|kv| {
            let s = kv.as_str().unwrap();
            let (k, v) = s.split_once('=').unwrap_or((s, ""));
            (k.to_string(), v.to_string())
        })
        .collect();
    let keys: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
    for &key in &keys {
        assert!(
            matches!(key, "PATH" | "TMPDIR" | "LANG" | "PWD" | "SHLVL" | "_"),
            "环境键 {key:?} 超出白名单基集（Manifest env 零追加 + bash 自设键）"
        );
    }
    for (key, _) in std::env::vars() {
        if matches!(
            key.as_str(),
            "PATH" | "TMPDIR" | "LANG" | "SHLVL" | "PWD" | "_"
        ) {
            continue;
        }
        assert!(
            !keys.contains(&key.as_str()),
            "宿主环境键 {key:?} 透传进子进程（SSH_AUTH_SOCK/HTTP_PROXY/数据库路径零出现，判据 8）"
        );
    }
    let _ = std::fs::remove_dir_all(&ws_root);
}

// ---------------------------------------------------------------------------
// 判据 9：嗅探权威（.pdf 扩展名 + PNG 魔数 → 图像通道 + mismatch）
// ---------------------------------------------------------------------------

#[test]
fn sniff_authority_routes_png_body_with_pdf_extension() {
    let provider = provider();
    if !require_capability(
        &provider,
        "sniff_authority_routes_png_body_with_pdf_extension",
        "image.magick",
    ) {
        return;
    }
    let engine = fresh_engine(&[]);
    let ws_root = workspace_root("authority");
    let router = router_for(&ws_root);
    let adapter = adapter_for(&ws_root);
    let bytes = smoke("sample.png");
    // 扩展名 .pdf + PNG 魔数 → 魔数权威路由图像通道 + mismatch=true。
    let outcome = router
        .route(
            &engine,
            &adapter,
            Some(&provider),
            &bytes,
            "sneaky.pdf",
            "pdf",
        )
        .expect("sniff-authoritative routing accepted");
    assert_eq!(outcome.status, document_router::RouteStatus::Succeeded);
    let unified = outcome.unified.as_ref().unwrap();
    assert_eq!(unified["channel"], json!("image"), "魔数权威路由图像通道");
    assert_eq!(unified["source"]["sniff"]["magic"], json!("png"));
    assert_eq!(unified["source"]["sniff"]["ext"], json!("pdf"));
    assert_eq!(unified["source"]["sniff"]["mismatch"], json!(true));
    let warnings = unified["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("routed by magic")),
        "warnings 记录嗅探/扩展名不一致：{warnings:?}"
    );
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// 判据 9 后半：xlsx 首条目反例样本正确判 zip_ooxml/xlsx 并走 Office 通道
/// （calc_pdf_Export 级联）。
#[test]
fn xlsx_counterexample_routes_office_with_xlsx_subtype() {
    let _guard = SOFFICE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let provider = provider();
    if !require_capability(
        &provider,
        "xlsx_counterexample_routes_office_with_xlsx_subtype",
        "document.libreoffice",
    ) || !require_capability(
        &provider,
        "xlsx_counterexample_routes_office_with_xlsx_subtype",
        "pdf.pdfinfo",
    ) || !require_capability(
        &provider,
        "xlsx_counterexample_routes_office_with_xlsx_subtype",
        "pdf.pdftotext",
    ) {
        return;
    }
    let engine = fresh_engine(&[]);
    let ws_root = workspace_root("xlsx");
    let router = router_for(&ws_root);
    let adapter = adapter_for(&ws_root);
    let bytes = smoke("sample.xlsx");
    let outcome = router
        .route(
            &engine,
            &adapter,
            Some(&provider),
            &bytes,
            "sample.xlsx",
            "xlsx",
        )
        .expect("xlsx accepted");
    assert_eq!(
        outcome.status,
        document_router::RouteStatus::Succeeded,
        "xlsx office chain failed: {:?}",
        outcome.reject
    );
    let unified = outcome.unified.as_ref().unwrap();
    assert_eq!(unified["channel"], json!("office"));
    assert_eq!(
        unified["metadata"]["office_subtype"],
        json!("xlsx"),
        "首条目反例正确判 xlsx 子型"
    );
    assert_eq!(unified["source"]["sniff"]["magic"], json!("zip_ooxml"));
    let _ = std::fs::remove_dir_all(&ws_root);
}

// ---------------------------------------------------------------------------
// 判据补（评审修订 2026-11-19）：D7 #8 像素帽行为级负例——超 100 MP 真实
// 穿过 route()：identify 子 run SUCCEEDED 留证（帽定案在其 stdout 解析），
// 链内 FAILED + ROUTER_PIXEL_BUDGET_EXCEEDED（D8 表对账）
// ---------------------------------------------------------------------------

#[test]
fn pixel_budget_exceeded_chain_fails_with_identify_run_evidence() {
    let provider = provider();
    if !require_capability(
        &provider,
        "pixel_budget_exceeded_chain_fails_with_identify_run_evidence",
        "image.magick",
    ) {
        return;
    }
    let engine = fresh_engine(&[]);
    let ws_root = workspace_root("pixcap");
    let router = router_for(&ws_root);
    let adapter = adapter_for(&ws_root);
    // 入库合成 fixture：完整合法 PNG（11000x11000 = 121 MP，IHDR 宣称超帽，
    // IDAT 完整——`identify -ping` 须读完整 zlib 流才成功；仅 IHDR 头部的
    // 截断 PNG identify exit 1 走 EXECUTION 到不了像素帽门，实测定形）。
    let bytes = smoke("oversized-pixels.png");
    let outcome = router
        .route(
            &engine,
            &adapter,
            Some(&provider),
            &bytes,
            "oversized-pixels.png",
            "png",
        )
        .expect("image channel accepted");
    assert_eq!(
        outcome.status,
        document_router::RouteStatus::Failed,
        "超帽必须链内 FAILED"
    );
    let reject = outcome.reject.as_ref().expect("D8 reject");
    assert_eq!(reject.category, "INPUT");
    assert_eq!(reject.code, "ROUTER_PIXEL_BUDGET_EXCEEDED");
    assert!(
        reject.message.contains("121000000"),
        "超帽像素如实进错误消息（D8）：{}",
        reject.message
    );
    // identify 子 run 行留证：SUCCEEDED 真实终态（像素帽在其输出解析后定案）。
    assert_eq!(outcome.runs.len(), 1, "identify 单步留证");
    assert_eq!(outcome.runs[0].tool_id, "rip.doc.image.meta");
    assert_eq!(
        run_row_status(&engine, &outcome.runs[0].run_id).as_deref(),
        Some("SUCCEEDED"),
        "identify 行真实终态"
    );
    let line = stdout_log(&ws_root, &outcome.runs[0].run_id);
    assert!(
        line.contains("11000x11000"),
        "identify 尺寸报告（帽定案输入）在 stdout.log 留证：{line}"
    );
    let _ = std::fs::remove_dir_all(&ws_root);
}

// ---------------------------------------------------------------------------
// 判据补（评审修订 2026-11-19）：D7 #7 页数帽行为级负例——2001 页真实穿过
// route()：pdfinfo 子 run SUCCEEDED 留证，链内 FAILED +
// ROUTER_PAGE_BUDGET_EXCEEDED，pdftotext 不再拉起
// ---------------------------------------------------------------------------

/// 本地构造 N 页最小合法 PDF（真实页树对象 + 正确 xref，页对象无 /Contents
/// ——空页合法，pdfinfo 只解析页树；测试内纯代码构造，零入库零下载）。
fn multi_page_pdf(pages: u64) -> Vec<u8> {
    let mut objs: BTreeMap<u64, Vec<u8>> = BTreeMap::new();
    objs.insert(1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    let kids: Vec<String> = (0..pages).map(|i| format!("{} 0 R", 10 + i)).collect();
    objs.insert(
        2,
        format!(
            "<< /Type /Pages /Count {pages} /Kids [{}] >>",
            kids.join(" ")
        )
        .into_bytes(),
    );
    for i in 0..pages {
        objs.insert(
            10 + i,
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".to_vec(),
        );
    }
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets: BTreeMap<u64, usize> = BTreeMap::new();
    for (num, body) in &objs {
        offsets.insert(*num, out.len());
        out.extend_from_slice(format!("{num} 0 obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_off = out.len();
    let size = objs.keys().last().unwrap() + 1;
    out.extend_from_slice(format!("xref\n0 {size}\n").as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for num in 1..size {
        match offsets.get(&num) {
            Some(off) => out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes()),
            None => out.extend_from_slice(b"0000000000 65535 f \n"),
        }
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{xref_off}\n%%EOF\n")
            .as_bytes(),
    );
    out
}

#[test]
fn page_budget_exceeded_chain_fails_with_pdfinfo_run_evidence() {
    let provider = provider();
    if !require_capability(
        &provider,
        "page_budget_exceeded_chain_fails_with_pdfinfo_run_evidence",
        "pdf.pdfinfo",
    ) {
        return;
    }
    let engine = fresh_engine(&[]);
    let ws_root = workspace_root("pagecap");
    let router = router_for(&ws_root);
    let adapter = adapter_for(&ws_root);
    let bytes = multi_page_pdf(MAX_PAGES + 1);
    let outcome = router
        .route(
            &engine,
            &adapter,
            Some(&provider),
            &bytes,
            "pages2001.pdf",
            "pdf",
        )
        .expect("pdf channel accepted");
    assert_eq!(
        outcome.status,
        document_router::RouteStatus::Failed,
        "超帽必须链内 FAILED"
    );
    let reject = outcome.reject.as_ref().expect("D8 reject");
    assert_eq!(reject.category, "INPUT");
    assert_eq!(reject.code, "ROUTER_PAGE_BUDGET_EXCEEDED");
    assert!(
        reject.message.contains("2001"),
        "超帽页数如实进错误消息（D8）：{}",
        reject.message
    );
    // pdfinfo 子 run 行留证：SUCCEEDED 真实终态（页数帽在其报告解析后定案）；
    // pdftotext 未拉起（帽中止先行，超帽输入不再进文本抽取面）。
    assert_eq!(
        outcome.runs.len(),
        1,
        "pdfinfo 单步留证（帽中止，pdftotext 未提交）"
    );
    assert_eq!(outcome.runs[0].tool_id, "rip.doc.pdf.info");
    assert_eq!(
        run_row_status(&engine, &outcome.runs[0].run_id).as_deref(),
        Some("SUCCEEDED"),
        "pdfinfo 行真实终态"
    );
    let report = stdout_log(&ws_root, &outcome.runs[0].run_id);
    assert!(
        report
            .lines()
            .any(|l| l.trim_start().starts_with("Pages:") && l.contains("2001")),
        "pdfinfo Pages: 2001 实测报告留证：{report}"
    );
    let _ = std::fs::remove_dir_all(&ws_root);
}

// ---------------------------------------------------------------------------
// 判据补（评审修订 2026-11-19）：页数帽定案输入缺席（pdfinfo Pages 未解析）
// → warnings 如实记录零静默（与图像通道 identify 尺寸未解析同款口径）；
// 链继续但不伪装帽已定案
// ---------------------------------------------------------------------------

#[test]
fn pdfinfo_pages_unparsed_carries_honest_warning() {
    let provider = provider();
    if !require_capability(
        &provider,
        "pdfinfo_pages_unparsed_carries_honest_warning",
        "pdf.pdftotext",
    ) {
        return;
    }
    // harness 注入（判据 5 同形态）：rip.doc.pdf.info harness 克隆——
    // binary = /usr/bin/true（stdout 恒空 → PdfinfoMeta::parse 无 Pages 行
    // → meta.pages=None），版本 0.9.0 supersede 真件（Registry 同 id 升版本
    // = current 翻转，register 语义实测）；RouterManifests 以 [harness_dir,
    // real_dir] 序装载同 id 优先 harness。钉值取本机实测（harness 载具非
    // 仓库钉值面）。
    let true_sha = tool_registry::hash::sha256_hex(
        &std::fs::read("/usr/bin/true").expect("/usr/bin/true (coreutils) present"),
    );
    let harness = json!({
        "schema_version": "1",
        "id": "rip.doc.pdf.info",
        "name": "PDF Info Probe (pages-unparsed harness)",
        "version": "0.9.0",
        "kind": "tool",
        "family": "document",
        "description": "harness：stdout 恒空（/usr/bin/true），pdfinfo Pages 未解析形态载具（页数帽定案输入缺席的如实告警面，评审修订 2026-11-19）",
        "runtime": {
            "type": "native_rust",
            "binary": "/usr/bin/true",
            "binary_sha256": true_sha,
            "argv_template": [],
            "result_mode": "file"
        },
        "input_schema": {
            "type": "object",
            "additionalProperties": false,
            "required": ["input_path"],
            "properties": {
                "input_path": {"type": "string", "minLength": 1}
            }
        },
        "output_schema": {
            "type": "object",
            "additionalProperties": false,
            "required": ["exit_code", "stdout_bytes", "stdout_capped", "artifacts"],
            "properties": {
                "exit_code": {"type": "integer", "const": 0},
                "stdout_bytes": {"type": "integer", "minimum": 0},
                "stdout_capped": {"type": "boolean"},
                "artifacts": {"type": "array", "items": {"type": "object"}}
            }
        },
        "resources": {"cpu": "1", "memory": "64Mi", "timeout_seconds": 30, "max_output_bytes": 1048576},
        "network": {"mode": "none"},
        "secrets": [],
        "permission_level": "T0",
        "idempotent": true,
        "health": {"probe": {"type": "none"}, "on_run_recover": false},
        "provenance": {"origin": "rip-native", "license": "repo-internal", "dependencies": [], "tested": "pages-unparsed harness"}
    });
    let harness_dir =
        std::env::temp_dir().join(format!("di1-pages-unparsed-harness-{}", std::process::id()));
    std::fs::create_dir_all(&harness_dir).unwrap();
    std::fs::write(
        harness_dir.join("rip.doc.pdf.info.json"),
        serde_json::to_string_pretty(&harness).unwrap(),
    )
    .unwrap();
    let engine = {
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
        for id in [
            "rip.doc.pdf.info",
            "rip.doc.pdf.text",
            "rip.doc.office.to_pdf",
            "rip.doc.image.meta",
        ] {
            let text =
                std::fs::read_to_string(repo_root().join(format!("manifests/tools/{id}.json")))
                    .unwrap();
            registry
                .register(&serde_json::from_str(&text).unwrap())
                .unwrap_or_else(|e| panic!("register {id}: {e}"));
        }
        // harness 克隆升版本 supersede 真件 → current = harness（0.9.0 > 0.1.0）。
        registry
            .register(&harness)
            .expect("harness clone supersedes real manifest");
        Engine::open(pool, registry)
    };
    let ws_root = workspace_root("pagesunparsed");
    let router = document_router::Router::new(
        document_router::RouterConfig {
            workspace_root: ws_root.clone(),
        },
        // harness 目录在前：RouterManifests 同 id 首见优先（from_dirs 语义）。
        document_router::RouterManifests::from_dirs(&[
            harness_dir.clone(),
            repo_root().join("manifests/tools"),
        ])
        .expect("manifests load with harness override"),
    );
    let adapter = adapter_for(&ws_root);
    let bytes = smoke("sample.pdf");
    let outcome = router
        .route(
            &engine,
            &adapter,
            Some(&provider),
            &bytes,
            "sample.pdf",
            "pdf",
        )
        .expect("pdf channel accepted");
    assert_eq!(
        outcome.status,
        document_router::RouteStatus::Succeeded,
        "Pages 未解析按如实告警口径继续（不中止不编造）：{:?}",
        outcome.reject
    );
    let unified = outcome.unified.as_ref().unwrap();
    let warnings = unified["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("did not parse a Pages line")),
        "warnings 记录页数帽定案输入缺席（零静默）：{warnings:?}"
    );
    assert!(
        unified["metadata"].get("pages").is_none(),
        "缺 Pages 行零编造（metadata 不产 pages 键）"
    );
    assert_eq!(outcome.runs.len(), 2, "harness pdfinfo + 真实 pdftotext");
    assert_eq!(
        run_row_status(&engine, &outcome.runs[0].run_id).as_deref(),
        Some("SUCCEEDED"),
        "harness 步真实终态"
    );
    let _ = std::fs::remove_dir_all(&ws_root);
    let _ = std::fs::remove_dir_all(&harness_dir);
}

// ===========================================================================
// DI-2 三通道 e2e（DI-2-TASK §5 判据 1-12 逐项；负例为硬性红线）
// ===========================================================================

use document_router::channel::{
    ARCHIVE_MAX_ENTRIES, MEDIA_DURATION_MAX_SECONDS, MEDIA_FRAME_PIXELS_MAX, OCR_PIXELS_MAX,
};

/// 判据 1：OCR 通道端到端——text-ocr.png + 请求 language=en → identify +
/// tesseract 两子 run SUCCEEDED；统一输出 channel="ocr"、text="HELLO OCR 123"
/// （TSV 重组）、metadata.ocr{language:"eng", requested_language:"en",
/// words=3, mean_conf>90}、page_anchors 单页锚。
#[test]
fn di2_ocr_channel_end_to_end_explicit_request_bit() {
    let test = "di2_ocr_channel_end_to_end_explicit_request_bit";
    let provider = provider();
    if !require_capability(&provider, test, "image.magick")
        || !require_capability(&provider, test, "ocr.tesseract")
    {
        return;
    }
    let engine = fresh_engine(&[]);
    let ws_root = workspace_root("di2-ocr");
    let router = router_for(&ws_root);
    let adapter = adapter_for(&ws_root);
    let bytes = smoke("text-ocr.png");
    let outcome = router
        .route_ocr(
            &engine,
            &adapter,
            Some(&provider),
            &bytes,
            "text-ocr.png",
            "png",
            Some("en"),
        )
        .expect("ocr channel accepted");
    assert_eq!(
        outcome.status,
        document_router::RouteStatus::Succeeded,
        "OCR 链失败：{:?}",
        outcome.reject
    );
    assert_eq!(outcome.runs.len(), 2, "identify 门步 + tesseract 步");
    assert_eq!(outcome.runs[0].tool_id, "rip.doc.image.meta");
    assert_eq!(outcome.runs[1].tool_id, "rip.doc.image.ocr");
    for run in &outcome.runs {
        assert_eq!(
            run_row_status(&engine, &run.run_id).as_deref(),
            Some("SUCCEEDED"),
            "子 run {} 行真实终态",
            run.run_id
        );
    }
    let unified = outcome.unified.as_ref().unwrap();
    assert_eq!(unified["channel"], json!("ocr"));
    assert_eq!(unified["text"], json!("HELLO OCR 123"), "TSV 重组正文");
    assert_eq!(unified["metadata"]["format"], json!("PNG"));
    assert_eq!(unified["metadata"]["dimensions"]["width"], json!(460));
    assert_eq!(unified["metadata"]["dimensions"]["height"], json!(80));
    let ocr = &unified["metadata"]["ocr"];
    assert_eq!(ocr["language"], json!("eng"), "tesseract -l 实际参数");
    assert_eq!(ocr["requested_language"], json!("en"), "路由输入原值");
    assert_eq!(ocr["words"], json!(3));
    assert!(
        ocr["mean_conf"].as_f64().unwrap() > 90.0,
        "mean_conf > 90（fixture 实测 93.6）：{:?}",
        ocr
    );
    assert_eq!(ocr["low_conf_words"], json!(0));
    let anchors = unified["page_anchors"].as_array().unwrap();
    assert_eq!(anchors.len(), 1, "单图单页锚");
    assert_eq!(anchors[0]["page"], json!(1));
    assert_eq!(anchors[0]["char_start"], json!(0));
    assert_eq!(
        anchors[0]["char_end"],
        json!(unified["text"].as_str().unwrap().chars().count())
    );
    // 判据 4 同款 trace 自锚定面：两子 run trace_id = run_id。
    for run in &outcome.runs {
        assert_eq!(
            engine.trace_id_of(&run.run_id).unwrap().as_deref(),
            Some(run.run_id.as_str())
        );
    }
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// 判据 2：OCR 缺省语言——无请求位语言（None）→ `-l chi_sim+eng`（目录双
/// 语言放行）；同一 fixture 文本一致；requested_language="auto"。
#[test]
fn di2_ocr_default_language_routes_chi_sim_plus_eng() {
    let test = "di2_ocr_default_language_routes_chi_sim_plus_eng";
    let provider = provider();
    if !require_capability(&provider, test, "image.magick")
        || !require_capability(&provider, test, "ocr.tesseract")
    {
        return;
    }
    let engine = fresh_engine(&[]);
    let ws_root = workspace_root("di2-ocr-auto");
    let router = router_for(&ws_root);
    let adapter = adapter_for(&ws_root);
    let bytes = smoke("text-ocr.png");
    let outcome = router
        .route_ocr(
            &engine,
            &adapter,
            Some(&provider),
            &bytes,
            "text-ocr.png",
            "png",
            None,
        )
        .expect("ocr channel accepted (auto)");
    assert_eq!(outcome.status, document_router::RouteStatus::Succeeded);
    let unified = outcome.unified.as_ref().unwrap();
    assert_eq!(
        unified["text"],
        json!("HELLO OCR 123"),
        "双语言同 fixture 同文"
    );
    let ocr = &unified["metadata"]["ocr"];
    assert_eq!(ocr["language"], json!("chi_sim+eng"), "缺省双语言组合");
    assert_eq!(ocr["requested_language"], json!("auto"));
    assert_eq!(outcome.runs.len(), 2);
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// 判据 3：chi_tra 请求显式 error——请求 zh-Hant → 前置门 Err：
/// category=DEPENDENCY、code=ROUTER_OCR_LANG_MISSING、零 run 行、消息注明
/// 禁静默回退与补装属用户决策；chi_sim 兜底面零出现。
#[test]
fn di2_ocr_chi_tra_request_rejected_at_gate_without_rows() {
    let test = "di2_ocr_chi_tra_request_rejected_at_gate_without_rows";
    let provider = provider();
    if !require_capability(&provider, test, "image.magick")
        || !require_capability(&provider, test, "ocr.tesseract")
    {
        return;
    }
    let engine = fresh_engine(&[]);
    let ws_root = workspace_root("di2-ocr-chitra");
    let router = router_for(&ws_root);
    let adapter = adapter_for(&ws_root);
    let bytes = smoke("text-ocr.png");
    for requested in ["zh-Hant", "chi_tra", "zh-TW"] {
        let err = router
            .route_ocr(
                &engine,
                &adapter,
                Some(&provider),
                &bytes,
                "text-ocr.png",
                "png",
                Some(requested),
            )
            .expect_err("chi_tra-family request must be rejected at the language gate");
        assert_eq!(
            err.category, "DEPENDENCY",
            "语言包缺失归因 DEPENDENCY（D6）"
        );
        assert_eq!(err.code, "ROUTER_OCR_LANG_MISSING");
        assert!(!err.retryable);
        assert!(
            err.message.contains("user decision"),
            "补装属用户决策：{}",
            err.message
        );
        assert!(
            err.message.contains("silent fallback"),
            "禁静默回退注明：{}",
            err.message
        );
        assert!(
            err.message.contains("chi_tra"),
            "缺失语言如实进消息：{}",
            err.message
        );
        assert!(
            !err.message.contains("fallback to chi_sim"),
            "chi_sim 兜底面零出现：{}",
            err.message
        );
    }
    // 零 run 行（前置门语义——拒绝发生在任何子 run 提交之前）。
    assert_eq!(run_row_count(&engine), 0, "chi_tra 前置门拒绝零 run 行");
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// 判据 4：OCR 像素帽——oversized-pixels.png（121 MP）→ OCR 管线 identify
/// 行 SUCCEEDED 留证 + 链内 FAILED ROUTER_PIXEL_BUDGET_EXCEEDED（消息含
/// 121000000 与 50 MP OCR 帽）；tesseract 未拉起。
#[test]
fn di2_ocr_pixel_budget_chain_fails_with_identify_evidence() {
    let test = "di2_ocr_pixel_budget_chain_fails_with_identify_evidence";
    let provider = provider();
    if !require_capability(&provider, test, "image.magick")
        || !require_capability(&provider, test, "ocr.tesseract")
    {
        return;
    }
    let engine = fresh_engine(&[]);
    let ws_root = workspace_root("di2-ocr-pixcap");
    let router = router_for(&ws_root);
    let adapter = adapter_for(&ws_root);
    let bytes = smoke("oversized-pixels.png");
    let outcome = router
        .route_ocr(
            &engine,
            &adapter,
            Some(&provider),
            &bytes,
            "oversized-pixels.png",
            "png",
            Some("en"),
        )
        .expect("ocr channel accepted");
    assert_eq!(outcome.status, document_router::RouteStatus::Failed);
    let reject = outcome.reject.as_ref().expect("D8 reject");
    assert_eq!(reject.category, "INPUT");
    assert_eq!(reject.code, "ROUTER_PIXEL_BUDGET_EXCEEDED", "码沿用 DI-1");
    assert!(
        reject.message.contains("121000000"),
        "超帽像素如实进消息：{}",
        reject.message
    );
    assert!(
        reject.message.contains(&OCR_PIXELS_MAX.to_string()),
        "OCR 50 MP 帽注明：{}",
        reject.message
    );
    // identify 单步留证 SUCCEEDED；tesseract 未拉起。
    assert_eq!(outcome.runs.len(), 1, "identify 单步留证");
    assert_eq!(outcome.runs[0].tool_id, "rip.doc.image.meta");
    assert_eq!(
        run_row_status(&engine, &outcome.runs[0].run_id).as_deref(),
        Some("SUCCEEDED")
    );
    let line = stdout_log(&ws_root, &outcome.runs[0].run_id);
    assert!(line.contains("11000x11000"));
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// 判据 5：畸形图像——PNG 魔数 + 垃圾体 → identify 子 run exit!=0 →
/// EXECUTION 透传 FAILED（子 run 行留证），tesseract 未拉起。
#[test]
fn di2_ocr_malformed_image_execution_passthrough_no_tesseract() {
    let test = "di2_ocr_malformed_image_execution_passthrough_no_tesseract";
    let provider = provider();
    if !require_capability(&provider, test, "image.magick")
        || !require_capability(&provider, test, "ocr.tesseract")
    {
        return;
    }
    let engine = fresh_engine(&[]);
    let ws_root = workspace_root("di2-ocr-malformed");
    let router = router_for(&ws_root);
    let adapter = adapter_for(&ws_root);
    // PNG 魔数 + 垃圾体（嗅探 Png 命中，identify 解码失败 exit != 0）。
    let mut malformed = vec![0x89u8, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    malformed.extend_from_slice(b"garbage body not a real png \xff\xfe\x01\x02");
    let outcome = router
        .route_ocr(
            &engine,
            &adapter,
            Some(&provider),
            &malformed,
            "malformed.png",
            "png",
            Some("en"),
        )
        .expect("ocr channel accepted at gate");
    assert_eq!(outcome.status, document_router::RouteStatus::Failed);
    let reject = outcome
        .reject
        .as_ref()
        .expect("EXECUTION passthrough reject");
    assert_eq!(reject.category, "EXECUTION", "identify 失败透传（D8）");
    assert_eq!(outcome.runs.len(), 1, "identify 单步留证");
    assert_eq!(outcome.runs[0].tool_id, "rip.doc.image.meta");
    assert_eq!(
        run_row_status(&engine, &outcome.runs[0].run_id).as_deref(),
        Some("FAILED"),
        "identify 子 run 行真实终态 FAILED"
    );
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// 判据 6：媒体端到端（音频）——tone.wav → ffprobe 单子 run SUCCEEDED；
/// metadata.media{format_name:"wav", duration_s≈0.5, streams 含 pcm_s16le}；
/// warnings 记录 no-video-stream 分支；抽帧步未提交。
#[test]
fn di2_media_tone_wav_metadata_only_no_frames_step() {
    let test = "di2_media_tone_wav_metadata_only_no_frames_step";
    let provider = provider();
    if !require_capability(&provider, test, "media.ffprobe") {
        return;
    }
    let engine = fresh_engine(&[]);
    let ws_root = workspace_root("di2-media-wav");
    let router = router_for(&ws_root);
    let adapter = adapter_for(&ws_root);
    let bytes = smoke("tone.wav");
    let outcome = router
        .route(
            &engine,
            &adapter,
            Some(&provider),
            &bytes,
            "tone.wav",
            "wav",
        )
        .expect("media channel accepted");
    assert_eq!(
        outcome.status,
        document_router::RouteStatus::Succeeded,
        "媒体链失败：{:?}",
        outcome.reject
    );
    assert_eq!(outcome.runs.len(), 1, "ffprobe 单子 run（抽帧步未提交）");
    assert_eq!(outcome.runs[0].tool_id, "rip.media.probe");
    assert_eq!(
        run_row_status(&engine, &outcome.runs[0].run_id).as_deref(),
        Some("SUCCEEDED")
    );
    let unified = outcome.unified.as_ref().unwrap();
    assert_eq!(unified["channel"], json!("media"));
    let media = &unified["metadata"]["media"];
    assert_eq!(media["format_name"], json!("wav"));
    assert!(
        (media["duration_s"].as_f64().unwrap() - 0.5).abs() < 0.01,
        "duration_s ≈ 0.5（ffprobe 字符串形态数值化）：{:?}",
        media
    );
    let streams = media["streams"].as_array().unwrap();
    assert!(
        streams
            .iter()
            .any(|s| s["codec_name"] == json!("pcm_s16le")),
        "streams 含 pcm_s16le：{:?}",
        streams
    );
    let warnings = unified["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("no video stream")),
        "warnings 记录 no-video-stream 分支（诚实分支）：{warnings:?}"
    );
    assert!(
        unified["metadata"].get("frames").is_none(),
        "纯音频 metadata-only：frames 块不在位"
    );
    assert_eq!(unified["text"], json!(""));
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// 判据 7：媒体端到端（视频）——small.mp4（运行时生成）→ ffprobe + ffmpeg
/// 两子 run；out/ 内帧 PNG（2s@fps2 = 4 帧 ≤ 32 帽）；metadata.frames
/// {extracted, capped:false}；stdout.log JSON 有 format.duration。
#[test]
fn di2_media_video_frames_extracted_with_budget() {
    let test = "di2_media_video_frames_extracted_with_budget";
    let provider = provider();
    if !require_capability(&provider, test, "media.ffprobe")
        || !require_capability(&provider, test, "media.ffmpeg")
    {
        return;
    }
    // 运行时生成 small.mp4（ffmpeg lavfi testsrc + libx264 显式编码器；
    // mp4 容器时间戳/编码器元数据非确定 → 行为断言不字节断言，DI-2-TASK §4）。
    let gen_dir = std::env::temp_dir().join(format!("di2-mp4-{}", std::process::id()));
    std::fs::create_dir_all(&gen_dir).unwrap();
    let mp4 = gen_dir.join("small.mp4");
    let ok = std::process::Command::new("/usr/bin/ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=2:size=128x64:rate=10",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-y",
            mp4.to_str().unwrap(),
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !ok || !mp4.exists() {
        println!("SKIP {test}: local ffmpeg could not generate small.mp4 fixture");
        let _ = std::fs::remove_dir_all(&gen_dir);
        return;
    }
    let bytes = std::fs::read(&mp4).unwrap();
    let engine = fresh_engine(&[]);
    let ws_root = workspace_root("di2-media-mp4");
    let router = router_for(&ws_root);
    let adapter = adapter_for(&ws_root);
    let outcome = router
        .route(
            &engine,
            &adapter,
            Some(&provider),
            &bytes,
            "small.mp4",
            "mp4",
        )
        .expect("media channel accepted");
    assert_eq!(
        outcome.status,
        document_router::RouteStatus::Succeeded,
        "媒体视频链失败：{:?}",
        outcome.reject
    );
    assert_eq!(outcome.runs.len(), 2, "ffprobe + ffmpeg 两子 run");
    assert_eq!(outcome.runs[0].tool_id, "rip.media.probe");
    assert_eq!(outcome.runs[1].tool_id, "rip.media.frames");
    for run in &outcome.runs {
        assert_eq!(
            run_row_status(&engine, &run.run_id).as_deref(),
            Some("SUCCEEDED")
        );
    }
    // 探测步 stdout.log JSON 有 format.duration。
    let probe_log = stdout_log(&ws_root, &outcome.runs[0].run_id);
    let probe_json: Value =
        serde_json::from_str(probe_log.trim()).expect("ffprobe stdout.log is JSON");
    assert!(
        probe_json["format"]["duration"].is_string(),
        "format.duration 字符串形态实测锚：{:?}",
        probe_json["format"]
    );
    // 抽帧步产物登记（out/frame-NNN.png Artifact 面）。
    let frames_run = &outcome.runs[1];
    let frame_artifacts: Vec<&Value> = frames_run.outcome.envelope.result.as_ref().unwrap()
        ["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["name"].as_str().unwrap_or("").starts_with("frame-"))
        .collect();
    assert_eq!(
        frame_artifacts.len(),
        4,
        "2s@fps2 = 4 帧（≤ 32 帽）：{:?}",
        frame_artifacts
            .iter()
            .map(|a| a["name"].clone())
            .collect::<Vec<_>>()
    );
    let unified = outcome.unified.as_ref().unwrap();
    assert_eq!(unified["channel"], json!("media"));
    let frames = &unified["metadata"]["frames"];
    assert_eq!(frames["extracted"], json!(4));
    assert_eq!(frames["capped"], json!(false));
    assert_eq!(frames["fps"], json!(2));
    assert_eq!(frames["pattern"], json!("frame-%03d.png"));
    let media = &unified["metadata"]["media"];
    assert_eq!(
        media["format_name"],
        json!("mov,mp4,m4a,3gp,3g2,mj2"),
        "format_name 逗号列表原样承载"
    );
    let _ = std::fs::remove_dir_all(&ws_root);
    let _ = std::fs::remove_dir_all(&gen_dir);
}

/// 判据 8：超时长媒体——400 s WAV（测试内构造，6.4 MB 内存字节不落盘全量）
/// → ffprobe 行 SUCCEEDED 留证 + 链内 FAILED ROUTER_MEDIA_DURATION_EXCEEDED；
/// ffmpeg 未拉起。
#[test]
fn di2_media_duration_exceeded_chain_fails_with_probe_evidence() {
    let test = "di2_media_duration_exceeded_chain_fails_with_probe_evidence";
    let provider = provider();
    if !require_capability(&provider, test, "media.ffprobe") {
        return;
    }
    // 400 s 8kHz 16-bit 零 PCM WAV：data 长度字段 = 6.4 MB，内存组装不落盘。
    let data_len: u32 = 8000 * 2 * 400;
    let mut wav = b"RIFF".to_vec();
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&8000u32.to_le_bytes());
    wav.extend_from_slice(&16000u32.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.extend(std::iter::repeat_n(0u8, data_len as usize));
    assert_eq!(wav.len(), 44 + data_len as usize);
    let engine = fresh_engine(&[]);
    let ws_root = workspace_root("di2-media-long");
    let router = router_for(&ws_root);
    let adapter = adapter_for(&ws_root);
    let outcome = router
        .route(&engine, &adapter, Some(&provider), &wav, "long.wav", "wav")
        .expect("media channel accepted at gate");
    assert_eq!(outcome.status, document_router::RouteStatus::Failed);
    let reject = outcome.reject.as_ref().expect("D8 reject");
    assert_eq!(reject.category, "INPUT");
    assert_eq!(reject.code, "ROUTER_MEDIA_DURATION_EXCEEDED");
    assert!(
        reject.message.contains("400"),
        "超帽时长如实进消息：{}",
        reject.message
    );
    assert!(
        reject
            .message
            .contains(&MEDIA_DURATION_MAX_SECONDS.to_string()),
        "300 s 帽在消息：{}",
        reject.message
    );
    // ffprobe 行 SUCCEEDED 留证；ffmpeg 未拉起（runs.len()==1）。
    assert_eq!(outcome.runs.len(), 1, "probe 单步留证，frames 步未提交");
    assert_eq!(outcome.runs[0].tool_id, "rip.media.probe");
    assert_eq!(
        run_row_status(&engine, &outcome.runs[0].run_id).as_deref(),
        Some("SUCCEEDED")
    );
    let _ = std::fs::remove_dir_all(&ws_root);
}

/// 评审修订 2026-11-20：媒体门 4 单帧像素帽 16 MP 行为级负例（媒体五门序
/// 中此前唯一无可运行证据的门——时长门有判据 8、视频流分支有判据 6、帧数
/// 有判据 7、畸形透传有判据 9）。载具 = ffprobe stdout harness 免编码
/// （pdfinfo_pages_unparsed 先例同形态）：rip.media.probe 克隆 binary =
/// 自持 stub 脚本，stdout 恒为 4100x4100（16 810 000 像素 > 16 MP 帽）单
/// 视频流 ffprobe JSON——streams[].width/height 整数形态命中 FfprobeMeta
/// 解析面（评审以 ffmpeg 实造 4100x4100 证实负例类真实可产，本测免编码钉
/// 门触发路径）；时长 1 s 不触时长门（判据 8 同款门序前提）。
///
/// 零 capability 守卫：载具 stub 自持（/bin/sh + heredoc），链条仅执行
/// probe 单步（门中止使 ffmpeg 步结构性不提交），真 ffprobe/ffmpeg 零依赖
/// ——异机无 ffprobe 亦应跑（对照 require_capability 只覆盖真二进制面）。
#[test]
fn di2_media_pixel_budget_exceeded_chain_fails_with_probe_evidence() {
    // stub：stdout 恒为 ffprobe JSON（width/height 整数形态 + duration
    // 字符串形态，D3 实测冻结形态同构）；exit 0。纯 shell 内建 echo——
    // adapter D4 白名单 PATH=钉值二进制所在目录（ws.root），外部命令
    // （cat/printf）结构性不可用，env_echo.sh fixture 同纪律。
    let probe_json = r#"{"streams":[{"index":0,"codec_name":"h264","codec_type":"video","width":4100,"height":4100}],"format":{"format_name":"mov,mp4,m4a,3gp,3g2,mj2","duration":"1.000000","bit_rate":"128000","size":"123456"}}"#;
    let ws_root = workspace_root("di2-media-pixcap");
    let stub = ws_root.join("ffprobe-pixel-harness.sh");
    std::fs::write(&stub, format!("#!/bin/sh\necho '{probe_json}'\n")).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    // harness 克隆（同 id rip.media.probe 升版本 0.9.0 supersede 真件；
    // capability_ref 移除——载具零真 ffprobe 依赖，钉值自持 stub 实时 sha
    // 写入使 identity 校验恒过，DI-1 判据 5 missingcap 先例同形态）。
    let mut doc: Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("manifests/tools/rip.media.probe.json")).unwrap(),
    )
    .unwrap();
    doc["version"] = json!("0.9.0");
    doc["name"] = json!("Media Stream Probe (pixel-cap harness)");
    doc["description"] = json!("harness：stdout 恒为 4100x4100 单视频流 ffprobe JSON（免编码载具），媒体单帧 16 MP 像素帽门定案输入（评审修订 2026-11-20）");
    doc["runtime"]["binary"] = json!(stub.to_str().unwrap());
    doc["runtime"]["binary_sha256"] = json!(tool_registry::hash::sha256_hex(
        &std::fs::read(&stub).unwrap()
    ));
    doc["runtime"]["argv_template"] = json!([]);
    doc["runtime"]
        .as_object_mut()
        .unwrap()
        .remove("capability_ref");
    doc["resources"] = json!({
        "cpu": "1",
        "memory": "64Mi",
        "timeout_seconds": 30,
        "max_output_bytes": 1048576
    });
    let harness_dir =
        std::env::temp_dir().join(format!("di2-pixel-harness-{}", std::process::id()));
    std::fs::create_dir_all(&harness_dir).unwrap();
    std::fs::write(
        harness_dir.join("rip.media.probe.json"),
        serde_json::to_string_pretty(&doc).unwrap(),
    )
    .unwrap();
    let engine = {
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
            let text =
                std::fs::read_to_string(repo_root().join(format!("manifests/tools/{id}.json")))
                    .unwrap();
            registry
                .register(&serde_json::from_str(&text).unwrap())
                .unwrap_or_else(|e| panic!("register {id}: {e}"));
        }
        // harness 克隆升版本 supersede 真件 → current = harness。
        registry
            .register(&doc)
            .expect("harness clone supersedes real manifest");
        Engine::open(pool, registry)
    };
    // harness 目录在前：RouterManifests 同 id 首见优先（from_dirs 语义）。
    let router = document_router::Router::new(
        document_router::RouterConfig {
            workspace_root: ws_root.clone(),
        },
        document_router::RouterManifests::from_dirs(&[
            harness_dir.clone(),
            repo_root().join("manifests/tools"),
        ])
        .expect("manifests load with harness override"),
    );
    let adapter = adapter_for(&ws_root);
    let provider = provider();
    // tone.wav：真 WAV 魔数 → 嗅探 Wav → 媒体通道（stub 不读输入，字节面
    // 仅承载嗅探定案）。
    let bytes = smoke("tone.wav");
    let outcome = router
        .route(
            &engine,
            &adapter,
            Some(&provider),
            &bytes,
            "tone.wav",
            "wav",
        )
        .expect("media channel accepted at gate");
    assert_eq!(
        outcome.status,
        document_router::RouteStatus::Failed,
        "16.81 MP 单帧必须链内 FAILED"
    );
    let reject = outcome.reject.as_ref().expect("D8 reject");
    assert_eq!(reject.category, "INPUT");
    assert_eq!(reject.code, "ROUTER_MEDIA_PIXEL_BUDGET_EXCEEDED");
    assert!(
        reject.message.contains("16810000"),
        "超帽单帧像素（4100x4100）如实进消息：{}",
        reject.message
    );
    assert!(
        reject.message.contains(&MEDIA_FRAME_PIXELS_MAX.to_string()),
        "16 MP 帽在消息：{}",
        reject.message
    );
    // probe 行 SUCCEEDED 留证（帽在其 stdout 解析后定案）；ffmpeg 未拉起
    //（runs.len()==1，frames 步结构性未提交）。
    assert_eq!(outcome.runs.len(), 1, "probe 单步留证，frames 步未提交");
    assert_eq!(outcome.runs[0].tool_id, "rip.media.probe");
    assert_eq!(
        run_row_status(&engine, &outcome.runs[0].run_id).as_deref(),
        Some("SUCCEEDED"),
        "probe（harness stub）行真实终态"
    );
    let line = stdout_log(&ws_root, &outcome.runs[0].run_id);
    assert!(
        line.contains("\"width\":4100") && line.contains("\"height\":4100"),
        "probe stdout 尺寸整数形态留证（帽定案输入）：{line}"
    );
    let _ = std::fs::remove_dir_all(&ws_root);
    let _ = std::fs::remove_dir_all(&harness_dir);
}

/// 判据 9：畸形媒体 / 媒体嗅探——RIFF 形态垃圾（嗅探 Wav 命中）→ ffprobe
/// exit 1 → EXECUTION 透传；gzip 字节 → 嗅探门 INPUT 拒绝零 run 行（魔数
/// 1f8b 不入表）。
#[test]
fn di2_media_malformed_execution_passthrough_and_gzip_gate() {
    let test = "di2_media_malformed_execution_passthrough_and_gzip_gate";
    let provider = provider();
    if !require_capability(&provider, test, "media.ffprobe") {
        return;
    }
    let engine = fresh_engine(&[]);
    let ws_root = workspace_root("di2-media-fake");
    let router = router_for(&ws_root);
    let adapter = adapter_for(&ws_root);
    // RIFF+WAVE 形态垃圾体：嗅探 Wav 命中 → ffprobe 拉起 → exit 1 透传。
    let fake_wav = b"RIFF\x24\x00\x00\x00WAVEgarbage-not-a-real-wave\xff\x01\x02".to_vec();
    let outcome = router
        .route(
            &engine,
            &adapter,
            Some(&provider),
            &fake_wav,
            "fake.wav",
            "wav",
        )
        .expect("media channel accepted at gate");
    assert_eq!(outcome.status, document_router::RouteStatus::Failed);
    let reject = outcome.reject.as_ref().expect("EXECUTION passthrough");
    assert_eq!(reject.category, "EXECUTION");
    assert_eq!(outcome.runs.len(), 1);
    assert_eq!(outcome.runs[0].tool_id, "rip.media.probe");
    assert_eq!(
        run_row_status(&engine, &outcome.runs[0].run_id).as_deref(),
        Some("FAILED")
    );
    // gzip 字节 → 嗅探门 INPUT 拒绝零 run 行。
    let gzip = [0x1Fu8, 0x8B, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0x02];
    let err = router
        .route(&engine, &adapter, Some(&provider), &gzip, "x.gz", "gz")
        .expect_err("gzip stays fail-closed at sniff gate");
    assert_eq!(err.category, "INPUT");
    assert_eq!(err.code, "ROUTER_UNSUPPORTED_TYPE");
    let _ = std::fs::remove_dir_all(&ws_root);
}

// ---------------------------------------------------------------------------
// 归档通道运行时生成器（判据 10/12：7z 容器内嵌创建时间戳字节非确定 →
// 行为断言不字节断言，生成器 = 被测能力自身，守卫沿用 require_capability）
// ---------------------------------------------------------------------------

/// 7z 本地生成双条目文本归档（demo.zip / demo.7z；archive.7z 能力）。
fn generated_archive(dir: &std::path::Path, seven_zip_format: bool) -> Option<Vec<u8>> {
    let a = dir.join("a.txt");
    let b = dir.join("b.txt");
    std::fs::write(&a, "alpha content\n").unwrap();
    std::fs::write(&b, "beta payload\n").unwrap();
    let out = dir.join(if seven_zip_format {
        "demo.7z"
    } else {
        "demo.zip"
    });
    let mut cmd = std::process::Command::new("/usr/bin/7z");
    cmd.arg("a");
    if !seven_zip_format {
        cmd.arg("-tzip");
    }
    cmd.arg(out.to_str().unwrap()).arg(&a).arg(&b);
    let ok = cmd.output().map(|o| o.status.success()).unwrap_or(false);
    if !ok || !out.exists() {
        println!("SKIP: local 7z could not generate demo archive");
        return None;
    }
    Some(std::fs::read(&out).unwrap())
}

/// 判据 10 + DI-1 判据 4 迁移正例：demo.zip / demo.7z（运行时生成）→
/// list + extract 两子 run 均 SUCCEEDED；解包产物落本 run out/（-oout cwd
/// 相对裁决行为级钉死）；metadata.archive 全形状；两子 run 各自独立预算行。
/// sample.zip / sample.epub（DI-1 zip_other 负例）→ 归档通道正例。
#[test]
fn di2_archive_channel_end_to_end_zip_and_sevenzip() {
    let test = "di2_archive_channel_end_to_end_zip_and_sevenzip";
    let provider = provider();
    if !require_capability(&provider, test, "archive.7z") {
        return;
    }
    let engine = fresh_engine(&[]);
    let ws_root = workspace_root("di2-archive-e2e");
    let router = router_for(&ws_root);
    let adapter = adapter_for(&ws_root);
    let gen_dir = std::env::temp_dir().join(format!("di2-archive-gen-{}", std::process::id()));
    std::fs::create_dir_all(&gen_dir).unwrap();

    for (name, ext, bytes) in [
        ("demo.zip", "zip", generated_archive(&gen_dir, false)),
        ("demo.7z", "7z", generated_archive(&gen_dir, true)),
        // DI-1 判据 4 迁移正例：zip_other 入库语料改路由归档通道（D1）。
        ("sample.zip", "zip", Some(smoke("sample.zip"))),
        ("sample.epub", "epub", Some(smoke("sample.epub"))),
    ] {
        let Some(bytes) = bytes else { return };
        let outcome = router
            .route(&engine, &adapter, Some(&provider), &bytes, name, ext)
            .unwrap_or_else(|e| panic!("{name} archive channel rejected at gate: {e:?}"));
        assert_eq!(
            outcome.status,
            document_router::RouteStatus::Succeeded,
            "{name} 归档链失败：{:?}",
            outcome.reject
        );
        assert_eq!(outcome.runs.len(), 2, "{name} list + extract 两子 run");
        assert_eq!(outcome.runs[0].tool_id, "rip.archive.list");
        assert_eq!(outcome.runs[1].tool_id, "rip.archive.extract");
        for run in &outcome.runs {
            assert_eq!(
                run_row_status(&engine, &run.run_id).as_deref(),
                Some("SUCCEEDED"),
                "{name} 子 run {} 真实终态",
                run.run_id
            );
        }
        // 解包产物落本 run out/（-oout cwd 相对裁决行为级钉死，D4）。
        let out_dir = ws_root.join(&outcome.runs[1].run_id).join("out");
        let extracted: Vec<_> = std::fs::read_dir(&out_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let n = e.file_name().to_string_lossy().to_string();
                n != "stdout.log" && n != "stderr.log"
            })
            .collect();
        assert!(
            !extracted.is_empty(),
            "{name} 解包产物落本 run out/（cwd 相对裁决）：{}",
            out_dir.display()
        );
        // 解包产物字节经信封 artifacts 面 sha256/size 登记（collect_out_artifacts）。
        let data = outcome.runs[1].outcome.envelope.result.as_ref().unwrap();
        let listed = data["artifacts"].as_array().unwrap();
        let row = outcome.runs[1]
            .outcome
            .envelope
            .artifacts
            .iter()
            .find(|a| {
                a.uri.ends_with("/out/a.txt")
                    || a.uri.ends_with("/out/docs/a.txt")
                    || a.uri.ends_with("/out/mimetype")
            })
            .or_else(|| {
                outcome.runs[1]
                    .outcome
                    .envelope
                    .artifacts
                    .iter()
                    .find(|a| a.role == "primary")
            })
            .expect("extract artifact registered");
        assert!(row.size_bytes > 0);
        assert_eq!(row.sha256.len(), 64);
        let _ = listed;
        // 统一输出 metadata.archive 全形状（清单 = 授权面记录）。
        let unified = outcome.unified.as_ref().unwrap();
        assert_eq!(unified["channel"], json!("archive"));
        let archive = &unified["metadata"]["archive"];
        assert!(archive["format"].is_string(), "{name} Type 头解析");
        assert!(archive["entries"].as_u64().unwrap() >= 1);
        assert!(archive["total_uncompressed_bytes"].as_u64().unwrap() >= 1);
        let files = archive["files"].as_array().unwrap();
        assert!(!files.is_empty(), "{name} files 清单非空");
        for f in files {
            assert!(f["path"].is_string());
            assert!(f["size"].as_u64().is_some());
        }
    }
    let _ = std::fs::remove_dir_all(&ws_root);
    let _ = std::fs::remove_dir_all(&gen_dir);
}

/// 最小合法 zip 构造器（LFH + central directory + EOCD；DI-1 multi_page_pdf
/// 先例同形态，纯代码构造零入库零下载）。通用形态按条目名列表构造（评审
/// 修订 2026-11-20：闸 1 绝对路径 / 反斜杠分支负例载具——7z l -slt 对两类
/// 名字逐字原样呈现，实测 exit 0）。
fn minimal_zip_named(names: &[&str]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    let mut offsets: Vec<u32> = Vec::new();
    for (i, name) in names.iter().enumerate() {
        offsets.push(out.len() as u32);
        // LFH：sig + ver + flags + method + time + date + crc + csize + usize
        // + namelen + extralen + name。
        out.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04]);
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        // central directory record。
        central.extend_from_slice(&[0x50, 0x4B, 0x01, 0x02]);
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&offsets[i].to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let cd_offset = out.len() as u32;
    let cd_size = central.len() as u32;
    out.extend_from_slice(&central);
    // EOCD。
    out.extend_from_slice(&[0x50, 0x4B, 0x05, 0x06]);
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(names.len() as u16).to_le_bytes());
    out.extend_from_slice(&(names.len() as u16).to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// 条目数帽负例载具（判据 11）：按数量派生 f0000.bin 形态条目名。
fn minimal_zip_entry_count(entries: usize) -> Vec<u8> {
    let names: Vec<String> = (0..entries).map(|i| format!("f{i:04}.bin")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    minimal_zip_named(&refs)
}

/// 判据 11 + DI-1 判据 4 迁移负例：归档四闸负例矩阵——evil2.zip →
/// ROUTER_ARCHIVE_ENTRY_INVALID（runs.len()==1 仅 list 行，解包未提交）；
/// bomb.zip → ROUTER_ARCHIVE_BUDGET_EXCEEDED（Size=67108864 进消息）；
/// symlink.zip → ROUTER_ARCHIVE_ENTRY_SYMLINK；513 条目 zip →
/// ROUTER_ARCHIVE_ENTRY_BUDGET_EXCEEDED；evil.zip（DI-1 穿越名语料）→
/// 闸 1；abs.zip / backslash.zip（评审修订 2026-11-20，测试内构造）→ 闸 1
/// 绝对路径 / 反斜杠两分支（evil2.zip `..` 条目先行短路使这两分支在既有
/// e2e 永不可达）。四闸任一触发解包阶段零提交。
#[test]
fn di2_archive_four_gates_negative_matrix() {
    let test = "di2_archive_four_gates_negative_matrix";
    let provider = provider();
    if !require_capability(&provider, test, "archive.7z") {
        return;
    }
    let engine = fresh_engine(&[]);
    let ws_root = workspace_root("di2-archive-gates");
    let router = router_for(&ws_root);
    let adapter = adapter_for(&ws_root);

    let cases: Vec<(&str, &str, Vec<u8>, &str, &str)> = vec![
        (
            "evil2.zip",
            "zip",
            smoke("evil2.zip"),
            "ROUTER_ARCHIVE_ENTRY_INVALID",
            "../escape.txt",
        ),
        // DI-1 判据 4 迁移负例：evil.zip 穿越名语料（嗅探门拒绝 → 归档闸 1）。
        (
            "evil.zip",
            "zip",
            smoke("evil.zip"),
            "ROUTER_ARCHIVE_ENTRY_INVALID",
            "..",
        ),
        // 评审修订 2026-11-20：闸 1 绝对路径分支——evil2.zip 的 `..` 条目
        // （第二条目）先行短路使 starts_with('/') 分支在既有 e2e 永不可达；
        // 纯绝对路径条目 zip 测试内构造（minimal_zip_named 先例同形态零入库；
        // 7z l -slt 对绝对路径条目名逐字原样呈现实测）。
        (
            "abs.zip",
            "zip",
            minimal_zip_named(&["/tmp/di2-fix/abs-escape.txt"]),
            "ROUTER_ARCHIVE_ENTRY_INVALID",
            "/tmp/di2-fix/abs-escape.txt",
        ),
        // 评审修订 2026-11-20：闸 1 反斜杠分支——contains('\\') 判据（保守
        // fail-closed，Windows 语义）。条目名 `a\b.txt` 字面字节构造；消息
        // 内条目名经 {path:?} Debug 转义呈双反斜杠字面。
        (
            "backslash.zip",
            "zip",
            minimal_zip_named(&["a\\b.txt"]),
            "ROUTER_ARCHIVE_ENTRY_INVALID",
            "a\\\\b.txt",
        ),
        (
            "bomb.zip",
            "zip",
            smoke("bomb.zip"),
            "ROUTER_ARCHIVE_BUDGET_EXCEEDED",
            "67109888",
        ),
        (
            "symlink.zip",
            "zip",
            smoke("symlink.zip"),
            "ROUTER_ARCHIVE_ENTRY_SYMLINK",
            "lrwxrwxrwx",
        ),
        (
            "entries513.zip",
            "zip",
            minimal_zip_entry_count(ARCHIVE_MAX_ENTRIES + 1),
            "ROUTER_ARCHIVE_ENTRY_BUDGET_EXCEEDED",
            "513",
        ),
    ];
    for (name, ext, bytes, want_code, want_msg) in cases {
        let outcome = router
            .route(&engine, &adapter, Some(&provider), &bytes, name, ext)
            .unwrap_or_else(|e| panic!("{name} rejected at gate (must fail in-chain): {e:?}"));
        assert_eq!(
            outcome.status,
            document_router::RouteStatus::Failed,
            "{name} 必须链内 FAILED"
        );
        let reject = outcome
            .reject
            .as_ref()
            .unwrap_or_else(|| panic!("{name} carries D8 reject"));
        assert_eq!(reject.category, "INPUT", "{name} 四闸归因 INPUT");
        assert_eq!(reject.code, want_code, "{name} 闸码");
        assert!(
            reject.message.contains(want_msg),
            "{name} 违规事实进消息（{want_msg}）：{}",
            reject.message
        );
        // 评审修订 2026-11-20：闸 1 三判据 reason 文案逐分支对账（`..` 段 /
        // 绝对路径 / 反斜杠），补齐此前零证据的两分支。
        match name {
            "abs.zip" => assert!(
                reject.message.contains("absolute path entry"),
                "{name} 闸 1 绝对路径 reason 原文：{}",
                reject.message
            ),
            "backslash.zip" => assert!(
                reject
                    .message
                    .contains("contains a backslash separator (Windows semantics, fail-closed)"),
                "{name} 闸 1 反斜杠 reason 原文：{}",
                reject.message
            ),
            _ => {}
        }
        assert!(
            reject.message.contains("extract phase never submitted"),
            "{name} 消息注明解包阶段零提交：{}",
            reject.message
        );
        // runs.len()==1 仅 list 行（解包阶段永不提交），list 行 SUCCEEDED 留证。
        assert_eq!(
            outcome.runs.len(),
            1,
            "{name} 解包步零提交（list 单行留证）"
        );
        assert_eq!(outcome.runs[0].tool_id, "rip.archive.list");
        assert_eq!(
            run_row_status(&engine, &outcome.runs[0].run_id).as_deref(),
            Some("SUCCEEDED"),
            "{name} list 行真实终态留证"
        );
    }
    // 列目成功但清单头块解析失败 → EXECUTION ARCHIVE_LIST_NO_OUTPUT
    // （D6 表同位语义）。载具 = harness 注入（pdfinfo_pages_unparsed 先例
    // 同形态）：rip.archive.list 克隆 binary=/usr/bin/true（exit 0 但 stdout
    // 恒空 → ArchiveListMeta::parse 头块 None）；零条目空 zip 全 ASCII 头
    // 落嗅探 Text 签名（NUL 合法 UTF-8）到不了归档通道，harness 是该防御
    // 面的确定性载具。RouterManifests 同 id 首见优先（harness 目录在前）。
    let true_sha = tool_registry::hash::sha256_hex(
        &std::fs::read("/usr/bin/true").expect("/usr/bin/true (coreutils) present"),
    );
    let harness = json!({
        "schema_version": "1",
        "id": "rip.archive.list",
        "name": "Archive Listing (list-no-output harness)",
        "version": "0.9.0",
        "kind": "tool",
        "family": "document",
        "description": "harness：stdout 恒空（/usr/bin/true），-slt 清单头块解析失败形态载具（ARCHIVE_LIST_NO_OUTPUT 防御面）",
        "runtime": {
            "type": "native_rust",
            "binary": "/usr/bin/true",
            "binary_sha256": true_sha,
            "argv_template": [],
            "result_mode": "file"
        },
        "input_schema": {
            "type": "object",
            "additionalProperties": false,
            "required": ["input_path"],
            "properties": {
                "input_path": {"type": "string", "minLength": 1}
            }
        },
        "output_schema": {
            "type": "object",
            "additionalProperties": false,
            "required": ["exit_code", "stdout_bytes", "stdout_capped", "artifacts"],
            "properties": {
                "exit_code": {"type": "integer", "const": 0},
                "stdout_bytes": {"type": "integer", "minimum": 0},
                "stdout_capped": {"type": "boolean"},
                "artifacts": {"type": "array", "items": {"type": "object"}}
            }
        },
        "resources": {"cpu": "1", "memory": "64Mi", "timeout_seconds": 30, "max_output_bytes": 1048576},
        "network": {"mode": "none"},
        "secrets": [],
        "permission_level": "T0",
        "idempotent": true,
        "health": {"probe": {"type": "none"}, "on_run_recover": false},
        "provenance": {"origin": "rip-native", "license": "repo-internal", "dependencies": [], "tested": "list-no-output harness"}
    });
    let harness_dir =
        std::env::temp_dir().join(format!("di2-list-no-output-{}", std::process::id()));
    std::fs::create_dir_all(&harness_dir).unwrap();
    std::fs::write(
        harness_dir.join("rip.archive.list.json"),
        serde_json::to_string_pretty(&harness).unwrap(),
    )
    .unwrap();
    let engine2 = {
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
            let text =
                std::fs::read_to_string(repo_root().join(format!("manifests/tools/{id}.json")))
                    .unwrap();
            registry
                .register(&serde_json::from_str(&text).unwrap())
                .unwrap_or_else(|e| panic!("register {id}: {e}"));
        }
        // harness 克隆升版本 supersede 真件 → current = harness。
        registry
            .register(&harness)
            .expect("harness clone supersedes real manifest");
        Engine::open(pool, registry)
    };
    let router2 = document_router::Router::new(
        document_router::RouterConfig {
            workspace_root: ws_root.clone(),
        },
        document_router::RouterManifests::from_dirs(&[
            harness_dir.clone(),
            repo_root().join("manifests/tools"),
        ])
        .expect("manifests load with harness override"),
    );
    let outcome = router2
        .route(
            &engine2,
            &adapter,
            Some(&provider),
            &smoke("sample.zip"),
            "sample.zip",
            "zip",
        )
        .expect("harness listing accepted at gate");
    assert_eq!(outcome.status, document_router::RouteStatus::Failed);
    let reject = outcome.reject.as_ref().unwrap();
    assert_eq!(reject.category, "EXECUTION");
    assert_eq!(reject.code, "ARCHIVE_LIST_NO_OUTPUT");
    assert_eq!(outcome.runs.len(), 1, "list 行留证（解包阶段未提交）");
    let _ = std::fs::remove_dir_all(&ws_root);
    let _ = std::fs::remove_dir_all(&harness_dir);
}

/// 判据 12：不支持格式 + 能力 missing——t.tar（运行时 bsdtar 生成，内容含
/// 非 UTF-8 字节保证 Unknown 落点）→ 嗅探门 INPUT ROUTER_UNSUPPORTED_TYPE
/// 零 run 行；harness 注入缺失 capability_ref（pdf.mutool 形态，DI-1 判据 5
/// 同款）→ PREPARING DEPENDENCY 显式信封 run 行 FAILED。
#[test]
fn di2_unsupported_tar_gate_and_missing_capability_ref() {
    let test = "di2_unsupported_tar_gate_and_missing_capability_ref";
    let provider = provider();
    // t.tar 生成器 = archive.bsdtar（「不支持格式」负例真实性：真 tar 而非
    // 伪字节；bsdtar 缺席按 SKIP）。
    let gen_dir = std::env::temp_dir().join(format!("di2-tar-{}", std::process::id()));
    std::fs::create_dir_all(&gen_dir).unwrap();
    let payload = gen_dir.join("blob.bin");
    std::fs::write(&payload, [0xFFu8, 0xFE, 0x00, 0x01]).unwrap();
    let tar_path = gen_dir.join("t.tar");
    let tar_ok = std::process::Command::new("/usr/bin/bsdtar")
        .arg("-cf")
        .arg(tar_path.to_str().unwrap())
        .arg("-C")
        .arg(gen_dir.to_str().unwrap())
        .arg("blob.bin")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if tar_ok && tar_path.exists() {
        let engine = fresh_engine(&[]);
        let ws_root = workspace_root("di2-tar");
        let router = router_for(&ws_root);
        let adapter = adapter_for(&ws_root);
        let bytes = std::fs::read(&tar_path).unwrap();
        let err = router
            .route(&engine, &adapter, Some(&provider), &bytes, "t.tar", "tar")
            .expect_err("tar must stay fail-closed at sniff gate");
        assert_eq!(err.category, "INPUT");
        assert_eq!(err.code, "ROUTER_UNSUPPORTED_TYPE");
        assert_eq!(run_row_count(&engine), 0, "tar 嗅探门拒绝零 run 行");
        let _ = std::fs::remove_dir_all(&ws_root);
    } else {
        println!("SKIP {test}: capability archive.bsdtar status=available-but-executable-missing (/usr/bin/bsdtar)");
    }
    let _ = std::fs::remove_dir_all(&gen_dir);

    // harness 注入缺失 capability_ref：rip.archive.list 克隆 capability_ref
    // → pdf.mutool（种子 status=missing 条目形态）→ PREPARING DEPENDENCY
    // 显式信封 run 行 FAILED（钉值自持 stub 同 DI-1 判据 5 形态）。
    let mut doc: Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("manifests/tools/rip.archive.list.json"))
            .unwrap(),
    )
    .unwrap();
    doc["runtime"]["capability_ref"] = json!("pdf.mutool");
    doc["id"] = json!("rip.archive.list.missingcap");
    doc["name"] = json!("Archive Listing (missingcap harness)");
    let ws_root = workspace_root("di2-missingcap");
    let stub = ws_root.join("missingcap-identity-stub.sh");
    std::fs::write(&stub, "#!/bin/sh\nexit 0\n").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    doc["runtime"]["binary"] = json!(stub.to_str().unwrap());
    doc["runtime"]["binary_sha256"] = json!(tool_registry::hash::sha256_hex(
        &std::fs::read(&stub).unwrap()
    ));
    doc["runtime"]
        .as_object_mut()
        .unwrap()
        .remove("binary_target_sha256");
    let engine = fresh_engine(std::slice::from_ref(&doc));
    let adapter = adapter_for(&ws_root);
    let run_id = "rn-di2-missingcap";
    let in_dir = ws_root.join(run_id).join("in");
    std::fs::create_dir_all(&in_dir).unwrap();
    std::fs::write(in_dir.join("demo.zip"), smoke("sample.zip")).unwrap();
    engine
        .submit(&json!({
            "protocol_version": "1",
            "run_id": run_id,
            "tool_id": "rip.archive.list.missingcap",
            "input": {"input_path": "demo.zip"},
            "workspace_id": "ws-di2-missingcap",
        }))
        .unwrap();
    let outcome = tool_engine::adapter::execute_native_rust_via_engine(
        tool_engine::adapter::NativeRustCall {
            engine: &engine,
            run_id,
            adapter: &adapter,
            manifest: &doc,
            capability: Some(&provider),
        },
    )
    .expect("drive ok");
    assert_eq!(outcome.envelope.status, domain::ToolRunStatus::Failed);
    let error = outcome
        .envelope
        .error
        .as_ref()
        .expect("explicit error envelope");
    assert_eq!(
        error.category, "DEPENDENCY",
        "能力 missing → DEPENDENCY（D8 表）"
    );
    assert!(
        error.message.to_lowercase().contains("missing"),
        "显式 missing 语义：{}",
        error.message
    );
    assert_eq!(run_row_status(&engine, run_id).as_deref(), Some("FAILED"));
    let _ = std::fs::remove_dir_all(&ws_root);
}
