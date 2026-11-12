use crate::application::error::ApplicationError;
use crate::domain::common::TimePeriod;
use crate::domain::ports::repositories::ReservedSlotQuery;
use crate::domain::read_models::SlotView;
use std::sync::Arc;

/// 指定期間の予約枠を一覧するユースケース
pub struct ListSlotsUseCase<R: ReservedSlotQuery> {
    repository: Arc<R>,
}

impl<R: ReservedSlotQuery> ListSlotsUseCase<R> {
    /// 新しいListSlotsUseCaseインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - 予約リポジトリ
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }

    /// 指定期間に重なる予約枠を取得
    ///
    /// # Arguments
    /// * `time_period` - 一覧する期間。どこまで先を見るかは呼び出し側が決める
    ///
    /// # Returns
    /// SlotViewのリスト（時系列順）
    ///
    /// # Errors
    /// - リポジトリエラー
    pub async fn execute(
        &self,
        time_period: &TimePeriod,
    ) -> Result<Vec<SlotView>, ApplicationError> {
        let mut slots = self.repository.find_slots_overlapping(time_period).await?;

        // 開始時刻でソート
        slots.sort_by_key(|a| a.time_period().start());

        Ok(slots)
    }
}
