-- ============================================================================
-- Research Intelligence Platform : Phase 4 Context Broker 数据面增量迁移
--   （P4-1 批次 = 设计稿 CONTEXT-BROKER-DESIGN §11 M4.0；设计见
--    docs/phase4/PHASE4-DESIGN.md §2.3/§5.2 决议 1）
-- ============================================================================
-- 编号依据（ROADMAP §1.4 迁移编号冻结规则）：迁移编号按合入顺序递增，先到先得，
-- 设计文档中的编号让位于本规则。设计稿 §6.1 草案号 005 让位（P4-0 决议 1）；
-- 本文件合入 008（本批实测 ls migrations/*.sql 为 001 到 007，
-- 005_tool_contract/006_agent_data_plane/007_runtime_node 均已占用）。
--
-- 范围（context 三表追加，不改既有表；计划 §12:703 ContextManifest 实体行）：
--   context_manifest：装配清单头（一次装配一行，不可变，重装配出新
--   manifest_id，设计稿 §2 第 5 条）；selection_hash 记所选集合指纹
--   （设计稿 §7.3，P4-4 落值）。
--   context_manifest_item：清单条目（layer/kind/included 词表 = 设计稿 §6.1
--   列注释，与 domain::context 枚举一一对应）；reason 与 matched_signals
--   承载可解释验收（计划 §14:794「可解释选择了哪些知识和工具、为什么选择」）。
--   context_ref_log：引用事件流水（ASSEMBLED/EXPANDED/EVICTED/CITED_IN_
--   TOOL_CALL/CITED_IN_EVIDENCE，设计稿 §6.1 event 列）；向下接工具调用
--   context_refs（计划:219），向上接 Evidence provenance（设计稿 §6.2）。
--
-- 唯一键、外键、索引、删除语义（计划 §12:706 评审要求，P4-1 补全）：
--   item 以 PK(manifest_id, item_id) 唯一；ref_log 同时挂 manifest 与
--   (manifest_id, item_id) 两级外键，ON DELETE CASCADE：清单不可变且无
--   UPDATE 路径，删除语义定义为「删清单头连带其条目与引用事件」，应用层
--   不逐条删 item。三表无 UPDATE 语句面（只追加）。
--
-- 词表扩展纪律：七类词表（layer/kind/included/intent_status/event/target_
-- slot/tool_family）由 domain::context 枚举承载（本批 P4-1 收编），本文件
-- 只落 layer/kind/included/intent_status/event 五类 CHECK（target_slot 与
-- tool_family 存于 intent_json 快照内，不入列）。
--
-- 红线（ROADMAP §13）：新迁移只在开发库副本与测试内存库执行，
-- migrations/platform.db 与 migrations/platform-dryrun.db 一律不写。
--
-- 幂等：CREATE TABLE IF NOT EXISTS + CREATE INDEX IF NOT EXISTS +
-- INSERT OR IGNORE + PRAGMA user_version = 8，可重复执行；
-- 由 crates/database 迁移执行器（migrate.rs:67）按 user_version 跳过。
-- ============================================================================

-- ---------------------------------------------------------------------------
-- context_manifest：装配清单头（设计稿 §6.1 草案列 + selection_hash）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS context_manifest (
  manifest_id    TEXT PRIMARY KEY,             -- ctxm-<ulid>（设计稿 §6.1；重装配出新 id）
  task_id        TEXT,                         -- 关联任务（MCP 外部场景可空）
  intent_json    TEXT,                         -- IntentQuery 快照（设计稿 §4.1，含 families/slots）
  policy_version TEXT NOT NULL,                -- 预算策略版本（设计稿 §5.1；变化即缓存失效，§7.2）
  broker_version TEXT NOT NULL,                -- 装配逻辑版本（设计稿 §9 broker_version）
  budget_json    TEXT,                         -- 分项分配快照（budget_report.allocated）
  totals_json    TEXT,                         -- 分项实际占用与驱逐计数（budget_report.used/evicted）
  intent_status  TEXT NOT NULL
                   CHECK (intent_status IN ('ok','ambiguous')),
                   -- 设计稿 §4.2 第 4 条：低于阈值标记 ambiguous 附 top3 候选，不静默猜
  selection_hash TEXT,                         -- 所选集合指纹（设计稿 §7.3，P4-4 落值）
  created_at     TEXT NOT NULL,                -- 签发时刻（ISO8601 UTC）
  schema_version TEXT NOT NULL DEFAULT '1'
);

-- ---------------------------------------------------------------------------
-- context_manifest_item：清单条目（设计稿 §6.1 草案列，词表 = domain::context）
--   reason/matched_signals_json 非空语义由装配面（P4-4）保证：可解释验收
--   （计划 §14:794）要求每个 item 携带命中信号；表层不设 CHECK 文本非空
--   （与 tool_run.run_id 同款：内容口径属 domain 校验面，表列不设文本 CHECK）。
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS context_manifest_item (
  manifest_id  TEXT NOT NULL
                 REFERENCES context_manifest(manifest_id) ON DELETE CASCADE,
  item_id      TEXT NOT NULL,                 -- 清单内条目 id（ki-/tb- 前缀形态，设计稿 §9.1）
  layer        TEXT NOT NULL
                 CHECK (layer IN ('L0','L1','L2','L3')),
                 -- 四级上下文（计划 §8:437-443 层表）
  kind         TEXT NOT NULL
                 CHECK (kind IN ('rule','tool','knowledge','plugin','artifact','schema','doc')),
                 -- 设计稿 §6.1 kind 列注释七值
  ref_id       TEXT NOT NULL,                 -- 来源对象 id（如 kb:900008 / rip.http.inspect）
  ref_version  TEXT,                          -- 来源版本（可缺省指 current）
  source       TEXT NOT NULL,
                 -- registry|platform-kb|rtis|eri|artifact-store|plugin:<id>
                 -- （设计稿 §6.1 source 列注释；plugin:<id> 为前缀形态，用 TEXT 不用 CHECK）
  title        TEXT NOT NULL DEFAULT '',
  reason       TEXT NOT NULL DEFAULT '',      -- 「为什么是它」（设计稿 §2 第 2 条）
  matched_signals_json TEXT NOT NULL DEFAULT '[]',
                 -- 命中信号数组（设计稿 §4.3 matched_signals[]）
  est_tokens   INTEGER NOT NULL CHECK (est_tokens >= 0),
                 -- 入库时预计算（设计稿 §5.2，装配期只做加法）
  included     TEXT NOT NULL
                 CHECK (included IN ('full','summary','ref-only','catalog-card')),
                 -- 设计稿 §6.1 included 列注释四值（降级链只改形态不删行，§5.3 第 6 条）
  content_hash TEXT,                          -- 内容 sha256 hex（计划 §12:703 版本追踪；L1 卡片可空）
  PRIMARY KEY (manifest_id, item_id)
);

-- ---------------------------------------------------------------------------
-- context_ref_log：引用事件流水（设计稿 §6.1；expand 记账与引用追踪闭环）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS context_ref_log (
  log_id       INTEGER PRIMARY KEY AUTOINCREMENT,
  manifest_id  TEXT NOT NULL
                 REFERENCES context_manifest(manifest_id) ON DELETE CASCADE,
  item_id      TEXT NOT NULL,
                 -- 复合外键以表级约束声明（SQLite 列级 REFERENCES 只能挂单列父键）
  event        TEXT NOT NULL
                 CHECK (event IN ('ASSEMBLED','EXPANDED','EVICTED','CITED_IN_TOOL_CALL','CITED_IN_EVIDENCE')),
                 -- 设计稿 §6.1 event 列注释五值
  detail_json  TEXT NOT NULL DEFAULT '{}',    -- 事件明细（expand 增量 tokens、驱逐原因等）
  created_at   TEXT NOT NULL,                 -- 事件时刻（ISO8601 UTC）
  FOREIGN KEY (manifest_id, item_id)
    REFERENCES context_manifest_item(manifest_id, item_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS ix_context_manifest_task
  ON context_manifest(task_id);
CREATE INDEX IF NOT EXISTS ix_context_manifest_item_manifest_layer
  ON context_manifest_item(manifest_id, layer);
CREATE INDEX IF NOT EXISTS ix_context_ref_log_event
  ON context_ref_log(event);
CREATE INDEX IF NOT EXISTS ix_context_ref_log_manifest_item
  ON context_ref_log(manifest_id, item_id);

-- ---------------------------------------------------------------------------
-- schema_version：Phase 4 Context Broker 数据面变更登记（version=8；1..7 见各自登记行）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (8, 'V1 additive Phase 4 context broker (context_manifest/context_manifest_item/context_ref_log)',
   'Phase 4 P4-1（M4.0）：context 三表建表（计划 §12:703 ContextManifest 实体 + 设计稿 §6.1 草案列）；layer/kind/included/intent_status/event 五类 CHECK 词表与 domain::context 枚举一一对应（P4-1 收编）；item PK(manifest_id,item_id)、ref_log 双外键 ON DELETE CASCADE 与四索引为计划 §12:706 评审补全面；编号按 ROADMAP §1.4 先到先得规则取 008（设计稿 §6.1 草案号 005 让位，P4-0 决议 1）；只在开发库副本与测试内存库执行，platform.db 与 platform-dryrun.db 不写',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '008_context_broker.sql');

-- user_version 双写（DEC-002 条件，与 001_init.sql 尾部同款）
PRAGMA user_version = 8;
