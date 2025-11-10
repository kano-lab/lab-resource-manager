use crate::application::error::ApplicationError;
use crate::domain::aggregates::resource_usage::{entity::ResourceUsage, value_objects::UsageId};
use crate::domain::ports::repositories::{RepositoryError, ResourceUsageRepository};
use std::sync::Arc;

/// IDでリソース使用予定を取得するユースケース
pub struct GetResourceUsageByIdUseCase<R: ResourceUsageRepository> {
    repository: Arc<R>,
}

impl<R: ResourceUsageRepository> GetResourceUsageByIdUseCase<R> {
    /// 新しいGetResourceUsageByIdUseCaseインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - ResourceUsageリポジトリ
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }

    /// 指定されたIDのリソース使用予定を取得
    ///
    /// # Arguments
    /// * `id` - 使用予定ID
    ///
    /// # Returns
    /// ResourceUsage
    ///
    /// # Errors
    /// - 指定されたIDの予約が見つからない場合
    /// - リポジトリエラー
    pub async fn execute(&self, id: &UsageId) -> Result<ResourceUsage, ApplicationError> {
        self.repository
            .find_by_id(id)
            .await?
            .ok_or(ApplicationError::Repository(RepositoryError::NotFound))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::resource_usage::value_objects::{Gpu, Resource, TimePeriod};
    use crate::domain::common::EmailAddress;
    use crate::infrastructure::repositories::resource_usage::mock::MockUsageRepository;
    use chrono::{Duration, Utc};

    #[tokio::test]
    async fn test_get_resource_usage_by_id_found() {
        let repository = Arc::new(MockUsageRepository::new());

        // テスト用の予約を作成
        let owner_email = EmailAddress::new("user@example.com".to_string()).unwrap();
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
            Some("テスト予約".to_string()),
        )
        .unwrap();

        repository.save(&usage).await.unwrap();

        // UseCaseの実行
        let usecase = GetResourceUsageByIdUseCase::new(repository);
        let result = usecase
            .execute(&UsageId::new("test-id".to_string()))
            .await
            .unwrap();

        // 検証
        assert_eq!(result.id().as_str(), "test-id");
        assert_eq!(result.notes().unwrap(), "テスト予約");
    }

    #[tokio::test]
    async fn test_get_resource_usage_by_id_not_found() {
        let repository = Arc::new(MockUsageRepository::new());
        let usecase = GetResourceUsageByIdUseCase::new(repository);

        let result = usecase
            .execute(&UsageId::new("non-existent".to_string()))
            .await;

        assert!(matches!(
            result,
            Err(ApplicationError::Repository(RepositoryError::NotFound))
        ));
    }
}
