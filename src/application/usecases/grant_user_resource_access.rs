use crate::application::error::ApplicationError;
use crate::domain::aggregates::identity_link::{
    entity::IdentityLink,
    value_objects::SlackUserId,
};
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::IdentityLinkRepository;
use crate::domain::ports::resource_collection_access::ResourceCollectionAccessService;
use crate::infrastructure::config::ResourceConfig;
use std::sync::Arc;

/// ユーザーにリソースアクセス権を付与するUseCase
///
/// Slackユーザーとmailアドレスを紐付け、すべてのリソースコレクションへのアクセス権を付与する。
pub struct GrantUserResourceAccessUseCase {
    identity_repo: Arc<dyn IdentityLinkRepository>,
    collection_access: Arc<dyn ResourceCollectionAccessService>,
    config: ResourceConfig,
}

impl GrantUserResourceAccessUseCase {
    pub fn new(
        identity_repo: Arc<dyn IdentityLinkRepository>,
        collection_access: Arc<dyn ResourceCollectionAccessService>,
        config: ResourceConfig,
    ) -> Self {
        Self {
            identity_repo,
            collection_access,
            config,
        }
    }

    pub async fn execute(
        &self,
        slack_user_id: SlackUserId,
        email: EmailAddress,
    ) -> Result<(), ApplicationError> {
        let identity = self
            .resolve_or_create_identity_link(slack_user_id, &email)
            .await?;
        self.grant_access_to_all_resources(&email).await?;
        self.save_identity_link(identity).await?;
        Ok(())
    }

    async fn resolve_or_create_identity_link(
        &self,
        slack_user_id: SlackUserId,
        email: &EmailAddress,
    ) -> Result<IdentityLink, ApplicationError> {
        match self.identity_repo.find_by_email(email).await? {
            Some(existing) => {
                if existing.is_slack_linked() {
                    return Err(ApplicationError::EmailAlreadyLinkedToAnotherUser {
                        email: email.as_str().to_string(),
                    });
                }
                Ok(existing.link_slack(slack_user_id)?)
            }
            None => Ok(IdentityLink::create_with_slack(
                email.clone(),
                slack_user_id,
            )),
        }
    }

    async fn grant_access_to_all_resources(
        &self,
        email: &EmailAddress,
    ) -> Result<(), ApplicationError> {
        for server in &self.config.servers {
            self.collection_access
                .grant_access(&server.calendar_id, email)
                .await?;
        }

        for room in &self.config.rooms {
            self.collection_access
                .grant_access(&room.calendar_id, email)
                .await?;
        }

        Ok(())
    }

    async fn save_identity_link(&self, identity: IdentityLink) -> Result<(), ApplicationError> {
        self.identity_repo.save(identity).await?;
        Ok(())
    }
}
