use crate::application::error::ApplicationError;
use crate::domain::aggregates::reservation::ReservedSlot;
use crate::domain::aggregates::reservation::value_objects::TimePeriod;
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::ReservationRepository;
use std::sync::Arc;

/// ユーザーの予約枠一覧を取得するユースケース
pub struct ListSlotsHeldByUseCase<R: ReservationRepository> {
    repository: Arc<R>,
}

impl<R: ReservationRepository> ListSlotsHeldByUseCase<R> {
    /// 新しいListSlotsHeldByUseCaseインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - 予約リポジトリ
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }

    /// 指定期間に重なる、特定のユーザーが所有する予約枠の一覧を取得
    ///
    /// # Arguments
    /// * `holder_email` - 予約者のメールアドレス
    /// * `time_period` - 一覧する期間。どこまで先を見るかは呼び出し側が決める
    ///
    /// # Returns
    /// ReservedSlotのリスト（時系列順）
    ///
    /// # Errors
    /// - リポジトリエラー
    pub async fn execute(
        &self,
        holder_email: &EmailAddress,
        time_period: &TimePeriod,
    ) -> Result<Vec<ReservedSlot>, ApplicationError> {
        let mut slots = self
            .repository
            .find_slots_held_by(holder_email, time_period)
            .await?;

        // 開始時刻でソート
        slots.sort_by_key(|a| a.time_period().start());

        Ok(slots)
    }
}
