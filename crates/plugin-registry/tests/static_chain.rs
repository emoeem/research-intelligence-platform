//! P6-1 集成测试：两示例 Manifest 七条校验通过件 + 反例拒绝 + 静态链
//! 全路径（ROADMAP P6-1 完成判据）。
//!
//! 判据对照（PHASE-ROADMAP §7.3 P6-1 行）：
//! - knowledge.rtis-redteam 与 tool.rip.python-workers（PLUGIN-CONTRACT
//!   §10，schemas/plugin-manifest/examples/ 落位件）按 §4.2 七条校验通过；
//! - 缺 license 反例与 git 浮动分支（pinned 缺 value）反例均被拒绝且输出
//!   机器可读报告；
//! - strict 模式未知字段反例拒绝（v1.json additionalProperties: false，D4）；
//! - DISCOVERED → VALIDATED → INSTALLED 静态链与 checksums.sha256 /
//!   plugin.lock 落盘物可复核。
//!
//! 测试纪律：全部用临时目录（std::env::temp_dir，测试后清理），不触
//! migrations/ 与任何 platform*.db，不在宿主机安装依赖。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use plugin_registry::{
    now_unix, parse_document, read_lock, render_report_text, validate_document, write_checksums,
    StaticAction, StaticChain, TrustLevel, CHECKSUMS_FILENAME,
};
use serde_json::Value;

const EXAMPLES_DIR: &str = "../../schemas/plugin-manifest/examples";

fn example_path(name: &str, ext: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(EXAMPLES_DIR)
        .join(format!("{name}.{ext}"))
}

fn read_example(name: &str, ext: &str) -> String {
    fs::read_to_string(example_path(name, ext))
        .unwrap_or_else(|e| panic!("read example {name}.{ext}: {e}"))
}

