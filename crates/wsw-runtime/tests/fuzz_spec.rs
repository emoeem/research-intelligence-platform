//! fuzz spec 词法/选择器/估算集成测试（WSW-4 S4；D3/D5/D10 判据面）：
//!
//! - §…§ 标记词法：成对解析正例 / 未闭合拒 / 空标记拒（fail-closed 负例）；
//! - 三模式估算（单位置 sum / 笛卡尔积 / zip 等长校验）+ u64 溢出饱和拒；
//! - 七类选择器目标在场预检（缺失拒）+ 选择器-标记同字段冲突拒；
//! - payloads 数量 ≠ 位置数拒；budget 缺失/非法拒（无显式 budget = 拒绝）；
//! - build_case_request 正例：marker/selector 双面注入（字节回读断言）；
//! - 字典文件 BOM/空行/`#` 注释跳过；范围生成器；编码链/变换/Processor 面；
//! - 模板 CRLF 头注入 → serialize 守卫拒（TemplateInvalid，既有词形）。

use wsw_message::serialize::{OutboundFraming, OutboundRequest};
use wsw_message::types::{HeaderRecord, HttpVersion};
use wsw_runtime::fuzz::mutator::{apply_mutators, apply_processors, ConcatPos, EncodeKind, Mutator, Processor};
use wsw_runtime::fuzz::spec::{
    build_case_request, estimate, parse_marker_spans, prepare_spec, CombinationIter,
    PayloadSource, SpecError,
};

fn template(target: &str, headers: Vec<HeaderRecord>, body: &[u8]) -> OutboundRequest {
    OutboundRequest {
        method: "GET".into(),
        target: target.into(),
        version: HttpVersion::Http11,
        headers,
        body: body.to_vec(),
        trailers: vec![],
        framing: OutboundFraming::ContentLength,
    }
}

fn spec_json(request: &OutboundRequest, extra: serde_json::Value) -> String {
    // FuzzSpec 形状：{"request": <模板>, ...extra}（模板嵌套在 request 字段下）。
    let mut obj = serde_json::Map::new();
    obj.insert(
        "request".to_string(),
        serde_json::to_value(request).expect("template json"),
    );
    if let Some(ex) = extra.as_object() {
        for (k, val) in ex {
            obj.insert(k.clone(), val.clone());
        }
    }
    serde_json::to_string(&serde_json::Value::Object(obj)).expect("spec json")
}

fn host_hdr() -> HeaderRecord {
    HeaderRecord::new("Host", "127.0.0.1")
}

// ---------------------------------------------------------------------------
// 标记词法（D3）
// ---------------------------------------------------------------------------

#[test]
fn marker_parse_pairs_ok() {
    // target 标记。
    let req = template("/f?q=§PAY§", vec![host_hdr()], b"");
    let spans = parse_marker_spans(&req).expect("pairs ok");
    assert_eq!(spans.len(), 1);
    match &spans[0] {
        wsw_runtime::fuzz::spec::MarkerSpan::Target { start, end, .. } => {
            assert_eq!(&req.target[*start..*end], "PAY");
        }
        other => panic!("expected target span, got {other:?}"),
    }
    // body 标记（UTF-8 字节面 0xC2 0xA7 对扫）。
    let body_text = "a§INJ§b";
    let req = template("/f", vec![host_hdr()], body_text.as_bytes());
    let spans = parse_marker_spans(&req).expect("body pairs ok");
    assert_eq!(spans.len(), 1);
    match &spans[0] {
        wsw_runtime::fuzz::spec::MarkerSpan::Body { start, end, .. } => {
            assert_eq!(&req.body[*start..*end], b"INJ");
        }
        other => panic!("expected body span, got {other:?}"),
    }
    // header 值标记。
    let req = template("/f", vec![host_hdr(), HeaderRecord::new("X-T", "h§HV§t")], b"");
    let spans = parse_marker_spans(&req).expect("header pairs ok");
    assert_eq!(spans.len(), 1);
    assert!(matches!(spans[0], wsw_runtime::fuzz::spec::MarkerSpan::Header { index: 1, .. }));
}

#[test]
fn marker_unclosed_and_empty_rejected() {
    // 未闭合（奇数个标记）。
    let req = template("/f?q=§PAY", vec![host_hdr()], b"");
    assert!(matches!(
        prepare_spec(&spec_json(
            &req,
            serde_json::json!({"mode":"single","budget":{"max_requests":1},
                               "payloads":[{"type":"list","values":["x"]}]})
        )),
        Err(SpecError::MarkerUnclosed)
    ));
    // 空标记 §§。
    let req = template("/f?q=§§", vec![host_hdr()], b"");
    assert!(matches!(
        prepare_spec(&spec_json(
            &req,
            serde_json::json!({"mode":"single","budget":{"max_requests":1},
                               "payloads":[{"type":"list","values":["x"]}]})
        )),
        Err(SpecError::MarkerEmpty)
    ));
}

