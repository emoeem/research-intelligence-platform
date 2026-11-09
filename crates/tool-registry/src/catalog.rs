//! 发现 / 查询 / Schema 获取接口（ROADMAP P1-4 判据：登记、查询、取 Schema
//! 全流程）。读路径只依赖 tool / tool_version 既有列；Manifest 全文不入库
//! （无 manifest_json 列，不新增列），可取回的登记面为 tool 行投影 +
//! 版本行（含 content_hash 与 provenance_json）。

use crate::error::RegistryError;
use crate::store::{self, ToolRow, VersionRow};
use database::Pool;
use serde_json::Value;

/// 发现结果条目（tool 行投影）。
#[derive(Debug, Clone)]
pub struct ToolSummary {
    /// 工具 id。
    pub tool_id: String,
    /// 名称。
    pub name: String,
    /// lifecycle 五态。
    pub lifecycle: String,
    /// governance 四态。
    pub governance: String,
    /// T0/T1/T2。
    pub permission_level: Option<String>,
}

/// 查询结果：tool 行 + 全部版本行。
#[derive(Debug, Clone)]
pub struct ToolDetail {
    /// tool 行投影。
    pub tool: ToolRow,
    /// 版本行（按登记顺序）。
    pub versions: Vec<VersionRow>,
    /// current 版本（无则 None）。
    pub current_version: Option<String>,
}

/// 发现：列出全部已登记工具（按 tool_id 排序）。
pub fn discover(pool: &Pool) -> Result<Vec<ToolSummary>, RegistryError> {
    pool.with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT tool_id, name, lifecycle, governance, permission_level
             FROM tool WHERE source_system = 'tool-registry' ORDER BY tool_id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ToolSummary {
                tool_id: r.get(0)?,
                name: r.get(1)?,
                lifecycle: r.get(2)?,
                governance: r.get(3)?,
                permission_level: r.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, database::rusqlite::Error>>()
    })
    .map_err(RegistryError::from)
}

/// 查询：按 id 取工具详情（含版本面）。找不到报 NotFound。
pub fn query(pool: &Pool, tool_id: &str) -> Result<ToolDetail, RegistryError> {
    let row = store::get_tool_row(pool, tool_id)?.ok_or_else(|| RegistryError::NotFound {
        tool_id: tool_id.to_string(),
        version: None,
    })?;
    let versions = store::list_versions(pool, tool_id)?;
    let current_version = versions
        .iter()
        .find(|v| v.status == "current")
        .map(|v| v.version.clone());
    Ok(ToolDetail {
        tool: row,
        versions,
        current_version,
    })
}

/// Schema 获取：返回 (input_schema, output_schema) 解析后的 JSON 文档。
/// `version` 缺省取 current 登记面；指定时须为该工具已登记版本
/// （列不随版本分叉，schema 存 tool 表当前登记面，如实返回）。
pub fn get_schemas(
    pool: &Pool,
    tool_id: &str,
    version: Option<&str>,
) -> Result<(Value, Value), RegistryError> {
    let row = store::get_tool_row(pool, tool_id)?.ok_or_else(|| RegistryError::NotFound {
        tool_id: tool_id.to_string(),
        version: version.map(str::to_string),
    })?;
    if let Some(v) = version {
        let known = store::list_versions(pool, tool_id)?
            .iter()
            .any(|vr| vr.version == v);
        if !known {
            return Err(RegistryError::NotFound {
                tool_id: tool_id.to_string(),
                version: Some(v.to_string()),
            });
        }
    }
    let input = row
        .input_schema
        .as_deref()
        .map(str::to_string)
        .ok_or_else(|| RegistryError::NotFound {
            tool_id: tool_id.to_string(),
            version: version.map(str::to_string),
        })?;
    let output = row
        .output_schema
        .as_deref()
        .map(str::to_string)
        .ok_or_else(|| RegistryError::NotFound {
            tool_id: tool_id.to_string(),
            version: version.map(str::to_string),
        })?;
    let input_value: Value = serde_json::from_str(&input).map_err(|e| {
        RegistryError::Storage(format!(
            "{tool_id} input_schema column is not valid JSON: {e}"
        ))
    })?;
    let output_value: Value = serde_json::from_str(&output).map_err(|e| {
        RegistryError::Storage(format!(
            "{tool_id} output_schema column is not valid JSON: {e}"
        ))
    })?;
    Ok((input_value, output_value))
}
