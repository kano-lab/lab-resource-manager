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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::resource_usage::value_objects::{
        Gpu, Resource, TimePeriod, UsageId,
    };
    use chrono::{Duration, Utc};

    #[test]
    fn test_owner_can_update() {
        let policy = ResourceUsageAuthorizationPolicy::new();
        let owner = EmailAddress::new("owner@example.com".to_string()).unwrap();
        let start = Utc::now();
        let end = start + Duration::hours(1);
        let usage = ResourceUsage::new(
            UsageId::new("test".to_string()),
            owner.clone(),
            TimePeriod::new(start, end).unwrap(),
            vec![Resource::Gpu(Gpu::new(
                "Server".to_string(),
                0,
                "A100".to_string(),
            ))],
            None,
        )
        .unwrap();

        assert!(policy.authorize_update(&owner, &usage).is_ok());
    }

    #[test]
    fn test_non_owner_cannot_update() {
        let policy = ResourceUsageAuthorizationPolicy::new();
        let owner = EmailAddress::new("owner@example.com".to_string()).unwrap();
        let other = EmailAddress::new("other@example.com".to_string()).unwrap();
        let start = Utc::now();
        let end = start + Duration::hours(1);
        let usage = ResourceUsage::new(
            UsageId::new("test".to_string()),
            owner,
            TimePeriod::new(start, end).unwrap(),
            vec![Resource::Gpu(Gpu::new(
                "Server".to_string(),
                0,
                "A100".to_string(),
            ))],
            None,
        )
        .unwrap();

        assert!(matches!(
            policy.authorize_update(&other, &usage),
            Err(AuthorizationError::Forbidden { .. })
        ));
    }

    #[test]
    fn test_non_owner_cannot_delete() {
        let policy = ResourceUsageAuthorizationPolicy::new();
        let owner = EmailAddress::new("owner@example.com".to_string()).unwrap();
        let other = EmailAddress::new("other@example.com".to_string()).unwrap();
        let start = Utc::now();
        let end = start + Duration::hours(1);
        let usage = ResourceUsage::new(
            UsageId::new("test".to_string()),
            owner,
            TimePeriod::new(start, end).unwrap(),
            vec![Resource::Gpu(Gpu::new(
                "Server".to_string(),
                0,
                "A100".to_string(),
            ))],
            None,
        )
        .unwrap();

        assert!(matches!(
            policy.authorize_delete(&other, &usage),
            Err(AuthorizationError::Forbidden { .. })
        ));
    }
}
