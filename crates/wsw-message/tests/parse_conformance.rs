//! Golden conformance corpus: wire fixtures + byte hash + full ParseOutcome equality.

use std::fs;
use std::path::{Path, PathBuf};
use wsw_message::cas::sha256_hex;
use wsw_message::parse::parse_wire;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/wire")
}

fn wire_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = fs::read_dir(dir)
        .expect("fixture directory")
        .map(|entry| entry.expect("fixture entry").path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|ext| ext == "http" || ext == "bin")
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}

fn assert_golden(path: &Path, expected_status: &str) -> serde_json::Value {
    let wire = fs::read(path).expect("read wire fixture");
    let expected_path = path.with_file_name(format!(
        "{}.expected.json",
        path.file_name()
            .expect("fixture filename")
            .to_string_lossy()
    ));
    let expected: serde_json::Value =
        serde_json::from_slice(&fs::read(&expected_path).expect("read expected JSON"))
            .unwrap_or_else(|err| {
                panic!("{}: invalid expected JSON: {err}", expected_path.display())
            });

    assert_eq!(
        expected["wire_sha256"].as_str(),
        Some(sha256_hex(&wire).as_str()),
        "{}: fixture byte hash changed; review and deliberately update golden",
        path.display()
    );

    let outcome = parse_wire(&wire);
    let actual = serde_json::to_value(&outcome).expect("serialize ParseOutcome");
    assert_eq!(
        actual,
        expected["outcome"],
        "{}: ParseOutcome differs from pinned JSON",
        path.display()
    );
    assert_eq!(
        actual["parse_status"].as_str(),
        Some(expected_status),
        "{}: unexpected parse status",
        path.display()
    );
    actual
}

#[test]
fn ten_positive_wire_fixtures_match_pinned_json_and_hashes() {
    let root = fixture_dir();
    let files = wire_files(&root);
    assert_eq!(
        files.len(),
        10,
        "positive corpus must contain exactly 10 cases"
    );
    for path in files {
        assert_golden(&path, "ok");
    }
}

#[test]
fn seven_negative_wire_fixtures_match_pinned_json_and_hashes() {
    let root = fixture_dir().join("negative");
    let files = wire_files(&root);
    assert_eq!(
        files.len(),
        7,
        "negative corpus must contain exactly 7 cases"
    );
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy();
        let status = match name.as_ref() {
            "trunc_chunked_no_term.http"
            | "trunc_header_no_crlf.http"
            | "trunc_body_short_cl.http" => "partial",
            "oversize_header_64k.http" | "req_no_host.http" => "failed",
            "oversize_body.http" | "body_invalid_utf8.bin" => "ok",
            other => panic!("unclassified negative fixture: {other}"),
        };
        assert_golden(&path, status);
    }
}

#[test]
fn negative_fixture_error_vocabulary_and_partial_bytes_are_pinned() {
    let root = fixture_dir().join("negative");

    let chunk = assert_golden(&root.join("trunc_chunked_no_term.http"), "partial");
    assert_eq!(chunk["error_code"], "truncated_chunk");
    assert_eq!(
        chunk["message"]["body"], "YWI=",
        "body JSON 使用 base64 词表"
    );

    let header = assert_golden(&root.join("trunc_header_no_crlf.http"), "partial");
    assert_eq!(header["error_code"], "truncated_header");
    assert!(header["message"].is_null());

    let body = assert_golden(&root.join("trunc_body_short_cl.http"), "partial");
    assert_eq!(body["error_code"], "truncated_body");
    assert_eq!(body["body_declared"], 10);
    assert_eq!(body["body_received"], 3);

    let oversized_header = assert_golden(&root.join("oversize_header_64k.http"), "failed");
    assert_eq!(oversized_header["error_code"], "header_cap_exceeded");

    let missing_host = assert_golden(&root.join("req_no_host.http"), "failed");
    assert_eq!(missing_host["error_code"], "missing_host");

    let invalid_utf8 = assert_golden(&root.join("body_invalid_utf8.bin"), "ok");
    assert_eq!(
        invalid_utf8["message"]["body"], "YWJj/w==",
        "二进制体 JSON 保持 base64 原样"
    );
}
