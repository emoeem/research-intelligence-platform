-- ============================================================================
-- Research Intelligence Platform — M1 全量批增量迁移（隔离环境 dry-run）
-- ============================================================================
-- 范围（任务书 M1 全量批）：
--   1. research_task 大 JSON 段：authorization / environment / knowledge 三个
--      源顶层 JSON 段映射入库（etl_tasks.py v2.0.0）。三列为追加列（TEXT，
--      可空），由 etl_tasks.py 守卫执行 ALTER TABLE ADD COLUMN（SQLite 无条件
--      DDL），本文件为权威记录。
--   2. evidence 全量迁移（etl_evidence.py v2.0.0，26 兼容库全表、无抽样上限）
--      属行数扩展，无 schema 变更，不在本文件登记范围。
--
-- 列语义（保真序列化）：
--   * 源 task JSON 顶层的 authorization / environment / knowledge 键存在且值
--     非 null → json.dumps(value, ensure_ascii=False, sort_keys=True) 逐字段
--     保真序列化入库（可 json.loads 复原为等价对象）。
--   * 键缺失或值为 null → 列置 NULL（忠实于源：不造空对象、不猜测默认值）。
--   * 序列化失败 → 该行整体进隔离清单（quarantine/research_task_isolation.jsonl），
--     不迁移、不静默丢弃。
--
-- 幂等：INSERT OR IGNORE，可重复执行。
-- ============================================================================

-- ---------------------------------------------------------------------------
-- schema_version：M1 全量批增量变更登记（version=4；version=3 为 M6 登记）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (4, 'V1 additive fullbatch (research_task big JSON segments)',
   'M1 全量批：research_task 追加 authorization_json/environment_json/knowledge_json 三列（etl_tasks.py v2.0.0 守卫执行 ALTER TABLE ADD COLUMN，SQLite 无条件 DDL，本行为权威记录与版本登记）；evidence 全量迁移（26 兼容库全表）为行数扩展、无 schema 变更',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '004_fullbatch.sql');
