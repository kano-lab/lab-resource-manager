use crate::application::error::ApplicationError;
use crate::domain::aggregates::resource_usage::value_objects::UsageId;
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::{RepositoryError, ResourceUsageRepository};
use crate::domain::services::{AuthorizationPolicy, ResourceUsageAuthorizationPolicy};
use std::sync::Arc;

/// リソース使用予定を削除するユースケース
pub struct DeleteResourceUsageUseCase<R: ResourceUsageRepository> {
    repository: Arc<R>,
    authorization_policy: ResourceUsageAuthorizationPolicy,
}

impl<R: ResourceUsageRepository> DeleteResourceUsageUseCase<R> {
    /// 新しいDeleteResourceUsageUseCaseインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - ResourceUsageリポジトリ
    pub fn new(repository: Arc<R>) -> Self {
        let authorization_policy = ResourceUsageAuthorizationPolicy::new();
        Self {
            repository,
            authorization_policy,
        }
    }

    /// リソース使用予定を削除
    ///
    /// # Arguments
    /// * `id` - 使用予定ID
    /// * `owner_email` - 所有者のメールアドレス（権限チェック用）
    ///
    /// # Returns
    /// 削除成功
    ///
    /// # Errors
    /// - 指定されたIDの予約が見つからない場合
    /// - 所有者が一致しない場合
    /// - リポジトリエラー
    pub async fn execute(
        &self,
        id: &UsageId,
        owner_email: &EmailAddress,
    ) -> Result<(), ApplicationError> {
        // 既存の予約を取得
        let usage = self
            .repository
            .find_by_id(id)
            .await?
            .ok_or(ApplicationError::Repository(RepositoryError::NotFound))?;

        // 認可チェック
        self.authorization_policy
            .authorize_delete(owner_email, &usage)
            .map_err(|e| ApplicationError::Unauthorized(e.to_string()))?;

        // 削除
        self.repository.delete(id).await?;

        Ok(())
    }
}
