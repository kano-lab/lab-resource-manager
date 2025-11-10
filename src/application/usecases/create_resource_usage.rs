use crate::application::error::ApplicationError;
use crate::domain::aggregates::resource_usage::{
    entity::ResourceUsage,
    value_objects::{Resource, TimePeriod, UsageId},
};
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::domain::services::ResourceConflictChecker;
use chrono::Utc;
use std::sync::Arc;

/// リソース使用予定を作成するユースケース
pub struct CreateResourceUsageUseCase<R: ResourceUsageRepository> {
    repository: Arc<R>,
    conflict_checker: ResourceConflictChecker,
}

impl<R: ResourceUsageRepository> CreateResourceUsageUseCase<R> {
    /// 新しいCreateResourceUsageUseCaseインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - ResourceUsageリポジトリ
    pub fn new(repository: Arc<R>) -> Self {
        let conflict_checker = ResourceConflictChecker::new();
        Self {
            repository,
            conflict_checker,
        }
    }

    /// リソース使用予定を作成
    ///
    /// # Arguments
    /// * `owner_email` - 所有者のメールアドレス
    /// * `time_period` - 使用期間
    /// * `resources` - 使用するリソースのリスト
    /// * `notes` - 備考（オプション）
    ///
    /// # Returns
    /// 作成されたResourceUsageのID
    ///
    /// # Errors
    /// - 開始時刻が過去の場合
    /// - 指定期間と重複するリソース使用がある場合
    /// - リポジトリエラー
    pub async fn execute(
        &self,
        owner_email: EmailAddress,
        time_period: TimePeriod,
        resources: Vec<Resource>,
        notes: Option<String>,
    ) -> Result<UsageId, ApplicationError> {
        // 開始時刻が過去でないことを確認
        if time_period.start() < Utc::now() {
            return Err(ApplicationError::InvalidTimePeriod(
                "開始時刻が過去です".to_string(),
            ));
        }

        // 競合チェック
        self.conflict_checker
            .check_conflicts(self.repository.as_ref(), &time_period, &resources, None)
            .await?;

        // 空のIDで新しいResourceUsageを作成（Google Calendarが自動採番）
        let usage = ResourceUsage::new(
            UsageId::new("".to_string()),
            owner_email,
            time_period,
            resources,
            notes,
        )?;

        // 保存
        self.repository.save(&usage).await?;

        Ok(usage.id().clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::resource_usage::value_objects::Gpu;
    use crate::infrastructure::repositories::resource_usage::mock::MockUsageRepository;
    use chrono::Duration;

    #[tokio::test]
    async fn test_create_resource_usage_success() {
        let repository = Arc::new(MockUsageRepository::new());
        let usecase = CreateResourceUsageUseCase::new(repository);

        let owner_email = EmailAddress::new("test@example.com".to_string()).unwrap();
        let start = Utc::now() + Duration::hours(1);
        let end = start + Duration::hours(2);
        let time_period = TimePeriod::new(start, end).unwrap();
        let resources = vec![Resource::Gpu(Gpu::new(
            "Thalys".to_string(),
            0,
            "A100".to_string(),
        ))];

        let result = usecase
            .execute(owner_email, time_period, resources, None)
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_resource_usage_past_time() {
        let repository = Arc::new(MockUsageRepository::new());
        let usecase = CreateResourceUsageUseCase::new(repository);

        let owner_email = EmailAddress::new("test@example.com".to_string()).unwrap();
        let start = Utc::now() - Duration::hours(2);
        let end = start + Duration::hours(1);
        let time_period = TimePeriod::new(start, end).unwrap();
        let resources = vec![Resource::Gpu(Gpu::new(
            "Thalys".to_string(),
            0,
            "A100".to_string(),
        ))];

        let result = usecase
            .execute(owner_email, time_period, resources, None)
            .await;

        assert!(matches!(
            result,
            Err(ApplicationError::InvalidTimePeriod(_))
        ));
    }

    #[tokio::test]
    async fn test_create_resource_usage_conflict() {
        let repository = Arc::new(MockUsageRepository::new());

        // 既存の予約を作成
        let existing_owner = EmailAddress::new("existing@example.com".to_string()).unwrap();
        let existing_start = Utc::now() + Duration::hours(1);
        let existing_end = existing_start + Duration::hours(2);
        let existing_time_period = TimePeriod::new(existing_start, existing_end).unwrap();
        let existing_resources = vec![Resource::Gpu(Gpu::new(
            "Thalys".to_string(),
            0,
            "A100".to_string(),
        ))];
        let existing_usage = ResourceUsage::new(
            UsageId::new("existing-id".to_string()),
            existing_owner,
            existing_time_period,
            existing_resources,
            None,
        )
        .unwrap();

        repository.save(&existing_usage).await.unwrap();

        // 競合する予約を作成しようとする
        let usecase = CreateResourceUsageUseCase::new(repository);
        let owner_email = EmailAddress::new("test@example.com".to_string()).unwrap();
        let start = existing_start + Duration::minutes(30); // 既存と重複
        let end = start + Duration::hours(2);
        let time_period = TimePeriod::new(start, end).unwrap();
        let resources = vec![Resource::Gpu(Gpu::new(
            "Thalys".to_string(),
            0,
            "A100".to_string(),
        ))];

        let result = usecase
            .execute(owner_email, time_period, resources, None)
            .await;

        assert!(matches!(
            result,
            Err(ApplicationError::ResourceConflict { .. })
        ));
    }
}
