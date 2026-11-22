//! WSW-6 集成测试（S5/S6 门）：Decoder 往返恒等矩阵（判据 4 负例硬门）+
//! Comparer 差异归因矩阵（判据 5 负例硬门）+ CAS/exchange/frame 比较源
//! 消费面 + 结果 Artifact 落账面。

use wsw_runtime::comparer::{
    compare_bytes, compare_headers, compare_json, compare_text, persist_report_as_blob, IgnoreRules,
};
use wsw_runtime::decoder::{apply_ops, DecodeError, DecodeOp, DECODER_OPS};
use wsw_runtime::fuzz::mutator::{apply_mutators, EncodeKind, Mutator};
use wsw_runtime::persist::{generate_frame_id, PersistStore, WsFrameRow};

fn temp_db(tag: &str) -> (std::path::PathBuf, PersistStore) {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "wsw56-dc-{tag}-{}-{}-{n:x}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let db = dir.join("t.db");
    let store = PersistStore::open_file(&db).expect("open");
    store
        .run_migrations(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../migrations")
                .as_path(),
        )
        .expect("migrate");
    (db, store)
}

fn artifacts(tag: &str) -> std::path::PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "wsw56-dc-art-{tag}-{}-{}-{n:x}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp artifacts");
    dir
}

#[test]
fn decoder_roundtrip_matrix_cross_module_identity() {
    // 判据 4：五变换双向恒等（编码方向 = mutator pub 面 apply_mutators，
    // 解码方向 = decoder 逆变换——跨面一致性 = 消费面非第二实现实证）。
    let samples = [
        "plain-text",
        "a b&c=d/e?f",
        "héllo 世界 🚀",
        "<script>alert('x')</script>",
        "",
    ];
    let enc_chains = [
        vec![EncodeKind::Url],
        vec![EncodeKind::Base64],
        vec![EncodeKind::Base64Url],
        vec![EncodeKind::Hex],
        vec![EncodeKind::HtmlEntity],
    ];
    let dec_ops = [
        DecodeOp::UrlDecode,
        DecodeOp::B64Decode,
        DecodeOp::B64UrlDecode,
        DecodeOp::HexDecode,
        DecodeOp::HtmlEntityDecode,
    ];
    for s in samples {
        for (chain, dec) in enc_chains.iter().zip(dec_ops) {
            let encoded = apply_mutators(
                &[s.to_string()],
                &[Mutator::Encode {
                    chain: chain.clone(),
                }],
            )
            .expect("mutator encode")
            .remove(0);
            let back = apply_ops(&encoded, &[dec]).expect("decode");
            assert_eq!(back, s, "decode(mutator_encode(x))==x for {dec:?}: {s:?}");
        }
    }
    // 正向恒等 encode(decode(x))==x（编码像稳定点）。
    let stable = [
        ("a%20b%26c", DecodeOp::UrlDecode, DecodeOp::UrlEncode),
        ("Zm9vYmFy", DecodeOp::B64Decode, DecodeOp::B64Encode),
        ("Zm9vYg", DecodeOp::B64UrlDecode, DecodeOp::B64UrlEncode),
        ("616263", DecodeOp::HexDecode, DecodeOp::HexEncode),
        (
            "&lt;a&gt;&amp;&quot;&#39;",
            DecodeOp::HtmlEntityDecode,
            DecodeOp::HtmlEntityEncode,
        ),
    ];
    for (x, dec, enc) in stable {
        let decoded = apply_ops(x, &[dec]).expect("decode");
        let re = apply_ops(&decoded, &[enc]).expect("encode");
        assert_eq!(re, x, "encode(decode(x))==x for {dec:?}: {x:?}");
    }
}