/// 唯一临时目录（进程内计数器 + 进程号，测试结束清理）。
fn temp_pkg(tag: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("p6-static-chain-{tag}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// knowledge 示例的包体：capability entry = README.md（P6-5 真实落位对齐
/// 后的示例口径），另有 L3 docs。
fn build_knowledge_package() -> PathBuf {
    let dir = temp_pkg("knowledge");
    fs::create_dir_all(dir.join("docs")).unwrap();
    fs::write(dir.join("README.md"), "# RTIS 红队知识包\n").unwrap();
    fs::write(dir.join("docs/usage.md"), "# RTIS 红队知识包使用说明\n").unwrap();
    dir
}

/// tool 示例的包体：三个 worker 入口脚本（占位内容，P6-1 只验存在性）。
fn build_tool_package() -> PathBuf {
    let dir = temp_pkg("tool");
    fs::write(dir.join("parsing_worker.py"), "# parsing worker entry\n").unwrap();
    fs::write(
        dir.join("embedding_worker.py"),
        "# embedding worker entry\n",
    )
    .unwrap();
    fs::write(
        dir.join("evaluation_worker.py"),
        "# evaluation worker entry\n",
    )
    .unwrap();
    dir
}

fn existing(ids: &[&str]) -> std::collections::HashSet<String> {
    ids.iter().map(|s| (*s).to_string()).collect()
}

fn assert_seven_checks_pass(report: &plugin_registry::ValidationReport, context: &str) {
    assert!(
        report.ok,
        "{context} must pass: {}",
        render_report_text(report)
    );
    let rules: Vec<u8> = report.checks.iter().map(|c| c.rule).collect();
    assert_eq!(
        rules,
        vec![1, 2, 3, 4, 5, 6, 7],
        "{context} checks in §4.2 order"
    );
    assert!(
        report.checks.iter().all(|c| c.ok),
        "{context} every executed check ok"
    );
}

// ---------------------------------------------------------------------------
// 正例：两示例 Manifest（json / yaml 双份）七条全过
// ---------------------------------------------------------------------------

#[test]
fn knowledge_example_passes_seven_rules() {
    let yaml_doc = read_example("knowledge.rtis-redteam", "yaml");
    let json_doc = read_example("knowledge.rtis-redteam", "json");
    // 双份一致：两个通道解析出的 Value 逐键相等。
    assert_eq!(
        parse_document(&yaml_doc).unwrap(),
        parse_document(&json_doc).unwrap(),
        "json/yaml twins must parse to identical values"
    );
    let pkg = build_knowledge_package();
    let report = validate_document(&yaml_doc, Some(&pkg), &existing(&[]), &Default::default());
    assert_seven_checks_pass(&report, "knowledge.rtis-redteam");
    assert_eq!(report.plugin_id.as_deref(), Some("knowledge.rtis-redteam"));
    assert_eq!(report.version.as_deref(), Some("0.1.0"));
    assert_eq!(report.kind.as_deref(), Some("knowledge"));
    // 机器可读：报告 JSON 可解析且 ok=true。
    let parsed: Value =
        serde_json::from_str(&report.to_json_string()).expect("report is valid JSON");
    assert_eq!(parsed["ok"], Value::Bool(true));
    let _ = fs::remove_dir_all(&pkg);
}

#[test]
fn tool_example_passes_seven_rules() {
    let yaml_doc = read_example("tool.rip.python-workers", "yaml");
    let json_doc = read_example("tool.rip.python-workers", "json");
    assert_eq!(
        parse_document(&yaml_doc).unwrap(),
        parse_document(&json_doc).unwrap(),
        "json/yaml twins must parse to identical values"
    );
    let pkg = build_tool_package();
    let report = validate_document(&yaml_doc, Some(&pkg), &existing(&[]), &Default::default());
    assert_seven_checks_pass(&report, "tool.rip.python-workers");
    assert_eq!(report.plugin_id.as_deref(), Some("tool.rip.python-workers"));
    assert_eq!(report.kind.as_deref(), Some("tool"));
    let _ = fs::remove_dir_all(&pkg);
}

// ---------------------------------------------------------------------------
// 反例 1：缺 license（rule 2 必填拒绝，报告机器可读且点名 license）
// ---------------------------------------------------------------------------

#[test]
fn missing_license_rejected_with_machine_readable_report() {
    let doc = read_example("knowledge.rtis-redteam", "yaml");
    let mut manifest = parse_document(&doc).unwrap();
    manifest.as_object_mut().unwrap().remove("license").unwrap();
    let tampered = serde_json::to_string_pretty(&manifest).unwrap();
    let pkg = build_knowledge_package();
    let report = validate_document(&tampered, Some(&pkg), &existing(&[]), &Default::default());
    assert!(!report.ok);
    assert_eq!(report.errors.len(), 1);
    assert_eq!(
        report.errors[0].rule, 2,
        "missing license is a schema required failure"
    );
    assert!(
        report.errors[0].message.contains("license"),
        "message must name the missing field: {}",
        report.errors[0].message
    );
    // §4.2 失败即停：rule 3 到 7 不再执行。
    let rules: Vec<u8> = report.checks.iter().map(|c| c.rule).collect();
    assert_eq!(rules, vec![1, 2]);
    // 机器可读报告行。
    let parsed: Value =
        serde_json::from_str(&report.to_json_string()).expect("report is valid JSON");
    assert_eq!(parsed["ok"], Value::Bool(false));
    assert_eq!(parsed["errors"][0]["rule"], Value::from(2));
    assert!(parsed["errors"][0]["code"].as_str().unwrap().len() > 4);
    let _ = fs::remove_dir_all(&pkg);
}

// ---------------------------------------------------------------------------
// 反例 2：git 浮动分支（pinned 缺 value 与 kind=branch 两种形态）
// ---------------------------------------------------------------------------

#[test]
fn git_floating_branch_rejected() {
    let doc = read_example("knowledge.rtis-redteam", "yaml");
    let mut manifest = parse_document(&doc).unwrap();
    let obj = manifest.as_object_mut().unwrap();
    obj["source"] = serde_json::json!({
        "type": "git",
        "url": "https://github.com/emoeem/rtis-kit",
        "host": "github",
        "pinned": {"kind": "commit"}
    });
    let pkg = build_knowledge_package();
    let tampered = serde_json::to_string(&manifest).unwrap();
    let report = validate_document(&tampered, Some(&pkg), &existing(&[]), &Default::default());
    // pinned 缺 value：schema 不拦（pinned 结构存在），rule 5 精确归因。
    assert_eq!(report.errors[0].rule, 5, "{}", render_report_text(&report));
    assert_eq!(report.errors[0].code, "pin_value_missing");
    assert!(
        report.errors[0].message.contains("PLUGIN-CONTRACT 6")
            || report.errors[0].message.contains("value"),
        "{}",
        report.errors[0].message
    );

    // kind=branch：浮动分支拒绝（草案 D8）。
    let mut manifest2 = parse_document(&doc).unwrap();
    manifest2.as_object_mut().unwrap()["source"] = serde_json::json!({
        "type": "git",
        "url": "https://github.com/emoeem/rtis-kit",
        "host": "github",
        "pinned": {"kind": "branch", "value": "main"}
    });
    let pkg2 = build_knowledge_package();
    let report2 = validate_document(
        &serde_json::to_string(&manifest2).unwrap(),
        Some(&pkg2),
        &existing(&[]),
        &Default::default(),
    );
    assert_eq!(report2.errors[0].rule, 5);
    assert_eq!(report2.errors[0].code, "floating_branch_rejected");
    assert!(
        report2.errors[0].message.contains("D8"),
        "floating branch message cites draft D8: {}",
        report2.errors[0].message
    );
    let _ = fs::remove_dir_all(&pkg);
    let _ = fs::remove_dir_all(&pkg2);
}

// ---------------------------------------------------------------------------
// 反例 3：strict 模式未知字段拒绝（D4）
// ---------------------------------------------------------------------------

#[test]
fn strict_unknown_field_rejected() {
    let doc = read_example("tool.rip.python-workers", "json");
    let mut manifest = parse_document(&doc).unwrap();
    manifest
        .as_object_mut()
        .unwrap()
        .insert("hacker_extra".to_string(), Value::from(true));
    let pkg = build_tool_package();
    let tampered = serde_json::to_string(&manifest).unwrap();
    let report = validate_document(&tampered, Some(&pkg), &existing(&[]), &Default::default());
    assert!(!report.ok);
    assert_eq!(report.errors[0].rule, 2);
    assert_eq!(report.errors[0].code, "schema_violation");
    assert!(
        report.errors[0].message.contains("hacker_extra"),
        "unknown field must be named in the machine-readable message: {}",
        report.errors[0].message
    );
    let _ = fs::remove_dir_all(&pkg);
}

// ---------------------------------------------------------------------------
// 静态链：DISCOVERED → VALIDATED → INSTALLED 全路径 + 落盘物复核
// ---------------------------------------------------------------------------

#[test]
fn static_chain_discover_validate_install() {
    for (name, pkg_builder) in [
        (
            "knowledge.rtis-redteam",
            build_knowledge_package as fn() -> PathBuf,
        ),
        (
            "tool.rip.python-workers",
            build_tool_package as fn() -> PathBuf,
        ),
    ] {
        let doc = read_example(name, "yaml");
        let pkg = pkg_builder();
        let mut chain = StaticChain::new();

        let id = chain
            .discover(&doc)
            .unwrap_or_else(|e| panic!("discover {name}: {e}"));
        assert_eq!(
            chain.state_of(&id),
            Some(plugin_registry::PluginState::Discovered)
        );

        let report = chain.validate(&id, Some(&pkg)).unwrap();
        assert!(
            report.ok,
            "{name} validate: {}",
            render_report_text(&report)
        );
        assert_eq!(
            chain.state_of(&id),
            Some(plugin_registry::PluginState::Validated)
        );

        let lock = chain
            .install(&id, &pkg, TrustLevel::Local)
            .unwrap_or_else(|e| panic!("install {name}: {e}"));
        assert_eq!(
            chain.state_of(&id),
            Some(plugin_registry::PluginState::Installed)
        );

        // plugin.lock 字段复核（§6 七要素）。
        assert_eq!(lock.plugin_id, id);
        assert_eq!(lock.resolved_version, "0.1.0");
        assert_eq!(lock.source_type, "local_dir");
        assert_eq!(lock.license, "UNLICENSED");
        assert_eq!(lock.trust_level, TrustLevel::Local);
        assert!(lock.installed_at_unix > 0);
        assert!(lock.installed_at_unix <= now_unix() + 1);
        assert_eq!(lock.sha256.len(), 64);
        let pinned = lock.pinned.as_ref().expect("local_dir pinned recorded");
        assert_eq!(pinned.kind, "commit");
        // P6-5 起 pinned 为全量 sha（git rev-parse HEAD，示例随批次演进）。
        assert_eq!(pinned.value.len(), 40, "full sha pinned: {}", pinned.value);

        // 落盘物在位且可独立复核：checksums.sha256 行格式 + plugin.lock 读回。
        let checksums = fs::read_to_string(pkg.join(CHECKSUMS_FILENAME)).unwrap();
        for line in checksums.lines() {
            let bytes = line.as_bytes();
            assert!(bytes.len() >= 67, "checksum line shape: {line}");
            assert!(
                bytes[..64]
                    .iter()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                    && &bytes[64..66] == b"  ",
                "sha256sum-format line: {line}"
            );
        }
        let reread = read_lock(&pkg).unwrap();
        assert_eq!(reread, lock, "plugin.lock roundtrip via file");

        // 跃迁流水：discover → validate → install 三行 JSONL。
        let jsonl = chain.records_jsonl();
        let parsed_lines: Vec<Value> = jsonl
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).expect("jsonl line is valid JSON"))
            .collect();
        assert_eq!(parsed_lines.len(), 3, "{name} transitions: {jsonl}");
        let actions: Vec<&str> = parsed_lines
            .iter()
            .map(|v| v["action"].as_str().unwrap())
            .collect();
        assert_eq!(actions, vec!["discover", "validate", "install"]);
        assert!(parsed_lines
            .iter()
            .all(|v| v.get("at_unix").and_then(Value::as_i64).is_some()));
        let _ = fs::remove_dir_all(&pkg);
    }
}

