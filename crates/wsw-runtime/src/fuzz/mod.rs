//! `wsw_runtime::fuzz`：Web Fuzzer / Intruder 引擎（WSW-4；PLAN §4.4 :465
//! Fuzz Engine 模块面 = Payload / Mutator / Processor / Job Scheduler /
//! Result Analyzer）。
//!
//! - [`spec`]：§…§ 标记词法 + 七类选择器 + 三模式估算（D3/D10）；
//! - [`mutator`]：生成器/编码链/变换/Processor（D4）；
//! - [`analyzer`]：分类/匹配/去重/baseline 对比（D6）；
//! - [`persist`]：persist_fuzz_case（exchange source='fuzzer' 闭环，D7）；
//! - 本模块：JobRunner 状态机（Job 九态 + Case 七态，PLAN :502-503 词表）
//!   + `Semaphore` 并发 + FuzzBudget 四闸（D5）+ watch 取消/暂停 +
//!     case 行 status 权威续派（判据 8 防重派）。
//!
//! 发送引擎单点（D1 红线）：所有 fuzz case 发送唯一路径 =
//! [`crate::sender::send_request`]（sender.rs:720）；本模块零 socket 零
//! serialize 旁路；scope 闸权威面在 send_request 内（hop 0 check + 逐跳
//! 重校验），本模块的 scope 预检只决定 SKIPPED_SCOPE 分流（不发送不落
//! exchange——D7），不替代闸。
//!
//! 并发模型：派发循环（单任务）逐 case：semaphore acquire → 四闸判定
//! （总数/速率/间隔/时长——D5 挂点 acquire 后、send_request 前）→ case 行
//! PENDING→SENT（CAS）→ tokio::spawn 发送任务（permit 随行）。任务完成
//! 落 case 终态 + exchange 行 + analyzer 派生列。SIGTERM/SIGINT →
//! cancel watch → CANCELLING（在途 case 等终态落行）→ CANCELLED；
//! 预算耗尽 → BUDGET_EXHAUSTED（已发送 case 终态齐全，PLAN :269/:270）；
//! 暂停 = PAUSED 状态位；恢复按 case 行 status 权威续派（PENDING 未派、
//! SENT/RESPONDED 不重派——D5/判据 8）。

pub mod analyzer;
pub mod mutator;
pub mod persist;
pub mod spec;

use crate::persist::{FuzzCasePatch, FuzzCaseRow, PersistStore};
use crate::scope::{self, ScopeRules, ScopeTarget};
use crate::sender::{SendError, SendOutcome, SendPolicy};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{watch, Semaphore};

/// JobRunner 错误面。
#[derive(Debug)]
pub enum FuzzError {
    /// store 面。
    Db(database::DbError),
    /// spec 预检（fail-closed 词表直通）。
    Spec(spec::SpecError),
    /// job 行不存在。
    JobNotFound,
    /// job 已终态（幂等面：调用方回显退出）。
    JobTerminal(String),
    /// job 状态不允许进入运行（非法入口态）。
    JobState(String),
}

impl std::fmt::Display for FuzzError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FuzzError::Db(e) => write!(f, "fuzz db: {e}"),
            FuzzError::Spec(e) => write!(f, "{e}"),
            FuzzError::JobNotFound => write!(f, "fuzz job not found"),
            FuzzError::JobTerminal(s) => write!(f, "fuzz job already terminal: {s}"),
            FuzzError::JobState(s) => write!(f, "fuzz job not runnable from state: {s}"),
        }
    }
}

impl From<database::DbError> for FuzzError {
    fn from(e: database::DbError) -> Self {
        FuzzError::Db(e)
    }
}

impl From<spec::SpecError> for FuzzError {
    fn from(e: spec::SpecError) -> Self {
        FuzzError::Spec(e)
    }
}

/// job 终态集合（九态中四态；非法迁移出边 = 无——persist.rs 迁移表同源）。
pub fn is_terminal_job_state(state: &str) -> bool {
    matches!(state, "CANCELLED" | "COMPLETED" | "FAILED" | "BUDGET_EXHAUSTED")
}

/// 速率滑动窗口计数器（CallerRateTracker 同型实现——std AtomicUsize +
/// Instant + Mutex，agent_loop.rs:281-312 实现形态移植；不跨域 import
/// agent-engine 符号——D5/红线 6 词表纪律）。
struct RateWindow {
    window_ms: u64,
    window_start: Mutex<Instant>,
    count: std::sync::atomic::AtomicUsize,
}

