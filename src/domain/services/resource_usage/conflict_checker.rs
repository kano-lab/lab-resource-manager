use crate::domain::aggregates::resource_usage::value_objects::{Resource, TimePeriod, UsageId};
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::domain::services::resource_usage::errors::{ConflictCheckError, ResourceConflictError};

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
    ) -> Result<(), ConflictCheckError> {
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
                        return Err(ConflictCheckError::Conflict(ResourceConflictError::new(
                            new_resource.to_string(),
                            existing_usage.id().clone(),
                        )));
                    }
                }
            }
        }

        Ok(())
    }
}
