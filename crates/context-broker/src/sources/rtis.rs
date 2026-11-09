//! rtis 只读源（子进程 Adapter；设计稿 §4.3 候选第二类红队面 + §12 复用
//! 对照 + 工具盘点:161 Adapter 纪律）。
//!
//! 红线：RTIS 本体零改动，只经 `python3 -I rt.py`（cwd = kit 目录）只读
//! 调用：`poc search --q <kw> --limit N --json`（机器可读输出）与
//! `poc get --id N --json`（L3 展开）。超时与失败语义按设计稿 §13.6 对齐
//! eri_adapter.py 先例：单次超时默认 5s、失败（超时/非零退出/坏 JSON）单次
//! 重试（每路最多 2 次尝试），两次失败即为「不可用」态，candidates 返回
//! 空集（设计稿 §9.2 第 4 条空结果合法，理由经 [`RtisStatus::Unavailable`]
//! 披露，审计随 P4-4 装配事件落）。

use crate::error::ContextBrokerError;
use crate::source::{Candidate, Content, ContextSource};
use crate::{ContentRef, ContextItemKind, ContextLayer, IntentQuery};
use serde_json::Value;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// 源 id（Manifest item source 词表成员，设计稿 §6.1 rtis）。
pub const RTIS_SOURCE_ID: &str = "rtis";

/// 默认单次尝试超时（eri_adapter.py 5s 先例，设计稿 §13.6）。
pub const RTIS_TIMEOUT: Duration = Duration::from_secs(5);

/// 每路最大尝试次数（eri_adapter.py「失败单次重试」口径）。
const MAX_ATTEMPTS: usize = 2;

/// rtis 源运行状态（三态回落的设计稿 §13.6 落面）。
#[derive(Debug, Clone, PartialEq)]
pub enum RtisStatus {
    /// 只读命中正常返回。
    Ok,
    /// 两次尝试后不可用（附原因；candidates 映射为空集）。
    Unavailable(String),
}

/// RTIS 只读源（kit 目录 + 超时；不持任何写路径）。
#[derive(Debug, Clone)]
pub struct RtisSource {
    kit_dir: PathBuf,
    timeout: Duration,
}

impl RtisSource {
    /// 用 kit 目录构造（integrations/rtis；timeout 默认 5s）。
    pub fn new(kit_dir: PathBuf) -> Self {
        Self {
            kit_dir,
            timeout: RTIS_TIMEOUT,
        }
    }

    /// 覆盖默认超时（测试与慢环境用）。
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// 关键词检索（返回候选与三态状态）。
    ///
    /// 查询策略（本批实测冻结）：rt.py `--q` 的 FTS5 匹配对整串多词与
    /// CJK 混排命中为 0（「kingdee http 检查」「kingdee http」实测均 0，
    /// 单词 kingdee 实测 2），故按 resource-engine tokenize 词元逐查合并
    /// （至多 3 个词元，按 item_id 去重，顺序保持）；全部词元不可用且零
    /// 结果时返回 Unavailable（部分可用记 Ok，三态回落设计稿 §13.6）。
    pub fn search_with_status(&self, query: &str, limit: usize) -> (Vec<Candidate>, RtisStatus) {
        let query = query.trim();
        if query.is_empty() {
            return (Vec::new(), RtisStatus::Ok);
        }
        let mut tokens = resource_engine::tokenizer::tokenize(query);
        tokens.truncate(3);
        if tokens.is_empty() {
            tokens.push(query.to_string());
        }
        let cap = limit.max(1);
        let mut merged: Vec<Candidate> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut last_unavailable: Option<String> = None;
        for token in &tokens {
            let args = vec![
                "-I".to_string(),
                "rt.py".to_string(),
                "poc".to_string(),
                "search".to_string(),
                "--q".to_string(),
                token.clone(),
                "--limit".to_string(),
                cap.to_string(),
                "--json".to_string(),
            ];
            match self.run_json(&args) {
                Ok(value) => {
                    let results = value
                        .get("results")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    for row in &results {
                        if let Some(candidate) = result_to_candidate(row) {
                            if seen.insert(candidate.item_id.clone()) {
                                merged.push(candidate);
                            }
                        }
                    }
                }
                Err(reason) => {
                    last_unavailable = Some(reason.to_string());
                }
            }
        }
        if merged.is_empty() {
            if let Some(reason) = last_unavailable {
                return (Vec::new(), RtisStatus::Unavailable(reason));
            }
        }
        merged.truncate(cap);
        (merged, RtisStatus::Ok)
    }

    /// 取单条 POC 全文（L3 展开；poc get --id N --json）。
    pub fn get_poc(&self, poc_id: i64) -> Result<Value, ContextBrokerError> {
        let args = vec![
            "-I".to_string(),
            "rt.py".to_string(),
            "poc".to_string(),
            "get".to_string(),
            "--id".to_string(),
            poc_id.to_string(),
            "--json".to_string(),
        ];
        self.run_json(&args)
    }

