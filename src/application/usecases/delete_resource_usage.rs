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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
    use crate::domain::aggregates::resource_usage::value_objects::{Gpu, Resource, TimePeriod};
    use crate::infrastructure::repositories::resource_usage::mock::MockUsageRepository;
    use chrono::{Duration, Utc};

    #[tokio::test]
    async fn test_delete_resource_usage_success() {
        let repository = Arc::new(MockUsageRepository::new());

        // テスト用の予約を作成
        let owner_email = EmailAddress::new("user@example.com".to_string()).unwrap();
        let start = Utc::now() + Duration::hours(1);
        let end = start + Duration::hours(2);
        let usage = ResourceUsage::new(
            UsageId::new("test-id".to_string()),
            owner_email.clone(),
            TimePeriod::new(start, end).unwrap(),
            vec![Resource::Gpu(Gpu::new(
                "Thalys".to_string(),
                0,
                "A100".to_string(),
            ))],
            None,
        )
        .unwrap();

        repository.save(&usage).await.unwrap();

        // UseCaseの実行
        let usecase = DeleteResourceUsageUseCase::new(repository.clone());
        let result = usecase
            .execute(&UsageId::new("test-id".to_string()), &owner_email)
            .await;

        assert!(result.is_ok());

        // 削除されたことを確認
        let deleted = repository
            .find_by_id(&UsageId::new("test-id".to_string()))
            .await
            .unwrap();

        assert!(deleted.is_none());
    }

    #[tokio::test]
    async fn test_delete_resource_usage_not_owner() {
        let repository = Arc::new(MockUsageRepository::new());

        // テスト用の予約を作成
        let owner_email = EmailAddress::new("user@example.com".to_string()).unwrap();
        let other_email = EmailAddress::new("other@example.com".to_string()).unwrap();
        let start = Utc::now() + Duration::hours(1);
        let end = start + Duration::hours(2);
        let usage = ResourceUsage::new(
            UsageId::new("test-id".to_string()),
            owner_email,
            TimePeriod::new(start, end).unwrap(),
            vec![Resource::Gpu(Gpu::new(
                "Thalys".to_string(),
                0,
                "A100".to_string(),
            ))],
            None,
        )
        .unwrap();

        repository.save(&usage).await.unwrap();

        // UseCaseの実行（他のユーザーとして）
        let usecase = DeleteResourceUsageUseCase::new(repository.clone());
        let result = usecase
            .execute(&UsageId::new("test-id".to_string()), &other_email)
            .await;

        assert!(matches!(result, Err(ApplicationError::Unauthorized(_))));

        // 削除されていないことを確認
        let still_exists = repository
            .find_by_id(&UsageId::new("test-id".to_string()))
            .await
            .unwrap();

        assert!(still_exists.is_some());
    }

    #[tokio::test]
    async fn test_delete_resource_usage_not_found() {
        let repository = Arc::new(MockUsageRepository::new());
        let usecase = DeleteResourceUsageUseCase::new(repository);
        let owner_email = EmailAddress::new("user@example.com".to_string()).unwrap();

        let result = usecase
            .execute(&UsageId::new("non-existent".to_string()), &owner_email)
            .await;

        assert!(matches!(
            result,
            Err(ApplicationError::Repository(RepositoryError::NotFound))
        ));
    }
}