#[test]
fn decoder_negative_hard_gate_and_word_table() {
    // 判据 4 负例硬门：非法输入 → 结构化拒绝（不静默透传）。
    // 结构化拒绝逐项 matches! 面：
    assert!(matches!(
        apply_ops("616", &[DecodeOp::HexDecode]),
        Err(DecodeError::InvalidHex { .. })
    ));
    assert!(matches!(
        apply_ops("zz", &[DecodeOp::HexDecode]),
        Err(DecodeError::InvalidHex { .. })
    ));
    assert!(matches!(
        apply_ops("a%2z", &[DecodeOp::UrlDecode]),
        Err(DecodeError::InvalidHex { .. })
    ));
    assert!(matches!(
        apply_ops("ab*cd", &[DecodeOp::B64Decode]),
        Err(DecodeError::InvalidBase64 { .. })
    ));
    assert!(matches!(
        apply_ops("A", &[DecodeOp::B64Decode]),
        Err(DecodeError::InvalidBase64 { .. })
    ));
    assert!(matches!(
        apply_ops("\\uD83D", &[DecodeOp::UnicodeUnescape]),
        Err(DecodeError::InvalidUnicodeEscape(_))
    ));
    assert!(matches!(
        apply_ops("\\uDC00", &[DecodeOp::UnicodeUnescape]),
        Err(DecodeError::InvalidUnicodeEscape(_))
    ));
    assert!(matches!(
        apply_ops("\\uD800\\uD800", &[DecodeOp::UnicodeUnescape]),
        Err(DecodeError::InvalidUnicodeEscape(_))
    ));
    assert!(matches!(
        apply_ops("{bad", &[DecodeOp::JsonPretty]),
        Err(DecodeError::InvalidJson(_))
    ));
    assert!(matches!(
        apply_ops("a.b.c", &[DecodeOp::JwtDecode]),
        Err(DecodeError::InvalidJwt(_))
    ));
    // 词表面：17 op = 12 词表项落地投影；裁剪面（gzip/deflate/brotli）不在表。
    assert_eq!(DECODER_OPS.len(), 17);
    assert!(!DECODER_OPS.contains(&"gzip_decode"));
    assert!(!DECODER_OPS.contains(&"deflate_decode"));
    assert!(!DECODER_OPS.contains(&"brotli_decode"));
}

#[test]
fn comparer_exchange_body_and_frame_payload_sources() {
    // 判据 5：比较对象 = exchange Body（CAS 回读）/ 帧 Payload（030 行）/
    // 手工输入——CAS 源经真实 store 落账承载。
    let (_db_path, store) = temp_db("sources");
    let root = artifacts("sources");
    let now = wsw_runtime::persist::now_iso();

    // 握手行 + 两个帧（payload CAS 化）→ hex 模式比较首差异偏移归因。
    let hs = wsw_runtime::persist::ExchangeRow {
        exchange_id: "wsw-20261122000000-0-aaaa".into(),
        session_id: "wsw-ses-dc".into(),
        listener_id: "wsw-lis-dc".into(),
        direction: "client_to_server",
        source: "proxy_capture",
        protocol: "ws",
        method: Some("GET".into()),
        scheme: Some("ws".into()),
        authority: Some("127.0.0.1:9".into()),
        path: Some("/dc".into()),
        started_at: Some(now.clone()),
        completed_at: Some(now.clone()),
        tags_json: "[]".into(),
        capture_fidelity: "degraded_no_wire",
        parse_status: "ok",
        ..Default::default()
    };
    store.insert_exchanges(&[hs]).expect("handshake row");
    let payloads: [Vec<u8>; 2] = [b"payload-A".to_vec(), b"payload-B".to_vec()];
    for (i, p) in payloads.iter().enumerate() {
        let body = wsw_message::cas::store_body(&root, p, 8 * 1024 * 1024).expect("cas");
        let row = wsw_message::store::BodyBlobRow {
            sha256: body.sha256.clone(),
            uri: body.uri.clone(),
            size_bytes: body.size as i64,
            mime_type: "application/octet-stream".into(),
            first_seen_exchange_id: None,
            created_at: now.clone(),
        };
        store
            .with_conn(|c| wsw_message::store::register_body_blob(c, &row))
            .expect("blob row");
        store
            .insert_ws_frames(&[WsFrameRow {
                frame_id: generate_frame_id(),
                connection_exchange_id: "wsw-20261122000000-0-aaaa".into(),
                direction: "client_to_server",
                sequence_no: i as i64,
                opcode: "binary",
                payload_len: p.len() as i64,
                payload_body_sha256: Some(body.sha256),
                payload_body_uri: Some(body.uri),
                payload_body_len: Some(p.len() as i64),
                payload_body_fidelity: Some("full"),
                masked: true,
                captured_at: now.clone(),
                scope_decision: Some("in_scope"),
            }])
            .expect("frame row");
    }
    // CAS 源比较（hex 模式）：首差异偏移 = 第一个不同字节位。
    let frames = store
        .list_frames_by_exchange("wsw-20261122000000-0-aaaa", None, 100)
        .expect("frames");
    assert_eq!(frames.len(), 2);
    let left =
        wsw_message::cas::read_body(&root, frames[0].payload_body_uri.as_deref().unwrap()).unwrap();
    let right =
        wsw_message::cas::read_body(&root, frames[1].payload_body_uri.as_deref().unwrap()).unwrap();
    let rep = compare_bytes(&left, &right);
    assert!(
        !rep.identical,
        "长度相同内容不同 → sha256 判相同=false（PLAN :303）"
    );
    assert_eq!(
        rep.hex_diff.as_ref().unwrap().first_diff_offset,
        Some(8),
        "payload-A vs payload-B 首差异 = 尾字节位"
    );
    // 结果 Artifact 落账（wsw_message cas/store 既有面）：blob 行 + 读回。
    let body_ref = persist_report_as_blob(&store, &root, &rep).expect("artifact");
    let back = wsw_message::cas::read_body(&root, &body_ref.uri).expect("read artifact");
    let parsed: serde_json::Value = serde_json::from_slice(&back).expect("artifact json");
    assert_eq!(parsed["mode"], "hex");
    let blob_count: i64 = store
        .with_conn(|c| c.query_row("SELECT COUNT(*) FROM wsw_body_blob", [], |r| r.get(0)))
        .unwrap();
    assert!(
        blob_count >= 3,
        "payload 两份 + 报告一份 CAS 行（sha256 寻址）"
    );
}

