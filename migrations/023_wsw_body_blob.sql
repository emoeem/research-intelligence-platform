-- ============================================================================
-- Research Intelligence Platform : RIP3 WSW-1 HTTP Message Foundation
--   增量迁移（WSW-1-TASK.md §二/§三 S6；DESIGN §3.2/§3.3）：
--
--   wsw_body_blob：被动捕获面（History）HTTP 正文 CAS 去重表。
--   依据：既有 artifact 表 run_id NOT NULL REFERENCES tool_run
--   （migrations/005_tool_contract.sql:78-90），被动捕获的 Exchange 没有
--   tool_run，直接写 artifact 表破坏外键语义 —— 故新增独立 CAS 登记表
--   （DESIGN §3.3 裁决双层第 1 层）。
--
--   列语义：
--   - sha256：正文 sha256，小写 hex 64 字符（migrations/005_tool_contract.sql
--     :86 口径），主键 = CAS 寻址；正文文件落 <RP_ARTIFACTS_DIR>/wsw/<前2>/
--     <sha256>，uri 记相对路径 wsw/<前2>/<sha256>（根在读取时按
--     RP_ARTIFACTS_DIR 解析，默认 rip-data/artifacts）。
--   - 同 sha256 去重复用（PLAN §8.2 :576「内容寻址、哈希校验和去重」），
--     INSERT OR IGNORE 幂等登记（crates/wsw-message/src/store.rs）。
--   - 超帽截断：登记 sha256 = 原始字节 sha256（tool-engine envelope.rs
--     :281-321 TruncationOutcome「超帽降级 + 原始字节 sha256」唯一口径，
--     红线 7；截断标注走 capture_fidelity=truncated，不另造截断字段）。
--   - first_seen_exchange_id：首见 Exchange 引用（exchange_id 形状，
--     wsw_exchange 表 WSW-3 History 批次落位后承担引用计数主源；
--     DESIGN §3.4 GC + 用户收藏 pin）。
--
--   GC：引用计数对账由 store::delete_body_blobs_unreferenced 执行
--   （零引用超期清理执行位归 WSW-3 retention，DESIGN §4.8）。
--
--   幂等：CREATE TABLE IF NOT EXISTS + CREATE INDEX IF NOT EXISTS +
--   INSERT OR IGNORE（002/018/020/021/022 先例）。
--   只在开发库副本与测试内存库执行；migrations/platform*.db 生产文件不写。
-- ============================================================================

CREATE TABLE IF NOT EXISTS wsw_body_blob (
  sha256                 TEXT PRIMARY KEY
                           CHECK (length(sha256) = 64 AND sha256 NOT GLOB '*[^0-9a-f]*'), -- 小写 hex CAS 主键
  uri                    TEXT NOT NULL
                           CHECK (uri = 'wsw/' || substr(sha256, 1, 2) || '/' || sha256), -- 固定相对路径
  size_bytes             INTEGER NOT NULL CHECK (size_bytes >= 0),       -- 原始字节数
  mime_type              TEXT NOT NULL CHECK (length(trim(mime_type)) > 0), -- MIME 类型
  first_seen_exchange_id TEXT
                           CHECK (first_seen_exchange_id IS NULL OR length(trim(first_seen_exchange_id)) > 0),
  created_at             TEXT NOT NULL CHECK (length(trim(created_at)) > 0) -- 登记时刻（ISO8601 UTC）
);

CREATE INDEX IF NOT EXISTS ix_wsw_body_blob_first_seen ON wsw_body_blob(first_seen_exchange_id);
CREATE INDEX IF NOT EXISTS ix_wsw_body_blob_size ON wsw_body_blob(size_bytes);

-- ---------------------------------------------------------------------------
-- schema_version：WSW-1 HTTP Message Foundation 登记（version=23；1..22 见
-- 各自登记行，链头 022_coverage_matrix.sql）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (23, 'V1 additive WSW-1 HTTP message foundation CAS dedup table (wsw_body_blob)',
   'WSW-1（docs/rip3/wsw/WSW-1-TASK.md §二/§三 S6 + DESIGN §3.2/§3.3）：wsw_body_blob 被动捕获面 HTTP 正文 CAS 去重表（sha256 小写 hex 主键 + 相对 uri + size_bytes + mime_type + first_seen_exchange_id + created_at；正文文件落 <RP_ARTIFACTS_DIR>/wsw/<前2>/<sha256>）。依据：artifact 表 run_id NOT NULL REFERENCES tool_run，被动捕获无 tool_run 不入 artifact 表（DESIGN §3.3 双层裁决）；超帽截断登记原始字节 sha256（tool-engine envelope.rs:281-321 唯一口径，红线 7）。同 sha256 INSERT OR IGNORE 幂等去重（PLAN :576）；GC 引用计数原语 store::delete_body_blobs_unreferenced，retention 执行位归 WSW-3（DESIGN §4.8）。只在开发库副本与测试内存库执行，platform*.db 生产文件不写',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '023_wsw_body_blob.sql');

-- user_version 双写（DEC-002 条件，001/018/022 先例）
PRAGMA user_version = 23;
