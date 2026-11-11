//! state/ 文件承载（PLUGIN-CONTRACT §8；草案 D3）。
//!
//! 职责：plugin-state.json（全量状态快照，原子写）与 transitions.jsonl
//! （跃迁流水，追加写）两个文件的读写。权威状态在 plugin 表（009 迁移），
//! state/ 文件是数据库旁的落盘快照与流水（备份、对账与 Plugin Center
//! 只读消费面；§11.3 口径：Phase 6 起权威落库，文件承载转为旁路留存）。
//!
//! 写纪律：快照写 tmp 临时文件后 rename（原子换新，仓库内 worker 缓存
//! 原子写同款先例）；流水 append-only，行 = 一条 JSON 对象。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use domain::plugin::Plugin;
use serde_json::{json, Value};

use crate::paths::{PathsError, PluginPaths, STATE_FILE_NAME, TRANSITIONS_FILE_NAME};

/// state/ 文件存取器。
#[derive(Debug, Clone)]
pub struct StateStore {
    state_dir: PathBuf,
}

impl StateStore {
    /// 从布局路径集打开（state/ 目录由 [`PluginPaths::ensure_layout`] 建立）。
    pub fn open(paths: &PluginPaths) -> Self {
        Self {
            state_dir: paths.state_dir(),
        }
    }

    /// 确保 state/ 目录存在（CLI 每次操作前调用；幂等）。
    pub fn ensure_dir(&self) -> Result<(), PathsError> {
        fs::create_dir_all(&self.state_dir)
            .map_err(|e| PathsError::new("io", format!("create state dir: {e}")))
    }

    /// 直接以 state 目录构造（测试与工具用）。
    pub fn open_dir(state_dir: impl Into<PathBuf>) -> Self {
        Self {
            state_dir: state_dir.into(),
        }
    }

    fn tmp_path(&self, target: &Path) -> PathBuf {
        let name = target
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".to_string());
        self.state_dir.join(format!(".{name}.tmp"))
    }

    /// 写全量状态快照 plugin-state.json（原子换新；tmp 残留即失败现场）。
    pub fn write_snapshot(&self, plugins: &[Plugin]) -> Result<(), PathsError> {
        fs::create_dir_all(&self.state_dir)
            .map_err(|e| PathsError::new("io", format!("create state dir: {e}")))?;
        let target = self.state_dir.join(STATE_FILE_NAME);
        let body = snapshot_of(plugins);
        let tmp = self.tmp_path(&target);
        {
            let mut f = fs::File::create(&tmp)
                .map_err(|e| PathsError::new("io", format!("create tmp: {e}")))?;
            f.write_all(body.to_string().as_bytes())
                .map_err(|e| PathsError::new("io", format!("write tmp: {e}")))?;
            f.sync_all()
                .map_err(|e| PathsError::new("io", format!("sync tmp: {e}")))?;
        }
        fs::rename(&tmp, &target).map_err(|e| PathsError::new("io", format!("rename: {e}")))?;
        Ok(())
    }

    /// 读状态快照（文件不存在视为空快照）。
    pub fn read_snapshot(&self) -> Result<Value, PathsError> {
        let target = self.state_dir.join(STATE_FILE_NAME);
        if !target.exists() {
            return Ok(json!({"generated_at": null, "plugins": []}));
        }
        let body =
            fs::read_to_string(&target).map_err(|e| PathsError::new("io", format!("read: {e}")))?;
        serde_json::from_str(&body).map_err(|e| PathsError::new("io", format!("parse: {e}")))
    }

    /// 追加跃迁流水行（transitions.jsonl，append-only）。
    pub fn append_transitions<'a, I>(&self, records: I) -> Result<u64, PathsError>
    where
        I: IntoIterator<Item = &'a Value>,
    {
        fs::create_dir_all(&self.state_dir)
            .map_err(|e| PathsError::new("io", format!("create state dir: {e}")))?;
        let target = self.state_dir.join(TRANSITIONS_FILE_NAME);
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&target)
            .map_err(|e| PathsError::new("io", format!("open append: {e}")))?;
        let mut n = 0u64;
        for record in records {
            let line = serde_json::to_string(record)
                .map_err(|e| PathsError::new("io", format!("serialize record: {e}")))?;
            writeln!(f, "{line}").map_err(|e| PathsError::new("io", format!("write line: {e}")))?;
            n += 1;
        }
        f.sync_all()
            .map_err(|e| PathsError::new("io", format!("sync: {e}")))?;
        Ok(n)
    }

    /// 读回流水（逐行 JSON 解析，空行跳过）。
    pub fn read_transitions(&self) -> Result<Vec<Value>, PathsError> {
        let target = self.state_dir.join(TRANSITIONS_FILE_NAME);
        if !target.exists() {
            return Ok(Vec::new());
        }
        let body =
            fs::read_to_string(&target).map_err(|e| PathsError::new("io", format!("read: {e}")))?;
        body.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                serde_json::from_str(l)
                    .map_err(|e| PathsError::new("io", format!("parse line {l:?}: {e}")))
            })
            .collect()
    }
}

