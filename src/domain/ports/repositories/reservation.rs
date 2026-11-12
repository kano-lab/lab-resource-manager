use crate::domain::{aggregates::reservation::Reservation, ports::repositories::RepositoryError};
use async_trait::async_trait;

/// 予約のリポジトリポート
///
/// 予約は作る・取り消すの単位である。予約を加えると、その予約枠が
/// 予約枠のリポジトリ（[`super::ReservedSlotQuery`]）から読めるようになる。
#[async_trait]
pub trait ReservationRepository {
    /// 新しい予約を加える
    async fn add_reservation(&self, reservation: &Reservation) -> Result<(), RepositoryError>;
}