// ---------------------------------------------------------------------------
// 估算（D5 判据 2 / D10 三模式）
// ---------------------------------------------------------------------------

#[test]
fn estimate_three_modes() {
    let eff = vec![
        vec!["a".to_string(), "b".to_string(), "c".to_string()],
        vec!["1".to_string(), "2".to_string()],
    ];
    assert_eq!(estimate(&eff, wsw_runtime::fuzz::spec::FuzzMode::Single).unwrap(), 5);
    assert_eq!(estimate(&eff, wsw_runtime::fuzz::spec::FuzzMode::Cartesian).unwrap(), 6);
    // zip 等长正例。
    let eff_even = vec![vec!["a".to_string(), "b".to_string()], vec!["1".to_string(), "2".to_string()]];
    assert_eq!(estimate(&eff_even, wsw_runtime::fuzz::spec::FuzzMode::Zip).unwrap(), 2);
    // zip 不等长拒。
    assert!(matches!(
        estimate(&eff, wsw_runtime::fuzz::spec::FuzzMode::Zip),
        Err(SpecError::ZipLengthMismatch)
    ));
}

#[test]
fn estimate_overflow_rejected() {
    let big: Vec<String> = std::iter::repeat_n("x".to_string(), 10).collect();
    // 10^8 可算。
    let eff: Vec<Vec<String>> = (0..8).map(|_| big.clone()).collect();
    assert_eq!(estimate(&eff, wsw_runtime::fuzz::spec::FuzzMode::Cartesian).unwrap(), 100_000_000);
    // 10^30 溢出饱和拒。
    let eff_of: Vec<Vec<String>> = (0..30).map(|_| big.clone()).collect();
    assert!(matches!(
        estimate(&eff_of, wsw_runtime::fuzz::spec::FuzzMode::Cartesian),
        Err(SpecError::EstimateOverflow)
    ));
}

#[test]
fn combination_iter_cartesian_and_single() {
    let eff = vec![vec!["a".to_string(), "b".to_string()], vec!["1".to_string(), "2".to_string()]];
    let it = CombinationIter::new(&eff, wsw_runtime::fuzz::spec::FuzzMode::Cartesian).unwrap();
    let all: Vec<Vec<Option<String>>> = it.collect();
    assert_eq!(all.len(), 4);
    // mixed-radix：末位最快。
    assert_eq!(all[0], vec![Some("a".to_string()), Some("1".to_string())]);
    assert_eq!(all[1], vec![Some("a".to_string()), Some("2".to_string())]);
    assert_eq!(all[3], vec![Some("b".to_string()), Some("2".to_string())]);
    let it = CombinationIter::new(&eff, wsw_runtime::fuzz::spec::FuzzMode::Single).unwrap();
    let all: Vec<Vec<Option<String>>> = it.collect();
    assert_eq!(all.len(), 4);
    // single：一次只动一个位置，其余 None（保模板原值）。
    assert_eq!(all[0], vec![Some("a".to_string()), None]);
    assert_eq!(all[1], vec![Some("b".to_string()), None]);
    assert_eq!(all[2], vec![None, Some("1".to_string())]);
    assert_eq!(all[3], vec![None, Some("2".to_string())]);
}

// ---------------------------------------------------------------------------
// 选择器预检（fail-closed 负例）
// ---------------------------------------------------------------------------

#[test]
fn selector_targets_missing_rejected() {
    let req = template("/f", vec![host_hdr()], b"a=1");
    for sel in ["query", "header", "cookie", "form", "json", "path_segment", "multipart"] {
        let (sel_json, payload_count) = match sel {
            "query" => (serde_json::json!({"selector":"query","name":"q"}), 1),
            "header" => (serde_json::json!({"selector":"header","name":"X-Missing"}), 1),
            "cookie" => (serde_json::json!({"selector":"cookie","name":"sid"}), 1),
            "form" => (serde_json::json!({"selector":"form","name":"missing_field"}), 1),
            "json" => (serde_json::json!({"selector":"json","pointer":"/nope"}), 1),
            "path_segment" => (serde_json::json!({"selector":"path_segment","index":9}), 1),
            "multipart" => (serde_json::json!({"selector":"multipart","field":"nof"}), 1),
            _ => unreachable!(),
        };
        let mut payload_json = serde_json::json!([{"type":"list","values":["x"]}]);
        // multipart/form/json 缺目标在 body 空时同样拒；path_segment 越界拒。
        let _ = &mut payload_json;
        let j = spec_json(
            &req,
            serde_json::json!({
                "mode":"single",
                "budget":{"max_requests":1},
                "positions":[sel_json],
                "payloads":[{"type":"list","values":["x"]}]
            }),
        );
        let result = prepare_spec(&j);
        let _ = payload_count;
        let ok = matches!(
            result,
            Err(SpecError::SelectorTargetMissing(_))
                | Err(SpecError::SelectorMarkerConflict(_))
                | Err(SpecError::Json(_))
        );
        assert!(
            ok,
            "selector {sel} missing target must be rejected (fail-closed)"
        );
    }
}

