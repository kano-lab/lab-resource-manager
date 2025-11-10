use crate::application::error::ApplicationError;
use crate::domain::aggregates::resource_usage::value_objects::{Resource, TimePeriod, UsageId};
use crate::domain::ports::repositories::ResourceUsageRepository;

/// リソース競合チェックサービス
///
/// 指定された時間帯とリソースが既存の予約と競合しないかをチェックする
#[derive(Debug, Clone, Default)]
pub struct ResourceConflictChecker;

impl ResourceConflictChecker {
    pub fn new() -> Self {
        Self
    }

    /// リソース競合をチェック
    ///
    /// # Arguments
    /// * `repository` - リソース使用リポジトリ
    /// * `time_period` - チェック対象の時間帯
    /// * `resources` - チェック対象のリソースリスト
    /// * `exclude_usage_id` - チェックから除外するUsageID（更新時に自分自身を除外するため）
    ///
    /// # Returns
    /// 競合がない場合はOk(())、競合がある場合はエラー
    ///
    /// # Errors
    /// - 競合するリソースがある場合
    /// - リポジトリエラー
    pub async fn check_conflicts<R: ResourceUsageRepository>(
        &self,
        repository: &R,
        time_period: &TimePeriod,
        resources: &[Resource],
        exclude_usage_id: Option<&UsageId>,
    ) -> Result<(), ApplicationError> {
        // 指定期間と重複する予約を検索
        let overlapping = repository.find_overlapping(time_period).await?;

        // リソースの競合チェック
        for new_resource in resources {
            for existing_usage in &overlapping {
                // 除外対象の場合はスキップ
                if let Some(exclude_id) = exclude_usage_id
                    && existing_usage.id() == exclude_id
                {
                    continue;
                }

                // 既存予約のリソースと競合チェック
                for existing_resource in existing_usage.resources() {
                    if new_resource.conflicts_with(existing_resource) {
                        return Err(ApplicationError::ResourceConflict {
                            resource_description: format!("{:?}", new_resource),
                            conflicting_usage_id: existing_usage.id().as_str().to_string(),
                        });
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
    use crate::domain::aggregates::resource_usage::value_objects::Gpu;
    use crate::domain::common::EmailAddress;
    use crate::infrastructure::repositories::resource_usage::mock::MockUsageRepository;
    use chrono::{Duration, Utc};

    #[tokio::test]
    async fn test_no_conflict() {
        let repository = MockUsageRepository::new();
        let checker = ResourceConflictChecker::new();

        let start = Utc::now() + Duration::hours(1);
        let end = start + Duration::hours(2);
        let time_period = TimePeriod::new(start, end).unwrap();
        let resources = vec![Resource::Gpu(Gpu::new(
            "Server".to_string(),
            0,
            "A100".to_string(),
        ))];

        let result = checker
            .check_conflicts(&repository, &time_period, &resources, None)
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_conflict_detected() {
        let repository = MockUsageRepository::new();

        // 既存の予約を作成
        let owner = EmailAddress::new("owner@example.com".to_string()).unwrap();
        let start = Utc::now() + Duration::hours(1);
        let end = start + Duration::hours(2);
        let time_period = TimePeriod::new(start, end).unwrap();
        let resources = vec![Resource::Gpu(Gpu::new(
            "Server".to_string(),
            0,
            "A100".to_string(),
        ))];
        let existing_usage = ResourceUsage::new(
            UsageId::new("existing-id".to_string()),
            owner,
            time_period.clone(),
            resources.clone(),
            None,
        )
        .unwrap();

        repository.save(&existing_usage).await.unwrap();

        // 競合チェック
        let checker = ResourceConflictChecker::new();
        let overlapping_start = start + Duration::minutes(30);
        let overlapping_end = overlapping_start + Duration::hours(1);
        let overlapping_period = TimePeriod::new(overlapping_start, overlapping_end).unwrap();

        let result = checker
            .check_conflicts(&repository, &overlapping_period, &resources, None)
            .await;

        assert!(matches!(
            result,
            Err(ApplicationError::ResourceConflict { .. })
        ));
    }

    #[tokio::test]
    async fn test_exclude_self_from_conflict_check() {
        let repository = MockUsageRepository::new();

        // 既存の予約を作成
        let owner = EmailAddress::new("owner@example.com".to_string()).unwrap();
        let start = Utc::now() + Duration::hours(1);
        let end = start + Duration::hours(2);
        let time_period = TimePeriod::new(start, end).unwrap();
        let resources = vec![Resource::Gpu(Gpu::new(
            "Server".to_string(),
            0,
            "A100".to_string(),
        ))];
        let usage_id = UsageId::new("test-id".to_string());
        let existing_usage = ResourceUsage::new(
            usage_id.clone(),
            owner,
            time_period.clone(),
            resources.clone(),
            None,
        )
        .unwrap();

        repository.save(&existing_usage).await.unwrap();

        // 自分自身を除外してチェック（競合しないはず）
        let checker = ResourceConflictChecker::new();
        let result = checker
            .check_conflicts(&repository, &time_period, &resources, Some(&usage_id))
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_no_conflict_different_resource() {
        let repository = MockUsageRepository::new();

        // GPU 0 の予約を作成
        let owner = EmailAddress::new("owner@example.com".to_string()).unwrap();
        let start = Utc::now() + Duration::hours(1);
        let end = start + Duration::hours(2);
        let time_period = TimePeriod::new(start, end).unwrap();
        let gpu0 = vec![Resource::Gpu(Gpu::new(
            "Server".to_string(),
            0,
            "A100".to_string(),
        ))];
        let existing_usage = ResourceUsage::new(
            UsageId::new("existing-id".to_string()),
            owner,
            time_period.clone(),
            gpu0,
            None,
        )
        .unwrap();

        repository.save(&existing_usage).await.unwrap();

        // GPU 1 をチェック（競合しないはず）
        let checker = ResourceConflictChecker::new();
        let gpu1 = vec![Resource::Gpu(Gpu::new(
            "Server".to_string(),
            1,
            "A100".to_string(),
        ))];

        let result = checker
            .check_conflicts(&repository, &time_period, &gpu1, None)
            .await;

        assert!(result.is_ok());
    }
}
