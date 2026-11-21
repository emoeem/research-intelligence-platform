//! RT-3 S4 决策矩阵测试面（144 格 totality + D6 代表性表 + D7 N1-N10 负例矩阵
//! + 可解释性断言 + D2 别名映射双向 + 只读面源码 grep）。
//!
//! # S2 测试载具设计（fixture 设计稿，任务书 §4 S2 落点）
//!
//! 1. **合成 CapabilityInput**：`CapabilityStatus` 五态（available/missing/
//!    incompatible/probe_failed/disabled）× `Freshness`（Fresh 与
//!    Stale{BinaryChanged} 代表值）组合构造；capability_id 统一 "t.cap"，
//!    fingerprint None/"0123456789abcdef" 两形态；目录缺条目形态 =
//!    capability_ref 在场而 capability=None。
//! 2. **合成 VenvStatus**：五态全值（available/missing/incompatible/
//!    probe_failed/permission_denied），venv_fingerprint None/Some。
//! 3. **四维枚举全值**：TrustLevel 3 × InputRisk 3（capability.rs 既有 enum，
//!    D2 不建第二词表）× DependencyComplexity 4 × IsolationRequirement 4
//!    = 144 格 totality。
//! 4. **宿主子事实基底**：egress_wired=true（现状 EgressProxy 面在位，
//!    RT-3-ADR D5 论据「现状形态」）、vm_ready=false（podman_vm adapter 未
//!    实现，RT-3-TASK §1 设计保留面）、零 capability_ref、零 venv、
//!    network none、零 secrets。
//! 5. **29 份 Manifest** 不在本文件复制——`policy_resolver_manifests.rs`
//!    文件系统直读（防漂移），本文件全内存构造（零网络零安装）。
//!
//! 全部断言离线单线程（`--test-threads=1` 全批统一门）。

use tool_engine::capability::{CapabilityStatus, Freshness, InputRisk, StaleReason};
use tool_engine::policy::{
    decide, derive_dependency_complexity, derive_isolation_requirement, derive_target,
    derive_trust, input_risk_from_short_alias, input_risk_short_alias, is_metadata_entry,
    CapabilityInput, CrossCheck, DecisionInput, DependencyComplexity, IsolationRequirement,
    RejectCode, Target, TrustLevel, Verdict,
};
use tool_engine::venv::VenvStatus;

const TRUST: [TrustLevel; 3] = [
    TrustLevel::Audited,
    TrustLevel::Registered,
    TrustLevel::Untrusted,
];
const RISK: [InputRisk; 3] = [
    InputRisk::TrustedLowRisk,
    InputRisk::Untrusted,
    InputRisk::HighRisk,
];
const CX: [DependencyComplexity; 4] = [
    DependencyComplexity::Simple,
    DependencyComplexity::Locked,
    DependencyComplexity::Conflicting,
    DependencyComplexity::Heavy,
];
const ISO: [IsolationRequirement; 4] = [
    IsolationRequirement::None,
    IsolationRequirement::Fs,
    IsolationRequirement::Net,
    IsolationRequirement::Kernel,
];

/// 144 格载具（S2 基底 + 四维注入）。
fn grid_input(
    trust: TrustLevel,
    risk: InputRisk,
    cx: DependencyComplexity,
    iso: IsolationRequirement,
) -> DecisionInput<'static> {
    let mut d = DecisionInput::base("rip.t.grid");
    d.trust_level = Some(trust);
    d.input_risk = Some(risk);
    d.dependency_complexity = Some(cx);
    d.isolation_requirement = Some(iso);
    d
}

// ---------------------------------------------------------------------------
// 判据 1：144 格 totality（每格产出 Target 或 Reject，无未处理格）
// ---------------------------------------------------------------------------