#[test]
fn selector_marker_conflict_rejected() {
    // target 含标记 + query 选择器 = 同字段歧义拒。
    let req = template("/f?q=§P§", vec![host_hdr()], b"a=1");
    let j = spec_json(
        &req,
        serde_json::json!({
            "mode":"single","budget":{"max_requests":1},
            "positions":[{"selector":"query","name":"q"}],
            "payloads":[{"type":"list","values":["x"]},{"type":"list","values":["y"]}]
        }),
    );
    assert!(matches!(prepare_spec(&j), Err(SpecError::SelectorMarkerConflict(_))));
}

#[test]
fn payload_count_mismatch_rejected() {
    let req = template("/f?q=§P§", vec![host_hdr()], b"");
    // 位置 1（marker），payloads 2。
    let j = spec_json(
        &req,
        serde_json::json!({"mode":"single","budget":{"max_requests":1},
            "payloads":[{"type":"list","values":["x"]},{"type":"list","values":["y"]}]})
    );
    assert!(matches!(
        prepare_spec(&j),
        Err(SpecError::PayloadCountMismatch { positions: 1, payloads: 2 })
    ));
}

#[test]
fn budget_missing_and_invalid_rejected() {
    let req = template("/f", vec![host_hdr()], b"");
    // 无 budget（无显式 budget = spec 拒绝，D5 fail-closed）。
    let j = spec_json(
        &req,
        serde_json::json!({"mode":"single","payloads":[{"type":"list","values":["x"]}]})
    );
    assert!(matches!(prepare_spec(&j), Err(SpecError::Json(_))));
    // max_requests = 0。
    let j = spec_json(
        &req,
        serde_json::json!({"mode":"single","budget":{"max_requests":0},
            "payloads":[{"type":"list","values":["x"]}]})
    );
    assert!(matches!(prepare_spec(&j), Err(SpecError::BudgetInvalid(_))));
    // 速率对不一致。
    let j = spec_json(
        &req,
        serde_json::json!({"mode":"single","budget":{"max_requests":1,"rate_window_ms":1000},
            "payloads":[{"type":"list","values":["x"]}]})
    );
    assert!(matches!(prepare_spec(&j), Err(SpecError::BudgetInvalid(_))));
}

#[test]
fn template_crlf_header_injection_rejected_at_preflight() {
    // 模板头值含 CRLF → serialize 守卫拒（既有词形复用）。
    let req = template(
        "/f",
        vec![host_hdr(), HeaderRecord::new("X-Base", "a\r\nX: y")],
        b"",
    );
    let j = spec_json(
        &req,
        serde_json::json!({"mode":"single","budget":{"max_requests":1},
            "payloads":[{"type":"list","values":["x"]}]})
    );
    assert!(matches!(prepare_spec(&j), Err(SpecError::TemplateInvalid(_))));
}

// ---------------------------------------------------------------------------
// build_case_request 正例（marker + 七类选择器字节回读）
// ---------------------------------------------------------------------------

