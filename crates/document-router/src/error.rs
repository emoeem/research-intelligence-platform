//! 错误映射（DI-1 ADR D8 总表）：Router 新增事件全部映射进既有九类词表
//! （envelope.rs:14-24）与 RT-1 NATIVE_* 码表，`ROUTER_*` 与
//! `OFFICE_CONVERT_NO_OUTPUT` 为信封 error.code 自由面（九类约束的是
//! category 非 code，envelope::validate 不受影响）；Router 自定义 category
//! 为 D8 否决项（超词表 envelope::validate 直接拒绝）。
//!
//! 拒绝语义分层（D7）：
//! - **嗅探门前置门**（无通道签名 / 文件超 64 MiB）：dispatch 层 INPUT 类
//!   错误载荷返回，无 run 行；
//! - **链级失败**（页数/像素超帽、soffice 产物缺失、子 run 失败透传）：子
//!   run 行已在库，dispatch 错误载荷 + 子 run 行留证。

/// Router 拒绝 / 链级失败（D8 映射行；信封 error 对象载荷来源）。
#[derive(Debug, Clone, PartialEq)]
pub struct RouterReject {
    /// §5.3 九类类别（词表内，D8 否决自定义 category；String 承载以兼容
    /// 子 run 信封透传，词表内取值由 chain_failed 构造器防线强制）。
    pub category: String,
    /// error.code（字符串自由面：ROUTER_* / OFFICE_CONVERT_NO_OUTPUT /
    /// RT-1 NATIVE_* 码表透传）。
    pub code: String,
    /// 错误消息（如实呈现，零静默降级）。
    pub message: String,
    /// 可重试性（瞬时故障 true，其余 false——D8 表逐行口径）。
    pub retryable: bool,
}

impl RouterReject {
    /// 嗅探门：无通道签名（zip_other / unknown 魔数）→ INPUT
    /// `ROUTER_UNSUPPORTED_TYPE`（无 run 行，D7 前置门语义）。
    pub fn unsupported_type(magic: &crate::sniff::Magic, detail: &str) -> Self {
        Self {
            category: "INPUT".into(),
            code: "ROUTER_UNSUPPORTED_TYPE".into(),
            message: format!(
                "sniff signature {:?} has no DI-1 channel: {detail}",
                magic.as_str()
            ),
            retryable: false,
        }
    }

    /// 嗅探门：text 签名 → INPUT `ROUTER_UNSUPPORTED_TYPE`，注明走既有
    /// rip.doc.parse（D6：rip.doc.parse 冻结不迁不扩，禁造第二套 md/txt/html
    /// 解析；判据 4 注明要求）。
    pub fn text_not_routed(ext: &str) -> Self {
        Self {
            category: "INPUT".into(),
            code: "ROUTER_UNSUPPORTED_TYPE".into(),
            message: format!(
                "sniff signature \"text\" is not routed by DI-1; pure text goes to rip.doc.parse (md/txt/html frozen scope, DI-0 §3; ext hint {ext:?})"
            ),
            retryable: false,
        }
    }

    /// 嗅探门：文件超 64 MiB → INPUT `ROUTER_INPUT_TOO_LARGE`（无 run 行）。
    pub fn input_too_large(bytes: u64, cap: u64) -> Self {
        Self {
            category: "INPUT".into(),
            code: "ROUTER_INPUT_TOO_LARGE".into(),
            message: format!(
                "input file {bytes} bytes exceeds cap {cap} bytes (D7 #7 input size cap)"
            ),
            retryable: false,
        }
    }

    /// 链内：页数超 2000 → INPUT `ROUTER_PAGE_BUDGET_EXCEEDED`（pdfinfo 子
    /// run 后解析定案；已存子 run 行保持真实终态，D8 表口径）。
    pub fn page_budget_exceeded(pages: u64, cap: u64) -> Self {
        Self {
            category: "INPUT".into(),
            code: "ROUTER_PAGE_BUDGET_EXCEEDED".into(),
            message: format!(
                "document has {pages} pages, exceeds DI-1 page cap {cap} (D7 #7); chain aborted"
            ),
            retryable: false,
        }
    }

    /// 链内：像素超 100 MP → INPUT `ROUTER_PIXEL_BUDGET_EXCEEDED`（identify
    /// 输出解析定案）。
    pub fn pixel_budget_exceeded(pixels: u64, cap: u64) -> Self {
        Self {
            category: "INPUT".into(),
            code: "ROUTER_PIXEL_BUDGET_EXCEEDED".into(),
            message: format!("image {pixels} pixels exceeds DI-1 pixel cap {cap} (D7 #8, metadata path -ping); chain aborted"),
            retryable: false,
        }
    }

    /// 链内：soffice 被 TERM/KILL 后 out/ 无转换产物 → EXECUTION
    /// `OFFICE_CONVERT_NO_OUTPUT`（挂死残迹 lu*.tmp 如实进 stderr/日志面）。
    pub fn office_no_output(run_id: &str, detail: &str) -> Self {
        Self {
            category: "EXECUTION".into(),
            code: "OFFICE_CONVERT_NO_OUTPUT".into(),
            message: format!("office convert produced no PDF artifact (run {run_id}): {detail}"),
            retryable: false,
        }
    }

