use crate::domain::aggregates::identity_link::{
    entity::IdentityLink, value_objects::ExternalSystem,
};
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::RepositoryError;
use async_trait::async_trait;

/// IdentityLink集約のリポジトリポート
#[async_trait]
pub trait IdentityLinkRepository: Send + Sync {
    /// メールアドレスでIdentityLinkを検索
    async fn find_by_email(
        &self,
        email: &EmailAddress,
    ) -> Result<Option<IdentityLink>, RepositoryError>;

    /// 外部システムのユーザーIDでIdentityLinkを検索
    async fn find_by_external_user_id(
        &self,
        system: &ExternalSystem,
        user_id: &str,
    ) -> Result<Option<IdentityLink>, RepositoryError>;

    /// IdentityLinkを保存
    async fn save(&self, identity_link: IdentityLink) -> Result<(), RepositoryError>;
}