#[test]
fn build_case_marker_query_header_cookie_form() {
    let body = b"user=ORIG&keep=1".to_vec();
    let req = template(
        "/f?q=§Q§&x=1",
        vec![
            host_hdr(),
            HeaderRecord::new("X-Trace", "TR-ORIG"),
            HeaderRecord::new("Cookie", "sid=ORIG; theme=dark"),
            HeaderRecord::new("Content-Type", "application/x-www-form-urlencoded"),
        ],
        &body,
    );
    let j = spec_json(
        &req,
        serde_json::json!({
            "mode":"single","budget":{"max_requests":10},
            "positions":[
                {"selector":"header","name":"X-Trace"},
                {"selector":"cookie","name":"sid"},
                {"selector":"form","name":"user"}
            ],
            "payloads":[
                {"type":"list","values":["Q1"]},
                {"type":"list","values":["H1"]},
                {"type":"list","values":["S1"]},
                {"type":"list","values":["F1"]}
            ]
        }),
    );
    let prepared = prepare_spec(&j).expect("spec ok");
    assert_eq!(prepared.position_count, 4);
    assert_eq!(prepared.estimated_requests, 4); // single = Σ 1×4
    // 四位置同时注入（cartesian 单 case 手工构建面）。
    let built = build_case_request(
        &prepared.spec,
        &prepared.marker_spans,
        &[Some("QVAL"), Some("HVAL"), Some("SVAL"), Some("FVAL")],
    )
    .expect("build ok");
    assert!(built.target.contains("q=QVAL"), "marker 注入 target：{}", built.target);
    assert_eq!(
        built
            .headers
            .iter()
            .find(|h| h.name == "X-Trace")
            .map(|h| h.value.as_str()),
        Some("HVAL"),
        "header 选择器注入"
    );
    assert_eq!(
        built
            .headers
            .iter()
            .find(|h| h.name == "Cookie")
            .map(|h| h.value.as_str()),
        Some("sid=SVAL; theme=dark"),
        "cookie 选择器注入（其余 cookie 保原值）"
    );
    assert!(String::from_utf8_lossy(&built.body).contains("user=FVAL"), "form 选择器注入");
    assert!(String::from_utf8_lossy(&built.body).contains("keep=1"), "非位置字段保原值");
    // serialize 守卫直通（构建产物可出站）。
    wsw_message::serialize::serialize_request(&built).expect("built serializable");
}

#[test]
fn build_case_json_pointer_and_path_segment() {
    let body = br#"{"a": {"b": "ORIG"}, "n": 5}"#.to_vec();
    let req = template("/aa/bb/cc", vec![host_hdr(), HeaderRecord::new("Content-Type", "application/json")], &body);
    let j = spec_json(
        &req,
        serde_json::json!({
            "mode":"single","budget":{"max_requests":10},
            "positions":[
                {"selector":"json","pointer":"/a/b"},
                {"selector":"path_segment","index":1}
            ],
            "payloads":[
                {"type":"list","values":["J1"]},
                {"type":"list","values":["SEG"]}
            ]
        }),
    );
    let prepared = prepare_spec(&j).expect("spec ok");
    assert_eq!(prepared.position_count, 2);
    let built = build_case_request(
        &prepared.spec,
        &prepared.marker_spans,
        &[Some("JVAL"), Some("SEG")],
    )
    .expect("build ok");
    let out = String::from_utf8_lossy(&built.body).to_string();
    assert!(out.contains("\"JVAL\""), "json pointer 注入：{out}");
    assert!(out.contains("\"n\":5") || out.contains("\"n\": 5"), "邻位标量保原值：{out}");
    assert!(built.target.starts_with("/aa/SEG/"), "path_segment 注入：{}", built.target);
}

#[test]
fn build_case_multipart_field() {
    let boundary = "bnd7";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"\r\n\r\nORIGINAL\r\n--{boundary}--\r\n"
    );
    let req = template(
        "/f",
        vec![
            host_hdr(),
            HeaderRecord::new(
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            ),
        ],
        body.as_bytes(),
    );
    let j = spec_json(
        &req,
        serde_json::json!({
            "mode":"single","budget":{"max_requests":10},
            "positions":[{"selector":"multipart","field":"file"}],
            "payloads":[{"type":"list","values":["MVAL"]}]
        }),
    );
    let prepared = prepare_spec(&j).expect("spec ok");
    let built = build_case_request(
        &prepared.spec,
        &prepared.marker_spans,
        &[Some("MVAL")],
    )
    .expect("build ok");
    let out = String::from_utf8_lossy(&built.body).to_string();
    assert!(out.contains("MVAL"), "multipart 注入：{out}");
    assert!(!out.contains("ORIGINAL"), "原值替换：{out}");
    assert!(out.contains(&format!("--{boundary}--")), "boundary 结构保持：{out}");
}

#[test]
fn build_case_processor_chain() {
    let req = template("/f", vec![host_hdr()], b"hello");
    let j = spec_json(
        &req,
        serde_json::json!({
            "mode":"single","budget":{"max_requests":10},
            "positions":[],
            "payloads":[],
            "processors":[
                {"type":"header_set","name":"X-Fuzz","value":"on"},
                {"type":"body_replace","from":"hello","to":"processed"}
            ]
        }),
    );
    let prepared = prepare_spec(&j).expect("spec ok");
    let built = build_case_request(&prepared.spec, &prepared.marker_spans, &[])
        .expect("build ok");
    assert!(built.headers.iter().any(|h| h.name == "X-Fuzz" && h.value == "on"));
    assert_eq!(built.body, b"processed");
}