#[test]
fn comparer_attribution_matrix_all_four_modes() {
    // 判据 5：增删改区间归因 + normalize_keys 掩码收敛 + regex 剔除收敛 +
    // JSON 键序不敏感 + Header 逐键 + Hex OFFSET + 相似度数值断言 + 负例。
    let ignore = IgnoreRules::default().compile().unwrap();
    // 增删改区间。
    let rep = compare_text("a\nb\nc", "a\nc\nd\nb", &ignore);
    let hunks = rep.hunks.unwrap();
    assert!(hunks.iter().any(|h| h.op == "delete"), "删除区间归因");
    assert!(hunks.iter().any(|h| h.op == "insert"), "插入区间归因");
    // 掩码/剔除（base 两维直通 diff 原语）。
    let rules = IgnoreRules {
        base: wsw_message::diff::NormalizeRules {
            normalize_keys: vec!["ts".into()],
            ..Default::default()
        },
        ..Default::default()
    };
    let rep2 = compare_text(
        "{\"ts\": 1, \"v\": 2}",
        "{\"ts\": 999, \"v\": 2}",
        &rules.compile().unwrap(),
    );
    assert!(rep2.identical, "base normalize_keys 掩码收敛");
    // regex 两维（本模块预处理层）。
    let ig = IgnoreRules {
        skip_line_regexes: vec![r"^\d+$".into()],
        normalize_key_regexes: vec![r"^token$".into()],
        ..Default::default()
    }
    .compile()
    .unwrap();
    let rep3 = compare_text("row\n12345\nkeep", "row\n99999\nkeep", &ig);
    assert!(rep3.identical, "skip_line_regexes 剔除收敛");
    let rep4 = compare_text("token=aaa", "token=bbb", &ig);
    assert!(rep4.identical, "normalize_key_regexes 键值掩码收敛");
    // JSON 键序不敏感 + 归因路径。
    let rep5 = compare_json("{\"a\":1,\"b\":[1,2]}", "{\"b\":[1,2],\"a\":1}").unwrap();
    assert!(rep5.identical && rep5.json_diff.as_ref().unwrap().is_empty());
    let rep6 = compare_json("{\"a\":{\"n\":1}}", "{\"a\":{\"n\":2}}").unwrap();
    assert_eq!(rep6.json_diff.as_ref().unwrap()[0].path, "$.a.n");
    // Header 逐键 + 相同判定。
    let rep7 = compare_headers(
        r#"[{"name":"A","value":"1"},{"name":"B","value":"2"}]"#,
        r#"[{"name":"B","value":"2"},{"name":"A","value":"1"}]"#,
    )
    .unwrap();
    assert!(rep7.identical, "header 键序不敏感");
    // 相似度数值断言（Dice 口径：2·LCS/(n+m)）。
    let rep8 = compare_text("x\ny", "x\nz", &ignore);
    assert!(
        (rep8.similarity.unwrap() - 0.5).abs() < 1e-9,
        "2·1/(2+2)=0.5"
    );
}
