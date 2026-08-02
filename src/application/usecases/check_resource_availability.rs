use crate::application::error::ApplicationError;
use crate::domain::aggregates::resource_usage::value_objects::{Resource, TimePeriod};
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::domain::services::resource_usage::availability::{self, ResourceAvailability};
use std::sync::Arc;

/// 指定期間における各リソースの空きを調べるユースケース
pub struct CheckResourceAvailabilityUseCase<R: ResourceUsageRepository> {
    repository: Arc<R>,
}

impl<R: ResourceUsageRepository> CheckResourceAvailabilityUseCase<R> {
    /// 新しいCheckResourceAvailabilityUseCaseインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - ResourceUsageリポジトリ
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }

    /// 対象リソースの空きを取得
    ///
    /// # Arguments
    /// * `resources` - 対象のリソース。何が予約できるかは設定が持つ知識のため、
    ///   母集合は呼び出し側が決める
    /// * `window` - 調べる期間。どこまで先を見るかは呼び出し側が決める
    ///
    /// # Returns
    /// `resources`と同じ並びの空き情報
    ///
    /// # Errors
    /// - リポジトリエラー
    pub async fn execute(
        &self,
        resources: &[Resource],
        window: &TimePeriod,
    ) -> Result<Vec<ResourceAvailability>, ApplicationError> {
        let usages = self.repository.find_overlapping(window).await?;

        Ok(availability::calculate(resources, window, &usages))
    }
}
