//! rp-image-pin CLI：镜像 digest 台账操作面（P10-2，ADR TOOL-RUNTIME §6
//! 「镜像回滚：镜像固定版本与 digest，更新先验证再切换，升级失败回滚旧
//! digest」的命令载具）。
//!
//! 子命令（--ledger 指向台账 JSON 文件，路径由调用方显式给定，属运行时
//! 数据；本 CLI 不假设目录布局，源码零 rip-data 绝对路径字面量）：
//!
//! - `show [--image <ref>]`：只读台账视图（全量或单镜像）。
//! - `pin --image <ref> --digest <sha256:...>`：初始入账（幂等）。
//! - `update --image <ref> --digest <sha256:...> [--manifest <tool-manifest>]`：
//!   更新先验证再切换。给 --manifest 时走真实验证链（tool-registry
//!   validate_static 第 1/3/4/5/6/8 条 + runtime.image / provenance
//!   .image_digest / 入参 digest 三方对账）；验证失败 verified=false，
//!   指针绝不切换（rolled_back，旧 digest 保持 current，ADR 升级失败回滚
//!   旧 digest 语义）。
//! - `rollback --image <ref>`：显式回滚到 previous_digest（无回滚位拒绝）。
//!
//! 输出统一单行 JSON；exit 0 成功、1 操作失败（digest_invalid /
//! image_unknown / rollback_unavailable / verification_failed 等机器可读
//! 错误）、2 用法错误（P9-3 帮助面统一口径）。

use std::path::PathBuf;

use serde_json::{json, Value};
use tool_engine::image_ledger::{digest_valid, ImageLedger};

fn usage_text() -> &'static str {
    "usage: rp-image-pin --ledger <path> <command> [flags]\n\n\
     commands:\n\
     \x20 show [--image <ref>]\n\
     \x20 pin --image <ref> --digest <sha256:...>\n\
     \x20 update --image <ref> --digest <sha256:...> [--manifest <tool-manifest>]\n\
     \x20 rollback --image <ref>\n\n\
     exit: ok=0, 操作失败=1（机器可读 JSON 到 stderr）, 用法错误=2\n\
     help: rp-image-pin --help（stdout, exit 0）"
}

fn usage() -> ! {
    eprintln!("{}", usage_text());
    std::process::exit(2);
}

fn die(err: tool_engine::image_ledger::LedgerError) -> ! {
    eprintln!("{}", serde_json::to_string(&err).unwrap_or_default());
    std::process::exit(1);
}

fn die_json(code: &str, message: &str) -> ! {
    eprintln!("{}", json!({"code": code, "message": message}));
    std::process::exit(1);
}

fn flag_value(raw: &[String], flag: &str) -> Option<String> {
    raw.iter()
        .position(|a| a == flag)
        .and_then(|i| raw.get(i + 1))
        .cloned()
}

fn require_flag(raw: &[String], flag: &str) -> String {
    match flag_value(raw, flag) {
        Some(v) => v,
        None => {
            eprintln!("missing required flag {flag}");
            usage();
        }
    }
}

fn parse_manifest(path: &str) -> Value {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| die_json("manifest_read_failed", &format!("read {path:?}: {e}")));
    if text.trim_start().starts_with('{') {
        serde_json::from_str(&text)
            .unwrap_or_else(|e| die_json("json_syntax", &format!("parse: {e}")))
    } else {
        serde_yaml_ng::from_str(&text)
            .unwrap_or_else(|e| die_json("yaml_syntax", &format!("parse: {e}")))
    }
}

/// runtime.image 的 digest 部分（`image@sha256:<hex>`；无 @ 则 None）。
fn image_digest_part(image: &str) -> Option<&str> {
    image.split('@').next_back().filter(|d| *d != image)
}

