use crate::client::ClientInfo;

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
    /// Create a new audit entry with four required fields.
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

    /// Attach metadata to the audit entry.
    ///
    /// Accepts a [`serde_json::Value`] — use [`serde_json::json!`] at the
    /// call site for structured data, or [`serde_json::to_value`] for custom
    /// types.
    pub fn metadata(mut self, meta: serde_json::Value) -> Self {
        self.metadata = Some(meta);
        self
    }

    /// Attach client context (IP, user-agent, parsed device fields, fingerprint).
    pub fn client_info(mut self, info: ClientInfo) -> Self {
        self.client_info = Some(info);
        self
    }

    /// Set tenant ID for multi-tenant apps.
    pub fn tenant_id(mut self, id: impl Into<String>) -> Self {
        self.tenant_id = Some(id.into());
        self
    }

    /// Returns the actor who performed the action.
    pub fn actor(&self) -> &str {
        &self.actor
    }

    /// Returns the action identifier (e.g. `"user.role.changed"`).
    pub fn action(&self) -> &str {
        &self.action
    }

    /// Returns the resource type (e.g. `"user"`, `"document"`).
    pub fn resource_type(&self) -> &str {
        &self.resource_type
    }

    /// Returns the resource identifier.
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }

    /// Returns the optional metadata JSON value.
    pub fn metadata_value(&self) -> Option<&serde_json::Value> {
        self.metadata.as_ref()
    }

    /// Returns the optional client context.
    pub fn client_info_value(&self) -> Option<&ClientInfo> {
        self.client_info.as_ref()
    }

    /// Returns the optional tenant ID.
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
            serde_json::to_value(RoleChange {
                old_role: "editor".into(),
                new_role: "admin".into(),
            })
            .unwrap(),
        );
        let meta = entry.metadata_value().unwrap();
        assert_eq!(meta["old_role"], "editor");
        assert_eq!(meta["new_role"], "admin");
    }

    #[test]
    fn client_info_attached() {
        use crate::client::ClientInfo;

        let info = ClientInfo::new().ip("1.2.3.4").user_agent("Bot/1.0");
        let entry = AuditEntry::new("system", "job.ran", "job", "job_1").client_info(info);
        let ci = entry.client_info_value().unwrap();
        assert_eq!(ci.ip_value(), Some("1.2.3.4"));
        assert_eq!(ci.user_agent_value(), Some("Bot/1.0"));
    }

    #[test]
    fn tenant_id_set() {
        let entry =
            AuditEntry::new("user_123", "doc.deleted", "document", "doc_1").tenant_id("tenant_abc");
        assert_eq!(entry.tenant_id_value(), Some("tenant_abc"));
    }
}
