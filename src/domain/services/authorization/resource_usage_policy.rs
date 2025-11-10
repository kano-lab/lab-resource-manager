use super::policy::{AuthorizationError, AuthorizationPolicy};
use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::common::EmailAddress;

/// ResourceUsageの認可ポリシー
///
/// 所有者（owner）のみが更新・削除できるシンプルなポリシー
#[derive(Debug, Clone, Default)]
pub struct ResourceUsageAuthorizationPolicy;

impl ResourceUsageAuthorizationPolicy {
    pub fn new() -> Self {
        Self
    }

    /// 所有者かどうかをチェック
    fn is_owner(&self, actor: &EmailAddress, resource: &ResourceUsage) -> bool {
        resource.owner_email() == actor
    }
}

impl AuthorizationPolicy<ResourceUsage> for ResourceUsageAuthorizationPolicy {
    fn authorize_update(
        &self,
        actor: &EmailAddress,
        resource: &ResourceUsage,
    ) -> Result<(), AuthorizationError> {
        if !self.is_owner(actor, resource) {
            return Err(AuthorizationError::Forbidden {
                actor: actor.clone(),
                action: "update".to_string(),
                resource: format!("ResourceUsage({})", resource.id().as_str()),
            });
        }
        Ok(())
    }

    fn authorize_delete(
        &self,
        actor: &EmailAddress,
        resource: &ResourceUsage,
    ) -> Result<(), AuthorizationError> {
        if !self.is_owner(actor, resource) {
            return Err(AuthorizationError::Forbidden {
                actor: actor.clone(),
                action: "delete".to_string(),
                resource: format!("ResourceUsage({})", resource.id().as_str()),
            });
        }
        Ok(())
    }
}
