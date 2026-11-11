use crate::application::error::ApplicationError;
use crate::domain::aggregates::reservation::{ReservedSlot, ReservedSlotId};
use crate::domain::ports::repositories::{RepositoryError, ReservationRepository};
use std::sync::Arc;

/// IDで予約枠を取得するユースケース
pub struct GetSlotUseCase<R: ReservationRepository> {
    repository: Arc<R>,
}

impl<R: ReservationRepository> GetSlotUseCase<R> {
    /// 新しいインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - 予約リポジトリ
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }

    /// 指定されたIDの予約枠を取得
    ///
    /// # Arguments
    /// * `id` - 予約枠ID
    ///
    /// # Errors
    /// - 指定されたIDの予約枠が見つからない場合
    /// - リポジトリエラー
    pub async fn execute(&self, id: &ReservedSlotId) -> Result<ReservedSlot, ApplicationError> {
        self.repository
            .find_slot(id)
            .await?
            .ok_or(ApplicationError::Repository(RepositoryError::NotFound))
    }
}
