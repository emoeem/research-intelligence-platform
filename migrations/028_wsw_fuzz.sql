-- ============================================================================
-- Research Intelligence Platform : RIP3 WSW-4 Web Fuzzer / Intruder
--   增量迁移（WSW-4-TASK.md §0 D2/D5 + §三 S3；PLAN §4.4 :502-503 状态词表）：
--
--   纯新增两表 + 索引，零 ALTER、零既有表改动（025 文件头纪律同源；批内若
--   发现确需 ALTER 既有表，另立 029+，禁改历史迁移）。
--
--   wsw_fuzz_job : Fuzzer 任务行（PLAN :502 九态词表逐字 CHECK；预算六字段
--                  列承载 = FuzzBudget 持久化面——重启恢复按行读预算，
--                  D5「服务重启后恢复任务状态，不重复派发已发送 Case」；
--                  spec_json = 请求模板 + 位置 + payloads + mutators +
--                  processors + mode 全量声明式配置（8MiB 帽同 025
--                  request_json 口径；模板不立独立表——D2 裁决，无 UI
--                  登记面，spec JSON 即可复用模板，独立模板表留 UI 批）；
--                  estimated_requests = 启动前组合数估算（PLAN :254）；
--                  baseline_case_id 显式 baseline（缺省取首个 RESPONDED
--                  case——D6）；trace_id/task_id/evidence_id 三引用列沿
--                  025 引用列口径直传（Evidence 工作流接线留后续批）。
--   wsw_fuzz_case : 单 case 行（PLAN :503 七态词表逐字 CHECK，含
--                  SKIPPED_SCOPE/SKIPPED_BUDGET 两跳过态；payload 值 +
--                  payload_sha256 = 变体对账锚；request_outbound_sha256 =
--                  逐字节重发依据（025 口径同源）；exchange_id 引用
--                  wsw_exchange 行（source='fuzzer'，History 闭环 D7）；
--                  response 五列 + match_hits_json（关键字/正则命中投影）
--                  + diff_class/similarity（baseline 对比派生列）。
--
--   状态机（Rust 侧 update_fuzz_job_state/update_fuzz_case WHERE state=from
--   单语句 CAS 强制，非法迁移 0 行受影响即错——判据 6）：
--   Job 九态：CREATED/QUEUED/RUNNING/PAUSED/CANCELLING/CANCELLED/
--             COMPLETED/FAILED/BUDGET_EXHAUSTED（PLAN :502 逐字）
--   Case 七态：PENDING/SENT/RESPONDED/TIMEOUT/NETWORK_ERROR/
--             SKIPPED_SCOPE/SKIPPED_BUDGET（PLAN :503 逐字）
--
--   写权分治（W3-D7 延续）：fuzz job/case 行 = rp-wsw fuzz 子进程 Rust-only；
--   rp_server 只做 spawn + 只读读面（D8 四端点）。
--
--   幂等：CREATE TABLE IF NOT EXISTS + CREATE INDEX IF NOT EXISTS +
--   INSERT OR IGNORE（002/018/020/021/022/023/024/025 先例）。
--   只在开发库副本与测试内存库执行；migrations/platform*.db 生产文件不写。
-- ============================================================================

CREATE TABLE IF NOT EXISTS wsw_fuzz_job (
  job_id       TEXT PRIMARY KEY
                 CHECK (job_id LIKE 'wsw-fzj-%'),
  session_id   TEXT NOT NULL CHECK (session_id LIKE 'wsw-ses-%'),
  listener_id  TEXT NOT NULL CHECK (listener_id LIKE 'wsw-lis-%'),
  state        TEXT NOT NULL CHECK (state IN
               ('CREATED','QUEUED','RUNNING','PAUSED','CANCELLING','CANCELLED',
                'COMPLETED','FAILED','BUDGET_EXHAUSTED')),
  spec_json    TEXT NOT NULL CHECK (length(spec_json) <= 8*1024*1024),
  estimated_requests INTEGER NOT NULL CHECK (estimated_requests >= 0),
  -- FuzzBudget 六字段组（D5；Rust 强制面，行承载 = 重启恢复读数依据）。
  max_requests INTEGER NOT NULL CHECK (max_requests > 0),
  max_concurrency INTEGER NOT NULL CHECK (max_concurrency > 0),
  rate_window_ms INTEGER NOT NULL CHECK (rate_window_ms >= 0),
  rate_max_requests INTEGER NOT NULL CHECK (rate_max_requests >= 0),
  min_request_interval_ms INTEGER NOT NULL CHECK (min_request_interval_ms >= 0),
  max_duration_ms INTEGER NOT NULL CHECK (max_duration_ms >= 0),
  baseline_case_id TEXT
                 CHECK (baseline_case_id IS NULL OR baseline_case_id LIKE 'wsw-fzc-%'),
  trace_id     TEXT,
  task_id      TEXT,
  evidence_id  TEXT,
  started_at   TEXT,
  completed_at TEXT,
  duration_ms  INTEGER CHECK (duration_ms IS NULL OR duration_ms >= 0),
  error_code   TEXT,
  created_at   TEXT NOT NULL CHECK (created_at GLOB
               '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z')
);

