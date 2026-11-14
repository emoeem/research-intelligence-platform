-- ============================================================================
-- Research Intelligence Platform : Phase 9 REST 对外访问审计增量迁移
--   （编排批次 P9-3 第 3/4 批，ROADMAP §10.3 P9-3「REST API 完善 + CLI」：
--   对外契约化收口的访问审计面；SESSION-HANDOFF §十 Phase 9 追加项
--   「外部接口暴露 correlation id」的可查落地面之一）
-- ============================================================================
-- 编号依据（ROADMAP §1.4 迁移编号冻结规则）：018 已被 workflow 控制面实际
-- 占用（P8-3 先到先得，migrations/ 目录 2026-11-14 实测 001..018 在位），
-- 本文件实际取 019。
--
-- 范围（一张新表 + 三个索引 + schema_version 登记；纯新增结构，零既有表
-- 改动、零 ALTER、零 CHECK 词值域改动）：
--   external_api_audit  对外 API 访问审计（append-only，一请求一行）。
--
-- 写面与读取面（职责切分，如实记录）：
--   - 写面是 rp_server.py 的 JSONL 审计流（RP_API_AUDIT_LOG 指向的追加写
--     文件，缺省关闭；每行一个 api_access 事件）。rp_server.py 本身保持
--     零 SQL 写（Phase 6 selftest_phase6.sh 静态门禁：全文件不得含
--     INSERT INTO / UPDATE / DELETE FROM），因此本表不是 rp_server 的
--     直写目标，而是审计流的 SQL 存储形状：消费侧（ETL/装载脚本或后续
--     批次的只读投影端点）按本表列形状把 JSONL 行装进来。
--   - JSONL 行键与本表列一一对应：ts / event / method / path / remote /
--     status / duration_ms / trace_id / correlation_id / rate_limited /
--     auth_enabled（meta_json 装载侧缺省 NULL）。
--   - 生产 migrations/platform*.db 不应用本迁移（平台红线 ROADMAP §13：
--     platform*.db 一律不写；本文件只在开发库副本与测试库执行）。
--
-- 列语义：
--   id              自增主键（装载顺序，不承诺与请求顺序一致）。
--   ts              ISO8601 UTC 时刻（服务端响应收尾时刻）。
--   event           事件词表：'api_access'（P9-3 唯一词；后续事件类型追加
--                   新词不回改本列语义）。
--   method          HTTP 方法（GET/POST/...）。
--   path            请求路径含 query 串（URL 原文，不做解码规整）。
--   remote          客户端地址（rp_server address_string 口径）。
--   status          响应状态码（200/401/404/405/429/...）。
--   duration_ms     服务端处理耗时（整数毫秒）。
--   trace_id        X-Trace-Id 透出值（trace_response_headers 提取，锚不在
--                   场为 NULL；与 015 trace_id 关联体系同 ID 空间）。
--   correlation_id  X-Correlation-Id 透出值（run 详情 = run_id，audit_event
--                   correlation_id 同锚语义，015/018）。
--   rate_limited    0/1：是否被全局限频拒绝（429 rate_limited）。
--   auth_enabled    0/1：服务进程鉴权开关快照（审计可解释 401 分布）。
--   meta_json       扩展位（JSON 文本，可空；不参与任何查询路径）。
--
-- 幂等：CREATE TABLE IF NOT EXISTS + CREATE INDEX IF NOT EXISTS +
-- INSERT OR IGNORE + PRAGMA user_version = 19；无 ALTER TABLE 语句。
-- ============================================================================

CREATE TABLE IF NOT EXISTS external_api_audit (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ts TEXT NOT NULL,
  event TEXT NOT NULL,
  method TEXT NOT NULL,
  path TEXT NOT NULL,
  remote TEXT,
  status INTEGER NOT NULL,
  duration_ms INTEGER,
  trace_id TEXT,
  correlation_id TEXT,
  rate_limited INTEGER NOT NULL DEFAULT 0 CHECK (rate_limited IN (0, 1)),
  auth_enabled INTEGER NOT NULL CHECK (auth_enabled IN (0, 1)),
  meta_json TEXT
);

CREATE INDEX IF NOT EXISTS ix_external_api_audit_ts          ON external_api_audit(ts);
CREATE INDEX IF NOT EXISTS ix_external_api_audit_status      ON external_api_audit(status);
CREATE INDEX IF NOT EXISTS ix_external_api_audit_correlation ON external_api_audit(correlation_id);

-- ---------------------------------------------------------------------------
-- schema_version：Phase 9 REST 对外契约收口变更登记（version=19；1..18 见各自登记行）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (19, 'V1 additive Phase 9 external API audit (external_api_audit append-only access audit)',
   'Phase 9 P9-3（ROADMAP §10.3 P9-3）：external_api_audit 对外 API 访问审计表（append-only 一请求一行：ts/event/method/path/remote/status/duration_ms/trace_id/correlation_id/rate_limited/auth_enabled/meta_json）+ 三个索引；写面是 rp_server 的 JSONL 审计流（RP_API_AUDIT_LOG，缺省关闭），rp_server 本身零 SQL 写（Phase 6 selftest 静态门禁），本表为审计流的 SQL 存储形状供消费侧装载；只在开发库副本与测试库执行，platform*.db 不写',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '019_external_api_audit.sql');

-- user_version 双写（DEC-002 条件，与 001_init.sql 尾部同款）
PRAGMA user_version = 19;
