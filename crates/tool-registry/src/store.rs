//! tool / tool_version 读写面（§2.5 第 9 条落库语义）。
//!
//! 列沿用（001_init.sql:278-315，不新增列）：
//! - tool：tool_id / name / kind / lifecycle / governance / permission_level /
//!   input_schema / output_schema / timeout_s / binary_ref / source_system /
//!   source_pk / created_at / updated_at / schema_version；
//! - tool_version：tool_id / version / provenance_json / content_hash /
//!   validated_at / status（current|superseded）/ created_at / updated_at。
//!
//! tool_id 列取 Manifest id 原文（如 `rip.doc.parse`），与 §4.1
//! ToolCallRequest.tool_id 及 §3.3 run.tool_id 同值，全链一致。
//! 同 id 旧 current 版本在新版本登记事务内置 superseded（§6.2：单工具至多
//! 一个 current）；并发重复登记由 UNIQUE(tool_id, version) 兜底（§6.2 原文）。

use crate::error::RegistryError;
use crate::validate::ValidatedManifest;
use database::Pool;
use domain::time::now_iso;
use serde_json::Value;

/// 登记结果。
#[derive(Debug, Clone, PartialEq)]
pub struct Registered {
    /// 工具 id。
    pub tool_id: String,
    /// 登记版本。
    pub version: String,
    /// 规范化 Manifest JSON 的 sha256（tool_version.content_hash）。
    pub content_hash: String,
    /// 本次登记是否将旧 current 置为 superseded。
    pub superseded_previous: bool,
}

/// 在事务内落库一个已通过静态校验与第 2 条检查的 Manifest（第 9 条）。
/// 任一步失败整体回滚，不产生半登记状态。
pub fn register(pool: &Pool, vm: &ValidatedManifest) -> Result<Registered, RegistryError> {
    let now = now_iso();
    let input_schema = normalize_field(vm.manifest.get("input_schema"));
    let output_schema = normalize_field(vm.manifest.get("output_schema"));
    let provenance = normalize_field(vm.manifest.get("provenance"));
    let permission = vm
        .manifest
        .get("permission_level")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let timeout_s = vm
        .manifest
        .pointer("/resources/timeout_seconds")
        .and_then(Value::as_i64);
    let binary_ref = vm
        .manifest
        .pointer("/runtime/binary")
        .and_then(Value::as_str)
        .map(str::to_string);
    let name = vm
        .manifest
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    pool.with_tx(|c| {
        let prior_current: i64 = c.query_row(
            "SELECT COUNT(*) FROM tool_version WHERE tool_id = ?1 AND status = 'current'",
            [&vm.id],
            |r| r.get(0),
        )?;
        if prior_current > 0 {
            c.execute(
                "UPDATE tool_version SET status = 'superseded', updated_at = ?2
                 WHERE tool_id = ?1 AND status = 'current'",
                database::rusqlite::params![vm.id, now],
            )?;
        }

        let tool_exists: bool = {
            let mut stmt = c.prepare("SELECT 1 FROM tool WHERE tool_id = ?1")?;
            stmt.exists([&vm.id])?
        };
        if tool_exists {
            c.execute(
                "UPDATE tool SET name = ?2, kind = 'tool', lifecycle = 'VALIDATED',
                 permission_level = ?3, input_schema = ?4, output_schema = ?5,
                 timeout_s = ?6, binary_ref = ?7, updated_at = ?8
                 WHERE tool_id = ?1",
                database::rusqlite::params![
                    vm.id,
                    name,
                    permission,
                    input_schema,
                    output_schema,
                    timeout_s,
                    binary_ref,
                    now
                ],
            )?;
        } else {
            c.execute(
                "INSERT INTO tool (tool_id, name, kind, lifecycle, governance, install_state,
                 health_state, permission_level, input_schema, output_schema, timeout_s,
                 binary_ref, source_system, source_pk, created_at, updated_at, schema_version)
                 VALUES (?1, ?2, 'tool', 'VALIDATED', 'ACTIVE', NULL, NULL, ?3, ?4, ?5, ?6, ?7,
                 'tool-registry', ?1, ?8, ?8, 1)",
                database::rusqlite::params![
                    vm.id,
                    name,
                    permission,
                    input_schema,
                    output_schema,
                    timeout_s,
                    binary_ref,
                    now
                ],
            )?;
        }

        c.execute(
            "INSERT INTO tool_version (tool_id, version, provenance_json, content_hash,
             validated_at, status, created_at, updated_at, schema_version)
             VALUES (?1, ?2, ?3, ?4, ?5, 'current', ?5, ?5, 1)",
            database::rusqlite::params![vm.id, vm.version, provenance, vm.content_hash, now],
        )?;

        Ok::<Registered, database::rusqlite::Error>(Registered {
            tool_id: vm.id.clone(),
            version: vm.version.clone(),
            content_hash: vm.content_hash.clone(),
            superseded_previous: prior_current > 0,
        })
    })
    .map_err(RegistryError::from)
}

