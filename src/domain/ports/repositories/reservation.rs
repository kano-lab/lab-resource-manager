use crate::domain::{
    aggregates::reservation::{Reservation, ReservationId},
    ports::repositories::RepositoryError,
};
use async_trait::async_trait;

/// 予約のリポジトリポート
///
/// 予約（単発予約・定期予約）は作る・取り消すの単位である。予約を加えると、その予約枠が
/// 予約枠のリポジトリ（[`super::ReservedSlotRepository`]）から読めるようになる。
/// 予約を取り除くと、そのすべての予約枠が無くなる。
#[async_trait]
pub trait ReservationRepository {
    /// 新しい予約を加える
    async fn add_reservation(&self, reservation: &Reservation) -> Result<(), RepositoryError>;

    /// IDで予約を検索
    ///
    /// 取り消された予約は`None`を返す。
    async fn find_reservation(
        &self,
        id: &ReservationId,
    ) -> Result<Option<Reservation>, RepositoryError>;

    /// 取り消された予約を取り除く
    ///
    /// 定期予約を取り除くと、そのすべての予約枠が無くなる。
    ///
    /// # Errors
    /// 予約が無い場合、`RepositoryError::NotFound`
    async fn remove_reservation(&self, id: &ReservationId) -> Result<(), RepositoryError>;
}
