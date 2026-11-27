//! RT-3 S4 集成期望表：32 份派发面 Manifest（31 JSON + 1 YAML-only）文件系统
//! 直读（防漂移——不复制 Manifest 内容入测试，以文件系统为准），经 D3/D4/D5
//! 派生函数 + RT-1 CapabilityProvider 既有面（D10c 注入式消费）产出
//! RuntimeDecision，逐份断言 verdict/runtime/cross_check/gap。
//!
//! # DI-3/RT-4 迁移后基线（本批判据 1，2026-11-22 实测口径）
//!
//! 29 assign + rip.ops.worker-runtime not_applicable（inconsistent 快照零成员：
//! B 组两份准入钉值补录 + C 组 rtk.mcp network none 修正后，RT-3 §6 三处
//! inconsistent gap 全部收敛——迁移完成态实证，verdict 四态词表不减；
//! WSW-7 批 zap 登记随更 29→30，设计内扩展非钉值改动——WSW-7-TASK D2）。
//!
//! # 三档风险路由（判据 6）
//!
//! 每份可派发 Manifest 按 InputRisk 三档（trusted_low_risk / untrusted /
//! high_risk）评估：native/python 声明 low→assign、unknown→inconsistent
//! (target=container)、high→reject(RT3_VM_NOT_READY)；podman_host
//! low/unknown→assign、high→reject。零「unknown 仍 assign native」反例。
//!
//! Host Capability 子事实 = RT-1 `StaticCatalogProvider::load` 种子 +
//! `CapabilityProvider::get`（D10c：不复制探测逻辑）；EgressProxy 接线子事实
//! = true（RT-3-ADR D5 论据「现状形态」三工具承载）；VM 就绪子事实 = false
//! （podman_vm adapter 设计保留面）；venv 子事实 = 声明在场即静态合成
//! Available（DI-3/RT-4 任务书 D3① 裁决：与 egress_wired=true 注入同口径，
//! 真探测面由 RT-2 python_worker_venv_e2e 既有承载 + adapter ensure_ready
//! 双层纵深）。全离线零安装，`--test-threads=1`。

use std::path::{Path, PathBuf};

use tool_engine::capability::{
    CapabilityId, CapabilityProvider, CapabilityStatus, Freshness, InputRisk, StaticCatalogProvider,
};
use tool_engine::policy::CapabilityInput;
use tool_engine::policy::{
    decide, derive_dependency_complexity, derive_isolation_requirement, derive_trust,
    input_risk_short_alias, DecisionInput, DependencyComplexity, IsolationRequirement, Target,
    TrustLevel, Verdict,
};
use tool_engine::venv::VenvStatus;

const TOOLS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../manifests/tools");
const SEED: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../manifests/capabilities/host-catalog-seed.json"
);

/// 元数据条目（R7 不派发面；RT-3-TASK §2 A9）。
const METADATA_ENTRY: &str = "rip.ops.worker-runtime";

/// YAML-only Manifest（无 JSON 孪生；合计 32 份）。
const YAML_ONLY: &str = "rip.http.tls_inspect.yaml";

/// Manifest 事实面（全部 owned，供三档路由重建 DecisionInput）。
struct Facts {
    tool_id: String,
    declared: String,
    origin: Option<String>,
    license_present: bool,
    identity_pinned: bool,
    trust_source: String,
    network_mode: Option<String>,
    secrets_present: bool,
    probe_type: Option<String>,
    capability_ref: Option<String>,
    venv_declared: bool,
    binary_path: Option<String>,
    binary_sha256: Option<String>,
    has_argv_or_subcommand: bool,
    has_budget: bool,
    permission_level: Option<String>,
}

fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// A8 派生模拟的 Rust 形态（与 2026-11-21 python 只读模拟口径一致）。
fn extract_facts(manifest: &serde_json::Value) -> Facts {
    let runtime = &manifest["runtime"];
    let rtype = runtime["type"].as_str().unwrap_or_default().to_string();
    let origin = manifest["provenance"]["origin"]
        .as_str()
        .map(str::to_string);
    let license_present = manifest["provenance"]["license"].as_str().is_some();
    let identity_pinned = match rtype.as_str() {
        "native_rust" => runtime["binary_sha256"]
            .as_str()
            .map(is_sha256_hex)
            .unwrap_or(false),
        "podman_host" => runtime["image"]
            .as_str()
            .map(|img| {
                img.find("@sha256:")
                    .map(|i| is_sha256_hex(&img[i + 9..]))
                    .unwrap_or(false)
            })
            .unwrap_or(false),
        _ => false,
    };
    let network_mode = manifest["network"]["mode"].as_str().map(str::to_string);
    let secrets_present = manifest["secrets"]
        .as_array()
        .map(|a| !a.is_empty())
        .unwrap_or(false);
    let probe_type = manifest["health"]["probe"]["type"]
        .as_str()
        .map(str::to_string);
    let capability_ref = runtime["capability_ref"].as_str().map(str::to_string);
    let venv_declared = runtime.get("venv").map(|v| !v.is_null()).unwrap_or(false);
    let binary_path = runtime["binary"].as_str().map(str::to_string);
    let binary_sha256 = runtime["binary_sha256"].as_str().map(str::to_string);
    let has_argv_or_subcommand =
        runtime.get("argv_template").is_some() || runtime.get("subcommand").is_some();
    let resources = &manifest["resources"];
    let has_budget = resources["timeout_seconds"].as_i64().is_some()
        && resources["max_output_bytes"].as_i64().is_some();
    let permission_level = manifest["permission_level"].as_str().map(str::to_string);
    let tool_id = manifest["id"].as_str().unwrap_or_default().to_string();
    let trust_source = match origin.as_deref() {
        Some("rip-native") => format!("provenance.origin=rip-native（自研在仓，{tool_id}）"),
        Some("internal-kit") => {
            format!("provenance.origin=internal-kit（平台发行 kit，{tool_id}）")
        }
        Some(other) => {
            if identity_pinned {
                format!("provenance.origin={other} + 身份钉值在位（{tool_id}）")
            } else {
                format!("provenance.origin={other} 钉值缺省 → registered（{tool_id}）")
            }
        }
        None => "provenance.origin 未声明 → registered".to_string(),
    };
    Facts {
        tool_id,
        declared: rtype,
        origin,
        license_present,
        identity_pinned,
        trust_source,
        network_mode,
        secrets_present,
        probe_type,
        capability_ref,
        venv_declared,
        binary_path,
        binary_sha256,
        has_argv_or_subcommand,
        has_budget,
        permission_level,
    }
}

/// 事实面 → DecisionInput（D3/D4/D5 派生函数逐个消费；输入风险档位为参数）。
fn to_input<'a>(
    facts: &'a Facts,
    provider: &StaticCatalogProvider,
    risk: InputRisk,
) -> DecisionInput<'a> {
    let trust = derive_trust(
        facts.origin.as_deref(),
        facts.license_present,
        facts.identity_pinned,
    );
    let complexity = derive_dependency_complexity(&facts.declared)
        .unwrap_or_else(|e| panic!("complexity 派生 {}：{e}", facts.tool_id));
    let isolation =
        derive_isolation_requirement(facts.network_mode.as_deref(), risk, facts.secrets_present);
    // Host Capability：RT-1 Provider 既有面只读取回（D10c 注入式消费）。
    let capability = facts.capability_ref.as_deref().and_then(|cap_ref| {
        let id = CapabilityId::new(cap_ref)
            .unwrap_or_else(|e| panic!("capability_ref 词表 {}：{e}", facts.tool_id));
        let cached = provider.get(&id)?;
        Some(CapabilityInput {
            capability_id: cap_ref.to_string(),
            status: cached.entry.status,
            freshness: cached.freshness,
            fingerprint: cached.entry.fingerprint.as_ref().map(|f| f.0.clone()),
            catalog_constraints: cached
                .entry
                .constraints
                .iter()
                .map(|c| format!("catalog:{}:{}", c.code.as_str(), c.detail))
                .collect(),
        })
    });
    let mut d = DecisionInput::base(&facts.tool_id);
    d.declared_runtime = Some(&facts.declared);
    d.trust_level = Some(trust);
    d.input_risk = Some(risk);
    d.dependency_complexity = Some(complexity);
    d.isolation_requirement = Some(isolation);
    d.trust_source = Some(&facts.trust_source);
    d.capability_ref = facts.capability_ref.as_deref();
    d.capability = capability;
    d.venv_declared = facts.venv_declared;
    // DI-3/RT-4 D3①：venv 声明在场 → 五态静态合成 Available（与本文件
    // egress_wired=true 注入同口径——R1 闸子事实注入，非真探测；真探测面由
    // RT-2 python_worker_venv_e2e 与 adapter ensure_ready 双层承载）。
    d.venv_status = if facts.venv_declared {
        Some(VenvStatus::Available)
    } else {
        None
    };
    d.network_mode = facts.network_mode.as_deref();
    d.secrets_present = facts.secrets_present;
    d.probe_type = facts.probe_type.as_deref();
    d.egress_wired = true;
    d.vm_ready = false;
    d.binary_path = facts.binary_path.as_deref();
    d.binary_sha256 = facts.binary_sha256.as_deref();
    d.has_argv_or_subcommand = facts.has_argv_or_subcommand;
    d.has_budget = facts.has_budget;
    d.permission_level = facts.permission_level.as_deref();
    d
}