/// 快照 JSON 构造（plugin-state.json 形状；数据库行到快照条目的唯一映射）。
pub fn snapshot_of(plugins: &[Plugin]) -> Value {
    json!({
        "generated_at": domain::time::now_iso(),
        "plugins": plugins.iter().map(|p| json!({
            "plugin_id": p.plugin_id.as_str(),
            "version": p.version,
            "kind": p.kind.as_str(),
            "state": p.state.as_str(),
            "update_pending": p.update_pending,
            "is_active": p.is_active,
            "trust_level": p.trust_level.as_str(),
            "governance_source": "plugin table (009)",
            "installed_dir": p.installed_dir,
            "source_uninstalled": p.source_uninstalled,
        })).collect::<Vec<_>>(),
    })
}

/// CLI/门面共用的跃迁流水行构造（transitions.jsonl 行形状）。
pub fn transition_record(
    action: &str,
    plugin_id: &str,
    version: &str,
    from: Option<&str>,
    to: &str,
) -> Value {
    json!({
        "action": action,
        "plugin_id": plugin_id,
        "version": version,
        "from": from,
        "to": to,
        "at_unix": plugin_registry::now_unix(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn memory_pool() -> database::Pool {
        database::Pool::open(database::PoolConfig {
            path: database::DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(5),
        })
        .unwrap()
    }

    #[test]
    fn snapshot_roundtrip_and_atomic_no_tmp_residue() {
        let dir = std::env::temp_dir().join(format!("p6state-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let store = StateStore::open_dir(&dir);
        // 空快照（文件不存在）。
        let empty = store.read_snapshot().unwrap();
        assert_eq!(empty["plugins"].as_array().unwrap().len(), 0);

        // 内存库全链迁移后插入一行（借 plugin 表真实列形状）。
        let pool = memory_pool();
        database::migrate(
            &pool,
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations"),
        )
        .unwrap();
        pool.with_conn(|c| {
            c.execute(
                "INSERT INTO plugin (plugin_id, version, name, kind, description, license,
                    trust_level, state, update_pending, is_active, governance, source_type,
                    source_ref, manifest_json, created_at, updated_at)
                 VALUES ('k.snap','0.1.0','S','knowledge','d','MIT','local','REGISTERED',
                         0,1,'ACTIVE','local_dir','p','{}','2026-01-01','2026-01-01')",
                [],
            )
        })
        .unwrap();
        let rows = crate::store::PluginStore::open(pool.clone())
            .list()
            .unwrap();
        store.write_snapshot(&rows).unwrap();
        assert!(
            !dir.join(".plugin-state.json.tmp").exists(),
            "no tmp residue"
        );
        let snap = store.read_snapshot().unwrap();
        assert_eq!(snap["plugins"][0]["state"], "REGISTERED");
        assert_eq!(snap["plugins"][0]["plugin_id"], "k.snap");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn transitions_jsonl_append_read_roundtrip() {
        let dir = std::env::temp_dir().join(format!("p6trans-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let store = StateStore::open_dir(&dir);
        let r1 = transition_record("install", "k.a", "0.1.0", None, "REGISTERED");
        let r2 = transition_record("enable", "k.a", "0.1.0", Some("REGISTERED"), "ENABLED");
        assert_eq!(store.append_transitions([&r1, &r2]).unwrap(), 2);
        assert_eq!(store.append_transitions([&r1]).unwrap(), 1, "append-only");
        let lines = store.read_transitions().unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0]["to"], "REGISTERED");
        assert_eq!(lines[1]["from"], "REGISTERED");
        assert!(lines[0]["at_unix"].is_i64());
        let _ = fs::remove_dir_all(&dir);
    }
}
