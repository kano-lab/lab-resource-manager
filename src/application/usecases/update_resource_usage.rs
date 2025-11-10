use crate::application::error::ApplicationError;
use crate::domain::aggregates::resource_usage::value_objects::{TimePeriod, UsageId};
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::{RepositoryError, ResourceUsageRepository};
use crate::domain::services::{AuthorizationPolicy, ResourceConflictChecker, ResourceUsageAuthorizationPolicy};
use std::sync::Arc;

/// リソース使用予定を更新するユースケース
pub struct UpdateResourceUsageUseCase<R: ResourceUsageRepository> {
    repository: Arc<R>,
    authorization_policy: ResourceUsageAuthorizationPolicy,
    conflict_checker: ResourceConflictChecker,
}

impl<R: ResourceUsageRepository> UpdateResourceUsageUseCase<R> {
    /// 新しいUpdateResourceUsageUseCaseインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - ResourceUsageリポジトリ
    pub fn new(repository: Arc<R>) -> Self {
        let authorization_policy = ResourceUsageAuthorizationPolicy::new();
        let conflict_checker = ResourceConflictChecker::new();
        Self {
            repository,
            authorization_policy,
            conflict_checker,
        }
    }

    /// リソース使用予定を更新
    ///
    /// # Arguments
    /// * `id` - 使用予定ID
    /// * `owner_email` - 所有者のメールアドレス（権限チェック用）
    /// * `new_time_period` - 新しい使用期間（Noneの場合は変更なし）
    /// * `new_notes` - 新しい備考（Noneの場合は変更なし）
    ///
    /// # Returns
    /// 更新成功
    ///
    /// # Errors
    /// - 指定されたIDの予約が見つからない場合
    /// - 所有者が一致しない場合
    /// - 新しい時間枠が競合する場合
    /// - リポジトリエラー
    pub async fn execute(
        &self,
        id: &UsageId,
        owner_email: &EmailAddress,
        new_time_period: Option<TimePeriod>,
        new_notes: Option<String>,
    ) -> Result<(), ApplicationError> {
        // 既存の予約を取得
        let mut usage = self
            .repository
            .find_by_id(id)
            .await?
            .ok_or(ApplicationError::Repository(RepositoryError::NotFound))?;

        // 認可チェック
        self.authorization_policy
            .authorize_update(owner_email, &usage)
            .map_err(|e| ApplicationError::Unauthorized(e.to_string()))?;

        // 時間枠の更新と競合チェック
        if let Some(new_period) = new_time_period {
            // 競合チェック（自分自身を除外）
            self.conflict_checker
                .check_conflicts(
                    self.repository.as_ref(),
                    &new_period,
                    usage.resources(),
                    Some(usage.id()),
                )
                .await?;

            usage.update_time_period(new_period);
        }

        // 備考の更新
        if let Some(notes) = new_notes {
            usage.update_notes(notes);
        }

        // 保存
        self.repository.save(&usage).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
    use crate::domain::aggregates::resource_usage::value_objects::{Gpu, Resource};
    use crate::infrastructure::repositories::resource_usage::mock::MockUsageRepository;
    use chrono::{Duration, Utc};

    #[tokio::test]
    async fn test_update_resource_usage_success() {
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
            Some("元の備考".to_string()),
        )
        .unwrap();

        repository.save(&usage).await.unwrap();

        // UseCaseの実行
        let usecase = UpdateResourceUsageUseCase::new(repository.clone());
        let new_start = Utc::now() + Duration::hours(3);
        let new_end = new_start + Duration::hours(2);
        let new_time_period = TimePeriod::new(new_start, new_end).unwrap();

        let result = usecase
            .execute(
                &UsageId::new("test-id".to_string()),
                &owner_email,
                Some(new_time_period.clone()),
                Some("新しい備考".to_string()),
            )
            .await;

        assert!(result.is_ok());

        // 更新されたことを確認
        let updated = repository
            .find_by_id(&UsageId::new("test-id".to_string()))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(updated.time_period(), &new_time_period);
        assert_eq!(updated.notes().unwrap(), "新しい備考");
    }

    #[tokio::test]
    async fn test_update_resource_usage_not_owner() {
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
        let usecase = UpdateResourceUsageUseCase::new(repository);
        let result = usecase
            .execute(
                &UsageId::new("test-id".to_string()),
                &other_email,
                None,
                Some("不正な更新".to_string()),
            )
            .await;

        assert!(matches!(result, Err(ApplicationError::Unauthorized(_))));
    }

    #[tokio::test]
    async fn test_update_resource_usage_conflict() {
        let repository = Arc::new(MockUsageRepository::new());

        // 既存の予約1
        let owner_email = EmailAddress::new("user@example.com".to_string()).unwrap();
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

        // 既存の予約2（別の時間帯）
        let start2 = Utc::now() + Duration::hours(5);
        let end2 = start2 + Duration::hours(2);
        let usage2 = ResourceUsage::new(
            UsageId::new("usage-2".to_string()),
            EmailAddress::new("other@example.com".to_string()).unwrap(),
            TimePeriod::new(start2, end2).unwrap(),
            vec![Resource::Gpu(Gpu::new(
                "Thalys".to_string(),
                0,
                "A100".to_string(),
            ))],
            None,
        )
        .unwrap();

        repository.save(&usage1).await.unwrap();
        repository.save(&usage2).await.unwrap();

        // usage1を usage2と競合する時間に更新しようとする
        let usecase = UpdateResourceUsageUseCase::new(repository);
        let conflicting_start = start2 + Duration::minutes(30);
        let conflicting_end = conflicting_start + Duration::hours(2);
        let conflicting_period = TimePeriod::new(conflicting_start, conflicting_end).unwrap();

        let result = usecase
            .execute(
                &UsageId::new("usage-1".to_string()),
                &owner_email,
                Some(conflicting_period),
                None,
            )
            .await;

        assert!(matches!(
            result,
            Err(ApplicationError::ResourceConflict { .. })
        ));
    }
}