/// 32 份派发面 Manifest 文件系统直读（31 JSON + 1 YAML-only；YAML 有 JSON
/// 孪生的跳过——孪生文件是展示副本，派发面以 JSON 为准）。
fn load_dispatch_face() -> Vec<(PathBuf, serde_json::Value)> {
    let dir = Path::new(TOOLS_DIR);
    let mut jsons: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("manifests/tools 目录在位")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
        .collect();
    jsons.sort();
    let mut out = Vec::new();
    for path in &jsons {
        let text = std::fs::read_to_string(path).expect("manifest 可读");
        let value: serde_json::Value = serde_json::from_str(&text).expect("manifest JSON 合法");
        out.push((path.clone(), value));
    }
    let yaml_path = dir.join(YAML_ONLY);
    let text = std::fs::read_to_string(&yaml_path).expect("YAML-only Manifest 在位");
    let value: serde_json::Value = serde_yaml_ng::from_str(&text).expect("YAML Manifest 合法");
    assert!(
        !dir.join(YAML_ONLY.trim_end_matches(".yaml"))
            .with_extension("json")
            .exists(),
        "YAML-only 前提成立（JSON 孪生不存在）"
    );
    out.push((yaml_path, value));
    out
}

fn provider() -> StaticCatalogProvider {
    StaticCatalogProvider::load(Path::new(SEED)).expect("能力种子加载（RT-1 16 条）")
}

/// 判据 5：31 份逐份断言（31 assign + 1 not_applicable；DI-3/RT-4 迁移后
/// 基线——3 处 RT-3 inconsistent 全部收敛，inconsistent 快照零成员；
/// WSW-7 zap 登记随更 30 份；WSW-8 browser 登记随更 31 份（设计内扩展非钉值
/// 改动——WSW-8-TASK §2.1）；verdict/runtime/cross_check/gap/reason 逐字段）。
#[test]
fn manifests_expectation_table_30() {
    let provider = provider();
    let face = load_dispatch_face();
    assert_eq!(face.len(), 32, "31 JSON + 1 YAML-only = 32 份派发面");
    let mut assign_runtimes: Vec<String> = Vec::new();
    let mut counts = [0usize; 4]; // assign/inconsistent/reject/not_applicable
    for (path, manifest) in &face {
        let facts = extract_facts(manifest);
        assert!(!facts.tool_id.is_empty(), "{}: id 在位", path.display());
        let input = to_input(&facts, &provider, InputRisk::TrustedLowRisk);
        let dec = decide(&input);
        // 可解释性通用断言：reason 非空、每条 rule/hit/anchor 非空。
        assert!(!dec.reason.is_empty(), "{}: reason 非空", facts.tool_id);
        for step in &dec.reason {
            assert!(
                !step.rule.is_empty() && !step.hit.is_empty() && !step.anchor.is_empty(),
                "{}: 命中链条目完整",
                facts.tool_id
            );
        }
        let declared = facts.declared.clone();
        if facts.tool_id == METADATA_ENTRY {
            counts[3] += 1;
            assert_eq!(dec.verdict, Verdict::NotApplicable, "{}", facts.tool_id);
            assert_eq!(dec.runtime, None);
            assert!(!dec.cross_check.consistent);
            assert!(dec.reject.is_none() && dec.gap.is_empty());
            continue;
        }
        // R0 hit 引用真实派生来源（origin/钉值；元数据条目 R7 短路无 R0 步）。
        assert!(
            dec.reason[0].hit.contains("provenance.origin"),
            "{}: R0 引用 origin 派生来源：{}",
            facts.tool_id,
            dec.reason[0].hit
        );
        // 28 assign（DI-3/RT-4 迁移完成态）：verdict/runtime/cross_check/gap/reject。
        counts[0] += 1;
        assert_eq!(dec.verdict, Verdict::Assign, "{}: {:?}", facts.tool_id, dec);
        let want = Target::parse(&declared).unwrap();
        assert_eq!(dec.runtime, Some(want), "{}: 目标=声明", facts.tool_id);
        assert!(dec.cross_check.consistent, "{}", facts.tool_id);
        assert_eq!(
            dec.cross_check.manifest_runtime, declared,
            "{}",
            facts.tool_id
        );
        assert!(dec.gap.is_empty(), "{}: assign 无 gap", facts.tool_id);
        assert!(dec.reject.is_none(), "{}", facts.tool_id);
        assign_runtimes.push(declared);
    }
    assert_eq!(
        counts,
        [31, 0, 0, 1],
        "DI-3/RT-4 判据 1 分布：31 assign + 1 not_applicable（inconsistent 快照零成员；WSW-7 zap 随更 + WSW-8 browser 随更）"
    );
    // 判据 2：native_rust/python_worker/podman_host 三行在期望表内有 assign 成员。
    for word in ["native_rust", "python_worker", "podman_host"] {
        assert!(
            assign_runtimes.iter().any(|r| r == word),
            "判据 2：{word} 行 assign 成员在位"
        );
    }
}

