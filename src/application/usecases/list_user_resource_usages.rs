use crate::application::error::ApplicationError;
use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::ResourceUsageRepository;
use std::sync::Arc;

/// ユーザーのリソース使用予定一覧を取得するユースケース
pub struct ListUserResourceUsagesUseCase<R: ResourceUsageRepository> {
    repository: Arc<R>,
}

impl<R: ResourceUsageRepository> ListUserResourceUsagesUseCase<R> {
    /// 新しいListUserResourceUsagesUseCaseインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - ResourceUsageリポジトリ
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }

    /// 特定のユーザーが所有するリソース使用予定の一覧を取得
    ///
    /// # Arguments
    /// * `owner_email` - 所有者のメールアドレス
    ///
    /// # Returns
    /// ResourceUsageのリスト（時系列順）
    ///
    /// # Errors
    /// - リポジトリエラー
    pub async fn execute(
        &self,
        owner_email: &EmailAddress,
    ) -> Result<Vec<ResourceUsage>, ApplicationError> {
        let mut usages = self.repository.find_by_owner(owner_email).await?;

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
    use crate::infrastructure::repositories::resource_usage::mock::MockUsageRepository;
    use chrono::{Duration, Utc};

    #[tokio::test]
    async fn test_list_user_resource_usages() {
        let repository = Arc::new(MockUsageRepository::new());

        // テスト用の予約を作成
        let owner_email = EmailAddress::new("user@example.com".to_string()).unwrap();
        let other_email = EmailAddress::new("other@example.com".to_string()).unwrap();

        let start1 = Utc::now() + Duration::hours(1);
        let end1 = start1 + Duration::hours(2);
        let usage1 = ResourceUsage::new(
            UsageId::new("usage-1".to_string()),
            owner_email.clone(),
            TimePeriod::new(start1, end1).unwrap(),
            vec![Resource::Gpu(Gpu::new(
                "Thalys".to_string(),
                0,
                "A100".to_string(),
            ))],
            None,
        )
        .unwrap();

        let start2 = Utc::now() + Duration::hours(3);
        let end2 = start2 + Duration::hours(2);
        let usage2 = ResourceUsage::new(
            UsageId::new("usage-2".to_string()),
            owner_email.clone(),
            TimePeriod::new(start2, end2).unwrap(),
            vec![Resource::Gpu(Gpu::new(
                "Thalys".to_string(),
                1,
                "A100".to_string(),
            ))],
            None,
        )
        .unwrap();

        // 別のユーザーの予約
        let start3 = Utc::now() + Duration::hours(2);
        let end3 = start3 + Duration::hours(2);
        let usage3 = ResourceUsage::new(
            UsageId::new("usage-3".to_string()),
            other_email,
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
        let usecase = ListUserResourceUsagesUseCase::new(repository);
        let result = usecase.execute(&owner_email).await.unwrap();

        // 検証
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id().as_str(), "usage-1");
        assert_eq!(result[1].id().as_str(), "usage-2");
    }
}
