-- ============================================================================
-- Research Intelligence Platform : Phase 6 Plugin 数据面增量迁移
--   （P6-2 批次 = ROADMAP §7.3 P6-2 + 计划 §12 PluginCapability 实体行 692 行
--    + PLUGIN-CONTRACT §3/§5/§6，docs/phase0/PLUGIN-CONTRACT-DRAFT.md）
-- ============================================================================
-- 编号依据（ROADMAP §1.4 迁移编号冻结规则）：迁移编号按合入顺序递增，先到
-- 先得。本文件合入 009（本批实测 ls migrations/*.sql 为 001 到 008，
-- 008_context_broker.sql 为当前最大号）。
--
-- 范围（Plugin 数据面两表，均为追加结构，不改既有表）：
--   1. plugin：插件版本化行（主键 (plugin_id, version)；PLUGIN-CONTRACT
--      §5.3 升级语义：新旧版本目录并存，is_active 为 active 版本指针，
--      回滚窗口默认保留上一版本一个（草案 D6），窗口结束清理属 P10-2）。
--      state 十二态词表 = PLUGIN-CONTRACT §5.1 主链七态 + 辅助五态
--      （草案 D2），与 domain::plugin::PluginState 枚举一一对应；
--      update_pending 为 UPDATE_PENDING 叠加标记列（§5.1 原文：非独占态，
--      状态本身不变），不占状态位。
--      governance 列沿用 001_init.sql resource 表的治理轴词表
--      （ACTIVE/QUARANTINED/REJECTED/ARCHIVED，K-08 正交轴）：插件 INVALID
--      处置复用 quality-engine 双轴状态机与 Review 队列骨架（PLUGIN-CONTRACT
--      §11.5 既有裁定），生命周期 INVALID（lifecycle 侧 state 列）与治理轴
--      QUARANTINED 分属两轴。
--      source_uninstalled：卸载后已入主库知识的来源标记（§5.3：卸载插件
--      不默认删除已进入主知识库的知识，计划 415 行原文语义），置 1 后在
--      Plugin Center 可查孤儿来源清单。
--   2. plugin_capability：插件 capability 登记行（capability_type 七种 =
--      §3.1 表 = 计划 692 行 PluginCapability，与 domain::plugin::
--      CapabilityType 枚举一一对应）；FK 指向插件版本行，ON DELETE CASCADE：
--      插件版本删除连带其 capability 行（应用层不逐条删）。
--
-- 红线（ROADMAP §13）：新迁移只在开发库副本与测试内存库执行，
-- migrations/platform.db 与 migrations/platform-dryrun.db 一律不写。
--
-- 幂等：CREATE TABLE IF NOT EXISTS + CREATE INDEX IF NOT EXISTS +
-- INSERT OR IGNORE + PRAGMA user_version = 9，可重复执行；
-- 由 crates/database 迁移执行器（migrate.rs）按 user_version 跳过。
-- ============================================================================

-- ---------------------------------------------------------------------------
-- plugin：插件版本化行（§5.1 状态承载 + §5.3 升级指针 + §5.4 保留声明）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS plugin (
  plugin_id          TEXT NOT NULL,               -- domain::plugin::PluginId（小写点分，草案 D1）
  version            TEXT NOT NULL,               -- X.Y.Z（版本化行）
  name               TEXT NOT NULL,               -- 展示名（L1 目录元数据）
  kind               TEXT NOT NULL
                       CHECK (kind IN ('knowledge','tool','project','agent','workflow','integration')),
                       -- PLUGIN-CONTRACT §4.1 六选一（计划 380 到 385 行）
  description        TEXT NOT NULL,               -- 一句话摘要（L1）
  license            TEXT NOT NULL,               -- SPDX 或 UNLICENSED（计划 417 行）
  trust_level        TEXT NOT NULL
                       CHECK (trust_level IN ('official','community','local','remote')),
                       -- §6 信任级四档（计划 584 行；草案 D10 仅 official 自动提示）
  state              TEXT NOT NULL
                       CHECK (state IN ('DISCOVERED','VALIDATED','INSTALLED','REGISTERED',
                                        'ENABLED','LOADED','ACTIVE','INVALID','DISABLED',
                                        'UPDATE_PENDING','ERROR','UNINSTALLED')),
                       -- §5.1 主链七态 + 辅助五态（草案 D2）= domain::plugin::PluginState
  update_pending     INTEGER NOT NULL DEFAULT 0 CHECK (update_pending IN (0,1)),
                       -- §5.1：UPDATE_PENDING 叠加标记，非独占态
  is_active          INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1)),
                       -- §5.3 active 版本指针；升级并存期旧版本置 0（回滚窗口，草案 D6）
  governance         TEXT NOT NULL DEFAULT 'ACTIVE'
                       CHECK (governance IN ('ACTIVE','QUARANTINED','REJECTED','ARCHIVED')),
                       -- K-08 治理正交轴（词表与 001_init.sql resource.governance 同源）；
                       -- INVALID 处置复用 quality-engine 双轴状态机（§11.5 既有裁定）
  source_type        TEXT NOT NULL
                       CHECK (source_type IN ('local_dir','git','archive','mcp_server','http_api')),
                       -- §6 来源五类（草案 D7 并 hosted git 三家）
  source_ref         TEXT NOT NULL,               -- url / path / endpoint / base_url
  pinned_json        TEXT,                        -- §6 pinned {kind, value} JSON（archive 以 sha256 为 pin）
  lock_sha256        TEXT,                        -- plugin.lock 包指纹 = checksums.sha256 文件 sha256
  manifest_json      TEXT NOT NULL,               -- Manifest 规范化 JSON 原文（plugin-registry 校验器输入）
  installed_dir      TEXT,                        -- installed/<id>/<version>/（P6-3 布局承载，位置配置化）
  ingested_policy    TEXT
                       CHECK (ingested_policy IS NULL OR
                              ingested_policy IN ('retain_with_provenance','delete_on_uninstall')),
                       -- data_retention.ingested_policy（§5.4：卸载插件不默认删已入主库知识）
  source_uninstalled INTEGER NOT NULL DEFAULT 0 CHECK (source_uninstalled IN (0,1)),
                       -- §5.3：卸载后已入主库知识的来源标记（孤儿来源清单）
  schema_version     TEXT NOT NULL DEFAULT '1',
  created_at         TEXT NOT NULL,               -- ISO8601 UTC
  updated_at         TEXT NOT NULL,
  PRIMARY KEY (plugin_id, version)
);

CREATE INDEX IF NOT EXISTS ix_plugin_state ON plugin(state);
CREATE INDEX IF NOT EXISTS ix_plugin_active ON plugin(plugin_id, is_active);
CREATE INDEX IF NOT EXISTS ix_plugin_governance ON plugin(governance);

-- ---------------------------------------------------------------------------
-- plugin_capability：插件 capability 登记行（§3.1 七种类型）
--   provides/requires 形状由插件自定义（v1.json capability 声明 payload），
--   以 JSON 文本存储；依赖解析（requires 指向其他插件 provides、环检测）属
--   P6-7 激活层，不在本表建跨插件外键。
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS plugin_capability (
  plugin_id       TEXT NOT NULL,
  plugin_version  TEXT NOT NULL,
  capability_id   TEXT NOT NULL,               -- 点分小写（§3.1 样例 knowledge-source.rtis.poc-corpus）
  capability_type TEXT NOT NULL
                    CHECK (capability_type IN ('knowledge_source','tool','tool_pack',
                                               'project_endpoint','agent_definition',
                                               'workflow_definition','integration_connector')),
  entry           TEXT NOT NULL,               -- 包内相对入口（rule 7 已验存在性）
  summary         TEXT NOT NULL,               -- L2 匹配级摘要
  runtime         TEXT,                        -- tool/tool_pack 才有（D-TC-01 词表 + podman_host 过渡型）
  provides_json   TEXT,                        -- 插件声明 payload JSON
  requires_json   TEXT,                        -- 依赖引用 JSON 数组（P6-7 激活层解析）
  schema_version  TEXT NOT NULL DEFAULT '1',
  PRIMARY KEY (plugin_id, plugin_version, capability_id),
  FOREIGN KEY (plugin_id, plugin_version)
    REFERENCES plugin(plugin_id, version) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS ix_plugin_capability_type
  ON plugin_capability(capability_type);

-- ---------------------------------------------------------------------------
-- schema_version：Phase 6 Plugin 数据面变更登记（version=9；1..8 见各自登记行）
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO schema_version (version, name, description, applied_at, applied_by) VALUES
  (9, 'V1 additive Phase 6 plugin (plugin/plugin_capability)',
   'Phase 6 P6-2（ROADMAP §7.3）：plugin/plugin_capability 两表建表（计划 §12:692 PluginCapability 实体行 + PLUGIN-CONTRACT §3/§5/§6）；state 十二态与 update_pending 叠加标记、is_active 升级指针（草案 D6）、governance 治理轴沿用 K-08 词表（INVALID 处置复用 quality-engine 双轴状态机，§11.5 既有裁定）、source_uninstalled 卸载孤儿来源标记（计划 415 行）、capability 七类型 CHECK 与 domain::plugin 枚举一一对应；capability FK 级联删除；编号按 ROADMAP §1.4 先到先得规则取 009；只在开发库副本与测试内存库执行，platform.db 与 platform-dryrun.db 不写',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '009_plugin.sql');

-- user_version 双写（DEC-002 条件，与 001_init.sql 尾部同款）
PRAGMA user_version = 9;