    /// 链内：子 run 失败透传（RT-1 NATIVE_* 码表 / 退出码语义，D8 表既有行）。
    pub fn chain_failed(category: &str, code: &str, message: String, retryable: bool) -> Self {
        Self {
            category: match category {
                "VALIDATION" | "INPUT" | "TIMEOUT" | "CANCELLED" | "EXECUTION"
                | "POLICY_DENIED" | "DEPENDENCY" | "INFRA" | "RUNNER_UNAVAILABLE" => {
                    category.to_string()
                }
                other => panic!(
                    "sub-run category {other:?} outside the nine-word vocabulary (D8: Router may not invent categories)"
                ),
            },
            code: match code_is_transparent(code) {
                true => code.to_string(),
                false => panic!(
                    "sub-run code {code:?} is not a transparent passthrough (D8 chain_failed contract)"
                ),
            },
            message,
            retryable,
        }
    }

    /// 信封 error 形状（run_tool 错误载荷 / RouterOutcome 携带共用）。
    pub fn to_error_value(&self) -> serde_json::Value {
        serde_json::json!({
            "category": self.category,
            "code": self.code,
            "message": self.message,
            "retryable": self.retryable,
        })
    }

    /// dispatch 层错误载荷（err_payload 同形状 + code 字段；无 run 行场景）。
    pub fn to_payload(&self) -> serde_json::Value {
        serde_json::json!({
            "ok": false,
            "error": {
                "category": self.category,
                "code": self.code,
                "message": self.message,
                "retryable": self.retryable,
            }
        })
    }
}

/// chain_failed 的 code 透传面：RT-1 NATIVE_* 码表 / 退出码整数串 /
/// D8 表自有码。非空即透传（code 自由面，category 才是词表约束——envelope.rs
/// validate 对 code 无词表；此处只挡空串防污染）。
fn code_is_transparent(code: &str) -> bool {
    !code.is_empty() && code.len() <= 128
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sniff::Magic;

    #[test]
    fn gate_rejects_carry_input_category() {
        let r = RouterReject::unsupported_type(
            &Magic::ZipOther,
            "archive two-phase defense is DI-3 scope",
        );
        assert_eq!(r.category, "INPUT");
        assert_eq!(r.code, "ROUTER_UNSUPPORTED_TYPE");
        let r = RouterReject::text_not_routed("md");
        assert!(
            r.message.contains("rip.doc.parse"),
            "判据 4：注明走 rip.doc.parse"
        );
        let r = RouterReject::input_too_large(67_108_865, 67_108_864);
        assert_eq!(r.code, "ROUTER_INPUT_TOO_LARGE");
        assert_eq!(r.category, "INPUT");
    }

    #[test]
    fn chain_failures_carry_d8_codes() {
        let r = RouterReject::page_budget_exceeded(2001, 2_000);
        assert_eq!(r.code, "ROUTER_PAGE_BUDGET_EXCEEDED");
        assert_eq!(r.category, "INPUT");
        let r = RouterReject::pixel_budget_exceeded(100_000_001, 100_000_000);
        assert_eq!(r.code, "ROUTER_PIXEL_BUDGET_EXCEEDED");
        let r = RouterReject::office_no_output("rn-x", "out/ empty after TERM");
        assert_eq!(r.category, "EXECUTION");
        assert_eq!(r.code, "OFFICE_CONVERT_NO_OUTPUT");
    }

    #[test]
    fn chain_failed_passthrough_maps_nine_categories() {
        let r =
            RouterReject::chain_failed("DEPENDENCY", "NATIVE_BINARY_MISSING", "m".into(), false);
        assert_eq!(r.category, "DEPENDENCY");
        assert_eq!(r.code, "NATIVE_BINARY_MISSING");
        let r = RouterReject::chain_failed("EXECUTION", "7", "exit 7".into(), false);
        assert_eq!(r.category, "EXECUTION");
        // 词表外 category / 空 code → 程序性 panic（D8 否决项防线）。
        let result = std::panic::catch_unwind(|| {
            let _ = RouterReject::chain_failed("ROUTER", "X", "m".into(), false);
        });
        assert!(result.is_err(), "自定义 category 必须被防线拦截");
    }

    #[test]
    fn payload_shapes_match_dispatch_convention() {
        let r = RouterReject::unsupported_type(&Magic::Unknown, "no magic");
        let payload = r.to_payload();
        assert_eq!(payload["ok"], serde_json::json!(false));
        assert_eq!(payload["error"]["category"], serde_json::json!("INPUT"));
        assert_eq!(
            payload["error"]["code"],
            serde_json::json!("ROUTER_UNSUPPORTED_TYPE")
        );
        let err = r.to_error_value();
        assert_eq!(err["retryable"], serde_json::json!(false));
    }
}
