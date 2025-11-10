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