/// 更新的真实验证链（ADR §6「更新先验证再切换」）：结构校验（tool-registry
/// validate_static）+ 三方 digest 对账（runtime.image digest 部分 =
/// provenance.image_digest = 入参 --digest）。返回 (verified, reason)。
fn verify_update(manifest: &Value, expected_digest: &str) -> (bool, String) {
    let image = manifest
        .get("runtime")
        .and_then(|r| r.get("image"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let image_part = match image_digest_part(&image) {
        Some(p) => p.to_string(),
        None => {
            return (false, "runtime.image has no digest part".to_string());
        }
    };
    let provenance_digest = manifest
        .get("provenance")
        .and_then(|p| p.get("image_digest"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if image_part != expected_digest {
        return (
            false,
            format!("runtime.image digest {image_part:?} != requested {expected_digest:?}"),
        );
    }
    if !provenance_digest.is_empty() && provenance_digest != expected_digest {
        return (
            false,
            format!(
                "provenance.image_digest {provenance_digest:?} != requested {expected_digest:?}"
            ),
        );
    }
    let opts = tool_registry::validate::Options::default();
    match tool_registry::validate::validate_static(manifest, &opts) {
        Ok(_) => (
            true,
            "validate_static passed + digest triple matched".to_string(),
        ),
        Err(e) => (false, format!("validate_static rejected: {e}")),
    }
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if raw.iter().any(|a| a == "--help" || a == "-h") {
        println!("{}", usage_text());
        std::process::exit(0);
    }
    // 位置参数收集：跳过 --flag value 对（known flags 均带值）。
    const FLAGS_WITH_VALUE: [&str; 4] = ["--ledger", "--image", "--digest", "--manifest"];
    let mut positional: Vec<String> = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        if FLAGS_WITH_VALUE.contains(&raw[i].as_str()) {
            i += 2;
        } else {
            positional.push(raw[i].clone());
            i += 1;
        }
    }
    let Some(command) = positional.first() else {
        usage()
    };
    let ledger_path = match flag_value(&raw, "--ledger") {
        Some(p) => PathBuf::from(p),
        None => {
            eprintln!("missing required flag --ledger");
            usage();
        }
    };
    let mut ledger = ImageLedger::load(&ledger_path).unwrap_or_else(|e| die(e));

    let out: Value = match command.as_str() {
        "show" => match flag_value(&raw, "--image") {
            Some(image) => {
                let pin = ledger.get(&image).unwrap_or_else(|| {
                    die_json("image_unknown", &format!("image {image:?} is not pinned"))
                });
                json!({"command": "show", "pin": pin})
            }
            None => json!({"command": "show", "pins": ledger.pins}),
        },
        "pin" => {
            let image = require_flag(&raw, "--image");
            let digest = require_flag(&raw, "--digest");
            ledger.pin(&image, &digest).unwrap_or_else(|e| die(e));
            ledger.save(&ledger_path).unwrap_or_else(|e| die(e));
            json!({"command": "pin", "image": image, "digest": digest, "ok": true})
        }
        "update" => {
            let image = require_flag(&raw, "--image");
            let digest = require_flag(&raw, "--digest");
            // 先验证（--manifest 给出时走真实验证链；否则仅 digest 格式）。
            let (verified, reason) = match flag_value(&raw, "--manifest") {
                Some(path) => verify_update(&parse_manifest(&path), &digest),
                None => (
                    digest_valid(&digest),
                    if digest_valid(&digest) {
                        "digest format ok (no --manifest, ledger-level check only)".to_string()
                    } else {
                        "digest format invalid".to_string()
                    },
                ),
            };
            let outcome = ledger
                .update(&image, &digest, verified, Some(&reason))
                .unwrap_or_else(|e| die(e));
            ledger.save(&ledger_path).unwrap_or_else(|e| die(e));
            json!({"command": "update", "image": image, "digest": digest,
                   "verified": verified, "verification_reason": reason, "outcome": outcome})
        }
        "rollback" => {
            let image = require_flag(&raw, "--image");
            let outcome = ledger.rollback(&image).unwrap_or_else(|e| die(e));
            ledger.save(&ledger_path).unwrap_or_else(|e| die(e));
            json!({"command": "rollback", "image": image, "outcome": outcome})
        }
        other => {
            eprintln!("unknown command {other:?}");
            usage();
        }
    };
    println!("{}", serde_json::to_string(&out).unwrap_or_default());
}