impl RateWindow {
    fn new(window_ms: u64) -> Self {
        Self {
            window_ms,
            window_start: Mutex::new(Instant::now()),
            count: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// 当前窗口内计数（窗口过期即复位清零——同型语义）。
    fn current(&self) -> usize {
        if self.window_ms == 0 {
            return 0;
        }
        let mut start = self.window_start.lock().expect("rate window mutex");
        if start.elapsed().as_millis() as u64 >= self.window_ms {
            *start = Instant::now();
            self.count
                .store(0, std::sync::atomic::Ordering::SeqCst);
            0
        } else {
            self.count.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    fn bump(&self) {
        self.count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// JobRunner 运行参数。
pub struct FuzzJobOptions {
    pub session_id: String,
    /// scope 规则（wsw_session 行 default deny 同 send 口径——调用方装配）。
    pub rules: ScopeRules,
    pub policy: SendPolicy,
    pub artifacts_root: PathBuf,
    pub extra_ca: Vec<PathBuf>,
    /// SIGTERM/SIGINT 取消面（rp-wsw 信号桥）。
    pub cancel: watch::Receiver<bool>,
    /// 暂停状态位（PAUSED；true = 暂停）。
    pub pause: watch::Receiver<bool>,
}

/// 运行汇总（CLI stdout JSON 投影源）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct FuzzJobSummary {
    pub job_id: String,
    pub state: &'static str,
    pub estimated_requests: u64,
    pub dispatched: u64,
    pub responded: u64,
    pub timed_out: u64,
    pub network_error: u64,
    pub skipped_scope: u64,
    pub skipped_budget: u64,
    pub duration_ms: i64,
}

/// baseline 共享单元（首个 RESPONDED case 缺省；显式 baseline_case_id 由
/// 入口预载）——(case_id, body_sha256, body_bytes)。
type BaselineCell = Arc<Mutex<Option<(String, String, Vec<u8>)>>>;

/// sha 去重表（duplicate 判定：先前见过的 body sha——PLAN :261）。
type ShaMap = Arc<Mutex<HashMap<String, ()>>>;

/// JobRunner 主入口（QUEUED 起跑 / RUNNING·PAUSED 续派；终态拒——幂等面
/// 由 CLI 处理）。返回汇总（job 行终态已落）。
pub async fn run_job(
    store: &PersistStore,
    job_id: &str,
    opts: FuzzJobOptions,
) -> Result<FuzzJobSummary, FuzzError> {
    let row = store
        .get_fuzz_job(job_id)?
        .ok_or(FuzzError::JobNotFound)?;
    let from_state = row.state.clone();
    if is_terminal_job_state(&from_state) {
        return Err(FuzzError::JobTerminal(from_state));
    }
    if !matches!(from_state.as_str(), "QUEUED" | "RUNNING" | "PAUSED") {
        return Err(FuzzError::JobState(from_state));
    }
    let prepared = spec::prepare_spec(&row.spec_json)?;
    let budget = prepared.spec.budget.clone();
    let mode = prepared.spec.mode;
    let estimated = prepared.estimated_requests;

    // resume 对账：case 行 status 权威（PENDING 未派 / SENT·终态不重派——D5）。
    let existing_rows = store.list_cases_by_job(job_id)?;
    let mut existing_by_ordinal: HashMap<i64, FuzzCaseRow> = HashMap::new();
    let mut initial_dispatched: u64 = 0;
    for r in &existing_rows {
        existing_by_ordinal.insert(r.ordinal, r.clone());
        if r.state != "PENDING" {
            initial_dispatched += 1;
        }
    }

    // baseline 预载：显式 baseline_case_id → case 行 response_sha256 →
    // CAS 回读（uri = wsw/<sha[0:2]>/<sha> 派生——cas.rs:205 格式）。
    let baseline: BaselineCell = Arc::new(Mutex::new(None));
    if let Some(bid) = &row.baseline_case_id {
        if let Some(brow) = existing_rows.iter().find(|r| &r.case_id == bid) {
            if let Some(sha) = &brow.response_sha256 {
                let uri = format!("wsw/{}/{}", &sha[..2], sha);
                if let Ok(body) = wsw_message::cas::read_body(&opts.artifacts_root, &uri) {
                    *baseline.lock().expect("baseline mutex") =
                        Some((brow.case_id.clone(), sha.clone(), body));
                }
            }
        }
    }
    let sha_map: ShaMap = Arc::new(Mutex::new(HashMap::new()));
    if let Some((_, bsha, _)) = baseline.lock().expect("baseline mutex").clone() {
        sha_map.lock().expect("sha mutex").insert(bsha, ());
    }

    // 入口态推进：QUEUED→RUNNING（started_at）/ PAUSED→RUNNING；
    // RUNNING（kill 残留态）续跑不迁移（无 RUNNING→RUNNING 边）。
    let now = crate::persist::now_iso();
    let mut job_state = from_state.clone();
    match from_state.as_str() {
        "QUEUED" => {
            store.advance_fuzz_job(
                job_id,
                "QUEUED",
                &crate::persist::FuzzJobPatch {
                    state: "RUNNING",
                    started_at: Some(now),
                    ..Default::default()
                },
            )?;
            job_state = "RUNNING".to_string();
        }
        "PAUSED" => {
            store.advance_fuzz_job(
                job_id,
                "PAUSED",
                &crate::persist::FuzzJobPatch {
                    state: "RUNNING",
                    ..Default::default()
                },
            )?;
            job_state = "RUNNING".to_string();
        }
        _ => {}
    }

    let started = Instant::now();
    let sem = Arc::new(Semaphore::new(budget.max_concurrency));
    let combo = spec::CombinationIter::new(&prepared.effective_payloads, mode)?;
    let total = combo.total();
    let rate = RateWindow::new(budget.rate_window_ms);
    let mut last_send: Option<Instant> = None;
    let mut dispatched = initial_dispatched;
    let mut counters = FuzzJobSummary {
        job_id: job_id.to_string(),
        state: "RUNNING",
        estimated_requests: estimated,
        dispatched: initial_dispatched,
        responded: 0,
        timed_out: 0,
        network_error: 0,
        skipped_scope: 0,
        skipped_budget: 0,
        duration_ms: 0,
    };
    // 计数修正：resume 时按既有行重算各终态计数（汇总口径 = 全 job 累计）。
    for r in &existing_rows {
        match r.state.as_str() {
            "RESPONDED" => counters.responded += 1,
            "TIMEOUT" => counters.timed_out += 1,
            "NETWORK_ERROR" => counters.network_error += 1,
            "SKIPPED_SCOPE" => counters.skipped_scope += 1,
            "SKIPPED_BUDGET" => counters.skipped_budget += 1,
            _ => {}
        }
    }

    let mut handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();
    let mut cancelling = false;
    let mut exhausted = false;
    let mut paused_by_us = false;

    let mut ord: u64 = 0;
    while ord < total {
        // 取消观察（派发循环头；在途任务等待终态——CANCELLING 语义）。
        if *opts.cancel.borrow() {
            cancelling = true;
            break;
        }
        // 暂停观察（PAUSED 状态位 + 恢复续派）。
        if *opts.pause.borrow() {
            if !paused_by_us && job_state == "RUNNING" {
                store.advance_fuzz_job(
                    job_id,
                    "RUNNING",
                    &crate::persist::FuzzJobPatch {
                        state: "PAUSED",
                        ..Default::default()
                    },
                )?;
                job_state = "PAUSED".to_string();
                paused_by_us = true;
            }
            // 暂停等待循环（取消优先）。
            while *opts.pause.borrow() {
                if *opts.cancel.borrow() {
                    cancelling = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            if cancelling {
                break;
            }
            if paused_by_us && job_state == "PAUSED" {
                store.advance_fuzz_job(
                    job_id,
                    "PAUSED",
                    &crate::persist::FuzzJobPatch {
                        state: "RUNNING",
                        ..Default::default()
                    },
                )?;
                job_state = "RUNNING".to_string();
                paused_by_us = false;
            }
        }
        // resume 权威对账：既有非 PENDING 行（SENT/终态）不重派——D5/判据 8。
        if let Some(row) = existing_by_ordinal.get(&(ord as i64)) {
            if row.state != "PENDING" {
                ord += 1;
                continue;
            }
        }
        let assignment = combo.assignment(ord);
        let permit = match sem.clone().acquire_owned().await {
            Ok(p) => p,
            Err(_) => break,
        };
        // ---- 四闸（D5 挂点：semaphore acquire 后、send_request 前）----
        // 1) 总数闸：sent_count（含 resume 已派）≥ max_requests →
        //    BUDGET_EXHAUSTED（剩余组合不再枚举；已发送 case 终态齐全由
        //    在途任务 drain 保证）。
        if dispatched >= budget.max_requests {
            drop(permit);
            exhausted = true;
            break;
        }
        // 2) 时长闸。
        if budget.max_duration_ms > 0
            && started.elapsed().as_millis() as u64 >= budget.max_duration_ms
        {
            drop(permit);
            exhausted = true;
            break;
        }
        // 3) 速率闸：窗口超帽 → 该 case SKIPPED_BUDGET（不发送；判定后
        //    ord 前进，不重试同一 case）。
        if budget.rate_max_requests > 0
            && rate.current() >= budget.rate_max_requests as usize
        {
            drop(permit);
            let existing_pending = existing_by_ordinal
                .get(&(ord as i64))
                .map(|r| r.state == "PENDING")
                .unwrap_or(false);
            match ensure_case_row(
                store,
                job_id,
                ord,
                &existing_by_ordinal,
                &assignment,
                "SKIPPED_BUDGET",
                Some("fuzz_budget_rate"),
            ) {
                Ok(case_id) => {
                    if existing_pending {
                        let _ = store.advance_fuzz_case(
                            &case_id,
                            "PENDING",
                            &FuzzCasePatch {
                                state: "SKIPPED_BUDGET",
                                error_code: Some("fuzz_budget_rate".into()),
                                started_at: Some(crate::persist::now_iso()),
                                completed_at: Some(crate::persist::now_iso()),
                                ..Default::default()
                            },
                        );
                    }
                    counters.skipped_budget += 1;
                }
                Err(e) => return Err(FuzzError::Db(e)),
            }
            ord += 1;
            continue;
        }
        // 4) 间隔闸：min_request_interval 生效 = 相邻 send 间隔保证
        //    （节流等待，非跳过——判据 5 相邻 started_at 间隔实测）。
        if budget.min_request_interval_ms > 0 {
            if let Some(last) = last_send {
                let min = Duration::from_millis(budget.min_request_interval_ms);
                let elapsed = last.elapsed();
                if elapsed < min {
                    tokio::time::sleep(min - elapsed).await;
                }
            }
        }

        // ---- case 构建 + scope 预检分流 ----
        let assignment_refs: Vec<Option<&str>> =
            assignment.iter().map(|o| o.as_deref()).collect();
        let request = match spec::build_case_request(
            &prepared.spec,
            &prepared.marker_spans,
            &assignment_refs,
        ) {
            Ok(r) => r,
            Err(_) => {
                // 构建面失败（processor 守卫/json 重序列化）→
                // NETWORK_ERROR + serialize 家族词形，不发送。
                drop(permit);
                let existing_pending = existing_by_ordinal
                    .get(&(ord as i64))
                    .map(|r| r.state == "PENDING")
                    .unwrap_or(false);
                let case_id = ensure_case_row(
                    store,
                    job_id,
                    ord,
                    &existing_by_ordinal,
                    &assignment,
                    "NETWORK_ERROR",
                    Some("serialize_failed"),
                )?;
                if existing_pending {
                    let now = crate::persist::now_iso();
                    store.advance_fuzz_case(
                        &case_id,
                        "PENDING",
                        &FuzzCasePatch {
                            state: "NETWORK_ERROR",
                            error_code: Some("serialize_failed".into()),
                            started_at: Some(now.clone()),
                            completed_at: Some(now),
                            ..Default::default()
                        },
                    )?;
                }
                counters.network_error += 1;
                ord += 1;
                continue;
            }
        };
        // scope 预检（SKIPPED_SCOPE：不发送不落 exchange——D7；error_code =
        // scope.rs:234 code() 词形，PLAN :528 越界可见）。
        if let Some(t) = persist::scope_precheck_target(&request) {
            let st = ScopeTarget {
                scheme: &t.scheme,
                host: &t.host,
                port: t.port,
                path: &t.path,
            };
            if let scope::ScopeDecision::Blocked { reason } = scope::check(&opts.rules, &st) {
                drop(permit);
                let existing_pending = existing_by_ordinal
                    .get(&(ord as i64))
                    .map(|r| r.state == "PENDING")
                    .unwrap_or(false);
                let case_id = ensure_case_row(
                    store,
                    job_id,
                    ord,
                    &existing_by_ordinal,
                    &assignment,
                    "SKIPPED_SCOPE",
                    Some(reason.code()),
                )?;
                if existing_pending {
                    store.advance_fuzz_case(
                        &case_id,
                        "PENDING",
                        &FuzzCasePatch {
                            state: "SKIPPED_SCOPE",
                            error_code: Some(reason.code().to_string()),
                            started_at: Some(crate::persist::now_iso()),
                            completed_at: Some(crate::persist::now_iso()),
                            ..Default::default()
                        },
                    )?;
                }
                counters.skipped_scope += 1;
                ord += 1;
                continue;
            }
        }

        // ---- PENDING 行登记 + SENT 推进（CAS）----
        let case_id = ensure_case_row(
            store,
            job_id,
            ord,
            &existing_by_ordinal,
            &assignment,
            "PENDING",
            None,
        )?;
        let now = crate::persist::now_iso();
        // resume 场景：既有 PENDING 行（此前插入未派）或新插行——同 CAS。
        store.advance_fuzz_case(
            &case_id,
            "PENDING",
            &FuzzCasePatch {
                state: "SENT",
                started_at: Some(now),
                ..Default::default()
            },
        )?;
        dispatched += 1;
        counters.dispatched = dispatched;
        rate.bump();
        last_send = Some(Instant::now());

        // ---- 发送任务（发送引擎单点：sender::send_request，D1）----
        let store2 = store.clone();
        let session_id = opts.session_id.clone();
        let rules = opts.rules.clone();
        let policy = opts.policy.clone();
        let extra_ca = opts.extra_ca.clone();
        let cancel = opts.cancel.clone();
        let artifacts_root = opts.artifacts_root.clone();
        let baseline2 = Arc::clone(&baseline);
        let sha_map2 = Arc::clone(&sha_map);
        let analyze = prepared.spec.analyze.clone();
        let regexes = compiled_regexes(&analyze.regexes)?;
        let case_id2 = case_id.clone();
        let job_id2 = job_id.to_string();
        handles.push(tokio::spawn(async move {
            let _permit = permit;
            let outcome =
                crate::sender::send_request(&rules, &request, &policy, &extra_ca, &cancel)
                    .await;
            finalize_case(
                &store2,
                &artifacts_root,
                &session_id,
                &job_id2,
                &case_id2,
                &request,
                outcome,
                &analyze,
                &regexes,
                &baseline2,
                &sha_map2,
            );
        }));
        ord += 1;
    }

    // ---- 取消态推进（CANCELLING：在途 case 等终态落行 → CANCELLED）----
    // 派发循环内观察到的取消：先落 CANCELLING 再 drain。
    if cancelling {
        let from = job_state.clone();
        if from == "RUNNING" || from == "PAUSED" {
            let _ = store.advance_fuzz_job(
                job_id,
                &from,
                &crate::persist::FuzzJobPatch {
                    state: "CANCELLING",
                    ..Default::default()
                },
            );
            job_state = "CANCELLING".to_string();
        }
    }
    // 在途任务 drain（终态落行保证——PLAN :269/:556）。
    for h in handles {
        let _ = h.await;
    }
    // 信号后到复查（drain 期收到 SIGTERM：在途 case 已被 cancel watch 打断，
    // job 终态仍收敛 CANCELLED——e2e 实测面：dispatched 全量 + case 终态
    // NETWORK_ERROR/cancelled + job CANCELLED）。
    if !cancelling && *opts.cancel.borrow() {
        let _ = store.advance_fuzz_job(
            job_id,
            &job_state,
            &crate::persist::FuzzJobPatch {
                state: "CANCELLING",
                ..Default::default()
            },
        );
        job_state = "CANCELLING".to_string();
    }
    // ---- 终态推进 ----
    let final_state: &'static str = if job_state == "CANCELLING" {
        let _ = store.advance_fuzz_job(
            job_id,
            "CANCELLING",
            &crate::persist::FuzzJobPatch {
                state: "CANCELLED",
                completed_at: Some(crate::persist::now_iso()),
                duration_ms: Some(started.elapsed().as_millis() as i64),
                ..Default::default()
            },
        );
        "CANCELLED"
    } else if exhausted {
        let _ = store.advance_fuzz_job(
            job_id,
            &job_state,
            &crate::persist::FuzzJobPatch {
                state: "BUDGET_EXHAUSTED",
                completed_at: Some(crate::persist::now_iso()),
                duration_ms: Some(started.elapsed().as_millis() as i64),
                ..Default::default()
            },
        );
        "BUDGET_EXHAUSTED"
    } else {
        let _ = store.advance_fuzz_job(
            job_id,
            &job_state,
            &crate::persist::FuzzJobPatch {
                state: "COMPLETED",
                completed_at: Some(crate::persist::now_iso()),
                duration_ms: Some(started.elapsed().as_millis() as i64),
                ..Default::default()
            },
        );
        "COMPLETED"
    };
    counters.state = final_state;
    counters.duration_ms = started.elapsed().as_millis() as i64;
    // 汇总终态计数从 DB 重算（finalize 任务只落库不回传内存计数；汇总
    // 口径 = 全 job 累计，含 resume 既有行）。
    let rows = store.list_cases_by_job(job_id)?;
    // dispatched 口径 = 实际发送面（SENT/RESPONDED/TIMEOUT/NETWORK_ERROR）；
    // SKIPPED_* 不发送不计入（总数闸同口径——判据 5/7）。
    counters.dispatched = rows
        .iter()
        .filter(|r| {
            matches!(r.state.as_str(), "SENT" | "RESPONDED" | "TIMEOUT" | "NETWORK_ERROR")
        })
        .count() as u64;
    counters.responded = rows.iter().filter(|r| r.state == "RESPONDED").count() as u64;
    counters.timed_out = rows.iter().filter(|r| r.state == "TIMEOUT").count() as u64;
    counters.network_error = rows.iter().filter(|r| r.state == "NETWORK_ERROR").count() as u64;
    counters.skipped_scope = rows.iter().filter(|r| r.state == "SKIPPED_SCOPE").count() as u64;
    counters.skipped_budget = rows.iter().filter(|r| r.state == "SKIPPED_BUDGET").count() as u64;
    Ok(counters)
}

/// 编译 spec 正则面（失败 = SpecError::MutatorInvalid 词形族）。
fn compiled_regexes(patterns: &[String]) -> Result<Vec<regex::Regex>, FuzzError> {
    patterns
        .iter()
        .map(|p| {
            regex::Regex::new(p).map_err(|e| {
                FuzzError::Spec(spec::SpecError::MutatorInvalid(format!("{p}: {e}")))
            })
        })
        .collect()
}

/// ordinal → case 行（既有行复用 case_id / 新插行；resume 权威对账面）。
fn ensure_case_row(
    store: &PersistStore,
    job_id: &str,
    ordinal: u64,
    existing_by_ordinal: &HashMap<i64, FuzzCaseRow>,
    assignment: &[Option<String>],
    initial_state: &str,
    skip_error_code: Option<&str>,
) -> Result<String, database::DbError> {
    if let Some(row) = existing_by_ordinal.get(&(ordinal as i64)) {
        return Ok(row.case_id.clone());
    }
    let payload_json = spec::payload_vector_json(assignment);
    let payload_sha = analyzer::payload_sha256(&payload_json);
    let now = crate::persist::now_iso();
    let row = FuzzCaseRow {
        case_id: wsw_message::ids::FuzzCaseId::generate().into_inner(),
        job_id: job_id.to_string(),
        ordinal: ordinal as i64,
        state: initial_state.to_string(),
        payload_json,
        payload_sha256: payload_sha,
        // 直插终态（SKIPPED_*）行：error_code/时间戳同落（可见性面）。
        error_code: skip_error_code.map(String::from),
        started_at: skip_error_code.map(|_| now.clone()),
        completed_at: skip_error_code.map(|_| now.clone()),
        created_at: now,
        ..Default::default()
    };
    store.insert_fuzz_case(&row)?;
    Ok(row.case_id)
}

/// 发送任务收尾：终态落行 + exchange(source='fuzzer') + analyzer 派生列
/// （同步面：无 await——baseline/sha 两锁无跨 await 持有）。
#[allow(clippy::too_many_arguments)]
fn finalize_case(
    store: &PersistStore,
    artifacts_root: &std::path::Path,
    session_id: &str,
    job_id: &str,
    case_id: &str,
    request: &wsw_message::serialize::OutboundRequest,
    outcome: Result<SendOutcome, SendError>,
    analyze: &spec::AnalyzeConfig,
    regexes: &[regex::Regex],
    baseline: &BaselineCell,
    sha_map: &ShaMap,
) {
    match outcome {
        Ok(o) => {
            // 1) exchange 行（发送成功面；CAS 三面复用——D7）。
            let refs = persist::persist_fuzz_case_exchange(
                store, artifacts_root, session_id, request, &o,
            )
            .ok();
            let exchange_id = refs.as_ref().map(|r| r.exchange_id.clone());
            let outbound_sha = refs.map(|r| r.outbound_sha256);
            // 2) 分类 + 命中（429/5xx 限流面——判据 9）。
            let error_code = analyzer::classify_status(o.response_status.unwrap_or(0))
                .map(String::from);
            let response_sha = (!o.response_bytes.is_empty())
                .then(|| wsw_message::cas::sha256_hex(&o.response_bytes));
            let hits = analyzer::match_hits(&o.response_bytes, &analyze.keywords, regexes);
            let hits_json =
                (!hits.is_empty()).then(|| serde_json::to_string(&hits).unwrap_or_else(|_| "[]".into()));
            // 3) baseline 对比 + 去重（body sha 分组——PLAN :261/:303）。
            let mut diff_class: Option<String> = None;
            let mut similarity: Option<f64> = None;
            if let Some(sha) = &response_sha {
                let mut cell = baseline.lock().expect("baseline mutex");
                let mut map = sha_map.lock().expect("sha mutex");
                match cell.as_ref() {
                    None => {
                        // 首个 RESPONDED case = baseline（D6 缺省面）；
                        // baseline_case_id 回写 job 行（独立 UPDATE 面）。
                        diff_class = Some(analyzer::DIFF_CLASS_BASELINE.to_string());
                        similarity = Some(1.0);
                        let _ = set_job_baseline(store, job_id, case_id);
                        map.insert(sha.clone(), ());
                        *cell = Some((case_id.to_string(), sha.clone(), o.response_bytes.clone()));
                    }
                    Some((_, bsha, bbody)) => {
                        similarity = Some(analyzer::similarity(bbody, &o.response_bytes));
                        if sha == bsha || map.contains_key(sha) {
                            diff_class = Some(analyzer::DIFF_CLASS_DUPLICATE.to_string());
                        } else {
                            diff_class = Some(analyzer::DIFF_CLASS_UNIQUE.to_string());
                        }
                        map.entry(sha.clone()).or_insert(());
                    }
                }
            }
            // 4) case 终态（SENT→RESPONDED CAS）。
            let now = crate::persist::now_iso();
            let _ = store.advance_fuzz_case(
                case_id,
                "SENT",
                &FuzzCasePatch {
                    state: "RESPONDED",
                    request_outbound_sha256: outbound_sha,
                    exchange_id,
                    response_status: o.response_status.map(|s| s as i64),
                    response_len: Some(o.response_bytes.len() as i64),
                    response_sha256: response_sha,
                    response_mime: o
                        .parsed
                        .as_ref()
                        .and_then(|p| p.message.as_ref())
                        .and_then(|m| {
                            m.headers
                                .iter()
                                .find(|h| h.name.eq_ignore_ascii_case("content-type"))
                                .map(|h| h.value.clone())
                        }),
                    duration_ms: Some(o.duration_ms),
                    ttfb_ms: o.ttfb_ms,
                    match_hits_json: hits_json,
                    diff_class,
                    similarity,
                    error_code,
                    completed_at: Some(now),
                    ..Default::default()
                },
            );
        }
        Err(e) => {
            let (state, code) = analyzer::classify_send_error(&e);
            let now = crate::persist::now_iso();
            let _ = store.advance_fuzz_case(
                case_id,
                "SENT",
                &FuzzCasePatch {
                    state,
                    error_code: Some(code.to_string()),
                    completed_at: Some(now),
                    ..Default::default()
                },
            );
        }
    }
}

/// baseline_case_id 回写（独立 UPDATE；非状态迁移面）。
fn set_job_baseline(store: &PersistStore, job_id: &str, case_id: &str) -> database::DbResult<()> {
    store.with_conn(|conn| {
        conn.execute(
            "UPDATE wsw_fuzz_job SET baseline_case_id=?2 WHERE job_id=?1",
            [
                &job_id as &dyn database::rusqlite::ToSql,
                &case_id as &dyn database::rusqlite::ToSql,
            ],
        )?;
        Ok::<(), database::rusqlite::Error>(())
    })?;
    Ok(())
}
