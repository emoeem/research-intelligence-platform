//! DI-1 通道表/帽门套件（S6；D3 表 + D7 帽值 + RouterManifests 装载 + 前置门
//! 拒绝语义 + D8 错误映射程序化面）。

use std::time::Duration;

use database::{migrate, DbPath, Pool, PoolConfig};
use document_router::channel::{
    channel_steps, Channel, StepOutput, ARCHIVE_MAX_ENTRIES, ARCHIVE_MAX_TOTAL_BYTES,
    MAX_INPUT_FILE_BYTES, MAX_PAGES, MAX_PIXELS, MEDIA_DURATION_MAX_SECONDS, MEDIA_FRAMES_MAX,
    MEDIA_FRAME_FPS, MEDIA_FRAME_PATTERN, MEDIA_FRAME_PIXELS_MAX, OCR_LANG_DEFAULT_CANDIDATES,
    OCR_LANG_JOIN, OCR_LANG_ROUTES, OCR_PIXELS_MAX, SUBRUN_MAX_OUTPUT_BYTES, TOOL_ARCHIVE_EXTRACT,
    TOOL_ARCHIVE_LIST, TOOL_IMAGE_META, TOOL_IMAGE_OCR, TOOL_MEDIA_FRAMES, TOOL_MEDIA_PROBE,
    TOOL_OFFICE_TO_PDF, TOOL_PDF_INFO, TOOL_PDF_TEXT,
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
/// DI-2：九份通道表 Manifest 全登记（四 DI-1 + 五 DI-2）。
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
        TOOL_IMAGE_OCR,
        TOOL_MEDIA_PROBE,
        TOOL_MEDIA_FRAMES,
        TOOL_ARCHIVE_LIST,
        TOOL_ARCHIVE_EXTRACT,
    ] {
        let text = std::fs::read_to_string(repo_root().join(format!("manifests/tools/{id}.json")))
            .unwrap();
        registry
            .register(&serde_json::from_str::<Value>(&text).unwrap())
            .unwrap_or_else(|e| panic!("register {id}: {e}"));
    }
    Engine::open(pool, registry)
}

/// D3/D2/D3/D4 通道表程序化面（表行与 Router 编排引用的同一常量集锁定；
/// DI-2 三通道扩行「两处同改」的对账面）。
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

    // DI-2 D2：OCR 通道两步（identify 门步复用 DI-1 件 + tesseract 步）。
    let ocr = channel_steps(Channel::Ocr);
    assert_eq!(ocr.len(), 2, "OCR 通道 = identify 门步 + tesseract 步");
    assert_eq!(ocr[0].tool_id, TOOL_IMAGE_META);
    assert_eq!(ocr[0].capability_ref, "image.magick");
    assert_eq!(ocr[1].tool_id, TOOL_IMAGE_OCR);
    assert_eq!(ocr[1].capability_ref, "ocr.tesseract");
    assert_eq!(ocr[1].kind, StepOutput::StdoutLog, "TSV stdout 承载");

    // DI-2 D3：媒体通道两步（probe 常步 + frames 条件步镜像行）。
    let media = channel_steps(Channel::Media);
    assert_eq!(media.len(), 2, "媒体通道 = probe + frames（条件步）");
    assert_eq!(media[0].tool_id, TOOL_MEDIA_PROBE);
    assert_eq!(media[0].capability_ref, "media.ffprobe");
    assert_eq!(media[1].tool_id, TOOL_MEDIA_FRAMES);
    assert_eq!(media[1].capability_ref, "media.ffmpeg");
    assert_eq!(media[1].kind, StepOutput::OutArtifacts, "帧产物 out/ 面");

    // DI-2 D4：归档通道两阶段（同一 archive.7z 能力两条 Manifest）。
    let archive = channel_steps(Channel::Archive);
    assert_eq!(archive.len(), 2, "归档通道 = list + extract 两阶段");
    assert_eq!(archive[0].tool_id, TOOL_ARCHIVE_LIST);
    assert_eq!(archive[0].capability_ref, "archive.7z");
    assert_eq!(archive[1].tool_id, TOOL_ARCHIVE_EXTRACT);
    assert_eq!(archive[1].capability_ref, "archive.7z");
    assert_eq!(
        archive[1].kind,
        StepOutput::OutArtifacts,
        "解包产物 out/ 面"
    );
}

