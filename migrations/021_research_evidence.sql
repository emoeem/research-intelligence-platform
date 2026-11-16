-- ============================================================================
-- Research Intelligence Platform : RIP3 Phase 5 P5-7 验证证据档增量迁移
--   research_evidence 表（Evidence Artifact 采集与 SHA256，补充计划 §7 P5-7
--   与 §3.1 职责表：Evidence 采集 Artifact/哈希/Trace 关联/检索入口在 RIP3，
--   结论性证据状态以 RTIS 为准）。
--
--   载荷语义：payload_json 是采集时点的原始证据快照（Provider 结果帧/归一化
--   结论帧/日志摘录），append-only 不可变（原始证据不会被知识更新覆盖）；
--   content_hash = payload_json 规范化序列化的 SHA256（重复观察识别：
--   同 hash 重复采集跳过并留痕，§5.8 第四层去重）。
--
--   幂等：CREATE TABLE IF NOT EXISTS + INSERT OR IGNORE（002/018/020 先例）。
--   只在开发库副本与测试内存库执行；migrations/platform*.db 生产文件不写。
-- ============================================================================

CREATE TABLE IF NOT EXISTS research_evidence (
  evidence_id   TEXT PRIMARY KEY,       -- re-YYYYMMDD-HHMMSS-ms-rand
  item_id       TEXT NOT NULL,          -- research_plan_item 引用
  plan_id       TEXT NOT NULL,          -- research_plan 引用（UI 跳转面）
  tool_run_id   TEXT,                   -- Trace 六 ID 锚：tool_run 关联
  trace_id      TEXT,                   -- 六 ID 统一关联（015 同款列语义）
  evidence_type TEXT NOT NULL
                CHECK (evidence_type IN ('provider_result','normalized_result','log_excerpt')),
  content_hash  TEXT NOT NULL,          -- SHA256 hex（payload_json 规范化序列化）
  payload_json  TEXT NOT NULL,          -- 原始证据快照（append-only 不可变）
  created_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS ix_research_evidence_item ON research_evidence(item_id);
CREATE INDEX IF NOT EXISTS ix_research_evidence_plan ON research_evidence(plan_id);
CREATE INDEX IF NOT EXISTS ix_research_evidence_hash ON research_evidence(content_hash);
CREATE INDEX IF NOT EXISTS ix_research_evidence_trace ON research_evidence(trace_id);

-- ---------------------------------------------------------------------------
-- schema_version：RIP3 Phase 5 研究编排域登记（version=21；1..20 见各自登记行）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (21, 'V1 additive RIP3 Phase 5 verification evidence ledger (research_evidence)',
   'RIP3 P5-7（补充计划 §7 P5-7）：验证证据档 research_evidence（Evidence Artifact 采集与 SHA256；§3.1 职责表：Evidence 采集 Artifact/哈希/Trace 关联/检索入口在 RIP3，结论性证据状态以 RTIS 为准）。payload_json 为采集时点原始证据快照 append-only 不可变（原始证据不会被知识更新覆盖）；content_hash = payload_json 规范化序列化 SHA256（重复观察识别，§5.8 第四层去重）；tool_run_id/trace_id 六 ID 关联锚沿用 015 面；evidence_type 词表 provider_result/normalized_result/log_excerpt；只在开发库副本与测试内存库执行，platform*.db 生产文件不写',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '021_research_evidence.sql');

-- user_version 双写（DEC-002 条件，001/018 先例）
PRAGMA user_version = 21;
