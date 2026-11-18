//! DI-1 通道表/帽门套件（S6；D3 表 + D7 帽值 + RouterManifests 装载 + 前置门
//! 拒绝语义 + D8 错误映射程序化面）。

use std::time::Duration;

use database::{migrate, DbPath, Pool, PoolConfig};
use document_router::channel::{
    channel_steps, Channel, StepOutput, MAX_INPUT_FILE_BYTES, MAX_PAGES, MAX_PIXELS,
    SUBRUN_MAX_OUTPUT_BYTES, TOOL_IMAGE_META, TOOL_OFFICE_TO_PDF, TOOL_PDF_INFO, TOOL_PDF_TEXT,
};
use document_router::error::RouterReject;
use document_router::sniff::Magic;
use document_router::{RouteStatus, Router, RouterConfig, RouterManifests};
use serde_json::Value;
use tool_engine::adapter::{NativeRustAdapter, NativeRustConfig};
use tool_engine::Engine;
use tool_registry::{Options, Registry};

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

/// 内存库引擎（前置门拒绝发生在任何子 run 提交前；行数断言面用）。
fn fresh_engine() -> Engine {
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
        TOOL_PDF_INFO,
        TOOL_PDF_TEXT,
        TOOL_OFFICE_TO_PDF,
        TOOL_IMAGE_META,
    ] {
        let text = std::fs::read_to_string(repo_root().join(format!("manifests/tools/{id}.json")))
            .unwrap();
        registry
            .register(&serde_json::from_str::<Value>(&text).unwrap())
            .unwrap_or_else(|e| panic!("register {id}: {e}"));
    }
    Engine::open(pool, registry)
}

/// D3 通道表程序化面（表行与 Router 编排引用的同一常量集锁定）。
#[test]
fn d3_channel_table_rows_locked() {
    let pdf = channel_steps(Channel::Pdf);
    assert_eq!(pdf.len(), 2);
    assert_eq!(pdf[0].tool_id, TOOL_PDF_INFO);
    assert_eq!(pdf[0].capability_ref, "pdf.pdfinfo");
    assert_eq!(pdf[0].kind, StepOutput::StdoutLog);
    assert_eq!(pdf[1].tool_id, TOOL_PDF_TEXT);
    assert_eq!(pdf[1].capability_ref, "pdf.pdftotext");

    let office = channel_steps(Channel::Office);
    assert_eq!(office.len(), 3);
    assert_eq!(office[0].tool_id, TOOL_OFFICE_TO_PDF);
    assert_eq!(office[0].capability_ref, "document.libreoffice");
    assert_eq!(
        office[0].kind,
        StepOutput::ConvertedPdf,
        "office 首步产物 = 转换 PDF"
    );

    let image = channel_steps(Channel::Image);
    assert_eq!(image.len(), 1);
    assert_eq!(image[0].tool_id, TOOL_IMAGE_META);
    assert_eq!(image[0].capability_ref, "image.magick");
}

/// D7 帽值表定稿值锁定（后端强制来源常量）。
#[test]
fn d7_cap_values_locked() {
    assert_eq!(
        MAX_INPUT_FILE_BYTES, 67_108_864,
        "输入 64 MiB（对齐 worker 上限量级口径）"
    );
    assert_eq!(MAX_PAGES, 2_000, "页数帽（D7 #7）");
    assert_eq!(MAX_PIXELS, 100_000_000, "像素帽 100 MP（D7 #8）");
    assert_eq!(
        SUBRUN_MAX_OUTPUT_BYTES, 1_048_576,
        "子 run stdout 读帽（D7 #10）"
    );
}

/// RouterManifests 装载：四份齐 → contains 四工具；缺一 → Err 列名。
#[test]
fn router_manifests_load_and_guard() {
    let manifests =
        RouterManifests::from_dirs(&[repo_root().join("manifests/tools")]).expect("four manifests");
    for id in [
        TOOL_PDF_INFO,
        TOOL_PDF_TEXT,
        TOOL_OFFICE_TO_PDF,
        TOOL_IMAGE_META,
    ] {
        assert!(manifests.contains(id), "{id} in channel table");
    }
    assert!(!manifests.contains("rip.doc.parse"), "冻结面不在通道表");
    // 空目录集 → Err（编排不可半途降级）。
    let empty = std::env::temp_dir().join(format!("di1-empty-manifests-{}", std::process::id()));
    std::fs::create_dir_all(&empty).unwrap();
    let err = RouterManifests::from_dirs(&[empty]).expect_err("missing manifests rejected");
    assert!(err.contains("missing"), "缺一即列名：{err}");
    for id in [
        TOOL_PDF_INFO,
        TOOL_PDF_TEXT,
        TOOL_OFFICE_TO_PDF,
        TOOL_IMAGE_META,
    ] {
        assert!(err.contains(id), "{id} 列入缺单");
    }
}

