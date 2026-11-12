use crate::application::error::ApplicationError;
use crate::domain::aggregates::reservation::value_objects::Resource;
use crate::domain::common::TimePeriod;
use crate::domain::ports::repositories::ReservedSlotRepository;
use crate::domain::services::reservation::availability::{self, ResourceAvailability};
use std::sync::Arc;

/// 指定期間における各リソースの空きを調べるユースケース
pub struct CheckResourceAvailabilityUseCase<R: ReservedSlotRepository> {
    repository: Arc<R>,
}

impl<R: ReservedSlotRepository> CheckResourceAvailabilityUseCase<R> {
    /// 新しいCheckResourceAvailabilityUseCaseインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - 予約リポジトリ
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
        let slots = self.repository.find_slots_overlapping(window).await?;

        Ok(availability::calculate(resources, window, &slots))
    }
}
