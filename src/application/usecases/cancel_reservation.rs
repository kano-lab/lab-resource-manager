use crate::application::error::ApplicationError;
use crate::domain::aggregates::reservation::ReservationId;
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::{RepositoryError, ReservationRepository};
use crate::domain::services::{AuthorizationPolicy, ReservationAuthorizationPolicy};
use std::sync::Arc;
use tracing::info;

/// 予約を取り消すユースケース
///
/// 定期予約を取り消すと、そのすべての予約枠が無くなる。予約枠を1つだけ取り消すのは
/// `CancelSlotUseCase`が担う。
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
            kind = ?reservation.kind(),
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
        RecurringReservation, ReservedSlot,
        value_objects::{Gpu, Resource, TimePeriod},
    };
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

    /// 毎週の定期予約と、その予約枠を3つ登録する
    async fn weekly(repository: &MockReservationRepository) -> ReservationId {
        let id = ReservationId::from_string("weekly".to_string());
        let slots: Vec<ReservedSlot> = [1, 8, 15]
            .into_iter()
            .map(|days| slot_in(days).of_recurring_reservation(id.clone()))
            .collect();
        for slot in &slots {
            repository.save_slot(slot).await.unwrap();
        }
        repository.put_recurring_reservation(RecurringReservation::from_first_slot(
            id.clone(),
            &slots[0],
            None,
        ));
        id
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
    async fn cancelling_a_recurring_reservation_removes_every_slot() {
        let repository = Arc::new(MockReservationRepository::new());
        let id = weekly(&repository).await;
        let usecase = CancelReservationUseCase::new(repository.clone());

        usecase
            .execute(&id, &email("holder@example.com"))
            .await
            .unwrap();

        assert_eq!(remaining_slots(&repository).await, 0);
        assert!(repository.find_reservation(&id).await.unwrap().is_none());
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
        let id = weekly(&repository).await;
        let usecase = CancelReservationUseCase::new(repository.clone());

        let error = usecase
            .execute(&id, &email("someone@example.com"))
            .await
            .unwrap_err();

        assert!(
            matches!(error, ApplicationError::Unauthorized(_)),
            "{error:?}"
        );
        assert_eq!(
            remaining_slots(&repository).await,
            3,
            "拒否された操作で消えてはいけない"
        );
    }
}
