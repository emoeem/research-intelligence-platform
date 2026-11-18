//! WSW-1 HTTP Message Foundation（`docs/rip3/wsw/WSW-1-TASK.md`，ADR-0004 下游）。
//!
//! HTTP 消息四表示与 Exchange 的 Rust 实现（DESIGN §1.1/§1.2）：
//!
//! - [`types`]：四表示类型。[`WireBytes`]（捕获原始字节，唯一原始事实）、
//!   [`ParsedMessage`](types::ParsedMessage)（httparse 驱动的结构化表示）、
//!   [`DisplayPayload`](types::DisplayPayload)（展示投影，永不回写）、
//!   [`OutboundBytes`](types::OutboundBytes)（重序列化发送字节）；
//!   另含 [`CaptureFidelity`](types::CaptureFidelity)/[`ParseStatus`](types::ParseStatus)
//!   词表枚举。
//! - [`ids`]：`ExchangeId`/`SessionId`/`ListenerId` 新类型（沿 `domain::ids`
//!   范式：域层只做非空与长度校验，前缀由生成侧承载）。
//! - [`exchange`]：Exchange 结构（DESIGN §1.2 全字段 + serde round-trip）。
//! - [`parse`]：HTTP/1.1 wire→Parsed（请求含绝对形式/CONNECT 后明文；
//!   chunked/CL/EOF 三种体定界；头序保持、重复头多值保留、头帽 64KiB）。
//! - [`serialize`]：Parsed/编辑态→Outbound 字节（编辑头序保持 + 体重定界）。
//! - [`redact`]：脱敏规则输入→Display 输出（DESIGN §4.5 唯一挂点）。
//! - [`cas`]：正文 CAS 写入（sha256 寻址、原子去重、完整字节保留、哈希读回校验）。
//! - [`artifact`]：BodyArtifactAdapter 先登记 `wsw_body_blob`，再通过 trait 对接
//!   平台现有 Artifact/Evidence Registrar，关联 ToolRun/Exchange/Task/Trace；不新建事实表。
//! - [`store`]：`wsw_body_blob` 登记/查询/GC 接口（rusqlite 经 `database`
//!   重导出使用，不直连；迁移执行器唯一为 `database::migrate`）。
//!
//! 不变式（DESIGN §1.1 三条，本 crate 机制化）：
//! 1. Wire 字节是唯一"原始事实"，Parsed 从 Wire 派生并记录
//!    [`ParseStatus`](types::ParseStatus)；解析失败不伪造 Parsed
//!    （`parse_status=failed` + `error_code`）。
//! 2. Outbound 只能从用户编辑态重序列化产生（[`serialize`] 只接受显式
//!    编辑态输入），任何路径不得把 Display 直接发送。
//! 3. Display 生成过程不得改变 Wire/Parsed 存储（[`redact`] 为纯函数，
//!    输入只读借用，输出独立新值）。
//!
//! 非目标（WSW-1 边界）：Listener/MITM 运行时（WSW-2）、UI、Repeater/Fuzzer
//! 发送（WSW-3）、HAR 导出（WSW-3/6）、h2 帧级消息（`http::Version::HTTP_2`
//! 只做标记不解析）。

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod artifact;
pub mod cas;
pub mod exchange;
pub mod ids;
pub mod parse;
pub mod redact;
pub mod serialize;
pub mod store;
pub mod types;