/// D7 帽值表定稿值锁定（后端强制来源常量）+ DI-2 D5 三通道定稿值。
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
    // DI-2 D5 定稿值（OCR/媒体/归档域）。
    assert_eq!(OCR_PIXELS_MAX, 50_000_000, "OCR 像素帽 50 MP（D2/D5 收紧）");
    const { assert!(OCR_PIXELS_MAX < MAX_PIXELS, "OCR 帽严格紧于元数据路径帽") }
    assert_eq!(
        MEDIA_DURATION_MAX_SECONDS, 300.0,
        "媒体内容时长帽 300 s（D3/D5）"
    );
    assert_eq!(MEDIA_FRAME_PIXELS_MAX, 16_000_000, "媒体单帧帽 16 MP");
    assert_eq!(MEDIA_FRAMES_MAX, 32, "媒体帧数帽 32（前缀采样语义）");
    assert_eq!(ARCHIVE_MAX_ENTRIES, 512, "归档条目数帽（闸 3）");
    assert_eq!(
        ARCHIVE_MAX_TOTAL_BYTES,
        256 * 1024 * 1024,
        "归档解压总量帽 256 MiB（闸 4）"
    );
}

/// RouterManifests 装载：九份齐 → contains 九工具；缺一 → Err 列名。
#[test]
fn router_manifests_load_and_guard() {
    let manifests = RouterManifests::from_dirs(&[repo_root().join("manifests/tools")])
        .expect("nine channel-table manifests");
    for id in [
        TOOL_PDF_INFO,
        TOOL_PDF_TEXT,
        TOOL_OFFICE_TO_PDF,
        TOOL_IMAGE_META,
        TOOL_IMAGE_OCR,
        TOOL_MEDIA_PROBE,
        TOOL_MEDIA_FRAMES,
        TOOL_ARCHIVE_LIST,
        TOOL_ARCHIVE_EXTRACT,
    ] {
        assert!(manifests.contains(id), "{id} in channel table");
    }
    assert!(!manifests.contains("rip.doc.parse"), "冻结面不在通道表");
    assert!(
        !manifests.contains("rip.doc.pdf.text.v2"),
        "未登记面不在通道表"
    );
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
        TOOL_IMAGE_OCR,
        TOOL_MEDIA_PROBE,
        TOOL_MEDIA_FRAMES,
        TOOL_ARCHIVE_LIST,
        TOOL_ARCHIVE_EXTRACT,
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

// ---------------------------------------------------------------------------
// DI-2 评审修订 2026-11-20：Manifest argv ↔ Router 帽常量双源恒等对账
// （metadata.frames / OCR 语言槽报告值取自 channel.rs 常量而实际行为取自
// manifest argv——漂移时 metadata 如实报告错误事实且预算对账仅 warning 不
// 失败，D3「帽是 Router 属性」裁决依赖两源恒等，本节使漂移即红）。
// ---------------------------------------------------------------------------

/// rip.media.frames argv_template 全模板 ↔ 常量集对账：fps=2 / -frames:v 32
/// / -c:v png / out/frame-%03d.png 逐元素锁死（manifest 漂移如 -frames:v 64
/// 或 fps=3，或常量改动未同步 manifest，任一侧即红）；并复核 {{}} 槽位面
/// 除 {{input_path}} 外零可调槽位（D3 否决项——调用方可控速率/帧数参数会
/// 绕开帽表）。
#[test]
fn media_frames_manifest_argv_locked_to_cap_constants() {
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("manifests/tools/rip.media.frames.json"))
            .expect("rip.media.frames.json reads"),
    )
    .expect("rip.media.frames.json parses");
    let argv: Vec<String> = manifest["runtime"]["argv_template"]
        .as_array()
        .expect("argv_template array")
        .iter()
        .map(|v| v.as_str().expect("argv element string").to_string())
        .collect();
    let expected: Vec<String> = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-i".into(),
        "{{input_path}}".into(),
        "-vf".into(),
        format!("fps={MEDIA_FRAME_FPS}"),
        "-frames:v".into(),
        MEDIA_FRAMES_MAX.to_string(),
        "-c:v".into(),
        "png".into(),
        MEDIA_FRAME_PATTERN.to_string(),
    ];
    assert_eq!(
        argv, expected,
        "rip.media.frames argv 与 Router 帽常量双源恒等（manifest fps/帧数/pattern/编码器漂移即红）"
    );
    let slots: Vec<&String> = argv.iter().filter(|a| a.contains("{{")).collect();
    assert_eq!(slots.len(), 1, "{{}} 槽位面：仅 {{input_path}} 一个槽位");
    assert_eq!(slots[0], "{{input_path}}");
}