/// D7 前置门：文件超 64 MiB → INPUT ROUTER_INPUT_TOO_LARGE，零 run 行。
#[test]
fn input_too_large_rejected_before_any_run() {
    let engine = fresh_engine();
    let ws = std::env::temp_dir().join(format!("di1-gate-big-{}", std::process::id()));
    std::fs::create_dir_all(&ws).unwrap();
    let manifests = RouterManifests::from_dirs(&[repo_root().join("manifests/tools")]).unwrap();
    let router = Router::new(
        RouterConfig {
            workspace_root: ws.clone(),
        },
        manifests,
    );
    let adapter = NativeRustAdapter::new(NativeRustConfig {
        workspace_root: ws.clone(),
        ..NativeRustConfig::default()
    });
    let oversized = vec![0u8; (MAX_INPUT_FILE_BYTES + 1) as usize];
    let err = router
        .route(&engine, &adapter, None, &oversized, "huge.pdf", "pdf")
        .expect_err("over-cap input rejected");
    assert_eq!(err.category, "INPUT");
    assert_eq!(err.code, "ROUTER_INPUT_TOO_LARGE");
    assert!(err.message.contains("exceeds cap"));
    let count: i64 = engine
        .pool()
        .with_conn(|c| c.query_row("SELECT count(*) FROM tool_run", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(count, 0, "前置门拒绝零 run 行（D7）");
    let _ = std::fs::remove_dir_all(&ws);
}

/// 前置门无通道签名（unknown 魔数）→ INPUT ROUTER_UNSUPPORTED_TYPE。
#[test]
fn unknown_magic_rejected_at_gate() {
    let engine = fresh_engine();
    let ws = std::env::temp_dir().join(format!("di1-gate-unknown-{}", std::process::id()));
    std::fs::create_dir_all(&ws).unwrap();
    let manifests = RouterManifests::from_dirs(&[repo_root().join("manifests/tools")]).unwrap();
    let router = Router::new(
        RouterConfig {
            workspace_root: ws.clone(),
        },
        manifests,
    );
    let adapter = NativeRustAdapter::new(NativeRustConfig {
        workspace_root: ws.clone(),
        ..NativeRustConfig::default()
    });
    let err = router
        .route(
            &engine,
            &adapter,
            None,
            &[0xDE, 0xAD, 0xBE, 0xEF],
            "blob.bin",
            "bin",
        )
        .expect_err("unknown magic rejected");
    assert_eq!(err.code, "ROUTER_UNSUPPORTED_TYPE");
    assert_eq!(err.category, "INPUT");
    let _ = std::fs::remove_dir_all(&ws);
}

/// D8 错误映射程序化面：九类词表内 category + code 透传 + 载荷形状。
#[test]
fn d8_reject_mapping_shapes() {
    let r = RouterReject::unsupported_type(&Magic::ZipOther, "archive defense is DI-3");
    assert_eq!(r.category, "INPUT");
    assert_eq!(r.code, "ROUTER_UNSUPPORTED_TYPE");
    let payload = r.to_payload();
    assert_eq!(payload["ok"], serde_json::json!(false));
    assert_eq!(
        payload["error"]["code"],
        serde_json::json!("ROUTER_UNSUPPORTED_TYPE")
    );

    let r = RouterReject::page_budget_exceeded(2001, MAX_PAGES);
    assert_eq!(r.code, "ROUTER_PAGE_BUDGET_EXCEEDED");
    let r = RouterReject::pixel_budget_exceeded(MAX_PIXELS + 1, MAX_PIXELS);
    assert_eq!(r.code, "ROUTER_PIXEL_BUDGET_EXCEEDED");
    let r = RouterReject::office_no_output("rn-x", "out/ empty");
    assert_eq!(r.category, "EXECUTION");
    assert_eq!(r.code, "OFFICE_CONVERT_NO_OUTPUT");
    // RT-1 码表透传（D8 chain_failed 契约）。
    let r = RouterReject::chain_failed(
        "POLICY_DENIED",
        "NATIVE_IDENTITY_MISMATCH",
        "sha mismatch".into(),
        false,
    );
    assert_eq!(r.category, "POLICY_DENIED");
    assert_eq!(r.code, "NATIVE_IDENTITY_MISMATCH");
}

/// staged 名派生（D5 Router 控制面：穿越折叠 / 无扩展名补 ext /
/// request.json 冲突规避）。
#[test]
fn staged_name_derivation_rules() {
    // 穿越与路径成分折叠：只有文件名部分存活。
    let name = document_router::router::staged_name_for_test("../../etc/passwd", "");
    assert_eq!(name, "passwd", "路径成分折叠为纯文件名");
    // 无扩展名 + ext 提示 → 补小写扩展名（basename 原样保留，ext 归一小写）。
    let name = document_router::router::staged_name_for_test("README", "PDF");
    assert_eq!(name, "README.pdf");
    // 非法字符折叠。
    let name = document_router::router::staged_name_for_test("we ird:na*me.pdf", "");
    assert_eq!(name, "we_ird_na_me.pdf");
    // 与 adapter 审计文件 request.json 冲突规避。
    let name = document_router::router::staged_name_for_test("request.json", "");
    assert_eq!(name, "doc-request.json");
}

/// RouteStatus/RouteOutcome 形状（Succeeded/Failed 互斥语义）。
#[test]
fn route_status_vocabulary() {
    assert_ne!(RouteStatus::Succeeded, RouteStatus::Failed);
}
