use crate::application::error::ApplicationError;
use crate::domain::aggregates::reservation::ReservedSlot;
use crate::domain::aggregates::reservation::value_objects::TimePeriod;
use crate::domain::ports::repositories::ReservationRepository;
use std::sync::Arc;

/// 指定期間の予約枠を一覧するユースケース
pub struct ListSlotsUseCase<R: ReservationRepository> {
    repository: Arc<R>,
}

impl<R: ReservationRepository> ListSlotsUseCase<R> {
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
    /// ReservedSlotのリスト（時系列順）
    ///
    /// # Errors
    /// - リポジトリエラー
    pub async fn execute(
        &self,
        time_period: &TimePeriod,
    ) -> Result<Vec<ReservedSlot>, ApplicationError> {
        let mut slots = self.repository.find_slots_overlapping(time_period).await?;

        // 開始時刻でソート
        slots.sort_by_key(|a| a.time_period().start());

        Ok(slots)
    }
}
