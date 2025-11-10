use crate::application::error::ApplicationError;
use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::ports::repositories::ResourceUsageRepository;
use std::sync::Arc;

/// 全ての未来のリソース使用予定を取得するユースケース
pub struct ListAllFutureResourceUsagesUseCase<R: ResourceUsageRepository> {
    repository: Arc<R>,
}

impl<R: ResourceUsageRepository> ListAllFutureResourceUsagesUseCase<R> {
    /// 新しいListAllFutureResourceUsagesUseCaseインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - ResourceUsageリポジトリ
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }

    /// 全ての未来のリソース使用予定を取得
    ///
    /// # Returns
    /// ResourceUsageのリスト（時系列順）
    ///
    /// # Errors
    /// - リポジトリエラー
    pub async fn execute(&self) -> Result<Vec<ResourceUsage>, ApplicationError> {
        let mut usages = self.repository.find_future().await?;

        // 開始時刻でソート
        usages.sort_by_key(|a| a.time_period().start());

        Ok(usages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::resource_usage::value_objects::{
        Gpu, Resource, TimePeriod, UsageId,
    };
    use crate::domain::common::EmailAddress;
    use crate::infrastructure::repositories::resource_usage::mock::MockUsageRepository;
    use chrono::{Duration, Utc};

    #[tokio::test]
    async fn test_list_all_future_resource_usages() {
        let repository = Arc::new(MockUsageRepository::new());

        // テスト用の予約を作成
        let email1 = EmailAddress::new("user1@example.com".to_string()).unwrap();
        let email2 = EmailAddress::new("user2@example.com".to_string()).unwrap();

        let start1 = Utc::now() + Duration::hours(3);
        let end1 = start1 + Duration::hours(2);
        let usage1 = ResourceUsage::new(
            UsageId::new("usage-1".to_string()),
            email1.clone(),
            TimePeriod::new(start1, end1).unwrap(),
            vec![Resource::Gpu(Gpu::new(
                "Thalys".to_string(),
                0,
                "A100".to_string(),
            ))],
            None,
        )
        .unwrap();

        let start2 = Utc::now() + Duration::hours(1);
        let end2 = start2 + Duration::hours(2);
        let usage2 = ResourceUsage::new(
            UsageId::new("usage-2".to_string()),
            email2,
            TimePeriod::new(start2, end2).unwrap(),
            vec![Resource::Gpu(Gpu::new(
                "Thalys".to_string(),
                1,
                "A100".to_string(),
            ))],
            None,
        )
        .unwrap();

        let start3 = Utc::now() + Duration::hours(5);
        let end3 = start3 + Duration::hours(2);
        let usage3 = ResourceUsage::new(
            UsageId::new("usage-3".to_string()),
            email1,
            TimePeriod::new(start3, end3).unwrap(),
            vec![Resource::Gpu(Gpu::new(
                "Thalys".to_string(),
                2,
                "A100".to_string(),
            ))],
            None,
        )
        .unwrap();

        repository.save(&usage1).await.unwrap();
        repository.save(&usage2).await.unwrap();
        repository.save(&usage3).await.unwrap();

        // UseCaseの実行
        let usecase = ListAllFutureResourceUsagesUseCase::new(repository);
        let result = usecase.execute().await.unwrap();

        // 検証：時系列順にソートされていること
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].id().as_str(), "usage-2"); // 1時間後
        assert_eq!(result[1].id().as_str(), "usage-1"); // 3時間後
        assert_eq!(result[2].id().as_str(), "usage-3"); // 5時間後
    }
}