#[test]
fn install_rejects_non_validated_state() {
    let doc = read_example("knowledge.rtis-redteam", "yaml");
    let pkg = build_knowledge_package();
    let mut chain = StaticChain::new();
    let id = chain.discover(&doc).unwrap();
    // DISCOVERED 直接 install：非法跃迁（机器可读）。
    let err = chain.install(&id, &pkg, TrustLevel::Local).unwrap_err();
    assert_eq!(err.code, "illegal_transition");
    assert!(err.to_json_string().contains("illegal_transition"));
    // discover 重复 id：冲突拒绝。
    let err2 = chain.discover(&doc).unwrap_err();
    assert_eq!(err2.code, "id_conflict");
    let _ = fs::remove_dir_all(&pkg);
}

#[test]
fn checksum_tamper_sends_install_to_invalid() {
    let doc = read_example("knowledge.rtis-redteam", "yaml");
    let pkg = build_knowledge_package();
    // 预写清单再篡改内容：install 的 verify 路径命中 mismatch。
    fs::write(pkg.join("docs/usage.md"), "original\n").unwrap();
    write_checksums(&pkg).unwrap();
    fs::write(pkg.join("docs/usage.md"), "tampered\n").unwrap();

    let mut chain = StaticChain::new();
    let id = chain.discover(&doc).unwrap();
    chain.validate(&id, Some(&pkg)).unwrap();
    assert_eq!(
        chain.state_of(&id),
        Some(plugin_registry::PluginState::Validated)
    );

    let err = chain.install(&id, &pkg, TrustLevel::Local).unwrap_err();
    assert_eq!(err.code, "checksum_verification_failed");
    let details = err.details.as_ref().expect("machine-readable issue list");
    assert!(
        serde_json::to_string(details).unwrap().contains("mismatch"),
        "details carry the mismatch entry: {details}"
    );
    // §5.2：校验和失败进 INVALID。
    assert_eq!(
        chain.state_of(&id),
        Some(plugin_registry::PluginState::Invalid)
    );
    // 流水含 validate_reject（install 期失败回落 INVALID 的记录）。
    assert!(chain
        .records()
        .iter()
        .any(|r| r.action == StaticAction::ValidateReject.as_str()));
    let _ = fs::remove_dir_all(&pkg);
}

