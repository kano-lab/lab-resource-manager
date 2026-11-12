use crate::application::error::ApplicationError;
use crate::domain::aggregates::reservation::{ReservationEvent, ReservedSlotId};
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::{RepositoryError, ReservationRepository};
use std::sync::Arc;
use tracing::info;

/// 予約枠を取り消すユースケース
///
/// 単発予約の予約枠を取り消すと、その単発予約も無くなる。
pub struct CancelSlotUseCase<R: ReservationRepository> {
    repository: Arc<R>,
}

impl<R: ReservationRepository> CancelSlotUseCase<R> {
    /// 新しいインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - 予約リポジトリ
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }

    /// 予約枠を取り消す
    ///
    /// # Arguments
    /// * `id` - 予約枠ID
    /// * `requested_by` - 操作者のメールアドレス（予約者本人であること）
    ///
    /// # Returns
    /// 取り消しによって起きた出来事（単発予約の予約枠なら、その予約の取り消し）
    ///
    /// # Errors
    /// - 指定されたIDの予約枠が見つからない場合
    /// - 操作者が予約者でない場合
    /// - 読んだあとに予約が変更または取り消されていた場合（`RepositoryError::Conflict`）
    /// - リポジトリエラー
    pub async fn execute(
        &self,
        id: &ReservedSlotId,
        requested_by: &EmailAddress,
    ) -> Result<ReservationEvent, ApplicationError> {
        let mut reservation = self
            .repository
            .find(id.reservation_id())
            .await?
            .ok_or(ApplicationError::Repository(RepositoryError::NotFound))?;
        let slot = reservation
            .slot(id.key())
            .ok_or(ApplicationError::Repository(RepositoryError::NotFound))?;

        let holder = reservation.holder_email().clone();
        let time_period = slot.time_period().clone();
        let event = reservation.cancel_slot(id.key(), requested_by)?;
        self.repository.save(&mut reservation).await?;

        info!(
            slot_id = %id.as_str(),
            reservation_id = %event.subject_id(),
            holder = %holder.as_str(),
            requested_by = %requested_by.as_str(),
            start = %time_period.start(),
            end = %time_period.end(),
            "reserved slot cancelled"
        );

        Ok(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::reservation::value_objects::{Gpu, Resource};
    use crate::domain::common::TimePeriod;
    use crate::domain::read_models::SlotView;
    use crate::infrastructure::repositories::reservation::mock::MockReservationRepository;
    use chrono::{Duration, Utc};

    fn email(address: &str) -> EmailAddress {
        EmailAddress::new(address.to_string()).unwrap()
    }

    fn upcoming_slot() -> SlotView {
        let start = Utc::now() + Duration::hours(1);
        SlotView::of_new_one_time_reservation(
            email("holder@example.com"),
            TimePeriod::new(start, start + Duration::hours(1)).unwrap(),
            vec![Resource::Gpu(Gpu::new(
                "Thalys".to_string(),
                0,
                "A100".to_string(),
            ))],
            None,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn cancelling_the_slot_of_a_one_time_reservation_cancels_the_reservation() {
        let slot = upcoming_slot();
        let repository = Arc::new(MockReservationRepository::new());
        repository.put_slot(&slot);

        let event = CancelSlotUseCase::new(repository.clone())
            .execute(slot.id(), &email("holder@example.com"))
            .await
            .unwrap();

        assert!(
            matches!(&event, ReservationEvent::Cancelled(reservation) if reservation.id() == slot.reservation_id()),
            "{event:?}"
        );
        assert!(repository.find_slot(slot.id()).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn someone_else_cannot_cancel_the_slot() {
        let slot = upcoming_slot();
        let repository = Arc::new(MockReservationRepository::new());
        repository.put_slot(&slot);

        let error = CancelSlotUseCase::new(repository.clone())
            .execute(slot.id(), &email("other@example.com"))
            .await
            .unwrap_err();

        assert!(
            matches!(error, ApplicationError::Unauthorized(_)),
            "{error:?}"
        );
        assert!(
            repository.find_slot(slot.id()).await.unwrap().is_some(),
            "断った取り消しで予約枠を消してはいけない"
        );
    }
}
