//! plugin / plugin_capability 两表唯一写面（009 迁移；只写这两张表，
//! 核心表零直写，验收口径见 tests/lifecycle_full_path.rs 的零直写对账）。

use std::collections::HashSet;

use database::Pool;
use domain::plugin::{
    CapabilityType, Plugin, PluginCapability, PluginId, PluginKind, PluginState, PluginTrust,
};
use domain::time::now_iso;
use serde_json::Value;

/// 插件存储（Pool 共享；写面仅 plugin / plugin_capability 两表）。
#[derive(Debug, Clone)]
pub struct PluginStore {
    pool: Pool,
}

impl PluginStore {
    /// 打开存储。
    pub fn open(pool: Pool) -> Self {
        Self { pool }
    }

    /// 底层连接池视图。
    pub fn pool(&self) -> &Pool {
        &self.pool
    }

    // ------------------------------------------------------------------
    // 写路径
    // ------------------------------------------------------------------

    /// 插入版本行（INSTALLED，is_active 由调用方置）+ capability 行（单事务）。
    pub fn insert_installed(
        &self,
        plugin: &Plugin,
        capabilities: &[PluginCapability],
    ) -> database::DbResult<()> {
        self.pool.with_tx(|c| -> database::rusqlite::Result<()> {
            c.execute(
                "INSERT INTO plugin (plugin_id, version, name, kind, description, license,
                    trust_level, state, update_pending, is_active, governance, source_type,
                    source_ref, pinned_json, lock_sha256, manifest_json, installed_dir,
                    ingested_policy, source_uninstalled, schema_version, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'ACTIVE',?11,?12,?13,?14,?15,?16,?17,0,'1',?18,?18)",
                database::rusqlite::params![
                    plugin.plugin_id.as_str(),
                    plugin.version,
                    plugin.name,
                    plugin.kind.as_str(),
                    plugin.description,
                    plugin.license,
                    plugin.trust_level.as_str(),
                    plugin.state.as_str(),
                    if plugin.update_pending { 1 } else { 0 },
                    if plugin.is_active { 1 } else { 0 },
                    plugin.source_type,
                    plugin.source_ref,
                    plugin.pinned_json,
                    plugin.lock_sha256,
                    plugin.manifest_json,
                    plugin.installed_dir,
                    plugin.ingested_policy,
                    plugin.created_at,
                ],
            )?;
            insert_capabilities(c, capabilities)?;
            Ok(())
        })?;
        Ok(())
    }

    /// 状态跃迁落库，返回跃迁前状态（调用方负责先过 §5.2 跃迁表）。
    pub fn set_state(
        &self,
        plugin_id: &str,
        version: &str,
        to: PluginState,
    ) -> database::DbResult<PluginState> {
        let from = self.pool.with_conn(|c| {
            c.query_row(
                "SELECT state FROM plugin WHERE plugin_id = ?1 AND version = ?2",
                database::rusqlite::params![plugin_id, version],
                |r| r.get::<_, String>(0),
            )
            .map_err(database::DbError::from)
        })?;
        let from_state =
            PluginState::parse(&from).map_err(|e| database::DbError::Validation(e.to_string()))?;
        self.pool.with_conn(|c| {
            c.execute(
                "UPDATE plugin SET state = ?1, updated_at = ?2 WHERE plugin_id = ?3 AND version = ?4",
                database::rusqlite::params![to.as_str(), now_iso(), plugin_id, version],
            )
            .map_err(database::DbError::from)
        })?;
        Ok(from_state)
    }

    /// UPDATE_PENDING 叠加标记（§5.1：状态本身不变）。
    pub fn set_update_pending(
        &self,
        plugin_id: &str,
        version: &str,
        pending: bool,
    ) -> database::DbResult<()> {
        self.pool.with_conn(|c| {
            c.execute(
                "UPDATE plugin SET update_pending = ?1, updated_at = ?2 WHERE plugin_id = ?3 AND version = ?4",
                database::rusqlite::params![if pending { 1 } else { 0 }, now_iso(), plugin_id, version],
            )
            .map_err(database::DbError::from)
        })?;
        Ok(())
    }

    /// 来源已卸载标记（§5.3 孤儿来源清单）。
    pub fn mark_source_uninstalled(
        &self,
        plugin_id: &str,
        version: &str,
    ) -> database::DbResult<()> {
        self.pool.with_conn(|c| {
            c.execute(
                "UPDATE plugin SET source_uninstalled = 1, updated_at = ?1 WHERE plugin_id = ?2 AND version = ?3",
                database::rusqlite::params![now_iso(), plugin_id, version],
            )
            .map_err(database::DbError::from)
        })?;
        Ok(())
    }

    /// 升级落库（§5.3 第 3/4 步，单事务自动回滚）：新版本行（REGISTERED）
    /// 加 capability 行加指针切换（全部旧行 is_active=0、update_pending=0、
    /// 新行 is_active=1）；任一步失败整事务回滚，旧版本指针不动。
    pub fn apply_upgrade(
        &self,
        active: &Plugin,
        new_plugin: &Plugin,
        capabilities: &[PluginCapability],
    ) -> database::DbResult<()> {
        self.pool.with_tx(|c| -> database::rusqlite::Result<()> {
            // 旧行全部退位（回滚窗口：目录并存、行保留，is_active=0）。
            c.execute(
                "UPDATE plugin SET is_active = 0, update_pending = 0, updated_at = ?1
                 WHERE plugin_id = ?2",
                database::rusqlite::params![now_iso(), active.plugin_id.as_str()],
            )?;
            // 新版本行直接以 REGISTERED 落库并接指针（事务内无中间态）。
            c.execute(
                "INSERT INTO plugin (plugin_id, version, name, kind, description, license,
                    trust_level, state, update_pending, is_active, governance, source_type,
                    source_ref, pinned_json, lock_sha256, manifest_json, installed_dir,
                    ingested_policy, source_uninstalled, schema_version, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,'REGISTERED',0,1,'ACTIVE',?8,?9,?10,?11,?12,?13,?14,0,'1',?15,?15)",
                database::rusqlite::params![
                    new_plugin.plugin_id.as_str(),
                    new_plugin.version,
                    new_plugin.name,
                    new_plugin.kind.as_str(),
                    new_plugin.description,
                    new_plugin.license,
                    new_plugin.trust_level.as_str(),
                    new_plugin.source_type,
                    new_plugin.source_ref,
                    new_plugin.pinned_json,
                    new_plugin.lock_sha256,
                    new_plugin.manifest_json,
                    new_plugin.installed_dir,
                    new_plugin.ingested_policy,
                    new_plugin.created_at,
                ],
            )?;
            insert_capabilities(c, capabilities)?;
            Ok(())
        })?;
        Ok(())
    }

    /// 删除版本行（capability 行随 FK ON DELETE CASCADE 连带删除）。
    pub fn delete_version(&self, plugin_id: &str, version: &str) -> database::DbResult<()> {
        self.pool.with_conn(|c| {
            c.execute(
                "DELETE FROM plugin WHERE plugin_id = ?1 AND version = ?2",
                database::rusqlite::params![plugin_id, version],
            )
            .map_err(database::DbError::from)
        })?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // 读路径
    // ------------------------------------------------------------------

    /// active 版本行。
    pub fn get_active(&self, plugin_id: &str) -> database::DbResult<Option<Plugin>> {
        self.pool.with_conn(|c| {
            c.query_row(
                "SELECT plugin_id, version, name, kind, description, license, trust_level,
                        state, update_pending, is_active, source_type, source_ref, pinned_json,
                        lock_sha256, manifest_json, installed_dir, ingested_policy,
                        source_uninstalled, created_at, updated_at
                 FROM plugin WHERE plugin_id = ?1 AND is_active = 1",
                [plugin_id],
                row_to_plugin,
            )
            .map(Some)
            .or_else(|e| match e {
                database::rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(database::DbError::from(other)),
            })
        })
    }

    /// 指定版本行。
    pub fn get(&self, plugin_id: &str, version: &str) -> database::DbResult<Option<Plugin>> {
        self.pool.with_conn(|c| {
            c.query_row(
                "SELECT plugin_id, version, name, kind, description, license, trust_level,
                        state, update_pending, is_active, source_type, source_ref, pinned_json,
                        lock_sha256, manifest_json, installed_dir, ingested_policy,
                        source_uninstalled, created_at, updated_at
                 FROM plugin WHERE plugin_id = ?1 AND version = ?2",
                database::rusqlite::params![plugin_id, version],
                row_to_plugin,
            )
            .map(Some)
            .or_else(|e| match e {
                database::rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(database::DbError::from(other)),
            })
        })
    }

    /// 全部版本行（Plugin Center 列表用）。
    pub fn list(&self) -> database::DbResult<Vec<Plugin>> {
        self.pool.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT plugin_id, version, name, kind, description, license, trust_level,
                        state, update_pending, is_active, source_type, source_ref, pinned_json,
                        lock_sha256, manifest_json, installed_dir, ingested_policy,
                        source_uninstalled, created_at, updated_at
                 FROM plugin ORDER BY plugin_id, version",
            )?;
            let rows = stmt.query_map([], row_to_plugin)?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok::<Vec<Plugin>, database::rusqlite::Error>(out)
        })
    }

    /// 已激活（is_active=1）插件 id 集。
    pub fn active_ids(&self) -> database::DbResult<Vec<String>> {
        self.pool.with_conn(|c| {
            let mut stmt = c.prepare("SELECT plugin_id FROM plugin WHERE is_active = 1")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok::<Vec<String>, database::rusqlite::Error>(out)
        })
    }

    /// capability 行读取。
    pub fn capabilities(
        &self,
        plugin_id: &str,
        version: &str,
    ) -> database::DbResult<Vec<PluginCapability>> {
        self.pool.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT plugin_id, capability_id, capability_type, entry, summary, runtime,
                        provides_json, requires_json
                 FROM plugin_capability WHERE plugin_id = ?1 AND plugin_version = ?2
                 ORDER BY capability_id",
            )?;
            let rows = stmt.query_map(database::rusqlite::params![plugin_id, version], |r| {
                Ok(PluginCapability {
                    plugin_id: PluginId::try_new(r.get::<_, String>(0)?).map_err(|e| {
                        database::rusqlite::Error::ToSqlConversionFailure(Box::new(e))
                    })?,
                    capability_id: r.get(1)?,
                    plugin_version: version.to_string(),
                    capability_type: CapabilityType::parse(&r.get::<_, String>(2)?).map_err(
                        |e| database::rusqlite::Error::ToSqlConversionFailure(Box::new(e)),
                    )?,
                    entry: r.get(3)?,
                    summary: r.get(4)?,
                    runtime: r.get(5)?,
                    provides_json: r.get(6)?,
                    requires_json: r.get(7)?,
                })
            })?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok::<Vec<PluginCapability>, database::rusqlite::Error>(out)
        })
    }
}

