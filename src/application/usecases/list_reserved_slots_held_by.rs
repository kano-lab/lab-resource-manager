use crate::application::error::ApplicationError;
use crate::domain::common::EmailAddress;
use crate::domain::common::TimePeriod;
use crate::domain::ports::repositories::ReservedSlotQuery;
use crate::domain::read_models::ReservedSlotView;
use std::sync::Arc;

/// ユーザーの予約枠一覧を取得するユースケース
pub struct ListReservedSlotsHeldByUseCase<R: ReservedSlotQuery> {
    repository: Arc<R>,
}

impl<R: ReservedSlotQuery> ListReservedSlotsHeldByUseCase<R> {
    /// 新しいListReservedSlotsHeldByUseCaseインスタンスを作成
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
    /// ReservedSlotViewのリスト（時系列順）
    ///
    /// # Errors
    /// - リポジトリエラー
    pub async fn execute(
        &self,
        holder_email: &EmailAddress,
        time_period: &TimePeriod,
    ) -> Result<Vec<ReservedSlotView>, ApplicationError> {
        let mut slots = self
            .repository
            .find_slots_held_by(holder_email, time_period)
            .await?;

        // 開始時刻でソート
        slots.sort_by_key(|a| a.time_period().start());

        Ok(slots)
    }
}
