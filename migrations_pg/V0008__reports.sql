-- RIP3 情报核迁移链 V0008：报告持久化面（RIP3 Phase 6b，PHASE-6B-TASK.md §0 D4；
-- 03-DOMAIN-MODEL.md §1.9 :124/:132-133 形状冻结）。
--
-- reports 纯新增表（零既有表改动）：
-- - report_id TEXT PRIMARY KEY：前缀 rep-（:133 冻结；new_entity_id("rep-") 生成面）。
-- - task_id TEXT NOT NULL：稳定 ID 引用非跨库 FK——:144 词形「全稳定 ID 关联」；
--   research_tasks PG 表 V0001-V0007 未建，无 FK 目标（V0002 零跨库引用标注同款）。
-- - report_ir JSONB NOT NULL：Report IR v1（schemas/report-ir/report-ir-v1.schema.json，
--   sha256 888e9210… 冻结；04/11 号文档 Schema）。禁 CHECK 词表——词表由 IR schema
--   单源承载，本表禁第二事实源（6A 红线 3）。
-- - format_exports JSONB NOT NULL DEFAULT '{}'：导出台账位，additive-only
--   {format:{sha256,bytes,exported_at}}；导出批按 format 键 upsert 不改他键。
-- - created_at TIMESTAMPTZ NOT NULL DEFAULT now()。
-- - idx_reports_task：按 task 列台账索引。
--
-- 写入口唯一 = report_repo（crates/intel-db/src/report_repo.rs）：store_report /
-- upsert_format_export。读面 = load_report / list_reports_by_task。
-- report-engine 纯函数零 I/O 红线不破（engine 只出 IR 与渲染字节，持久化在本层）。
-- RTIS 零接触：report_ir 内 rtis:// 引用只读透传零解析（6A 红线 4 词形）。
--
-- 幂等：refinery 每文件事务 + checksum（intel-db lib.rs:131-143）；已应用
-- 版本跳过。只在开发库副本与测试库执行；生产 PG 实例不接本批写面。
-- 编号登记：SQLite 036 不立项（reports 属 PG 面；migrations/001-030 零 diff）。
-- V0007:12 词形「扩词值须升 V0008+」——donor_channel 扩词若发生走 V0009+，
-- 与 V0008=reports 兼容（不同表不同面）。
CREATE TABLE reports (
    report_id      TEXT PRIMARY KEY,
    task_id        TEXT NOT NULL,
    report_ir      JSONB NOT NULL,
    format_exports JSONB NOT NULL DEFAULT '{}',
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_reports_task ON reports(task_id);