// ---------------------------------------------------------------------------
// Payload 源（字典/范围）+ Mutator 展开
// ---------------------------------------------------------------------------

#[test]
fn wordlist_bom_comments_blank_lines_skipped() {
    let dir = std::env::temp_dir().join(format!(
        "wsw4-wl-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("tempdir");
    let path = dir.join("wl.txt");
    std::fs::write(&path, "\u{feff}admin\r\n\r\n# 注释行\n   \npassword\n#x\nroot\n").expect("write");
    let out = PayloadSource::Wordlist {
        path: path.to_string_lossy().to_string(),
    }
    .resolve()
    .expect("resolve ok");
    assert_eq!(out, vec!["admin", "password", "root"], "BOM/空行/# 注释全跳过");
    // 缺文件拒。
    assert!(matches!(
        PayloadSource::Wordlist { path: dir.join("nope.txt").to_string_lossy().to_string() }.resolve(),
        Err(SpecError::PayloadSource(_))
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn range_generators() {
    let out = PayloadSource::RangeNumber { from: 2, to: 8, step: 3 }.resolve().unwrap();
    assert_eq!(out, vec!["2", "5", "8"]);
    let out = PayloadSource::RangeAlpha { from_len: 1, to_len: 2 }.resolve().unwrap();
    assert_eq!(out.len(), 26 + 26 * 26);
    assert_eq!(out[0], "a");
    assert_eq!(out[25], "z");
    assert_eq!(out[26], "aa");
    let out = PayloadSource::RangeDate { from: "2026-01-30".into(), to: "2026-02-02".into(), step_days: 1 }
        .resolve()
        .unwrap();
    assert_eq!(out, vec!["2026-01-30", "2026-01-31", "2026-02-01", "2026-02-02"]);
    // 空列表拒（fail-closed）。
    assert!(matches!(
        PayloadSource::RangeNumber { from: 5, to: 1, step: 1 }.resolve(),
        Err(SpecError::PayloadSource(_))
    ));
}

#[test]
fn mutator_expansion_in_prepared_estimate() {
    // mutator 扇出后估算按有效列表口径（判据 2）。
    let req = template("/f?q=§P§", vec![host_hdr()], b"");
    let j = spec_json(
        &req,
        serde_json::json!({
            "mode":"cartesian","budget":{"max_requests":100},
            "payloads":[{"type":"list","values":["a","b"]}],
            "mutators":[{"type":"concat","values":["1","2"],"position":"suffix"}]
        }),
    );
    let prepared = prepare_spec(&j).expect("spec ok");
    assert_eq!(prepared.effective_payloads[0].len(), 4); // a1,a2,b1,b2
    assert_eq!(prepared.estimated_requests, 4);
}

#[test]
fn encode_kinds_via_mutators() {
    let base = vec!["a b".to_string()];
    let out = apply_mutators(&base, &[Mutator::Encode { chain: vec![EncodeKind::Url] }]).unwrap();
    assert_eq!(out, vec!["a%20b"]);
    let out = apply_mutators(&base, &[Mutator::Encode { chain: vec![EncodeKind::Base64] }]).unwrap();
    assert_eq!(out, vec!["YSBi"]);
    let out = apply_mutators(&base, &[Mutator::Encode { chain: vec![EncodeKind::Base64Url] }]).unwrap();
    assert_eq!(out, vec!["YSBh".replace('h', "i")]); // base64url 无 padding：YSBi
    let out = apply_mutators(&base, &[Mutator::Encode { chain: vec![EncodeKind::Hex] }]).unwrap();
    assert_eq!(out, vec!["612062"]);
    let out = apply_mutators(&base, &[Mutator::Encode { chain: vec![EncodeKind::HtmlEntity] }]).unwrap();
    assert_eq!(out, vec!["a b"]);
    let out = apply_mutators(&base, &[Mutator::Concat { values: vec!["+z".into()], position: ConcatPos::Suffix }]).unwrap();
    assert_eq!(out, vec!["a b+z"]);
}

#[test]
fn processors_crlf_value_rejected_at_build() {
    let req = template("/f", vec![host_hdr()], b"");
    let err = apply_processors(
        &mut req.clone(),
        &[Processor::HeaderSet { name: "X".into(), value: "a\r\nX: y".into() }],
    );
    assert!(matches!(err, Err(SpecError::CaseBuild(_))));
}