#[test]
fn tamper_recovery_allows_fresh_chain_to_install() {
    let doc = read_example("tool.rip.python-workers", "yaml");
    let pkg = build_tool_package();
    let mut chain = StaticChain::new();
    let id = chain.discover(&doc).unwrap();
    let report = chain.validate(&id, Some(&pkg)).unwrap();
    assert!(report.ok);
    chain.install(&id, &pkg, TrustLevel::Local).unwrap();

    // 安装后篡改 + 复原：verify 回到绿（内容幂等）。
    let target = pkg.join("parsing_worker.py");
    let original = fs::read(&target).unwrap();
    fs::write(&target, b"changed").unwrap();
    assert!(plugin_registry::verify_checksums(&pkg).is_err());
    fs::write(&target, &original).unwrap();
    plugin_registry::verify_checksums(&pkg).expect("restored content verifies");
    let _ = fs::remove_dir_all(&pkg);
}

// ---------------------------------------------------------------------------
// SK-2：三源真实 Manifest（manifests/plugins/）七条校验 + rule 3 反例拒绝
// （P6-10，安全知识源计划 §3/§5；git 来源固定 Commit，fixture 为最小快照）
// ---------------------------------------------------------------------------

const SK2_MANIFESTS_DIR: &str = "../../manifests/plugins";
const SK2_FIXTURES_DIR: &str = "../../tests/fixtures/sk2";