#[test]
fn grid_totality_144_cells() {
    let mut targets = 0usize;
    let mut rejects = 0usize;
    let mut vm_not_ready = 0usize;
    for &t in &TRUST {
        for &r in &RISK {
            for &c in &CX {
                for &i in &ISO {
                    let input = grid_input(t, r, c, i);
                    match derive_target(&input) {
                        Ok(d) => {
                            targets += 1;
                            assert!(
                                !d.target.as_str().is_empty(),
                                "target 词表串非空（{}）",
                                d.admission_gaps.len()
                            );
                        }
                        Err(rej) => {
                            rejects += 1;
                            assert!(
                                rej.reject.code.as_str().starts_with("RT3_"),
                                "拒绝码词表 RT3_ 前缀"
                            );
                            assert!(
                                !rej.reason.is_empty(),
                                "拒绝格 reason 链非空（{}）",
                                rej.reject.code.as_str()
                            );
                            if rej.reject.code == RejectCode::VmNotReady {
                                vm_not_ready += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(targets + rejects, 144, "totality：3×3×4×4 无未处理格");
    // floor 合成的确定性分布：high(48) + kernel×risk∈{low,unknown}(24) +
    // untrusted×heavy×iso∈{none,fs,net}(6) = 78 格 floor=vm × VM 未就绪。
    assert_eq!(vm_not_ready, 78, "VM 未就绪格数 = 48+24+6");
    assert_eq!(targets, 66);
}

/// 判据 3 派生守护：risk=high 全 144 子空间永不产出 podman_host/native 目标
/// （rootless ≠ VM 隔离，N2；VM 未就绪 → 全 Reject，N3）。
#[test]
fn grid_high_risk_never_downgrades() {
    for &t in &TRUST {
        for &c in &CX {
            for &i in &ISO {
                let input = grid_input(t, InputRisk::HighRisk, c, i);
                match derive_target(&input) {
                    Ok(d) => panic!(
                        "high 风险格产出目标 {:?}（降级违规，trust={:?} cx={:?} iso={:?}）",
                        d.target, t, c, i
                    ),
                    Err(rej) => assert_eq!(rej.reject.code, RejectCode::VmNotReady),
                }
            }
        }
    }
}

/// 判据 3 派生守护：isolation=kernel 格（risk 非 high）永不产出
/// podman_host/native 目标（仅 vm_isolated，N5）。
#[test]
fn grid_kernel_never_downgrades() {
    for &t in &TRUST {
        for r in [InputRisk::TrustedLowRisk, InputRisk::Untrusted] {
            for &c in &CX {
                let input = grid_input(t, r, c, IsolationRequirement::Kernel);
                match derive_target(&input) {
                    Ok(d) => panic!("kernel 格产出目标 {:?}（N5 违规）", d.target),
                    Err(rej) => assert_eq!(rej.reject.code, RejectCode::VmNotReady),
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 判据 1/2：D6 代表性表（audited 行逐格，dual candidate 承载 R5 候选）
// ---------------------------------------------------------------------------

#[test]
fn d6_audited_representative_row() {
    // audited × low × {none,fs,net}：simple→native_rust / locked→python_worker /
    // conflicting→podman_host / heavy→podman_host（D6 表「native（按 R5 候选）」）。
    for iso in [
        IsolationRequirement::None,
        IsolationRequirement::Fs,
        IsolationRequirement::Net,
    ] {
        for (cx, want) in [
            (DependencyComplexity::Simple, Target::NativeRust),
            (DependencyComplexity::Locked, Target::PythonWorker),
            (DependencyComplexity::Conflicting, Target::PodmanHost),
            (DependencyComplexity::Heavy, Target::PodmanHost),
        ] {
            let d = derive_target(&grid_input(
                TrustLevel::Audited,
                InputRisk::TrustedLowRisk,
                cx,
                iso,
            ))
            .unwrap_or_else(|rej| panic!("audited/low 格拒绝 {:?}", rej.reject.code));
            assert_eq!(d.target, want, "audited×low×{:?}×{:?}", iso, cx);
        }
        // audited × unknown × {none,fs,net} → container（E §1:16）。
        for cx in &CX {
            let d = derive_target(&grid_input(
                TrustLevel::Audited,
                InputRisk::Untrusted,
                *cx,
                iso,
            ))
            .unwrap_or_else(|rej| panic!("audited/unknown 格拒绝 {:?}", rej.reject.code));
            assert_eq!(
                d.target,
                Target::PodmanHost,
                "audited×unknown×{:?}×{:?}",
                iso,
                cx
            );
        }
    }
    // audited × {low,unknown} × kernel → vm→VM 未就绪 Reject(RT3_VM_NOT_READY)。
    // audited × high × 全列 → vm→Reject（同上表）。
    for r in [
        InputRisk::TrustedLowRisk,
        InputRisk::Untrusted,
        InputRisk::HighRisk,
    ] {
        for iso in [IsolationRequirement::Kernel, IsolationRequirement::None] {
            if r == InputRisk::HighRisk || iso == IsolationRequirement::Kernel {
                let rej = derive_target(&grid_input(
                    TrustLevel::Audited,
                    r,
                    DependencyComplexity::Simple,
                    iso,
                ))
                .expect_err("audited vm 格必须拒绝");
                assert_eq!(rej.reject.code, RejectCode::VmNotReady);
            }
        }
    }
}

/// podman_vm 行（判据 2：VM 行由矩阵承载）——VM 就绪子事实翻转时 kernel/high
/// 格产出 podman_vm（设计保留面语义），未就绪一律 Reject。
#[test]
fn vm_row_ready_flip() {
    let mut ready = grid_input(
        TrustLevel::Audited,
        InputRisk::HighRisk,
        DependencyComplexity::Simple,
        IsolationRequirement::None,
    );
    ready.vm_ready = true;
    assert_eq!(derive_target(&ready).unwrap().target, Target::PodmanVm);
    let mut ready_kernel = grid_input(
        TrustLevel::Audited,
        InputRisk::TrustedLowRisk,
        DependencyComplexity::Simple,
        IsolationRequirement::Kernel,
    );
    ready_kernel.vm_ready = true;
    assert_eq!(
        derive_target(&ready_kernel).unwrap().target,
        Target::PodmanVm
    );
}

// ---------------------------------------------------------------------------
// 判据 3：N1-N10 负例矩阵（逐条独立 fixture；verdict + code + category）
// ---------------------------------------------------------------------------

#[test]
fn n1_untrusted_native_python_denied() {
    for declared in ["native_rust", "python_worker"] {
        let mut d = grid_input(
            TrustLevel::Untrusted,
            InputRisk::TrustedLowRisk,
            DependencyComplexity::Simple,
            IsolationRequirement::None,
        );
        d.declared_runtime = Some(declared);
        let dec = decide(&d);
        assert_eq!(dec.verdict, Verdict::Reject, "N1 {}", declared);
        let rej = dec.reject.as_ref().unwrap();
        assert_eq!(rej.code.as_str(), "RT3_UNTRUSTED_NATIVE_DENIED");
        assert_eq!(rej.category, "POLICY_DENIED");
        assert_eq!(dec.runtime, None, "硬拒绝无降级目标");
        assert!(dec.reason.iter().any(|s| s.rule == "N1"), "N1 命中链");
    }
    // 对照：untrusted × 声明 podman_host → assign（§3 总表:57-58 容器行）。
    let mut ok = grid_input(
        TrustLevel::Untrusted,
        InputRisk::TrustedLowRisk,
        DependencyComplexity::Simple,
        IsolationRequirement::None,
    );
    ok.declared_runtime = Some("podman_host");
    assert_eq!(decide(&ok).verdict, Verdict::Assign);
}

#[test]
fn n2_high_risk_never_podman_host() {
    // input_risk=high × 声明 podman_host（rootless）→ 硬拒绝（rootless≠VM 隔离）。
    let mut d = grid_input(
        TrustLevel::Audited,
        InputRisk::HighRisk,
        DependencyComplexity::Simple,
        IsolationRequirement::None,
    );
    d.declared_runtime = Some("podman_host");
    let dec = decide(&d);
    assert_eq!(dec.verdict, Verdict::Reject);
    assert_eq!(
        dec.reject.as_ref().unwrap().code.as_str(),
        "RT3_VM_NOT_READY"
    );
    assert_eq!(dec.runtime, None, "podman_host 非输出值");
}

#[test]
fn n3_vm_not_ready_no_fallback_target() {
    // input_risk=high × VM 未启用 → Reject(RT3_VM_NOT_READY) 且无任何降级目标。
    let mut d = grid_input(
        TrustLevel::Audited,
        InputRisk::HighRisk,
        DependencyComplexity::Locked,
        IsolationRequirement::None,
    );
    d.declared_runtime = Some("python_worker");
    let dec = decide(&d);
    assert_eq!(dec.verdict, Verdict::Reject);
    let rej = dec.reject.as_ref().unwrap();
    assert_eq!(rej.code.as_str(), "RT3_VM_NOT_READY");
    assert_eq!(rej.category, "RUNNER_UNAVAILABLE");
    assert_eq!(dec.runtime, None, "podman_host/native 均非输出值");
    // isolation=kernel × VM 未启用 → 同码（N3 第二分支）。
    let mut d2 = grid_input(
        TrustLevel::Audited,
        InputRisk::TrustedLowRisk,
        DependencyComplexity::Simple,
        IsolationRequirement::Kernel,
    );
    d2.declared_runtime = Some("native_rust");
    assert_eq!(
        decide(&d2).reject.as_ref().unwrap().code.as_str(),
        "RT3_VM_NOT_READY"
    );
}

#[test]
fn n4_unknown_risk_not_silently_native() {
    let mut d = grid_input(
        TrustLevel::Audited,
        InputRisk::Untrusted,
        DependencyComplexity::Simple,
        IsolationRequirement::None,
    );
    d.declared_runtime = Some("native_rust");
    let dec = decide(&d);
    assert_eq!(dec.verdict, Verdict::Inconsistent, "N4 声明冲突语境");
    assert_eq!(dec.runtime, Some(Target::PodmanHost), "target=container");
    assert!(!dec.cross_check.consistent);
    assert!(dec.reason.iter().any(|s| s.rule == "N4"), "N4 命中链");
    assert!(dec
        .gap
        .iter()
        .any(|g| g.contains("E §1:16") || g.contains("不静默原生化")));
    // unknown × 声明 python_worker 同判。
    d.declared_runtime = Some("python_worker");
    assert_eq!(decide(&d).verdict, Verdict::Inconsistent);
    // 对照：unknown × 声明 podman_host → assign（隔离执行承载）。
    d.declared_runtime = Some("podman_host");
    assert_eq!(decide(&d).verdict, Verdict::Assign);
}

#[test]
fn n5_kernel_only_vm_isolated() {
    // isolation=kernel × 目标 ∈ {native, python, podman_host} → 硬拒绝（仅 vm_isolated）。
    for declared in ["native_rust", "python_worker", "podman_host"] {
        let mut d = grid_input(
            TrustLevel::Audited,
            InputRisk::TrustedLowRisk,
            DependencyComplexity::Simple,
            IsolationRequirement::Kernel,
        );
        d.declared_runtime = Some(declared);
        let dec = decide(&d);
        assert_eq!(dec.verdict, Verdict::Reject, "N5 {}", declared);
        assert_eq!(
            dec.reject.as_ref().unwrap().code.as_str(),
            "RT3_VM_NOT_READY"
        );
        assert_eq!(dec.runtime, None);
    }
}

#[test]
fn n6_vm_local_declared_native_python_inconsistent() {
    // network.mode=vm_local（派生 kernel）× 声明 python/native → Inconsistent
    // （D7 细则：声明值与目标冲突语境；现仓命中 rip.evidence.rtk.mcp）。
    for declared in ["python_worker", "native_rust"] {
        let mut d = grid_input(
            TrustLevel::Audited,
            InputRisk::TrustedLowRisk,
            DependencyComplexity::Locked,
            IsolationRequirement::Kernel,
        );
        d.declared_runtime = Some(declared);
        d.network_mode = Some("vm_local");
        let dec = decide(&d);
        assert_eq!(dec.verdict, Verdict::Inconsistent, "N6 {}", declared);
        assert_eq!(dec.runtime, Some(Target::PodmanVm), "目标 = VM");
        assert!(!dec.cross_check.consistent);
        assert!(dec.reason.iter().any(|s| s.rule == "N6"));
        let gap = dec.gap.join("\n");
        assert!(
            gap.contains("vm_local") && gap.contains("kernel") && gap.contains("podman_vm"),
            "gap = vm_local→kernel→vm：{}",
            gap
        );
    }
}

#[test]
fn n7_capability_gate_hard_reject() {
    // capability_ref 在场 + 五态非 available（missing/incompatible/probe_failed/
    // disabled）→ Reject(RT3_CAPABILITY_UNAVAILABLE, DEPENDENCY)，零猜测。
    for status in [
        CapabilityStatus::Missing,
        CapabilityStatus::Incompatible,
        CapabilityStatus::ProbeFailed,
        CapabilityStatus::Disabled,
    ] {
        let mut d = grid_input(
            TrustLevel::Audited,
            InputRisk::TrustedLowRisk,
            DependencyComplexity::Simple,
            IsolationRequirement::None,
        );
        d.capability_ref = Some("t.cap");
        d.capability = Some(CapabilityInput {
            capability_id: "t.cap".to_string(),
            status,
            freshness: Freshness::Fresh,
            fingerprint: Some("0123456789abcdef".to_string()),
            catalog_constraints: Vec::new(),
        });
        let dec = decide(&d);
        assert_eq!(dec.verdict, Verdict::Reject, "N7 status={:?}", status);
        let rej = dec.reject.as_ref().unwrap();
        assert_eq!(rej.code.as_str(), "RT3_CAPABILITY_UNAVAILABLE");
        assert_eq!(rej.category, "DEPENDENCY");
        assert_eq!(dec.runtime, None, "不静默换实现");
    }
    // available + stale → 同码（stale 条目不可供决策使用，C §5.3:290）。
    let mut stale = grid_input(
        TrustLevel::Audited,
        InputRisk::TrustedLowRisk,
        DependencyComplexity::Simple,
        IsolationRequirement::None,
    );
    stale.capability_ref = Some("t.cap");
    stale.capability = Some(CapabilityInput {
        capability_id: "t.cap".to_string(),
        status: CapabilityStatus::Available,
        freshness: Freshness::Stale {
            reason: StaleReason::BinaryChanged,
        },
        fingerprint: None,
        catalog_constraints: Vec::new(),
    });
    let dec = decide(&stale);
    assert_eq!(dec.verdict, Verdict::Reject);
    assert_eq!(
        dec.reject.as_ref().unwrap().code.as_str(),
        "RT3_CAPABILITY_UNAVAILABLE"
    );
    // capability_ref 在场 + 目录缺条目（capability=None）→ 同码。
    let mut absent = grid_input(
        TrustLevel::Audited,
        InputRisk::TrustedLowRisk,
        DependencyComplexity::Simple,
        IsolationRequirement::None,
    );
    absent.capability_ref = Some("t.missing");
    assert_eq!(
        decide(&absent).reject.as_ref().unwrap().code.as_str(),
        "RT3_CAPABILITY_UNAVAILABLE"
    );
    // 对照：available + fresh → R1 通过（决策继续走目标合成，非 capability 拒绝）。
    let mut ok = grid_input(
        TrustLevel::Audited,
        InputRisk::TrustedLowRisk,
        DependencyComplexity::Simple,
        IsolationRequirement::None,
    );
    ok.capability_ref = Some("t.cap");
    ok.capability = Some(CapabilityInput {
        capability_id: "t.cap".to_string(),
        status: CapabilityStatus::Available,
        freshness: Freshness::Fresh,
        fingerprint: Some("0123456789abcdef".to_string()),
        catalog_constraints: vec!["catalog:page_cap:页数帽由前步承载".to_string()],
    });
    ok.declared_runtime = Some("native_rust");
    ok.binary_path = Some("/usr/bin/pdftotext");
    ok.binary_sha256 = Some("f85872f94fb67433d6fa411ea3a10c263205eada4622b48ccc700442ad29b13c");
    ok.has_argv_or_subcommand = true;
    ok.has_budget = true;
    let dec = decide(&ok);
    assert_ne!(
        dec.reject.as_ref().map(|r| r.code),
        Some(RejectCode::CapabilityUnavailable),
        "available+fresh 通过 R1"
    );
    assert_eq!(dec.verdict, Verdict::Assign);
    assert!(
        dec.constraints.iter().any(|c| c.starts_with("catalog:")),
        "catalog 直通"
    );
}

#[test]
fn n8_secrets_native_tier_denied() {
    for (cx, declared) in [
        (DependencyComplexity::Simple, "native_rust"),
        (DependencyComplexity::Locked, "python_worker"),
    ] {
        let mut d = grid_input(
            TrustLevel::Audited,
            InputRisk::TrustedLowRisk,
            cx,
            IsolationRequirement::None,
        );
        d.declared_runtime = Some(declared);
        d.secrets_present = true;
        let dec = decide(&d);
        assert_eq!(dec.verdict, Verdict::Reject, "N8 {}", declared);
        let rej = dec.reject.as_ref().unwrap();
        assert_eq!(rej.code.as_str(), "RT3_SECRET_NATIVE_DENIED");
        assert_eq!(rej.category, "POLICY_DENIED");
        assert!(dec.reason.iter().any(|s| s.rule == "N8"));
    }
    // 对照：secrets × 容器目标 → 容器承载不拒绝。
    let mut ok = grid_input(
        TrustLevel::Audited,
        InputRisk::Untrusted,
        DependencyComplexity::Simple,
        IsolationRequirement::None,
    );
    ok.declared_runtime = Some("podman_host");
    ok.secrets_present = true;
    assert_eq!(decide(&ok).verdict, Verdict::Assign);
}

#[test]
fn n9_conflicting_heavy_not_native_python() {
    for (cx, declared) in [
        (DependencyComplexity::Conflicting, "native_rust"),
        (DependencyComplexity::Heavy, "python_worker"),
    ] {
        let mut d = grid_input(
            TrustLevel::Audited,
            InputRisk::TrustedLowRisk,
            cx,
            IsolationRequirement::None,
        );
        d.declared_runtime = Some(declared);
        let dec = decide(&d);
        assert_eq!(dec.verdict, Verdict::Inconsistent, "N9 {}", declared);
        assert_eq!(dec.runtime, Some(Target::PodmanHost));
        assert!(dec.reason.iter().any(|s| s.rule == "N9"));
        assert!(dec.gap.iter().any(|g| g.contains("§3.2")));
    }
    // untrusted + heavy × 声明 native → N1 优先（信任边界 > 依赖面，D7 细则）。
    let mut d = grid_input(
        TrustLevel::Untrusted,
        InputRisk::TrustedLowRisk,
        DependencyComplexity::Heavy,
        IsolationRequirement::None,
    );
    d.declared_runtime = Some("native_rust");
    assert_eq!(
        decide(&d).reject.as_ref().unwrap().code.as_str(),
        "RT3_UNTRUSTED_NATIVE_DENIED"
    );
    // 对照：heavy × 声明 podman_host → assign（独立生命周期由容器承载）。
    let mut ok = grid_input(
        TrustLevel::Audited,
        InputRisk::TrustedLowRisk,
        DependencyComplexity::Heavy,
        IsolationRequirement::None,
    );
    ok.declared_runtime = Some("podman_host");
    assert_eq!(decide(&ok).verdict, Verdict::Assign);
}

#[test]
fn n10_vocab_and_missing_inputs() {
    // 四维任一缺省 → Reject(RT3_INPUT_INCOMPLETE, VALIDATION)。
    let mut d = grid_input(
        TrustLevel::Audited,
        InputRisk::TrustedLowRisk,
        DependencyComplexity::Simple,
        IsolationRequirement::None,
    );
    d.declared_runtime = Some("native_rust");
    for unset in 0..4 {
        let mut miss = d.clone();
        match unset {
            0 => miss.trust_level = None,
            1 => miss.input_risk = None,
            2 => miss.dependency_complexity = None,
            _ => miss.isolation_requirement = None,
        }
        let dec = decide(&miss);
        assert_eq!(dec.verdict, Verdict::Reject, "缺省维 {}", unset);
        let rej = dec.reject.as_ref().unwrap();
        assert_eq!(rej.code.as_str(), "RT3_INPUT_INCOMPLETE");
        assert_eq!(rej.category, "VALIDATION");
    }
    // 词表外 runtime.type 声明（§3 总表 mcp_client/http_api/cli_adapter 行）
    // → Reject(RT3_RUNTIME_VOCAB)（判据 2）。
    for word in ["mcp_client", "http_api", "cli_adapter"] {
        let mut d = grid_input(
            TrustLevel::Audited,
            InputRisk::TrustedLowRisk,
            DependencyComplexity::Simple,
            IsolationRequirement::None,
        );
        d.declared_runtime = Some(word);
        let dec = decide(&d);
        assert_eq!(dec.verdict, Verdict::Reject, "N10 {}", word);
        let rej = dec.reject.as_ref().unwrap();
        assert_eq!(rej.code.as_str(), "RT3_RUNTIME_VOCAB");
        assert_eq!(rej.category, "VALIDATION");
    }
    // runtime.type 缺省 → RT3_INPUT_INCOMPLETE。
    let mut d = grid_input(
        TrustLevel::Audited,
        InputRisk::TrustedLowRisk,
        DependencyComplexity::Simple,
        IsolationRequirement::None,
    );
    d.declared_runtime = Some("");
    assert_eq!(
        decide(&d).reject.as_ref().unwrap().code.as_str(),
        "RT3_INPUT_INCOMPLETE"
    );
    // 词表外维度串在 parse 层拒绝（D2 词表冻结；不猜默认值）。
    assert!(TrustLevel::parse("semi-audited").is_err());
    assert!(DependencyComplexity::parse("tiny").is_err());
    assert!(IsolationRequirement::parse("cpu").is_err());
    assert!(Target::parse("mcp_client").is_err());
    assert!(Target::parse("").is_err());
}

// ---------------------------------------------------------------------------
// 判据 4：可解释性（reason 链/锚/回显/确定性）+ D2 别名双向
// ---------------------------------------------------------------------------

#[test]
fn reason_chain_ordered_anchored_with_echo() {
    let mut d = grid_input(
        TrustLevel::Audited,
        InputRisk::TrustedLowRisk,
        DependencyComplexity::Simple,
        IsolationRequirement::None,
    );
    d.declared_runtime = Some("native_rust");
    d.trust_source = Some("provenance.origin=opensource + binary_sha256 在位");
    d.binary_path = Some("/usr/bin/pdftotext");
    d.binary_sha256 = Some("f85872f94fb67433d6fa411ea3a10c263205eada4622b48ccc700442ad29b13c");
    d.has_argv_or_subcommand = true;
    d.has_budget = true;
    d.permission_level = Some("T0");
    let dec = decide(&d);
    assert_eq!(dec.verdict, Verdict::Assign);
    assert_eq!(dec.runtime, Some(Target::NativeRust));
    assert_eq!(
        dec.cross_check,
        CrossCheck {
            manifest_runtime: "native_rust".to_string(),
            consistent: true
        }
    );
    // reason 有序：R0 起步、R7 收尾，rule/hit/anchor 全非空。
    let rules: Vec<&str> = dec.reason.iter().map(|s| s.rule).collect();
    assert_eq!(rules.first(), Some(&"R0"), "R0 起步：{:?}", rules);
    assert_eq!(rules.last(), Some(&"R7"), "R7 收尾：{:?}", rules);
    for (i, s) in dec.reason.iter().enumerate() {
        assert!(!s.hit.is_empty(), "step {} hit 非空", i);
        assert!(
            !s.anchor.is_empty(),
            "step {} anchor 非空（判据 2 行锚）",
            i
        );
    }
    // 命中链覆盖 R2-R5（风险/信任/隔离/依赖闸）。
    for want in ["R2", "R3", "R4", "R5", "R6"] {
        assert!(
            rules.contains(&want),
            "reason 缺 {} 命中：{:?}",
            want,
            rules
        );
    }
    // 四维取值回显（D8）。
    assert_eq!(dec.inputs.trust_level, "audited");
    assert_eq!(dec.inputs.input_risk, "trusted_low_risk");
    assert_eq!(dec.inputs.dependency_complexity, "simple");
    assert_eq!(dec.inputs.isolation_requirement, "none");
    assert!(!dec.inputs.trust_level.is_empty() && dec.inputs.host_capability.is_none());
    // trust 派生来源回显在 R0 hit（决策可复现最小集）。
    assert!(dec.reason[0].hit.contains("provenance.origin=opensource"));
    // constraints 词表（e2:* + permission_level）。
    assert!(dec
        .constraints
        .iter()
        .any(|c| c == "e2:network-default-off"));
    assert!(dec.constraints.iter().any(|c| c == "e2:resource-limits"));
    assert!(dec.constraints.iter().any(|c| c == "e2:output-cap"));
    assert!(dec
        .constraints
        .iter()
        .any(|c| c.starts_with("permission_level=T0")));
    assert!(dec.reject.is_none() && dec.gap.is_empty());
}

#[test]
fn determinism_same_inputs_same_decision() {
    let mut d = grid_input(
        TrustLevel::Registered,
        InputRisk::Untrusted,
        DependencyComplexity::Locked,
        IsolationRequirement::Net,
    );
    d.declared_runtime = Some("python_worker");
    let a = decide(&d);
    let b = decide(&d);
    assert_eq!(a, b, "纯函数确定性：同 inputs 两次 decide 逐字段相等");
    // serde 序列化面也逐字节一致（批次报告快照的可复现前提）。
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap()
    );
}

#[test]
fn input_risk_alias_bidirectional() {
    // D2 别名映射双向：low↔trusted_low_risk / unknown↔untrusted / high↔high_risk。
    for (short, code) in [
        ("low", "trusted_low_risk"),
        ("unknown", "untrusted"),
        ("high", "high_risk"),
    ] {
        let r = input_risk_from_short_alias(short).unwrap();
        assert_eq!(r.as_str(), code, "短名→既有枚举串");
        assert_eq!(input_risk_short_alias(r), short, "既有枚举→短名");
        // 既有枚举 parse 双向（代码串 ↔ enum ↔ 短名）。
        assert_eq!(
            input_risk_short_alias(InputRisk::parse(code).unwrap()),
            short
        );
    }
    // 词表外短名/代码串一律 Err。
    assert!(input_risk_from_short_alias("medium").is_err());
    assert!(InputRisk::parse("low").is_err());
    assert!(InputRisk::parse("trusted").is_err());
}

// ---------------------------------------------------------------------------
// D3/D4/D5 派生函数逐值 + R7 元数据条目 + venv 闸
// ---------------------------------------------------------------------------

#[test]
fn derive_trust_d3_table() {
    assert_eq!(
        derive_trust(Some("rip-native"), true, false),
        TrustLevel::Audited
    );
    assert_eq!(
        derive_trust(Some("internal-kit"), true, false),
        TrustLevel::Audited
    );
    assert_eq!(
        derive_trust(Some("opensource"), true, true),
        TrustLevel::Audited
    );
    assert_eq!(
        derive_trust(Some("opensource"), true, false),
        TrustLevel::Registered
    );
    assert_eq!(derive_trust(None, true, true), TrustLevel::Registered);
    assert_eq!(
        derive_trust(Some("rip-native"), false, true),
        TrustLevel::Registered,
        "license 缺 fail-closed"
    );
    assert_eq!(
        derive_trust(Some("unknown-origin"), true, true),
        TrustLevel::Registered
    );
}

#[test]
fn derive_complexity_d4_table() {
    assert_eq!(
        derive_dependency_complexity("python_worker").unwrap(),
        DependencyComplexity::Locked
    );
    assert_eq!(
        derive_dependency_complexity("native_rust").unwrap(),
        DependencyComplexity::Simple
    );
    assert_eq!(
        derive_dependency_complexity("podman_host").unwrap(),
        DependencyComplexity::Heavy
    );
    assert!(
        derive_dependency_complexity("mcp_client").is_err(),
        "词表外无派生判据"
    );
}

#[test]
fn derive_isolation_d5_table() {
    use IsolationRequirement::{Fs, Kernel, Net, None as INone};
    assert_eq!(
        derive_isolation_requirement(Some("none"), InputRisk::TrustedLowRisk, false),
        INone
    );
    assert_eq!(
        derive_isolation_requirement(Some("egress_allowlist"), InputRisk::TrustedLowRisk, false),
        Net
    );
    assert_eq!(
        derive_isolation_requirement(Some("vm_local"), InputRisk::TrustedLowRisk, false),
        Kernel
    );
    assert_eq!(
        derive_isolation_requirement(Some("none"), InputRisk::Untrusted, false),
        Fs
    );
    assert_eq!(
        derive_isolation_requirement(Some("none"), InputRisk::HighRisk, false),
        Kernel
    );
    assert_eq!(
        derive_isolation_requirement(Some("none"), InputRisk::TrustedLowRisk, true),
        Fs
    );
    // 多来源 max 合成（kernel > net > fs > none）。
    assert_eq!(
        derive_isolation_requirement(Some("vm_local"), InputRisk::HighRisk, true),
        Kernel
    );
    assert_eq!(
        derive_isolation_requirement(Some("egress_allowlist"), InputRisk::Untrusted, true),
        Net
    );
    assert!(INone < Fs && Fs < Net && Net < Kernel);
}

#[test]
fn r7_metadata_entry_not_applicable() {
    assert!(is_metadata_entry(Some("python_worker"), Some("none")));
    assert!(
        !is_metadata_entry(Some("native_rust"), Some("none")),
        "native probe=none 常态"
    );
    assert!(!is_metadata_entry(Some("podman_host"), Some("none")));
    assert!(!is_metadata_entry(
        Some("python_worker"),
        Some("worker_ping")
    ));
    let mut d = grid_input(
        TrustLevel::Audited,
        InputRisk::TrustedLowRisk,
        DependencyComplexity::Locked,
        IsolationRequirement::None,
    );
    d.declared_runtime = Some("python_worker");
    d.probe_type = Some("none");
    let dec = decide(&d);
    assert_eq!(dec.verdict, Verdict::NotApplicable);
    assert_eq!(dec.runtime, None);
    assert_eq!(dec.cross_check.manifest_runtime, "python_worker");
    assert!(!dec.cross_check.consistent);
    assert!(dec.reject.is_none() && dec.gap.is_empty());
    assert!(dec.reason.iter().any(|s| s.rule == "R7"));
}

#[test]
fn r1_venv_gate_five_states() {
    // venv 声明在场 + 五态非 available → Reject(RT3_VENV_UNAVAILABLE, DEPENDENCY)。
    for status in [
        VenvStatus::Missing,
        VenvStatus::Incompatible,
        VenvStatus::ProbeFailed,
        VenvStatus::PermissionDenied,
    ] {
        let mut d = grid_input(
            TrustLevel::Audited,
            InputRisk::TrustedLowRisk,
            DependencyComplexity::Locked,
            IsolationRequirement::None,
        );
        d.declared_runtime = Some("python_worker");
        d.venv_declared = true;
        d.venv_status = Some(status);
        let dec = decide(&d);
        assert_eq!(dec.verdict, Verdict::Reject, "venv {:?}", status);
        let rej = dec.reject.as_ref().unwrap();
        assert_eq!(rej.code.as_str(), "RT3_VENV_UNAVAILABLE");
        assert_eq!(rej.category, "DEPENDENCY");
        assert_eq!(dec.runtime, None, "零系统解释器回退");
    }
    // 声明在场但无探测五态 → 同码。
    let mut d = grid_input(
        TrustLevel::Audited,
        InputRisk::TrustedLowRisk,
        DependencyComplexity::Locked,
        IsolationRequirement::None,
    );
    d.venv_declared = true;
    d.declared_runtime = Some("python_worker");
    assert_eq!(
        decide(&d).reject.as_ref().unwrap().code.as_str(),
        "RT3_VENV_UNAVAILABLE"
    );
    // available → 闸通过（决策继续；裸名 python3 声明 + available venv → assign）。
    let mut ok = grid_input(
        TrustLevel::Audited,
        InputRisk::TrustedLowRisk,
        DependencyComplexity::Locked,
        IsolationRequirement::None,
    );
    ok.venv_declared = true;
    ok.venv_status = Some(VenvStatus::Available);
    ok.venv_fingerprint = Some("aabbccdd01234567");
    ok.declared_runtime = Some("python_worker");
    let dec = decide(&ok);
    assert_eq!(dec.verdict, Verdict::Assign);
    assert!(dec
        .reason
        .iter()
        .any(|s| s.rule == "R1" && s.hit.contains("available")));
}

// ---------------------------------------------------------------------------
// 判据 8：决策只读面（policy.rs 源码 grep 断言）
// ---------------------------------------------------------------------------

#[test]
fn policy_module_read_only_surface() {
    let src = include_str!("../src/policy.rs");
    // 禁词面 = 对外副作用/环境读取符号（判据 8「无 registry 写入符号/无文件
    // 写路径/零时钟零随机」；局部 &mut Vec 属内部命中链构造，非写入口，
    // 不在禁词面）。
    let forbidden = [
        "std::fs",
        "std::net",
        "std::process",
        "std::time",
        "SystemTime",
        "Instant",
        "tokio",
        "unsafe",
        "static mut",
        "Mutex<",
        "RefCell<",
        "Cell<",
        "Registry::open",
        ".register(",
        "OpenOptions",
        "rand::",
        "env::var",
        "::Command",
        ".lock(",
        "conn.execute",
    ];
    for token in forbidden {
        assert!(
            !src.contains(token),
            "policy.rs 只读面违规符号 {:?}（红线 1/D10b）",
            token
        );
    }
    // 决策入口全部以 &DecisionInput 只读引用为参（无可变入口）。
    for sig in [
        "pub fn decide(input: &DecisionInput)",
        "pub fn derive_target(input: &DecisionInput)",
        "pub fn derive_trust(",
        "pub fn derive_dependency_complexity(",
        "pub fn derive_isolation_requirement(",
    ] {
        assert!(src.contains(sig), "决策入口签名在位：{sig}");
    }
}