/// rip.doc.image.ocr argv/{{ocr_lang}} enum ↔ Router 语言路由表对账：
/// argv 五元素全常量 + 单 {{ocr_lang}} 整 token 槽位（D2）；input_schema
/// enum 含缺省候选单语言与组合形态（缺省/auto →
/// OCR_LANG_DEFAULT_CANDIDATES join OCR_LANG_JOIN = chi_sim+eng，Router
/// 生成值不得被 schema 二道收紧拒绝）。chi_tra 路由候选不入 enum 属正确
/// 形态——目录 language 集无 chi_tra 时该请求在 ROUTER_OCR_LANG_MISSING
/// 前置门被拒（零 run 行），结构性到不了 schema 校验面。
#[test]
fn ocr_manifest_lang_slot_locked_to_router_routes() {
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("manifests/tools/rip.doc.image.ocr.json"))
            .expect("rip.doc.image.ocr.json reads"),
    )
    .expect("rip.doc.image.ocr.json parses");
    let argv: Vec<String> = manifest["runtime"]["argv_template"]
        .as_array()
        .expect("argv_template array")
        .iter()
        .map(|v| v.as_str().expect("argv element string").to_string())
        .collect();
    assert_eq!(
        argv,
        vec![
            "{{input_path}}".to_string(),
            "stdout".to_string(),
            "-l".to_string(),
            "{{ocr_lang}}".to_string(),
            "tsv".to_string(),
        ],
        "OCR argv 全常量 + 单 {{ocr_lang}} 整 token 槽位（D2）"
    );
    let enum_vals: Vec<&str> = manifest["input_schema"]["properties"]["ocr_lang"]["enum"]
        .as_array()
        .expect("ocr_lang enum array")
        .iter()
        .map(|v| v.as_str().expect("enum element string"))
        .collect();
    for candidate in OCR_LANG_DEFAULT_CANDIDATES {
        assert!(
            enum_vals.contains(candidate),
            "缺省候选单语言 {candidate} 必须在 schema enum 内"
        );
    }
    let default_combo = OCR_LANG_DEFAULT_CANDIDATES.join(&OCR_LANG_JOIN.to_string());
    assert!(
        enum_vals.contains(&default_combo.as_str()),
        "缺省候选组合 {default_combo} 必须在 schema enum 内（auto 请求放行形态，D2）"
    );
    // 多语言路由的 join 形态同锁（当前表内无多语言路由——循环空转即形态
    // 备忘，路由表扩行时本断言自动生效）。
    for (label, langs) in OCR_LANG_ROUTES {
        if langs.len() > 1 {
            let combo = langs.join(&OCR_LANG_JOIN.to_string());
            assert!(
                enum_vals.contains(&combo.as_str()),
                "路由 {label} 组合形态 {combo} 必须在 schema enum 内"
            );
        }
    }
}