/// 判据 6：三档风险路由（每份可派发 Manifest × InputRisk 三档）。
#[test]
fn manifests_three_tier_risk_routing() {
    let provider = provider();
    let face = load_dispatch_face();
    for (path, manifest) in &face {
        let facts = extract_facts(manifest);
        if facts.tool_id == METADATA_ENTRY {
            continue;
        }
        let is_native_decl = matches!(facts.declared.as_str(), "native_rust" | "python_worker");
        // low（trusted_low_risk）：基线档 = 期望表行为（DI-3/RT-4 迁移后
        // 29 份全 assign——ADMISSION_GAP/VM_LOCAL 特例随迁移撤除；WSW-7 zap 随更）。
        let low = decide(&to_input(&facts, &provider, InputRisk::TrustedLowRisk));
        assert_eq!(low.verdict, Verdict::Assign, "{} low", facts.tool_id);
        // unknown（untrusted 代码枚举）：native/python 声明 → inconsistent
        // (target=container)；podman_host 声明 → assign。
        let unknown = decide(&to_input(&facts, &provider, InputRisk::Untrusted));
        if is_native_decl {
            assert_eq!(
                unknown.verdict,
                Verdict::Inconsistent,
                "{} unknown",
                facts.tool_id
            );
            assert_eq!(
                unknown.runtime,
                Some(Target::PodmanHost),
                "{} unknown → target=container（{}）",
                facts.tool_id,
                path.display()
            );
            assert!(!unknown.cross_check.consistent);
        } else {
            assert_eq!(
                unknown.verdict,
                Verdict::Assign,
                "{} unknown",
                facts.tool_id
            );
        }
        // high（high_risk）：VM 闸 → Reject(RT3_VM_NOT_READY) 零降级
        // （DI-3/RT-4 迁移后 N6 声明冲突语境在 31 份快照零成员）。
        let high = decide(&to_input(&facts, &provider, InputRisk::HighRisk));
        assert_eq!(high.verdict, Verdict::Reject, "{} high", facts.tool_id);
        assert_eq!(
            high.reject.as_ref().unwrap().code.as_str(),
            "RT3_VM_NOT_READY",
            "{} high",
            facts.tool_id
        );
        assert_eq!(high.runtime, None, "{} high 零降级目标", facts.tool_id);
    }
}