fn sk2_manifest_text(name: &str, ext: &str) -> String {
    fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(SK2_MANIFESTS_DIR)
            .join(format!("{name}.{ext}")),
    )
    .unwrap_or_else(|e| panic!("read sk2 manifest {name}.{ext}: {e}"))
}

/// fixture 递归拷贝进临时目录（install 落 checksums/plugin.lock，不得污染仓库 fixture）。
fn sk2_package(fixture: &str) -> PathBuf {
    fn copy_tree(src: &Path, dst: &Path) {
        fs::create_dir_all(dst).unwrap();
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let target = dst.join(entry.file_name());
            if entry.path().is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), &target).unwrap();
            }
        }
    }
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(SK2_FIXTURES_DIR)
        .join(fixture);
    let dir = temp_pkg(fixture);
    copy_tree(&src, &dir);
    dir
}

/// 三源 Manifest：yaml/json 双份解析一致 + 七条校验通过 + 固定 Commit pin 在位。
#[test]
fn sk2_source_manifests_pass_seven_rules() {
    for (name, fixture, _license) in [
        ("knowledge.awesome-hacking", "awesome-hacking", "CC0-1.0"),
        ("knowledge.secskills", "secskills", "Apache-2.0"),
        ("knowledge.tongui-course-notes", "tongui-course-notes", "UNLICENSED"),
    ] {
        let yaml_doc = sk2_manifest_text(name, "yaml");
        let json_doc = sk2_manifest_text(name, "json");
        assert_eq!(
            parse_document(&yaml_doc).unwrap(),
            parse_document(&json_doc).unwrap(),
            "{name}: json/yaml twins must parse to identical values"
        );
        let pkg = sk2_package(fixture);
        let report = validate_document(&yaml_doc, Some(&pkg), &existing(&[]), &Default::default());
        assert_seven_checks_pass(&report, name);
        assert_eq!(report.plugin_id.as_deref(), Some(name));
        assert_eq!(report.kind.as_deref(), Some("knowledge"));
        assert_eq!(report.version.as_deref(), Some("0.1.0"));
        let parsed: Value =
            serde_json::from_str(&report.to_json_string()).expect("report is valid JSON");
        assert_eq!(parsed["ok"], Value::Bool(true));
        let _ = fs::remove_dir_all(&pkg);
    }
}

