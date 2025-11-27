use crate::application::error::ApplicationError;
use crate::domain::aggregates::resource_usage::value_objects::{TimePeriod, UsageId};
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::{RepositoryError, ResourceUsageRepository};
use crate::domain::services::{
    AuthorizationPolicy, ResourceConflictChecker, ResourceUsageAuthorizationPolicy,
};
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
        tracing::info!("🔄 UpdateResourceUsageUseCase::execute: id={}", id.as_str());

        // 既存の予約を取得
        let mut usage = self
            .repository
            .find_by_id(id)
            .await?
            .ok_or(ApplicationError::Repository(RepositoryError::NotFound))?;

        tracing::info!("  → 取得した予約のID: {}", usage.id().as_str());

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
                .await
                .map_err(|e| match e {
                    crate::domain::services::resource_usage::errors::ConflictCheckError::Conflict(
                        conflict_err,
                    ) => ApplicationError::ResourceConflict {
                        resource_description: conflict_err.resource_description.clone(),
                        conflicting_usage_id: conflict_err.conflicting_usage_id.as_str().to_string(),
                    },
                    crate::domain::services::resource_usage::errors::ConflictCheckError::Repository(
                        repo_err,
                    ) => ApplicationError::Repository(repo_err),
                })?;

            usage.update_time_period(new_period);
        }

        // 備考の更新
        if let Some(notes) = new_notes {
            usage.update_notes(notes);
        }

        // 更新
        tracing::info!("  → save呼び出し: usage.id()={}", usage.id().as_str());
        self.repository.save(&usage).await?;

        Ok(())
    }
}
