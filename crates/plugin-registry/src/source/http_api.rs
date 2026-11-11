//! http_api 来源（PLUGIN-CONTRACT §6 第五行；ROADMAP P6-4）。
//!
//! 语义：source.base_url + api_version。版本固定 = API 版本 + 契约文件哈希：
//! GET base_url 取契约 JSON，api_version 与清单声明一致后，契约体 sha256
//! 与契约内容进协商快照（安装链写 plugin.lock.negotiation）。契约内容
//! 落位 contract.json 成包，解压后逐文件 checksums 由安装链生成。
//!
//! 形态边界（如实声明）：契约路径 PLUGIN-CONTRACT §12.4 明文待验证，本批
//! 落最小形态 = GET base_url 根路径返回契约 JSON；传输用 std TcpStream
//! 最小 HTTP/1.0 GET，仅 http://（明文、本机/内网优先，TLS 归后续批次）；
//! 重试与限流归 P6-7 运行层。

use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};

use super::{FetchOutcome, Source, SourceError};
use tool_registry::hash::sha256_hex;

use super::TrustLevel;

/// http_api 来源实现。
pub struct HttpApiSource;

/// 最小 HTTP/1.0 GET（std TcpStream，仅 http://；返回状态码与响应体）。
pub fn http_get(url: &str) -> Result<(u16, Vec<u8>), SourceError> {
    let rest = url.strip_prefix("http://").ok_or_else(|| {
        SourceError::new(
            "scheme_unsupported",
            format!("http_api url must be http:// in this batch (TLS deferred): {url:?}"),
        )
    })?;
    let (host_port, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let addr = format!("{host_port}:80");
    // base_url 带 :port 时 host_port 已含端口， addr 拼接会双冒号，处理之。
    let addr = if host_port.contains(':') {
        host_port.to_string()
    } else {
        addr
    };
    let mut stream = std::net::TcpStream::connect(&addr)
        .map_err(|e| SourceError::new("connect_failed", format!("connect {addr:?}: {e}")))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| SourceError::new("io", format!("set timeout: {e}")))?;
    let request = format!("GET {path} HTTP/1.0\r\nHost: {host_port}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|e| SourceError::new("io", format!("write request: {e}")))?;
    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .map_err(|e| SourceError::new("io", format!("read response: {e}")))?;
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| SourceError::new("http_invalid", "response has no header/body split"))?;
    let header = String::from_utf8_lossy(&raw[..split]).to_string();
    let status: u16 = header
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| {
            SourceError::new(
                "http_invalid",
                format!("bad status line: {}", header.lines().next().unwrap_or("")),
            )
        })?;
    Ok((status, raw[split + 4..].to_vec()))
}

impl Source for HttpApiSource {
    fn source_type(&self) -> &'static str {
        "http_api"
    }

    fn verify(&self, manifest: &Value) -> Result<(), SourceError> {
        let source = super::source_of(manifest)?;
        let base_url = source
            .get("base_url")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                SourceError::new("base_url_missing", "http_api source requires `base_url`")
            })?;
        if !base_url.starts_with("http://") {
            return Err(SourceError::new(
                "scheme_unsupported",
                format!("base_url {base_url:?} must be http:// in this batch (TLS deferred)"),
            ));
        }
        if source
            .get("api_version")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .is_empty()
        {
            return Err(SourceError::new(
                "api_version_missing",
                "http_api source requires `api_version`",
            ));
        }
        Ok(())
    }

    fn fetch(&self, manifest: &Value, target: &Path) -> Result<FetchOutcome, SourceError> {
        let source = super::source_of(manifest)?;
        let base_url = source
            .get("base_url")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                SourceError::new("base_url_missing", "http_api source requires `base_url`")
            })?;
        let declared_version = source
            .get("api_version")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let (status, body) = http_get(base_url)?;
        if status != 200 {
            return Err(SourceError::new(
                "contract_fetch_failed",
                format!("GET {base_url:?} returned status {status}"),
            )
            .with_details(json!({"status": status})));
        }
        let contract: Value = serde_json::from_slice(&body)
            .map_err(|e| SourceError::new("contract_invalid", format!("contract body: {e}")))?;
        // API 版本一致性（§6 版本固定半边）。
        let served_version = contract
            .get("api_version")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if served_version != declared_version {
            return Err(SourceError::new(
                "api_version_mismatch",
                format!("contract api_version {served_version:?} does not match declared {declared_version:?}"),
            )
            .with_details(json!({"served": served_version, "declared": declared_version})));
        }
        let contract_sha256 = sha256_hex(&body);
        let negotiation = json!({
            "api_version": served_version,
            "contract_sha256": contract_sha256,
            "contract": contract,
        });
        std::fs::create_dir_all(target.join("capabilities"))
            .map_err(|e| SourceError::new("io", format!("create_dir {target:?}: {e}")))?;
        std::fs::write(target.join("contract.json"), &body)
            .map_err(|e| SourceError::new("io", format!("write contract.json: {e}")))?;
        std::fs::write(target.join("negotiation.json"), negotiation.to_string())
            .map_err(|e| SourceError::new("io", format!("write negotiation.json: {e}")))?;
        let descriptor = format!(
            "# HTTP API contract snapshot (contract_sha256: {})\n{}\n",
            contract_sha256,
            serde_json::to_string_pretty(&contract).unwrap_or_default()
        );
        std::fs::write(target.join("capabilities/http-contract.yaml"), descriptor)
            .map_err(|e| SourceError::new("io", format!("write capability descriptor: {e}")))?;
        Ok(FetchOutcome {
            target: target.to_path_buf(),
            negotiation: Some(negotiation),
        })
    }

    fn default_trust(&self) -> TrustLevel {
        TrustLevel::Remote
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_rejected_this_batch() {
        let manifest = serde_json::json!({"source": {"type": "http_api", "base_url": "https://example.com", "api_version": "1"}});
        let err = HttpApiSource.verify(&manifest).unwrap_err();
        assert_eq!(err.code, "scheme_unsupported");
    }

    #[test]
    fn non_200_rejected() {
        let manifest = serde_json::json!({"source": {"type": "http_api", "base_url": "http://127.0.0.1:9/", "api_version": "1"}});
        let err = HttpApiSource
            .fetch(&manifest, Path::new("/tmp/p6-http-none"))
            .unwrap_err();
        assert!(
            matches!(err.code.as_str(), "connect_failed"),
            "got {}",
            err.code
        );
    }
}