/// rule 3 反例：parser 配错与 source_path 路径穿越以 knowledge_provides_invalid
/// 精确拒绝（license_policy 词表由 v1.json enum 在 rule 2 先拒，一并断言双处
/// 防御）；附带 rule 5 浮动分支拒绝。
#[test]
fn sk2_rule3_counterexamples_rejected() {
    // 反例 1：skill_candidate 的 parser 配成 markdown_links（配对表要求
    // skill_frontmatter）。
    let doc = sk2_manifest_text("knowledge.secskills", "yaml");
    let mut manifest = parse_document(&doc).unwrap();
    manifest.as_object_mut().unwrap()["capabilities"][0]["provides"]["kinds"]
        ["skill_candidate"]["parser"] = Value::from("markdown_links");
    let pkg = sk2_package("secskills");
    let report =
        validate_document(&serde_json::to_string(&manifest).unwrap(), Some(&pkg), &existing(&[]), &Default::default());
    assert!(!report.ok);
    assert_eq!(report.errors[0].rule, 3);
    assert_eq!(report.errors[0].code, "knowledge_provides_invalid");
    assert!(
        report.errors[0].message.contains("skill_frontmatter"),
        "message names the expected parser: {}",
        report.errors[0].message
    );

    // 反例 2：license_policy 出词表（v1.json enum 冻结词表，rule 2 先拒，
    // ingest 侧 LICENSE_POLICIES 为双处防御）。
    let mut manifest2 = parse_document(&doc).unwrap();
    manifest2.as_object_mut().unwrap()["capabilities"][0]["provides"]["kinds"]
        ["catalog_entry"]["license_policy"] = Value::from("copy_everything");
    let pkg2 = sk2_package("secskills");
    let report2 = validate_document(
        &serde_json::to_string(&manifest2).unwrap(),
        Some(&pkg2),
        &existing(&[]),
        &Default::default(),
    );
    assert!(!report2.ok);
    assert_eq!(report2.errors[0].rule, 2, "policy vocabulary is schema-frozen");
    assert_eq!(report2.errors[0].code, "schema_violation");

    // 反例 3：source_path 路径穿越（.. 上跳拒绝）。
    let tongui = sk2_manifest_text("knowledge.tongui-course-notes", "yaml");
    let mut manifest3 = parse_document(&tongui).unwrap();
    manifest3.as_object_mut().unwrap()["capabilities"][0]["provides"]["kinds"]
        ["catalog_entry"]["source_path"] = Value::from("../escape.md");
    let pkg3 = sk2_package("tongui-course-notes");
    let report3 = validate_document(
        &serde_json::to_string(&manifest3).unwrap(),
        Some(&pkg3),
        &existing(&[]),
        &Default::default(),
    );
    assert!(!report3.ok);
    assert_eq!(report3.errors[0].rule, 3);
    assert_eq!(report3.errors[0].code, "knowledge_provides_invalid");

    // 反例 4（rule 5 附带复核）：浮动分支安装被拒（pinned.kind=branch）。
    let mut manifest4 = parse_document(&tongui).unwrap();
    manifest4.as_object_mut().unwrap()["source"]["pinned"] =
        serde_json::json!({"kind": "branch", "value": "master"});
    let report4 = validate_document(
        &serde_json::to_string(&manifest4).unwrap(),
        Some(&pkg3),
        &existing(&[]),
        &Default::default(),
    );
    assert!(!report4.ok);
    assert_eq!(report4.errors[0].rule, 5);
    assert_eq!(report4.errors[0].code, "floating_branch_rejected");

    let _ = fs::remove_dir_all(&pkg);
    let _ = fs::remove_dir_all(&pkg2);
    let _ = fs::remove_dir_all(&pkg3);
}

// ---------------------------------------------------------------------------
// rule 7 边界：缺包体时显式 deferred，不伪装成已检查
// ---------------------------------------------------------------------------

#[test]
fn validate_without_package_defers_rule7() {
    let doc = read_example("knowledge.rtis-redteam", "yaml");
    let report = validate_document(&doc, None, &existing(&[]), &Default::default());
    assert!(report.ok);
    let rule7 = report.checks.iter().find(|c| c.rule == 7).unwrap();
    assert_eq!(
        rule7.note.as_deref().unwrap(),
        "package_root not provided: entry existence check deferred to install"
    );
}

// ---------------------------------------------------------------------------
// id 唯一性贯穿 validate（rule 3）：与既有索引冲突即拒
// ---------------------------------------------------------------------------

#[test]
fn validate_rejects_id_conflict_with_existing_index() {
    let doc = read_example("knowledge.rtis-redteam", "yaml");
    let pkg = build_knowledge_package();
    let report = validate_document(
        &doc,
        Some(&pkg),
        &existing(&["knowledge.rtis-redteam"]),
        &Default::default(),
    );
    assert!(!report.ok);
    assert_eq!(report.errors[0].rule, 3);
    assert_eq!(report.errors[0].code, "id_conflict");
    let _ = fs::remove_dir_all(&pkg);
}
