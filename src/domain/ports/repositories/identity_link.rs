use crate::domain::aggregates::identity_link::{
    entity::IdentityLink, value_objects::ExternalSystem,
};
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::RepositoryError;
use async_trait::async_trait;

#[async_trait]
pub trait IdentityLinkRepository: Send + Sync {
    async fn find_by_email(
        &self,
        email: &EmailAddress,
    ) -> Result<Option<IdentityLink>, RepositoryError>;

    async fn find_by_external_user_id(
        &self,
        system: &ExternalSystem,
        user_id: &str,
    ) -> Result<Option<IdentityLink>, RepositoryError>;

    async fn save(&self, identity_link: IdentityLink) -> Result<(), RepositoryError>;

    async fn find_all(&self) -> Result<Vec<IdentityLink>, RepositoryError>;

    async fn delete(&self, email: &EmailAddress) -> Result<(), RepositoryError>;
}
