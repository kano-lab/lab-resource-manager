use crate::application::error::ApplicationError;
use crate::domain::aggregates::identity_link::{
    entity::IdentityLink,
    value_objects::{ExternalIdentity, ExternalSystem},
};
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::IdentityLinkRepository;
use crate::domain::ports::resource_collection_access::{
    ResourceCollectionAccessError, ResourceCollectionAccessService,
};
use std::sync::Arc;

/// ユーザーにリソースアクセス権を付与するUseCase
///
/// 外部システムのユーザーとメールアドレスを紐付け、すべてのリソースコレクションへのアクセス権を付与する。
pub struct GrantUserResourceAccessUseCase {
    identity_repo: Arc<dyn IdentityLinkRepository>,
    collection_access: Arc<dyn ResourceCollectionAccessService>,
    /// アクセス権を付与するコレクションIDのリスト
    collection_ids: Vec<String>,
}

impl GrantUserResourceAccessUseCase {
    pub fn new(
        identity_repo: Arc<dyn IdentityLinkRepository>,
        collection_access: Arc<dyn ResourceCollectionAccessService>,
        collection_ids: Vec<String>,
    ) -> Self {
        Self {
            identity_repo,
            collection_access,
            collection_ids,
        }
    }

    pub async fn execute(
        &self,
        external_system: ExternalSystem,
        external_user_id: String,
        email: EmailAddress,
    ) -> Result<(), ApplicationError> {
        let mut identity = self.resolve_or_create_identity_link(&email).await?;
        self.link_external_identity(&mut identity, external_system, external_user_id)?;

        // アクセス権付与を先に実行（全て成功した場合のみIdentityLinkを保存）
        self.grant_access_to_all_resources(&email).await?;

        // 成功した場合のみIdentityLinkを保存
        self.save_identity_link(identity).await?;
        Ok(())
    }

    async fn resolve_or_create_identity_link(
        &self,
        email: &EmailAddress,
    ) -> Result<IdentityLink, ApplicationError> {
        match self.identity_repo.find_by_email(email).await? {
            Some(existing) => Ok(existing),
            None => Ok(IdentityLink::new(email.clone())),
        }
    }

    fn link_external_identity(
        &self,
        identity: &mut IdentityLink,
        external_system: ExternalSystem,
        external_user_id: String,
    ) -> Result<(), ApplicationError> {
        // 既に指定された外部システムと紐付いているかチェック
        if identity.has_identity_for_system(&external_system) {
            return Err(ApplicationError::ExternalSystemAlreadyLinked {
                email: identity.email().as_str().to_string(),
                external_system: external_system.as_str().to_string(),
            });
        }

        let external_identity = ExternalIdentity::new(external_system, external_user_id);
        identity.link_external_identity(external_identity)?;
        Ok(())
    }

    async fn grant_access_to_all_resources(
        &self,
        email: &EmailAddress,
    ) -> Result<(), ApplicationError> {
        let mut failed_collections = Vec::new();

        for collection_id in &self.collection_ids {
            match self
                .collection_access
                .grant_access(collection_id, email)
                .await
            {
                Ok(_) => {
                    // 成功した場合は次へ
                }
                Err(ResourceCollectionAccessError::AlreadyGranted(_)) => {
                    // 既にアクセス権がある場合は成功とみなす（べき等性）
                    continue;
                }
                Err(e) => {
                    // その他のエラーは記録して処理を継続
                    tracing::warn!(
                        "Failed to grant access to collection '{}' for {}: {}",
                        collection_id,
                        email.as_str(),
                        e
                    );
                    failed_collections.push((collection_id.clone(), e));
                }
            }
        }

        // 失敗したコレクションがある場合はエラーを返す
        if !failed_collections.is_empty() {
            return Err(ApplicationError::PartialAccessGrantFailure {
                failed: failed_collections,
            });
        }

        Ok(())
    }

    async fn save_identity_link(&self, identity: IdentityLink) -> Result<(), ApplicationError> {
        self.identity_repo.save(identity).await?;
        Ok(())
    }
}