    /// 跑子进程并解析 stdout JSON（超时/非零退出/坏 JSON 单次重试；两次
    /// 失败报 Source 错误，eri_adapter.py:115-134 口径）。
    fn run_json(&self, args: &[String]) -> Result<Value, ContextBrokerError> {
        let mut last_reason = String::new();
        for attempt in 1..=MAX_ATTEMPTS {
            match run_with_timeout(&self.kit_dir, args, self.timeout) {
                Ok(output) => {
                    if !output.status.success() {
                        last_reason = format!(
                            "rt.py exited with {} (stderr: {})",
                            output.status,
                            String::from_utf8_lossy(&output.stderr).trim()
                        );
                    } else {
                        match serde_json::from_slice::<Value>(&output.stdout) {
                            Ok(value) => return Ok(value),
                            Err(e) => {
                                last_reason = format!("rt.py stdout is not valid JSON: {e}");
                            }
                        }
                    }
                }
                Err(reason) => {
                    last_reason = reason;
                }
            }
            if attempt < MAX_ATTEMPTS {
                // 重试间隔 ≥1s（低并发与频控纪律，AGENTS.md §3）。
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        Err(ContextBrokerError::Source {
            source: RTIS_SOURCE_ID.to_string(),
            reason: format!("unavailable after {MAX_ATTEMPTS} attempts: {last_reason}"),
        })
    }
}

/// 带超时的子进程执行（std 无原生 timeout：spawn 后按 20ms 粒度轮询
/// wait，到点 kill；eri_adapter.py subprocess.run(timeout=) 的 std 落面）。
fn run_with_timeout(
    kit_dir: &std::path::Path,
    args: &[String],
    timeout: Duration,
) -> Result<std::process::Output, String> {
    let mut child = Command::new("python3")
        .args(args)
        .current_dir(kit_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn python3: {e}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return child
                    .wait_with_output()
                    .map_err(|e| format!("collect output: {e}"))
                    .map(|mut out| {
                        out.status = status;
                        out
                    });
            }
            Ok(None) => {
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("timeout after {:.1}s", timeout.as_secs_f32()));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(format!("wait failed: {e}")),
        }
    }
}

/// rt.py poc search 结果行到源级候选（引用口径按设计稿 §9.1 示例：
/// source=rtis、ref_id=kb:<id>、uri=rtis://poc/<id>）。
fn result_to_candidate(row: &Value) -> Option<Candidate> {
    let poc_id = row.get("id").and_then(Value::as_i64)?;
    let title = row
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let summary = row
        .get("description")
        .and_then(Value::as_str)
        .filter(|d| !d.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| title.clone());
    let verified = row.get("verified").and_then(Value::as_i64).unwrap_or(0);
    let _ = verified; // 通路字段：健康分输入随 P4-4 打分接线
    Some(Candidate {
        item_id: format!("kb:{poc_id}"),
        kind: ContextItemKind::Knowledge,
        layer: ContextLayer::L2,
        title,
        summary: summary.clone(),
        family: None,
        governance: None,
        quality_grade: None,
        reference: ContentRef {
            source: RTIS_SOURCE_ID.to_string(),
            ref_id: format!("kb:{poc_id}"),
            ref_version: None,
            content_hash: None,
            uri: Some(format!("rtis://poc/{poc_id}")),
        },
        est_tokens: crate::est_tokens(&summary),
    })
}

impl ContextSource for RtisSource {
    fn id(&self) -> &str {
        RTIS_SOURCE_ID
    }

    fn candidates(
        &self,
        q: &IntentQuery,
        limit: usize,
    ) -> Result<Vec<Candidate>, ContextBrokerError> {
        // 查询词取归一化全文（rt.py FTS5 + 失败降级 LIKE，TOOL-INVENTORY §8
        // 冒烟口径：「kingdee HTTP 检查」类任务直送）。不可用态映射空集
        // （设计稿 §9.2 第 4 条），状态经 search_with_status 披露。
        let (candidates, _status) = self.search_with_status(&q.text_normalized, limit);
        Ok(candidates)
    }

    fn load(&self, r: &ContentRef) -> Result<Content, ContextBrokerError> {
        if r.source != RTIS_SOURCE_ID {
            return Err(ContextBrokerError::Source {
                source: RTIS_SOURCE_ID.to_string(),
                reason: format!("ref source {:?} is not rtis", r.source),
            });
        }
        let poc_id = r
            .ref_id
            .strip_prefix("kb:")
            .and_then(|id| id.parse::<i64>().ok())
            .ok_or_else(|| ContextBrokerError::Source {
                source: RTIS_SOURCE_ID.to_string(),
                reason: format!("ref_id {:?} is not kb:<id>", r.ref_id),
            })?;
        let value = self.get_poc(poc_id)?;
        let body = serde_json::to_string(&value).map_err(|e| ContextBrokerError::Source {
            source: RTIS_SOURCE_ID.to_string(),
            reason: format!("serialize poc get payload: {e}"),
        })?;
        Ok(Content {
            reference: ContentRef {
                uri: Some(format!("rtis://poc/{poc_id}")),
                ..r.clone()
            },
            est_tokens: crate::est_tokens(&body),
            body,
        })
    }
}
