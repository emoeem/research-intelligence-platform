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

    // -----------------------------------------------------------------
    // DI-2 ADR D6 新增 code 表（category 全部在九类词表内；code 走信封
    // error.code 自由面）
    // -----------------------------------------------------------------

    /// 前置门：OCR 请求语言在 capability 目录 language 集缺失（chi_tra 面
    /// 等）→ DEPENDENCY `ROUTER_OCR_LANG_MISSING`，**零 run 行**（D7 前置门
    /// 语义），retryable=false。消息注明：语言包缺失属环境持久事实，补装属
    /// 用户决策不自动安装；禁静默回退（RT-0.5-C §3.3 constraints 原文）。
    pub fn ocr_lang_missing(requested: &str, missing: &[String]) -> Self {
        Self {
            category: "DEPENDENCY".into(),
            code: "ROUTER_OCR_LANG_MISSING".into(),
            message: format!(
                "requested OCR language {requested:?} maps to language pack(s) {missing:?} absent from the capability catalog (ocr.tesseract language set): language-pack absence is a persistent environment fact, installing it is a user decision and is never performed automatically; silent fallback to other language packs is forbidden (RT-0.5-C 3.3 constraints, DI-2 ADR D2)"
            ),
            retryable: false,
        }
    }

    /// 链内：OCR 门步像素超 50 MP（DI-2 OCR 派生帽，D2/D5 收紧口径）→
    /// INPUT `ROUTER_PIXEL_BUDGET_EXCEEDED`（码沿用 DI-1，消息注明 OCR 帽）；
    /// identify 行留证，tesseract 未拉起。
    pub fn pixel_budget_exceeded_ocr(pixels: u64, cap: u64) -> Self {
        Self {
            category: "INPUT".into(),
            code: "ROUTER_PIXEL_BUDGET_EXCEEDED".into(),
            message: format!(
                "image {pixels} pixels exceeds the OCR decode-path pixel cap {cap} (DI-2 ADR D2/D5: OCR is a full-decode face, cap tightened from the 100 MP metadata -ping path); tesseract never sees over-cap input; chain aborted"
            ),
            retryable: false,
        }
    }

    /// 链内：媒体内容时长超 300 s → INPUT `ROUTER_MEDIA_DURATION_EXCEEDED`
    /// （ffprobe format.duration 解析定案，ffmpeg 派生前；probe 行留证）。
    pub fn media_duration_exceeded(duration_s: f64, cap_s: f64) -> Self {
        Self {
            category: "INPUT".into(),
            code: "ROUTER_MEDIA_DURATION_EXCEEDED".into(),
            message: format!(
                "media content duration {duration_s} s exceeds cap {cap_s} s (DI-2 ADR D3/D5, ffprobe format.duration resolved before any ffmpeg spawn); chain aborted"
            ),
            retryable: false,
        }
    }

    /// 链内：媒体单帧像素超 16 MP → INPUT `ROUTER_MEDIA_PIXEL_BUDGET_
    /// EXCEEDED`（ffprobe streams[video].width×height 定案；probe 行留证）。
    pub fn media_pixel_budget_exceeded(pixels: u64, cap: u64) -> Self {
        Self {
            category: "INPUT".into(),
            code: "ROUTER_MEDIA_PIXEL_BUDGET_EXCEEDED".into(),
            message: format!(
                "video frame {pixels} pixels exceeds cap {cap} (DI-2 ADR D3/D5, ffprobe stream dimensions resolved before any ffmpeg spawn); chain aborted"
            ),
            retryable: false,
        }
    }

    /// 链内：归档条目路径不安全（`..` 段 / 以 `/` 起头 / 含 `\`）→ INPUT
    /// `ROUTER_ARCHIVE_ENTRY_INVALID`（闸 1；list 行留证，解包零提交）。
    pub fn archive_entry_invalid(path: &str, reason: &str) -> Self {
        Self {
            category: "INPUT".into(),
            code: "ROUTER_ARCHIVE_ENTRY_INVALID".into(),
            message: format!(
                "archive entry {path:?} fails path-safety gate: {reason} (DI-2 ADR D4 gate 1; 7z sanitization is a second line of defense, the Router verdict is authoritative); extract phase never submitted"
            ),
            retryable: false,
        }
    }

    /// 链内：归档符号链接条目 → INPUT `ROUTER_ARCHIVE_ENTRY_SYMLINK`（闸 2：
    /// `Symbolic Link = ` 字段非空或 Attributes 含 `l` 权限前缀）。
    pub fn archive_entry_symlink(path: &str, detail: &str) -> Self {
        Self {
            category: "INPUT".into(),
            code: "ROUTER_ARCHIVE_ENTRY_SYMLINK".into(),
            message: format!(
                "archive entry {path:?} is a symbolic link ({detail}) and is rejected by the link gate before extraction (DI-2 ADR D4 gate 2, RT-0.5-E 4.3); extract phase never submitted"
            ),
            retryable: false,
        }
    }

    /// 链内：归档条目数超 512 → INPUT `ROUTER_ARCHIVE_ENTRY_BUDGET_EXCEEDED`
    /// （闸 3，含目录条目计数）。
    pub fn archive_entry_budget_exceeded(count: usize, cap: usize) -> Self {
        Self {
            category: "INPUT".into(),
            code: "ROUTER_ARCHIVE_ENTRY_BUDGET_EXCEEDED".into(),
            message: format!(
                "archive lists {count} entries, exceeds entry cap {cap} (DI-2 ADR D4 gate 3); extract phase never submitted"
            ),
            retryable: false,
        }
    }

    /// 链内：归档解压总量超 256 MiB 或单条目超 64 MiB → INPUT
    /// `ROUTER_ARCHIVE_BUDGET_EXCEEDED`（闸 4；压缩炸弹 Size 如实进消息）。
    pub fn archive_budget_exceeded(
        total: u64,
        total_cap: u64,
        worst: (&str, u64),
        entry_cap: u64,
    ) -> Self {
        let (worst_path, worst_bytes) = worst;
        Self {
            category: "INPUT".into(),
            code: "ROUTER_ARCHIVE_BUDGET_EXCEEDED".into(),
            message: format!(
                "archive decompressed total {total} bytes vs total cap {total_cap} bytes; worst entry {worst_path:?} declares {worst_bytes} bytes vs per-entry cap {entry_cap} bytes (DI-2 ADR D4 gate 4, uncompressed-size declarations from the 7z -slt listing); extract phase never submitted"
            ),
            retryable: false,
        }
    }

    /// 链内：解包后 out/ 残留符号链接 → POLICY_DENIED
    /// `ROUTER_ARCHIVE_SYMLINK_SURVIVED`（post-extract 终检，闸 2 之后的
    /// 第二道防线兜底，D4）。
    pub fn archive_symlink_survived(paths: &[String]) -> Self {
        Self {
            category: "POLICY_DENIED".into(),
            code: "ROUTER_ARCHIVE_SYMLINK_SURVIVED".into(),
            message: format!(
                "post-extract sweep found symbolic links surviving in the run out/ directory: {paths:?} (DI-2 ADR D4 post-extract terminal check, second line of defense after the listing gate); chain aborted"
            ),
            retryable: false,
        }
    }

    /// 链内：列目成功但清单零条目 / 头块解析失败 → EXECUTION
    /// `ARCHIVE_LIST_NO_OUTPUT`（OFFICE_CONVERT_NO_OUTPUT 同位语义，D6）。
    pub fn archive_list_no_output(run_id: &str, detail: &str) -> Self {
        Self {
            category: "EXECUTION".into(),
            code: "ARCHIVE_LIST_NO_OUTPUT".into(),
            message: format!(
                "archive listing produced no parseable inventory (run {run_id}): {detail}"
            ),
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

    /// DI-2 ADR D6 新增 code 表：九类词表内 category + D6 表 code/时点逐行。
    #[test]
    fn di2_new_codes_match_d6_table() {
        // OCR 语言缺失：DEPENDENCY + 零静默回退消息 + retryable=false。
        let r = RouterReject::ocr_lang_missing("zh-Hant", &["chi_tra".to_string()]);
        assert_eq!(r.category, "DEPENDENCY");
        assert_eq!(r.code, "ROUTER_OCR_LANG_MISSING");
        assert!(!r.retryable);
        assert!(r.message.contains("chi_tra"));
        assert!(r.message.contains("user decision"), "补装属用户决策");
        assert!(r.message.contains("silent fallback"), "禁静默回退注明");
        // OCR 像素帽：码沿用 DI-1，消息注明 OCR 派生帽。
        let r = RouterReject::pixel_budget_exceeded_ocr(121_000_000, 50_000_000);
        assert_eq!(r.category, "INPUT");
        assert_eq!(r.code, "ROUTER_PIXEL_BUDGET_EXCEEDED");
        assert!(r.message.contains("121000000"));
        assert!(r.message.contains("OCR decode-path"));
        // 媒体两门。
        let r = RouterReject::media_duration_exceeded(400.0, 300.0);
        assert_eq!(r.category, "INPUT");
        assert_eq!(r.code, "ROUTER_MEDIA_DURATION_EXCEEDED");
        assert!(r.message.contains("400"));
        let r = RouterReject::media_pixel_budget_exceeded(20_000_000, 16_000_000);
        assert_eq!(r.code, "ROUTER_MEDIA_PIXEL_BUDGET_EXCEEDED");
        // 归档四闸 + 终检 + 列目空清单。
        let r = RouterReject::archive_entry_invalid("../escape.txt", "contains a `..` segment");
        assert_eq!(r.category, "INPUT");
        assert_eq!(r.code, "ROUTER_ARCHIVE_ENTRY_INVALID");
        let r = RouterReject::archive_entry_symlink("link", "Attributes lrwxrwxrwx");
        assert_eq!(r.code, "ROUTER_ARCHIVE_ENTRY_SYMLINK");
        let r = RouterReject::archive_entry_budget_exceeded(513, 512);
        assert_eq!(r.code, "ROUTER_ARCHIVE_ENTRY_BUDGET_EXCEEDED");
        let r = RouterReject::archive_budget_exceeded(
            67_108_864,
            256 * 1024 * 1024,
            ("zeros.bin", 67_108_864),
            64 * 1024 * 1024,
        );
        assert_eq!(r.category, "INPUT");
        assert_eq!(r.code, "ROUTER_ARCHIVE_BUDGET_EXCEEDED");
        assert!(
            r.message.contains("67108864"),
            "压缩炸弹 Size 如实进消息：{}",
            r.message
        );
        let r = RouterReject::archive_symlink_survived(&["out/link".to_string()]);
        assert_eq!(r.category, "POLICY_DENIED", "终检越 POLICY_DENIED（D6 表）");
        assert_eq!(r.code, "ROUTER_ARCHIVE_SYMLINK_SURVIVED");
        let r = RouterReject::archive_list_no_output("rn-x", "no Type header");
        assert_eq!(r.category, "EXECUTION");
        assert_eq!(r.code, "ARCHIVE_LIST_NO_OUTPUT");
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
