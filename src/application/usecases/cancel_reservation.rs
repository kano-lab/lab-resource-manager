use crate::application::error::ApplicationError;
use crate::domain::aggregates::reservation::ReservationId;
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::{RepositoryError, ReservationRepository};
use crate::domain::services::{AuthorizationPolicy, ReservationAuthorizationPolicy};
use std::sync::Arc;
use tracing::info;

/// 予約を取り消すユースケース
///
/// 予約を取り消すと、その予約枠も無くなる。
pub struct CancelReservationUseCase<R: ReservationRepository> {
    repository: Arc<R>,
    authorization_policy: ReservationAuthorizationPolicy,
}

impl<R: ReservationRepository> CancelReservationUseCase<R> {
    /// 新しいインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - 予約リポジトリ
    pub fn new(repository: Arc<R>) -> Self {
        Self {
            repository,
            authorization_policy: ReservationAuthorizationPolicy::new(),
        }
    }

    /// 予約を取り消す
    ///
    /// # Arguments
    /// * `id` - 予約ID
    /// * `requested_by` - 操作者のメールアドレス（権限チェック用）
    ///
    /// # Errors
    /// - 指定されたIDの予約が見つからない場合
    /// - 操作者が予約者でない場合
    /// - リポジトリエラー
    pub async fn execute(
        &self,
        id: &ReservationId,
        requested_by: &EmailAddress,
    ) -> Result<(), ApplicationError> {
        let reservation = self
            .repository
            .find_reservation(id)
            .await?
            .ok_or(ApplicationError::Repository(RepositoryError::NotFound))?;

        self.authorization_policy
            .authorize_delete(requested_by, &reservation)
            .map_err(|e| ApplicationError::Unauthorized(e.to_string()))?;

        self.repository.delete_reservation(id).await?;

        info!(
            reservation_id = %id.as_str(),
            holder = %reservation.holder_email().as_str(),
            requested_by = %requested_by.as_str(),
            "reservation cancelled"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::reservation::{
        ReservedSlot,
        value_objects::{Gpu, Resource},
    };
    use crate::domain::common::TimePeriod;
    use crate::infrastructure::repositories::reservation::mock::MockReservationRepository;
    use chrono::{Duration, Utc};

    fn email(address: &str) -> EmailAddress {
        EmailAddress::new(address.to_string()).unwrap()
    }

    fn slot_in(days_ahead: i64) -> ReservedSlot {
        let start = Utc::now() + Duration::days(days_ahead);
        ReservedSlot::new(
            email("holder@example.com"),
            TimePeriod::new(start, start + Duration::hours(2)).unwrap(),
            vec![Resource::Gpu(Gpu::new(
                "Thalys".to_string(),
                0,
                "A100".to_string(),
            ))],
            None,
        )
        .unwrap()
    }

    async fn remaining_slots(repository: &MockReservationRepository) -> usize {
        let window = TimePeriod::new(Utc::now(), Utc::now() + Duration::days(30)).unwrap();
        repository
            .find_slots_overlapping(&window)
            .await
            .unwrap()
            .len()
    }

    #[tokio::test]
    async fn cancelling_a_one_time_reservation_removes_its_slot() {
        let repository = Arc::new(MockReservationRepository::new());
        let slot = slot_in(1);
        repository.save_slot(&slot).await.unwrap();
        let usecase = CancelReservationUseCase::new(repository.clone());

        usecase
            .execute(slot.reservation_id(), &email("holder@example.com"))
            .await
            .unwrap();

        assert_eq!(remaining_slots(&repository).await, 0);
    }

    #[tokio::test]
    async fn only_the_holder_can_cancel_a_reservation() {
        let repository = Arc::new(MockReservationRepository::new());
        let slot = slot_in(1);
        repository.save_slot(&slot).await.unwrap();
        let usecase = CancelReservationUseCase::new(repository.clone());

        let error = usecase
            .execute(slot.reservation_id(), &email("someone@example.com"))
            .await
            .unwrap_err();

        assert!(
            matches!(error, ApplicationError::Unauthorized(_)),
            "{error:?}"
        );
        assert_eq!(
            remaining_slots(&repository).await,
            1,
            "拒否された操作で消えてはいけない"
        );
    }
}
