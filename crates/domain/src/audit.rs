//! AuditEvent 纯数据类型（audit-engine 的 Emit 载荷，全实体多态指向）。
//!
//! 口径（UNIFIED_DOMAIN_MODEL §7.15）：只追加，不可改；correlation_id 打通
//! "任务-查询-工具-证据" 链。物理表为 audit_event（migrations/001_init.sql）。

/// 审计事件（纯数据，写入由 audit-engine 唯一出口 Emit 承担）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    /// 事件时间（UTC ISO8601）。
    pub ts: String,
    /// 执行者（agent_id / user / system）。
    pub actor: String,
    /// 动作（标准动词表，M2 阶段沿用调用方传入值）。
    pub action: String,
    /// 目标实体类型（多态引用）。
    pub entity_type: Option<String>,
    /// 目标实体 ID。
    pub entity_id: Option<String>,
    /// 关联 ID（任务-查询-工具-证据链）。
    pub correlation_id: Option<String>,
    /// 明细（已序列化 JSON 文本，建议）。
    pub detail: Option<String>,
    /// 事件发生时的权限级（T0/T1/T2）。
    pub permission_level: Option<String>,
    /// 来源系统标识。
    pub source_system: Option<String>,
    /// 来源系统主键（保真线索）。
    pub source_pk: Option<String>,
}

impl AuditEvent {
    /// 构造事件，ts 取当前 UTC 时间。
    pub fn new(actor: impl Into<String>, action: impl Into<String>) -> Self {
        Self {
            ts: crate::time::now_iso(),
            actor: actor.into(),
            action: action.into(),
            entity_type: None,
            entity_id: None,
            correlation_id: None,
            detail: None,
            permission_level: None,
            source_system: None,
            source_pk: None,
        }
    }

    /// 设置目标实体（多态引用）。
    pub fn with_entity(mut self, entity_type: &str, entity_id: &str) -> Self {
        self.entity_type = Some(entity_type.to_string());
        self.entity_id = Some(entity_id.to_string());
        self
    }

    /// 设置关联 ID。
    pub fn with_correlation(mut self, correlation_id: &str) -> Self {
        self.correlation_id = Some(correlation_id.to_string());
        self
    }

    /// 设置明细文本（建议为已序列化 JSON）。
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// 设置来源系统与主键。
    pub fn with_source(mut self, source_system: &str, source_pk: &str) -> Self {
        self.source_system = Some(source_system.to_string());
        self.source_pk = Some(source_pk.to_string());
        self
    }

    /// 校验必填字段（actor / action 非空）。
    pub fn validate(&self) -> Result<(), crate::error::DomainError> {
        if self.actor.trim().is_empty() {
            return Err(crate::error::DomainError::InvalidAuditEvent(
                "actor must not be empty".to_string(),
            ));
        }
        if self.action.trim().is_empty() {
            return Err(crate::error::DomainError::InvalidAuditEvent(
                "action must not be empty".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_and_validation() {
        let e = AuditEvent::new("system", "resource.resolved")
            .with_entity("resource", "gh:o/r")
            .with_correlation("erq-x")
            .with_detail(r#"{"k":1}"#)
            .with_source("resource-engine", "erq-x");
        assert!(e.validate().is_ok());
        assert_eq!(e.entity_id.as_deref(), Some("gh:o/r"));
        assert!(AuditEvent::new("", "a").validate().is_err());
        assert!(AuditEvent::new("a", "").validate().is_err());
    }
}
