//! Independent Projects and Research Assets persistence for the RIP workbench.
//!
//! Project and Asset identities are separate from RTIS task mirrors, Knowledge
//! documents, document extraction assets, and immutable verification Evidence.
//! All writes use the shared SQLite Pool and parameterized SQL.
use database::{rusqlite::OptionalExtension, Pool};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainStoreError {
    NotFound(String),
    Invalid(String),
    Conflict(String),
    Db(String),
}
impl fmt::Display for DomainStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(v) => write!(f, "not found: {v}"),
            Self::Invalid(v) => write!(f, "invalid: {v}"),
            Self::Conflict(v) => write!(f, "conflict: {v}"),
            Self::Db(v) => write!(f, "database: {v}"),
        }
    }
}
impl From<database::DbError> for DomainStoreError {
    fn from(value: database::DbError) -> Self {
        Self::Db(value.to_string())
    }
}
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProjectRecord {
    pub project_id: String,
    pub workspace_id: String,
    pub name: String,
    pub slug: String,
    pub summary: String,
    pub objective: String,
    pub status: String,
    pub priority: String,
    pub scope: Value,
    pub tags: Value,
    pub owner_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AssetRecord {
    pub asset_id: String,
    pub workspace_id: String,
    pub asset_type: String,
    pub canonical_value: String,
    pub display_name: String,
    pub description: String,
    pub lifecycle_status: String,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ObservationRecord {
    pub observation_id: String,
    pub asset_id: String,
    pub endpoint_id: Option<String>,
    pub observation_type: String,
    pub source_type: String,
    pub source_ref: String,
    pub evidence_ref: String,
    pub observed_at: String,
    pub content_hash: String,
    pub payload: Value,
    pub created_at: String,
}
#[derive(Clone)]
pub struct ProjectAssetStore {
    pool: Pool,
}
impl ProjectAssetStore {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    fn id(prefix: &str) -> String {
        format!("{prefix}-{}", domain::time::now_compact_micros())
    }
    fn now() -> String {
        domain::time::now_iso()
    }
    fn validate_text(field: &str, value: &str, max: usize) -> Result<(), DomainStoreError> {
        let v = value.trim();
        if v.is_empty() || v.len() > max || v.chars().any(char::is_control) {
            return Err(DomainStoreError::Invalid(format!(
                "{field} must be 1..{max} printable bytes"
            )));
        }
        Ok(())
    }
    fn project_from_row(
        row: &database::rusqlite::Row<'_>,
    ) -> database::rusqlite::Result<ProjectRecord> {
        let scope: String = row.get(8)?;
        let tags: String = row.get(9)?;
        Ok(ProjectRecord {
            project_id: row.get(0)?,
            workspace_id: row.get(1)?,
            name: row.get(2)?,
            slug: row.get(3)?,
            summary: row.get(4)?,
            objective: row.get(5)?,
            status: row.get(6)?,
            priority: row.get(7)?,
            scope: serde_json::from_str(&scope).unwrap_or_else(|_| json!({})),
            tags: serde_json::from_str(&tags).unwrap_or_else(|_| json!([])),
            owner_id: row.get(10)?,
            created_at: row.get(11)?,
            updated_at: row.get(12)?,
            archived_at: row.get(13)?,
        })
    }
    fn asset_from_row(
        row: &database::rusqlite::Row<'_>,
    ) -> database::rusqlite::Result<AssetRecord> {
        Ok(AssetRecord {
            asset_id: row.get(0)?,
            workspace_id: row.get(1)?,
            asset_type: row.get(2)?,
            canonical_value: row.get(3)?,
            display_name: row.get(4)?,
            description: row.get(5)?,
            lifecycle_status: row.get(6)?,
            first_seen_at: row.get(7)?,
            last_seen_at: row.get(8)?,
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
        })
    }
    pub fn create_project(&self, input: &Value) -> Result<ProjectRecord, DomainStoreError> {
        let name = input
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let slug = input
            .get("slug")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        Self::validate_text("name", name, 160)?;
        Self::validate_text("slug", slug, 180)?;
        if !slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(DomainStoreError::Invalid(
                "slug must contain lowercase ASCII letters, digits, and hyphens".into(),
            ));
        }
        let workspace = input
            .get("workspace_id")
            .and_then(Value::as_str)
            .unwrap_or("default")
            .trim();
        Self::validate_text("workspace_id", workspace, 120)?;
        let status = input
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("draft");
        let priority = input
            .get("priority")
            .and_then(Value::as_str)
            .unwrap_or("normal");
        if !["draft", "active", "paused", "completed", "archived"].contains(&status) {
            return Err(DomainStoreError::Invalid(
                "unsupported project status".into(),
            ));
        }
        if !["low", "normal", "high", "critical"].contains(&priority) {
            return Err(DomainStoreError::Invalid(
                "unsupported project priority".into(),
            ));
        }
        let scope = input.get("scope").cloned().unwrap_or_else(|| json!({}));
        let tags = input.get("tags").cloned().unwrap_or_else(|| json!([]));
        if !scope.is_object() || !tags.is_array() {
            return Err(DomainStoreError::Invalid(
                "scope must be object and tags must be array".into(),
            ));
        }
        let now = Self::now();
        let id = Self::id("prj");
        let result = self.pool.with_conn(|c| c.execute(
            "INSERT INTO research_project(project_id,workspace_id,name,slug,summary,objective,status,priority,scope_json,tags_json,owner_id,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)",
            database::rusqlite::params![id,workspace,name,slug,input.get("summary").and_then(Value::as_str).unwrap_or(""),input.get("objective").and_then(Value::as_str).unwrap_or(""),status,priority,scope.to_string(),tags.to_string(),input.get("owner_id").and_then(Value::as_str).unwrap_or("owner-local"),now,now]
        ));
        match result {
            Ok(_) => self
                .get_project(&id)?
                .ok_or(DomainStoreError::NotFound(id)),
            Err(e) if e.to_string().contains("UNIQUE") => Err(DomainStoreError::Conflict(
                "project slug already exists in workspace".into(),
            )),
            Err(e) => Err(DomainStoreError::Db(e.to_string())),
        }
    }
    pub fn list_projects(
        &self,
        workspace: &str,
        status: Option<&str>,
        q: Option<&str>,
        limit: u32,
    ) -> Result<Vec<ProjectRecord>, DomainStoreError> {
        let limit = limit.clamp(1, 100) as i64;
        let needle = q.unwrap_or("").trim();
        self.pool.with_conn(|c| {
            let mut stmt = c.prepare("SELECT project_id,workspace_id,name,slug,summary,objective,status,priority,scope_json,tags_json,owner_id,created_at,updated_at,archived_at FROM research_project WHERE workspace_id=? AND (?='' OR status=?) AND (?='' OR name LIKE '%'||?||'%' OR slug LIKE '%'||?||'%') ORDER BY updated_at DESC LIMIT ?")?;
            let rows = stmt.query_map(database::rusqlite::params![workspace,status.unwrap_or(""),status.unwrap_or(""),needle,needle,needle,limit], Self::project_from_row)?;
            rows.collect::<Result<Vec<_>,_>>()
        }).map_err(|e| DomainStoreError::Db(e.to_string()))
    }
    pub fn get_project_in_workspace(
        &self,
        id: &str,
        workspace: &str,
    ) -> Result<Option<ProjectRecord>, DomainStoreError> {
        self.pool.with_conn(|c| c.query_row("SELECT project_id,workspace_id,name,slug,summary,objective,status,priority,scope_json,tags_json,owner_id,created_at,updated_at,archived_at FROM research_project WHERE project_id=? AND workspace_id=?", [id,workspace], Self::project_from_row).optional())
            .map_err(|e| DomainStoreError::Db(e.to_string()))
    }
    pub fn get_project(&self, id: &str) -> Result<Option<ProjectRecord>, DomainStoreError> {
        self.pool.with_conn(|c| c.query_row("SELECT project_id,workspace_id,name,slug,summary,objective,status,priority,scope_json,tags_json,owner_id,created_at,updated_at,archived_at FROM research_project WHERE project_id=?", [id], Self::project_from_row).optional())
            .map_err(|e| DomainStoreError::Db(e.to_string()))
    }
    pub fn update_project(
        &self,
        id: &str,
        workspace: &str,
        input: &Value,
    ) -> Result<ProjectRecord, DomainStoreError> {
        let current = self
            .get_project_in_workspace(id, workspace)?
            .ok_or_else(|| DomainStoreError::NotFound(id.into()))?;
        let name = input
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&current.name);
        Self::validate_text("name", name, 160)?;
        let status = input
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or(&current.status);
        let priority = input
            .get("priority")
            .and_then(Value::as_str)
            .unwrap_or(&current.priority);
        if !["draft", "active", "paused", "completed", "archived"].contains(&status)
            || !["low", "normal", "high", "critical"].contains(&priority)
        {
            return Err(DomainStoreError::Invalid(
                "unsupported status or priority".into(),
            ));
        }
        let scope = input.get("scope").cloned().unwrap_or(current.scope);
        let tags = input.get("tags").cloned().unwrap_or(current.tags);
        if !scope.is_object() || !tags.is_array() {
            return Err(DomainStoreError::Invalid(
                "scope must be object and tags must be array".into(),
            ));
        }
        let now = Self::now();
        self.pool.with_conn(|c| c.execute("UPDATE research_project SET name=?,summary=?,objective=?,status=?,priority=?,scope_json=?,tags_json=?,updated_at=?,archived_at=CASE WHEN ?='archived' THEN COALESCE(archived_at,?) ELSE NULL END WHERE project_id=? AND workspace_id=?",
            database::rusqlite::params![name,input.get("summary").and_then(Value::as_str).unwrap_or(&current.summary),input.get("objective").and_then(Value::as_str).unwrap_or(&current.objective),status,priority,scope.to_string(),tags.to_string(),now,status,now,id,workspace]))
            .map_err(|e| DomainStoreError::Db(e.to_string()))?;
        self.get_project_in_workspace(id, workspace)?
            .ok_or_else(|| DomainStoreError::NotFound(id.into()))
    }
    pub fn link_task(&self, project: &str, task: &str, role: &str) -> Result<(), DomainStoreError> {
        if !["research", "validation", "monitoring"].contains(&role) {
            return Err(DomainStoreError::Invalid(
                "unsupported task relation role".into(),
            ));
        }
        let now = Self::now();
        self.pool.with_conn(|c| c.execute("INSERT INTO research_project_task(project_id,task_id,relation_role,linked_at) VALUES(?,?,?,?) ON CONFLICT(project_id,task_id) DO UPDATE SET relation_role=excluded.relation_role", database::rusqlite::params![project,task,role,now]))
            .map_err(|e| DomainStoreError::Db(e.to_string()))?;
        Ok(())
    }
    pub fn unlink_task(&self, project: &str, task: &str) -> Result<(), DomainStoreError> {
        self.pool
            .with_conn(|c| {
                c.execute(
                    "DELETE FROM research_project_task WHERE project_id=? AND task_id=?",
                    [project, task],
                )
            })
            .map_err(|e| DomainStoreError::Db(e.to_string()))?;
        Ok(())
    }
    pub fn project_tasks(&self, project: &str) -> Result<Vec<Value>, DomainStoreError> {
        self.pool.with_conn(|c| {
            let mut stmt = c.prepare("SELECT t.task_id,t.title,t.objective,t.target,t.task_type,t.state,pt.relation_role,pt.linked_at FROM research_project_task pt JOIN research_task t ON t.task_id=pt.task_id WHERE pt.project_id=? ORDER BY pt.linked_at DESC")?;
            let rows = stmt.query_map([project], |r| Ok(json!({
                "task_id": r.get::<_,String>(0)?, "title": r.get::<_,Option<String>>(1)?,
                "objective": r.get::<_,String>(2)?, "target": r.get::<_,Option<String>>(3)?,
                "task_type": r.get::<_,Option<String>>(4)?, "state": r.get::<_,String>(5)?,
                "relation_role": r.get::<_,String>(6)?, "linked_at": r.get::<_,String>(7)?
            })))?;
            rows.collect::<Result<Vec<_>,_>>()
        }).map_err(|e| DomainStoreError::Db(e.to_string()))
    }
    pub fn link_asset(
        &self,
        project: &str,
        asset: &str,
        role: &str,
    ) -> Result<(), DomainStoreError> {
        if !["in_scope", "out_of_scope", "watchlist"].contains(&role) {
            return Err(DomainStoreError::Invalid(
                "unsupported project asset relation role".into(),
            ));
        }
        let now = Self::now();
        self.pool.with_conn(|c| c.execute(
            "INSERT INTO research_project_asset(project_id,asset_id,relation_role,linked_at) VALUES(?,?,?,?) ON CONFLICT(project_id,asset_id) DO UPDATE SET relation_role=excluded.relation_role",
            database::rusqlite::params![project,asset,role,now]
        )).map_err(|e| DomainStoreError::Db(e.to_string()))?;
        Ok(())
    }
    pub fn unlink_asset(&self, project: &str, asset: &str) -> Result<(), DomainStoreError> {
        self.pool
            .with_conn(|c| {
                c.execute(
                    "DELETE FROM research_project_asset WHERE project_id=? AND asset_id=?",
                    [project, asset],
                )
            })
            .map_err(|e| DomainStoreError::Db(e.to_string()))?;
        Ok(())
    }
    pub fn project_assets(&self, project: &str) -> Result<Vec<AssetRecord>, DomainStoreError> {
        self.pool.with_conn(|c| {
            let mut stmt = c.prepare("SELECT a.asset_id,a.workspace_id,a.asset_type,a.canonical_value,a.display_name,a.description,a.lifecycle_status,a.first_seen_at,a.last_seen_at,a.created_at,a.updated_at FROM research_project_asset pa JOIN research_asset a ON a.asset_id=pa.asset_id WHERE pa.project_id=? ORDER BY pa.linked_at DESC")?;
            let rows = stmt.query_map([project], Self::asset_from_row)?;
            rows.collect::<Result<Vec<_>,_>>()
        }).map_err(|e| DomainStoreError::Db(e.to_string()))
    }

    pub fn create_asset(&self, input: &Value) -> Result<AssetRecord, DomainStoreError> {
        let kind = input
            .get("asset_type")
            .and_then(Value::as_str)
            .unwrap_or("");
        if ![
            "domain",
            "ip",
            "url",
            "service",
            "repository",
            "application",
            "other",
        ]
        .contains(&kind)
        {
            return Err(DomainStoreError::Invalid("unsupported asset_type".into()));
        }
        let canonical = input
            .get("canonical_value")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        Self::validate_text("canonical_value", canonical, 2048)?;
        let workspace = input
            .get("workspace_id")
            .and_then(Value::as_str)
            .unwrap_or("default")
            .trim();
        Self::validate_text("workspace_id", workspace, 120)?;
        let display = input
            .get("display_name")
            .and_then(Value::as_str)
            .unwrap_or("");
        let desc = input
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("");
        let now = Self::now();
        let id = Self::id("ast");
        let obs = Self::id("obs");
        let payload =
            json!({"asset_type":kind,"canonical_value":canonical,"source":"manual_import"});
        let hash = format!("{:x}", Sha256::digest(payload.to_string().as_bytes()));
        let inserted=self.pool.with_conn(|c| {
            c.execute("INSERT INTO research_asset(asset_id,workspace_id,asset_type,canonical_value,display_name,description,first_seen_at,last_seen_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)",
                database::rusqlite::params![id,workspace,kind,canonical,display,desc,now,now,now,now])?;
            c.execute("INSERT INTO research_asset_observation(observation_id,asset_id,observation_type,source_type,source_ref,evidence_ref,observed_at,content_hash,payload_json,created_at) VALUES(?,?,'manual_import','user_import','manual-ui','',?,?,?,?)",
                database::rusqlite::params![obs,id,now,hash,payload.to_string(),now])?;
            Ok::<(),database::rusqlite::Error>(())
        });
        match inserted {
            Ok(()) => self
                .get_asset(&id)?
                .ok_or(DomainStoreError::NotFound(id)),
            Err(e) if e.to_string().contains("UNIQUE") => Err(DomainStoreError::Conflict(
                "asset already exists in workspace".into(),
            )),
            Err(e) => Err(DomainStoreError::Db(e.to_string())),
        }
    }
    pub fn list_assets(
        &self,
        workspace: &str,
        kind: Option<&str>,
        q: Option<&str>,
        limit: u32,
    ) -> Result<Vec<AssetRecord>, DomainStoreError> {
        let needle = q.unwrap_or("").trim();
        let limit = limit.clamp(1, 100) as i64;
        self.pool.with_conn(|c| {
            let mut stmt=c.prepare("SELECT asset_id,workspace_id,asset_type,canonical_value,display_name,description,lifecycle_status,first_seen_at,last_seen_at,created_at,updated_at FROM research_asset WHERE workspace_id=? AND (?='' OR asset_type=?) AND (?='' OR canonical_value LIKE '%'||?||'%' OR display_name LIKE '%'||?||'%') ORDER BY last_seen_at DESC LIMIT ?")?;
            let rows=stmt.query_map(database::rusqlite::params![workspace,kind.unwrap_or(""),kind.unwrap_or(""),needle,needle,needle,limit],Self::asset_from_row)?;
            rows.collect::<Result<Vec<_>,_>>()
        }).map_err(|e| DomainStoreError::Db(e.to_string()))
    }
    pub fn get_asset_in_workspace(
        &self,
        id: &str,
        workspace: &str,
    ) -> Result<Option<AssetRecord>, DomainStoreError> {
        self.pool.with_conn(|c|c.query_row("SELECT asset_id,workspace_id,asset_type,canonical_value,display_name,description,lifecycle_status,first_seen_at,last_seen_at,created_at,updated_at FROM research_asset WHERE asset_id=? AND workspace_id=?",[id,workspace],Self::asset_from_row).optional())
            .map_err(|e|DomainStoreError::Db(e.to_string()))
    }
    pub fn get_asset(&self, id: &str) -> Result<Option<AssetRecord>, DomainStoreError> {
        self.pool.with_conn(|c|c.query_row("SELECT asset_id,workspace_id,asset_type,canonical_value,display_name,description,lifecycle_status,first_seen_at,last_seen_at,created_at,updated_at FROM research_asset WHERE asset_id=?",[id],Self::asset_from_row).optional())
            .map_err(|e|DomainStoreError::Db(e.to_string()))
    }
    pub fn endpoints(&self, id: &str) -> Result<Vec<Value>, DomainStoreError> {
        self.pool.with_conn(|c| {
            let mut stmt = c.prepare("SELECT endpoint_id,asset_id,scheme,host,port,protocol,path,lifecycle_status,first_seen_at,last_seen_at FROM research_asset_endpoint WHERE asset_id=? ORDER BY last_seen_at DESC")?;
            let rows = stmt.query_map([id], |r| Ok(json!({
                "endpoint_id":r.get::<_,String>(0)?,"asset_id":r.get::<_,String>(1)?,
                "scheme":r.get::<_,String>(2)?,"host":r.get::<_,String>(3)?,
                "port":r.get::<_,Option<u16>>(4)?,"protocol":r.get::<_,String>(5)?,
                "path":r.get::<_,String>(6)?,"lifecycle_status":r.get::<_,String>(7)?,
                "first_seen_at":r.get::<_,String>(8)?,"last_seen_at":r.get::<_,String>(9)?
            })))?;
            rows.collect::<Result<Vec<_>,_>>()
        }).map_err(|e| DomainStoreError::Db(e.to_string()))
    }
    pub fn technologies(&self, id: &str) -> Result<Vec<Value>, DomainStoreError> {
        self.pool.with_conn(|c| {
            let mut stmt = c.prepare("SELECT technology_id,endpoint_id,observation_id,technology,version,category,confidence,evidence_ref,first_seen_at,last_seen_at FROM research_asset_technology WHERE asset_id=? ORDER BY confidence DESC,last_seen_at DESC")?;
            let rows = stmt.query_map([id], |r| Ok(json!({
                "technology_id":r.get::<_,String>(0)?,"endpoint_id":r.get::<_,Option<String>>(1)?,
                "observation_id":r.get::<_,String>(2)?,"technology":r.get::<_,String>(3)?,
                "version":r.get::<_,String>(4)?,"category":r.get::<_,String>(5)?,
                "confidence":r.get::<_,f64>(6)?,"evidence_ref":r.get::<_,String>(7)?,
                "first_seen_at":r.get::<_,String>(8)?,"last_seen_at":r.get::<_,String>(9)?
            })))?;
            rows.collect::<Result<Vec<_>,_>>()
        }).map_err(|e| DomainStoreError::Db(e.to_string()))
    }
    pub fn add_endpoint(&self, asset: &str, input: &Value) -> Result<Value, DomainStoreError> {
        if self.get_asset(asset)?.is_none() {
            return Err(DomainStoreError::NotFound(asset.into()));
        }
        let host = input
            .get("host")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        Self::validate_text("host", host, 255)?;
        let port = input.get("port").and_then(Value::as_u64);
        if port.is_some_and(|v| !(1..=65535).contains(&v)) {
            return Err(DomainStoreError::Invalid("port must be 1..65535".into()));
        }
        let scheme = input.get("scheme").and_then(Value::as_str).unwrap_or("");
        let protocol = input
            .get("protocol")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let path = input.get("path").and_then(Value::as_str).unwrap_or("/");
        let now = Self::now();
        let id = Self::id("end");
        self.pool.with_conn(|c|{
            c.execute("INSERT INTO research_asset_endpoint(endpoint_id,asset_id,scheme,host,port,protocol,path,first_seen_at,last_seen_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
                database::rusqlite::params![id,asset,scheme,host,port,protocol,path,now,now,now,now])?;
            c.execute("UPDATE research_asset SET last_seen_at=?,updated_at=? WHERE asset_id=?",
                database::rusqlite::params![now,now,asset])?;
            Ok::<(),database::rusqlite::Error>(())
        }).map_err(|e| if e.to_string().contains("UNIQUE") { DomainStoreError::Conflict("endpoint already exists".into()) } else { DomainStoreError::Db(e.to_string()) })?;
        Ok(
            json!({"endpoint_id":id,"asset_id":asset,"scheme":scheme,"host":host,"port":port,"protocol":protocol,"path":path,"first_seen_at":now,"last_seen_at":now}),
        )
    }
    pub fn add_observation(
        &self,
        asset: &str,
        input: &Value,
    ) -> Result<ObservationRecord, DomainStoreError> {
        if self.get_asset(asset)?.is_none() {
            return Err(DomainStoreError::NotFound(asset.into()));
        }
        let kind = input
            .get("observation_type")
            .and_then(Value::as_str)
            .unwrap_or("other");
        if ![
            "discovery",
            "http_response",
            "tls",
            "service_probe",
            "technology_fingerprint",
            "manual_import",
            "other",
        ]
        .contains(&kind)
        {
            return Err(DomainStoreError::Invalid(
                "unsupported observation_type".into(),
            ));
        }
        let source = input
            .get("source_type")
            .and_then(Value::as_str)
            .unwrap_or("manual");
        if ![
            "provider",
            "tool_run",
            "user_import",
            "manual",
            "external_feed",
        ]
        .contains(&source)
        {
            return Err(DomainStoreError::Invalid("unsupported source_type".into()));
        }
        let payload = input.get("payload").cloned().unwrap_or_else(|| json!({}));
        if !payload.is_object() {
            return Err(DomainStoreError::Invalid(
                "payload must be a JSON object".into(),
            ));
        }
        let endpoint = input.get("endpoint_id").and_then(Value::as_str);
        if let Some(endpoint_id) = endpoint {
            let belongs =
                self.pool
                    .with_conn(|c| {
                        c.query_row(
                "SELECT 1 FROM research_asset_endpoint WHERE endpoint_id=? AND asset_id=?",
                database::rusqlite::params![endpoint_id,asset], |_| Ok(())
            ).optional()
                    })
                    .map_err(|e| DomainStoreError::Db(e.to_string()))?;
            if belongs.is_none() {
                return Err(DomainStoreError::Invalid(
                    "endpoint_id must belong to this asset".into(),
                ));
            }
        }
        let now = Self::now();
        let id = Self::id("obs");
        let hash = format!("{:x}", Sha256::digest(payload.to_string().as_bytes()));
        self.pool.with_conn(|c|{
            c.execute("INSERT INTO research_asset_observation(observation_id,asset_id,endpoint_id,observation_type,source_type,source_ref,evidence_ref,observed_at,content_hash,payload_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
                database::rusqlite::params![id,asset,endpoint,kind,source,input.get("source_ref").and_then(Value::as_str).unwrap_or(""),input.get("evidence_ref").and_then(Value::as_str).unwrap_or(""),input.get("observed_at").and_then(Value::as_str).unwrap_or(&now),hash,payload.to_string(),now])?;
            c.execute("UPDATE research_asset SET last_seen_at=?,updated_at=? WHERE asset_id=?",
                database::rusqlite::params![now,now,asset])?;
            Ok::<(),database::rusqlite::Error>(())
        }).map_err(|e|DomainStoreError::Db(e.to_string()))?;
        Ok(ObservationRecord {
            observation_id: id,
            asset_id: asset.into(),
            endpoint_id: endpoint.map(str::to_owned),
            observation_type: kind.into(),
            source_type: source.into(),
            source_ref: input
                .get("source_ref")
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
            evidence_ref: input
                .get("evidence_ref")
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
            observed_at: input
                .get("observed_at")
                .and_then(Value::as_str)
                .unwrap_or(&now)
                .into(),
            content_hash: hash,
            payload,
            created_at: now,
        })
    }
    pub fn add_technology(&self, asset: &str, input: &Value) -> Result<Value, DomainStoreError> {
        let technology = input
            .get("technology")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        Self::validate_text("technology", technology, 160)?;
        let observation = input
            .get("observation_id")
            .and_then(Value::as_str)
            .unwrap_or("");
        let evidence = input
            .get("evidence_ref")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        Self::validate_text("evidence_ref", evidence, 1024)?;
        let confidence = input
            .get("confidence")
            .and_then(Value::as_f64)
            .unwrap_or(0.5);
        if !(0.0..=1.0).contains(&confidence) {
            return Err(DomainStoreError::Invalid(
                "confidence must be between 0 and 1".into(),
            ));
        }
        let version = input.get("version").and_then(Value::as_str).unwrap_or("");
        let category = input
            .get("category")
            .and_then(Value::as_str)
            .unwrap_or("application");
        let id = Self::id("tech");
        let now = Self::now();
        self.pool.with_conn(|c|{
            let exists:Option<String>=c.query_row("SELECT observation_id FROM research_asset_observation WHERE observation_id=? AND asset_id=?",
                database::rusqlite::params![observation,asset],|r|r.get(0)).optional()?;
            if exists.is_none() { return Err(database::rusqlite::Error::QueryReturnedNoRows); }
            c.execute("INSERT INTO research_asset_technology(technology_id,asset_id,endpoint_id,observation_id,technology,version,category,confidence,evidence_ref,first_seen_at,last_seen_at) VALUES(?,?,(SELECT endpoint_id FROM research_asset_observation WHERE observation_id=?),?,?,?,?,?,?,?,?)",
                database::rusqlite::params![id,asset,observation,observation,technology,version,category,confidence,evidence,now,now])?;
            Ok::<(),database::rusqlite::Error>(())
        }).map_err(|e|DomainStoreError::Db(e.to_string()))?;
        Ok(
            json!({"technology_id":id,"asset_id":asset,"observation_id":observation,"technology":technology,"version":version,"category":category,"confidence":confidence,"evidence_ref":evidence,"first_seen_at":now,"last_seen_at":now}),
        )
    }

    pub fn observations(&self, id: &str) -> Result<Vec<ObservationRecord>, DomainStoreError> {
        self.pool.with_conn(|c|{
            let mut stmt=c.prepare("SELECT observation_id,asset_id,endpoint_id,observation_type,source_type,source_ref,evidence_ref,observed_at,content_hash,payload_json,created_at FROM research_asset_observation WHERE asset_id=? ORDER BY observed_at DESC LIMIT 100")?;
            let rows=stmt.query_map([id],|r|{let p:String=r.get(9)?;Ok(ObservationRecord{observation_id:r.get(0)?,asset_id:r.get(1)?,endpoint_id:r.get(2)?,observation_type:r.get(3)?,source_type:r.get(4)?,source_ref:r.get(5)?,evidence_ref:r.get(6)?,observed_at:r.get(7)?,content_hash:r.get(8)?,payload:serde_json::from_str(&p).unwrap_or_else(|_|json!({})),created_at:r.get(10)?})})?;
            rows.collect::<Result<Vec<_>,_>>()
        }).map_err(|e|DomainStoreError::Db(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::{migrate, DbPath, PoolConfig};
    use std::time::Duration;

    fn store() -> ProjectAssetStore {
        let pool = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(2),
        })
        .unwrap();
        let migrations = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
        migrate(&pool, &migrations).unwrap();
        ProjectAssetStore::new(pool)
    }

    #[test]
    fn projects_and_assets_are_independent_and_observations_are_immutable() {
        let store = store();
        let project = store
            .create_project(&json!({
                "name":"Kernel review", "slug":"kernel-review", "workspace_id":"test",
                "objective":"Review reachable kernel interfaces", "tags":["kernel"]
            }))
            .unwrap();
        assert_eq!(project.status, "draft");
        assert!(store
            .get_project_in_workspace(&project.project_id, "other")
            .unwrap()
            .is_none());
        assert!(store
            .update_project(&project.project_id, "other", &json!({"status":"archived"}))
            .is_err());
        assert_eq!(
            store
                .list_projects("test", None, Some("kernel"), 10)
                .unwrap()
                .len(),
            1
        );
        assert!(matches!(
            store.create_project(&json!({
                "name":"Duplicate", "slug":"kernel-review", "workspace_id":"test"
            })),
            Err(DomainStoreError::Conflict(_))
        ));
        let asset = store
            .create_asset(&json!({
                "workspace_id":"test", "asset_type":"domain", "canonical_value":"example.test"
            }))
            .unwrap();
        assert_eq!(asset.canonical_value, "example.test");
        assert!(store
            .get_asset_in_workspace(&asset.asset_id, "other")
            .unwrap()
            .is_none());
        let observations = store.observations(&asset.asset_id).unwrap();
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].source_type, "user_import");
        let endpoint = store
            .add_endpoint(
                &asset.asset_id,
                &json!({
                    "scheme":"https","host":"example.test","port":443,"protocol":"https","path":"/"
                }),
            )
            .unwrap();
        let extra_observation = store.add_observation(&asset.asset_id, &json!({
            "observation_type":"http_response","source_type":"manual","source_ref":"test-fixture",
            "evidence_ref":"evidence:test-1","endpoint_id":endpoint["endpoint_id"],
            "payload":{"status":200,"server":"fixture"}
        })).unwrap();
        let technology = store
            .add_technology(
                &asset.asset_id,
                &json!({
                    "observation_id":extra_observation.observation_id,"technology":"ExampleServer",
                    "version":"1.2","confidence":0.95,"evidence_ref":"evidence:test-1"
                }),
            )
            .unwrap();
        assert_eq!(technology["technology"], "ExampleServer");
        assert_eq!(store.endpoints(&asset.asset_id).unwrap().len(), 1);
        assert_eq!(store.technologies(&asset.asset_id).unwrap().len(), 1);
        store
            .link_asset(&project.project_id, &asset.asset_id, "in_scope")
            .unwrap();
        assert_eq!(store.project_assets(&project.project_id).unwrap().len(), 1);
        store.pool.with_conn(|c| c.execute(
            "INSERT INTO research_task(task_id,objective,state,status_history,source_file,source_system,created_at,updated_at) VALUES('rtis-test','Test task','CREATED','[]','test.json','rtis-tasks','now','now')", []
        )).unwrap();
        store
            .link_task(&project.project_id, "rtis-test", "research")
            .unwrap();
        assert_eq!(store.project_tasks(&project.project_id).unwrap().len(), 1);
        let blocked = store.pool.with_conn(|c| {
            c.execute(
                "UPDATE research_asset_observation SET payload_json='{}' WHERE observation_id=?",
                [&observations[0].observation_id],
            )
        });
        assert!(blocked.is_err(), "observation update must be rejected");
        store
            .update_project(&project.project_id, "test", &json!({"status":"active"}))
            .unwrap();
        assert_eq!(
            store
                .get_project(&project.project_id)
                .unwrap()
                .unwrap()
                .status,
            "active"
        );
    }

    #[test]
    fn rejects_invalid_asset_type_and_project_slug() {
        let store = store();
        assert!(matches!(
            store.create_project(&json!({"name":"X","slug":"Bad Slug"})),
            Err(DomainStoreError::Invalid(_))
        ));
        assert!(matches!(
            store.create_asset(&json!({"asset_type":"guess","canonical_value":"x"})),
            Err(DomainStoreError::Invalid(_))
        ));
    }
}
