use crate::application::error::ApplicationError;
use crate::domain::aggregates::reservation::{Reservation, ReservationId};
use crate::domain::ports::repositories::{RepositoryError, ReservationRepository};
use std::sync::Arc;

/// IDで予約（単発予約・定期予約）を取得するユースケース
pub struct GetReservationUseCase<R: ReservationRepository> {
    repository: Arc<R>,
}

impl<R: ReservationRepository> GetReservationUseCase<R> {
    /// 新しいインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - 予約リポジトリ
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }

    /// 指定されたIDの予約を取得
    ///
    /// # Errors
    /// - 指定されたIDの予約が見つからない場合（予約枠IDを指定した場合を含む）
    /// - リポジトリエラー
    pub async fn execute(&self, id: &ReservationId) -> Result<Reservation, ApplicationError> {
        self.repository
            .find(id)
            .await?
            .ok_or(ApplicationError::Repository(RepositoryError::NotFound))
    }
}
