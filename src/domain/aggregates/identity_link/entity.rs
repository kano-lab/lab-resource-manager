use super::errors::IdentityLinkError;
use super::value_objects::{ExternalIdentity, ExternalSystem};
use crate::domain::common::EmailAddress;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 外部システムとの識別情報の紐付けを管理する集約ルート
///
/// メールアドレスを主キーとして、複数の外部システム（Slack等）での
/// ユーザー識別情報との紐付けを管理する。
///
/// この集約は通知送信、コマンド実行時のユーザー特定など、
/// 外部システムとの連携が必要な場面で使用される。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IdentityLink {
    /// 主識別子（このシステム内での一意なユーザー識別子）
    email: EmailAddress,
    /// 外部システムでの識別情報
    external_identities: Vec<ExternalIdentity>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl IdentityLink {
    /// メールアドレスのみで新規作成
    pub fn new(email: EmailAddress) -> Self {
        let now = Utc::now();
        Self {
            email,
            external_identities: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// 外部システムの識別情報付きで作成
    pub fn with_external_identity(email: EmailAddress, identity: ExternalIdentity) -> Self {
        let now = Utc::now();
        Self {
            email,
            external_identities: vec![identity],
            created_at: now,
            updated_at: now,
        }
    }

    /// 永続化層からの復元
    ///
    /// **Repository実装専用**。保存されていた状態をそのまま復元する。
    /// ビジネスロジックは適用されない（時刻は指定された値がそのまま使われる）。
    pub(crate) fn reconstitute(
        email: EmailAddress,
        external_identities: Vec<ExternalIdentity>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            email,
            external_identities,
            created_at,
            updated_at,
        }
    }

    /// 外部システムの識別情報を追加
    pub fn link_external_identity(
        &mut self,
        identity: ExternalIdentity,
    ) -> Result<(), IdentityLinkError> {
        if self.has_identity_for_system(identity.system()) {
            return Err(IdentityLinkError::IdentityAlreadyExists {
                system: identity.system().clone(),
            });
        }

        self.external_identities.push(identity);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// 外部システムの識別情報を削除
    pub fn unlink_external_identity(
        &mut self,
        system: &ExternalSystem,
    ) -> Result<(), IdentityLinkError> {
        let initial_len = self.external_identities.len();
        self.external_identities.retain(|id| id.system() != system);

        if self.external_identities.len() == initial_len {
            return Err(IdentityLinkError::IdentityNotFound {
                system: system.clone(),
            });
        }

        self.updated_at = Utc::now();
        Ok(())
    }

    /// 特定の外部システムでの識別情報を取得
    pub fn get_identity_for_system(&self, system: &ExternalSystem) -> Option<&ExternalIdentity> {
        self.external_identities
            .iter()
            .find(|id| id.system() == system)
    }

    /// 特定の外部システムと紐付けられているか
    pub fn has_identity_for_system(&self, system: &ExternalSystem) -> bool {
        self.external_identities
            .iter()
            .any(|id| id.system() == system)
    }

    /// いずれかの外部システムと紐付けられているか
    pub fn is_linked_to_any_system(&self) -> bool {
        !self.external_identities.is_empty()
    }

    pub fn email(&self) -> &EmailAddress {
        &self.email
    }

    pub fn external_identities(&self) -> &[ExternalIdentity] {
        &self.external_identities
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_identity_link() {
        let email = EmailAddress::new("user@example.com".to_string()).unwrap();
        let identity = IdentityLink::new(email.clone());

        assert_eq!(identity.email(), &email);
        assert!(!identity.is_linked_to_any_system());
        assert_eq!(identity.external_identities().len(), 0);
    }

    #[test]
    fn test_link_external_identity() {
        let email = EmailAddress::new("user@example.com".to_string()).unwrap();
        let mut identity = IdentityLink::new(email);

        let external_id = ExternalIdentity::new(ExternalSystem::Slack, "U12345678".to_string());
        let result = identity.link_external_identity(external_id);

        assert!(result.is_ok());
        assert!(identity.is_linked_to_any_system());
        assert!(identity.has_identity_for_system(&ExternalSystem::Slack));
    }

    #[test]
    fn test_link_duplicate_system() {
        let email = EmailAddress::new("user@example.com".to_string()).unwrap();
        let mut identity = IdentityLink::new(email);

        let external_id1 = ExternalIdentity::new(ExternalSystem::Slack, "U12345678".to_string());
        identity.link_external_identity(external_id1).unwrap();

        let external_id2 = ExternalIdentity::new(ExternalSystem::Slack, "U87654321".to_string());
        let result = identity.link_external_identity(external_id2);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            IdentityLinkError::IdentityAlreadyExists {
                system: ExternalSystem::Slack
            }
        );
    }

    #[test]
    fn test_unlink_external_identity() {
        let email = EmailAddress::new("user@example.com".to_string()).unwrap();
        let mut identity = IdentityLink::new(email);

        let external_id = ExternalIdentity::new(ExternalSystem::Slack, "U12345678".to_string());
        identity.link_external_identity(external_id).unwrap();

        let result = identity.unlink_external_identity(&ExternalSystem::Slack);
        assert!(result.is_ok());
        assert!(!identity.has_identity_for_system(&ExternalSystem::Slack));
    }

    #[test]
    fn test_get_identity_for_system() {
        let email = EmailAddress::new("user@example.com".to_string()).unwrap();
        let mut identity = IdentityLink::new(email);

        let slack_id = "U12345678".to_string();
        let external_id = ExternalIdentity::new(ExternalSystem::Slack, slack_id.clone());
        identity.link_external_identity(external_id).unwrap();

        let found = identity.get_identity_for_system(&ExternalSystem::Slack);
        assert!(found.is_some());
        assert_eq!(found.unwrap().user_id(), slack_id);
    }
}