fn normalize_field(v: Option<&Value>) -> Option<String> {
    v.map(crate::validate::normalize_json)
}

/// 按 id 取 tool 行投影。
pub fn get_tool_row(pool: &Pool, tool_id: &str) -> Result<Option<ToolRow>, RegistryError> {
    pool.with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT tool_id, name, kind, lifecycle, governance, permission_level,
             input_schema, output_schema, timeout_s, source_system, created_at, updated_at
             FROM tool WHERE tool_id = ?1",
        )?;
        let mut rows = stmt.query_map([tool_id], |r| {
            Ok(ToolRow {
                tool_id: r.get(0)?,
                name: r.get(1)?,
                kind: r.get(2)?,
                lifecycle: r.get(3)?,
                governance: r.get(4)?,
                permission_level: r.get(5)?,
                input_schema: r.get(6)?,
                output_schema: r.get(7)?,
                timeout_s: r.get(8)?,
                source_system: r.get(9)?,
                created_at: r.get(10)?,
                updated_at: r.get(11)?,
            })
        })?;
        match rows.next() {
            Some(row) => Ok::<Option<ToolRow>, database::rusqlite::Error>(Some(row?)),
            None => Ok(None),
        }
    })
    .map_err(RegistryError::from)
}

/// tool 行投影（既有列子集）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolRow {
    /// 工具 id。
    pub tool_id: String,
    /// 名称。
    pub name: String,
    /// kind（恒 tool）。
    pub kind: String,
    /// lifecycle 五态（REGISTERED/VALIDATED/ENABLED/DISABLED/DEPRECATED）。
    pub lifecycle: String,
    /// governance 四态。
    pub governance: String,
    /// T0/T1/T2。
    pub permission_level: Option<String>,
    /// input_schema 列原文（JSON 文本）。
    pub input_schema: Option<String>,
    /// output_schema 列原文（JSON 文本）。
    pub output_schema: Option<String>,
    /// resources.timeout_s。
    pub timeout_s: Option<i64>,
    /// 来源系统（本 crate 固定 tool-registry）。
    pub source_system: String,
    /// 创建时间。
    pub created_at: String,
    /// 更新时间。
    pub updated_at: String,
}

/// 某工具的全部版本行（按登记顺序）。
pub fn list_versions(pool: &Pool, tool_id: &str) -> Result<Vec<VersionRow>, RegistryError> {
    pool.with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT version, status, content_hash, validated_at, provenance_json
             FROM tool_version WHERE tool_id = ?1 ORDER BY tv_id",
        )?;
        let rows = stmt.query_map([tool_id], |r| {
            Ok(VersionRow {
                version: r.get(0)?,
                status: r.get(1)?,
                content_hash: r.get(2)?,
                validated_at: r.get(3)?,
                provenance_json: r.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, database::rusqlite::Error>>()
    })
    .map_err(RegistryError::from)
}

/// tool_version 行投影。
#[derive(Debug, Clone, serde::Serialize)]
pub struct VersionRow {
    /// 版本字符串。
    pub version: String,
    /// current | superseded。
    pub status: String,
    /// 规范化 Manifest JSON 的 sha256。
    pub content_hash: String,
    /// 校验时间。
    pub validated_at: Option<String>,
    /// provenance JSON 原文。
    pub provenance_json: Option<String>,
}
