use serde::Serialize;

use crate::extractor::ClientInfo;

/// An audit event to be recorded.
///
/// Constructed with four required fields (actor, action, resource_type,
/// resource_id) and optional builder methods for metadata, client context,
/// and tenant.
///
/// ```
/// use modo::audit::AuditEntry;
///
/// let entry = AuditEntry::new("user_123", "user.role.changed", "user", "usr_abc")
///     .metadata(serde_json::json!({"old_role": "editor"}))
///     .tenant_id("tenant_1");
/// ```
#[derive(Debug, Clone)]
pub struct AuditEntry {
    actor: String,
    action: String,
    resource_type: String,
    resource_id: String,
    metadata: Option<serde_json::Value>,
    client_info: Option<ClientInfo>,
    tenant_id: Option<String>,
}

impl AuditEntry {
    pub fn new(
        actor: impl Into<String>,
        action: impl Into<String>,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
    ) -> Self {
        Self {
            actor: actor.into(),
            action: action.into(),
            resource_type: resource_type.into(),
            resource_id: resource_id.into(),
            metadata: None,
            client_info: None,
            tenant_id: None,
        }
    }

    /// Serialize any type into the metadata JSON field.
    ///
    /// # Panics
    ///
    /// Panics if `meta` cannot be serialized to JSON. This is intentional —
    /// silently dropping metadata in an audit system would cause data loss.
    pub fn metadata(mut self, meta: impl Serialize) -> Self {
        self.metadata =
            Some(serde_json::to_value(meta).expect("audit metadata must be serializable to JSON"));
        self
    }

    /// Attach client context (IP, user-agent, fingerprint).
    pub fn client_info(mut self, info: ClientInfo) -> Self {
        self.client_info = Some(info);
        self
    }

    /// Set tenant ID for multi-tenant apps.
    pub fn tenant_id(mut self, id: impl Into<String>) -> Self {
        self.tenant_id = Some(id.into());
        self
    }

    pub fn actor(&self) -> &str {
        &self.actor
    }

    pub fn action(&self) -> &str {
        &self.action
    }

    pub fn resource_type(&self) -> &str {
        &self.resource_type
    }

    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }

    pub fn metadata_value(&self) -> Option<&serde_json::Value> {
        self.metadata.as_ref()
    }

    pub fn client_info_value(&self) -> Option<&ClientInfo> {
        self.client_info.as_ref()
    }

    pub fn tenant_id_value(&self) -> Option<&str> {
        self.tenant_id.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_sets_required_fields() {
        let entry = AuditEntry::new("user_123", "user.created", "user", "usr_abc");
        assert_eq!(entry.actor(), "user_123");
        assert_eq!(entry.action(), "user.created");
        assert_eq!(entry.resource_type(), "user");
        assert_eq!(entry.resource_id(), "usr_abc");
        assert!(entry.metadata_value().is_none());
        assert!(entry.client_info_value().is_none());
        assert!(entry.tenant_id_value().is_none());
    }

    #[test]
    fn metadata_with_json_value() {
        let entry = AuditEntry::new("user_123", "user.role.changed", "user", "usr_abc")
            .metadata(serde_json::json!({"old_role": "editor", "new_role": "admin"}));
        let meta = entry.metadata_value().unwrap();
        assert_eq!(meta["old_role"], "editor");
        assert_eq!(meta["new_role"], "admin");
    }

    #[test]
    fn metadata_with_serializable_struct() {
        #[derive(serde::Serialize)]
        struct RoleChange {
            old_role: String,
            new_role: String,
        }

        let entry = AuditEntry::new("user_123", "user.role.changed", "user", "usr_abc").metadata(
            RoleChange {
                old_role: "editor".into(),
                new_role: "admin".into(),
            },
        );
        let meta = entry.metadata_value().unwrap();
        assert_eq!(meta["old_role"], "editor");
        assert_eq!(meta["new_role"], "admin");
    }

    #[test]
    fn client_info_attached() {
        use crate::extractor::ClientInfo;

        let info = ClientInfo::new().ip("1.2.3.4").user_agent("Bot/1.0");
        let entry = AuditEntry::new("system", "job.ran", "job", "job_1").client_info(info);
        let ci = entry.client_info_value().unwrap();
        assert_eq!(ci.ip.as_deref(), Some("1.2.3.4"));
        assert_eq!(ci.user_agent.as_deref(), Some("Bot/1.0"));
    }

    #[test]
    fn tenant_id_set() {
        let entry =
            AuditEntry::new("user_123", "doc.deleted", "document", "doc_1").tenant_id("tenant_abc");
        assert_eq!(entry.tenant_id_value(), Some("tenant_abc"));
    }
}