fn insert_capabilities(
    c: &database::rusqlite::Connection,
    capabilities: &[PluginCapability],
) -> database::rusqlite::Result<()> {
    for cap in capabilities {
        c.execute(
            "INSERT INTO plugin_capability (plugin_id, plugin_version, capability_id,
                capability_type, entry, summary, runtime, provides_json, requires_json,
                schema_version)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'1')",
            database::rusqlite::params![
                cap.plugin_id.as_str(),
                cap.plugin_version,
                cap.capability_id,
                cap.capability_type.as_str(),
                cap.entry,
                cap.summary,
                cap.runtime,
                cap.provides_json,
                cap.requires_json,
            ],
        )?;
    }
    Ok(())
}

fn row_to_plugin(r: &database::rusqlite::Row<'_>) -> database::rusqlite::Result<Plugin> {
    let state_raw: String = r.get(7)?;
    let trust_raw: String = r.get(6)?;
    let kind_raw: String = r.get(3)?;
    Ok(Plugin {
        plugin_id: PluginId::try_new(r.get::<_, String>(0)?)
            .map_err(|e| database::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
        version: r.get(1)?,
        name: r.get(2)?,
        kind: PluginKind::parse(&kind_raw)
            .map_err(|e| database::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
        description: r.get(4)?,
        license: r.get(5)?,
        trust_level: PluginTrust::parse(&trust_raw)
            .map_err(|e| database::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
        state: PluginState::parse(&state_raw)
            .map_err(|e| database::rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
        update_pending: r.get::<_, i64>(8)? != 0,
        is_active: r.get::<_, i64>(9)? != 0,
        source_type: r.get(10)?,
        source_ref: r.get(11)?,
        pinned_json: r.get(12)?,
        lock_sha256: r.get(13)?,
        manifest_json: r.get(14)?,
        installed_dir: r.get(15)?,
        ingested_policy: r.get(16)?,
        source_uninstalled: r.get::<_, i64>(17)? != 0,
        created_at: r.get(18)?,
        updated_at: r.get(19)?,
    })
}

/// 从 Manifest 的 capabilities 数组构造实体列表（install/upgrade 共用；
/// 类型不在七种词表即拒绝）。
pub fn capabilities_from_manifest(
    plugin_id: &PluginId,
    version: &str,
    manifest: &Value,
) -> Result<Vec<PluginCapability>, crate::RuntimeError> {
    let mut out = Vec::new();
    let caps = manifest
        .get("capabilities")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            crate::RuntimeError::new("manifest_invalid", "manifest has no `capabilities` array")
        })?;
    for cap in caps {
        let obj = cap.as_object().ok_or_else(|| {
            crate::RuntimeError::new("manifest_invalid", "capability item must be an object")
        })?;
        let capability_type =
            CapabilityType::parse(obj.get("type").and_then(Value::as_str).ok_or_else(|| {
                crate::RuntimeError::new("manifest_invalid", "capability has no `type`")
            })?)
            .map_err(|e| crate::RuntimeError::new("manifest_invalid", e.to_string()))?;
        out.push(PluginCapability {
            capability_id: obj
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    crate::RuntimeError::new("manifest_invalid", "capability has no `id`")
                })?
                .to_string(),
            plugin_id: plugin_id.clone(),
            plugin_version: version.to_string(),
            capability_type,
            entry: obj
                .get("entry")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            summary: obj
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            runtime: obj
                .get("runtime")
                .and_then(|r| r.get("type"))
                .and_then(Value::as_str)
                .map(str::to_string),
            provides_json: obj.get("provides").map(|p| p.to_string()),
            requires_json: obj.get("requires").map(|p| p.to_string()),
        });
    }
    Ok(out)
}

/// id 集合便捷构造（测试与 CLI 用）。
pub fn id_set(ids: &[String]) -> HashSet<String> {
    ids.iter().cloned().collect()
}