/// 判据 5 补充：assign 决策的 constraints 词表与 permission 授权面引用
/// （T 档语义归既有 Tool Policy，不入四维）。
#[test]
fn assign_constraints_vocabulary() {
    let provider = provider();
    let face = load_dispatch_face();
    let mut checked = 0usize;
    for (_path, manifest) in &face {
        let facts = extract_facts(manifest);
        if facts.tool_id == METADATA_ENTRY {
            continue;
        }
        let dec = decide(&to_input(&facts, &provider, InputRisk::TrustedLowRisk));
        if dec.verdict != Verdict::Assign {
            continue;
        }
        checked += 1;
        // e2:* 全局清单引用（E §2 统一强制 + 输出帽/网络词表）。
        assert!(
            dec.constraints.iter().any(|c| c == "e2:resource-limits"),
            "{}: E §2 统一强制引用",
            facts.tool_id
        );
        match facts.network_mode.as_deref() {
            Some("egress_allowlist") => assert!(
                dec.constraints.iter().any(|c| c == "e2:egress-allowlist"),
                "{}",
                facts.tool_id
            ),
            _ => assert!(
                dec.constraints
                    .iter()
                    .any(|c| c == "e2:network-default-off"),
                "{}",
                facts.tool_id
            ),
        }
        // permission_level 授权面引用（T0/T1/T2 语义原文）。
        if let Some(p) = facts.permission_level.as_deref() {
            assert!(
                dec.constraints
                    .iter()
                    .any(|c| c.starts_with(&format!("permission_level={p}"))),
                "{}: permission 引用",
                facts.tool_id
            );
        }
        // capability_ref 在场的 assign（9 份 DI native）：R1 命中 + status available。
        if facts.capability_ref.is_some() {
            assert!(
                dec.reason
                    .iter()
                    .any(|s| s.rule == "R1" && s.hit.contains("available")),
                "{}: R1 available 命中",
                facts.tool_id
            );
            assert!(
                matches!(
                    dec.inputs.host_capability,
                    Some(tool_engine::policy::HostCapabilityEcho::Capability { ref status, .. })
                        if status == "available"
                ),
                "{}: 目录条目回显",
                facts.tool_id
            );
        }
        // python_worker：R6 命中引用 §3 总表:57（lock 面/协议兼容判据）。
        if facts.declared == "python_worker" {
            assert!(
                dec.reason
                    .iter()
                    .any(|s| s.rule == "R6" && s.hit.contains("python_worker")),
                "{}: R6 python_worker 候选命中",
                facts.tool_id
            );
        }
    }
    assert_eq!(
        checked, 31,
        "31 份 assign 全部过 constraints 断言（含 T37 unified search；DI-3/RT-4 与 WSW 更新基线）"
    );
}

/// D2/判据 4 联断：Integration 面 InputRisk 三档经既有枚举串（非短名）。
#[test]
fn integration_uses_existing_input_risk_enum() {
    // 三档代码串与既有 capability.rs 枚举 as_str 一致（D2 不建第二词表）。
    assert_eq!(InputRisk::TrustedLowRisk.as_str(), "trusted_low_risk");
    assert_eq!(InputRisk::Untrusted.as_str(), "untrusted");
    assert_eq!(InputRisk::HighRisk.as_str(), "high_risk");
    // TrustLevel/Isolation 词表串。
    assert_eq!(TrustLevel::Audited.as_str(), "audited");
    assert_eq!(DependencyComplexity::Locked.as_str(), "locked");
    assert_eq!(IsolationRequirement::Kernel.as_str(), "kernel");
    // capability 五态串（种子序列化口径）。
    assert_eq!(CapabilityStatus::Available.as_str(), "available");
    assert!(matches!(Freshness::Fresh, Freshness::Fresh));
}

/// S8 快照 dump（任务书交付物 5：RT-4 前置输入 = 31 份 RuntimeDecision 全量
/// 快照 + 三档风险评估；机器可判 JSON）。默认 no-op，设 `RT3_SNAPSHOT_DUMP=1`
/// 并以 `--nocapture` 实跑输出（离线零网络；输出只落批次报告 §5，决策面
/// 本身零写路径——红线 1 由矩阵面源码 grep 断言承载）。
#[test]
fn rt3_snapshot_dump_for_batch_report() {
    if std::env::var("RT3_SNAPSHOT_DUMP").as_deref() != Ok("1") {
        return;
    }
    let provider = provider();
    let face = load_dispatch_face();
    assert_eq!(face.len(), 32, "31 份派发面快照");
    for (path, manifest) in &face {
        let facts = extract_facts(manifest);
        // 基线档 = trusted_low_risk（报告 §1 A8 表同口径）。
        let dec = decide(&to_input(&facts, &provider, InputRisk::TrustedLowRisk));
        // 三档风险评估（判据 6）：每份 × {low, unknown, high}。
        let tiers: Vec<String> = [
            InputRisk::TrustedLowRisk,
            InputRisk::Untrusted,
            InputRisk::HighRisk,
        ]
        .iter()
        .map(|r| {
            let d = decide(&to_input(&facts, &provider, *r));
            format!(
                "{}={}/{}/{}",
                input_risk_short_alias(*r),
                d.verdict.as_str(),
                d.runtime.map(|t| t.as_str()).unwrap_or("null"),
                d.reject.as_ref().map(|x| x.code.as_str()).unwrap_or("-")
            )
        })
        .collect();
        println!(
            "SNAPSHOT {} file={} declared={} tiers=[{}] {}",
            facts.tool_id,
            path.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
            facts.declared,
            tiers.join(" "),
            serde_json::to_string(&dec).expect("decision 序列化")
        );
    }
}