CREATE TABLE IF NOT EXISTS wsw_fuzz_case (
  case_id      TEXT PRIMARY KEY
                 CHECK (case_id LIKE 'wsw-fzc-%'),
  job_id       TEXT NOT NULL REFERENCES wsw_fuzz_job(job_id),
  ordinal      INTEGER NOT NULL CHECK (ordinal >= 0),
  state        TEXT NOT NULL CHECK (state IN
               ('PENDING','SENT','RESPONDED','TIMEOUT','NETWORK_ERROR',
                'SKIPPED_SCOPE','SKIPPED_BUDGET')),
  -- payload 变体：该 case 各位置注入值列表 JSON + 组合值 sha256（对账锚）。
  payload_json TEXT NOT NULL CHECK (length(payload_json) <= 65536),
  payload_sha256 TEXT NOT NULL
                 CHECK (length(payload_sha256) = 64 AND payload_sha256 GLOB '[0-9a-f]*'),
  request_outbound_sha256 TEXT
                 CHECK (request_outbound_sha256 IS NULL OR
                        (length(request_outbound_sha256) = 64 AND request_outbound_sha256 GLOB '[0-9a-f]*')),
  exchange_id  TEXT CHECK (exchange_id IS NULL OR exchange_id LIKE 'wsw-%'),
  response_status INTEGER,
  response_len    INTEGER CHECK (response_len IS NULL OR response_len >= 0),
  response_sha256 TEXT
                 CHECK (response_sha256 IS NULL OR
                        (length(response_sha256) = 64 AND response_sha256 GLOB '[0-9a-f]*')),
  response_mime   TEXT,
  duration_ms  INTEGER CHECK (duration_ms IS NULL OR duration_ms >= 0),
  ttfb_ms      INTEGER CHECK (ttfb_ms IS NULL OR ttfb_ms >= 0),
  match_hits_json TEXT,
  -- baseline 对比派生列（D6）：diff_class 词表 = baseline/baseline_diff/
  -- duplicate/unique/error（analyzer 落值）；similarity 0.0..1.0。
  diff_class   TEXT,
  similarity   REAL CHECK (similarity IS NULL OR (similarity >= 0.0 AND similarity <= 1.0)),
  error_code   TEXT,
  started_at   TEXT,
  completed_at TEXT,
  created_at   TEXT NOT NULL CHECK (created_at GLOB
               '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z'),
  UNIQUE(job_id, ordinal)
);

CREATE INDEX IF NOT EXISTS ix_wsw_fuzz_case_job
  ON wsw_fuzz_case(job_id, ordinal);
CREATE INDEX IF NOT EXISTS ix_wsw_fuzz_case_status
  ON wsw_fuzz_case(job_id, state);
CREATE INDEX IF NOT EXISTS ix_wsw_fuzz_job_session
  ON wsw_fuzz_job(session_id, created_at DESC);
CREATE INDEX IF NOT EXISTS ix_wsw_fuzz_job_state
  ON wsw_fuzz_job(state, created_at DESC);

-- ---------------------------------------------------------------------------
-- schema_version：WSW-4 Fuzzer 持久化登记（version=28；链头 027→028 单文件
-- 单版本推进，D2；编号冻结规则顺延——migration_dev_copy_p7_3.rs:76 注释原文）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (28, 'V1 additive WSW-4 fuzz tables (wsw_fuzz_job, wsw_fuzz_case)',
   'WSW-4（docs/rip3/wsw/WSW-4-TASK.md §0 D2/D5 + §三 S3）：纯新增两表零 ALTER。wsw_fuzz_job 九态 CHECK（PLAN :502 词表逐字：CREATED/QUEUED/RUNNING/PAUSED/CANCELLING/CANCELLED/COMPLETED/FAILED/BUDGET_EXHAUSTED）+ FuzzBudget 六字段列承载（max_requests/max_concurrency/rate_window_ms/rate_max_requests/min_request_interval_ms/max_duration_ms，Rust 强制面）+ spec_json 8MiB 帽（模板+位置+payloads+mutators+processors+mode 声明式全量，模板不立独立表）+ estimated_requests 组合数估算 + baseline_case_id/trace_id/task_id/evidence_id 引用列。wsw_fuzz_case 七态 CHECK（PLAN :503 词表逐字：PENDING/SENT/RESPONDED/TIMEOUT/NETWORK_ERROR/SKIPPED_SCOPE/SKIPPED_BUDGET）+ payload_json/payload_sha256 变体对账锚 + request_outbound_sha256 逐字节重发依据（025 口径）+ exchange_id 引用 wsw_exchange(source=fuzzer) History 闭环 + response 五列 + match_hits_json + diff_class/similarity baseline 派生列，UNIQUE(job_id,ordinal)。发送单点 = sender::send_request（D1 红线）；状态迁移 Rust CAS 单语句强制；写权 = rp-wsw fuzz 子进程 Rust-only。只在开发库副本与测试内存库执行，platform*.db 生产文件不写',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '028_wsw_fuzz.sql');

-- user_version 双写（DEC-002 条件，001/018/022/023/024/025 先例）
PRAGMA user_version = 28;
