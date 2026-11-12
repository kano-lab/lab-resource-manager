use crate::application::error::ApplicationError;
use crate::domain::aggregates::reservation::{ReservationEvent, ReservationId};
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::{RepositoryError, ReservationRepository};
use std::sync::Arc;
use tracing::info;

/// 予約を取り消すユースケース
///
/// 定期予約を取り消すと、そのすべての予約枠が無くなる。予約枠を1つだけ取り消すのは
/// `CancelReservedSlotUseCase`が担う。
pub struct CancelReservationUseCase<R: ReservationRepository> {
    repository: Arc<R>,
}

impl<R: ReservationRepository> CancelReservationUseCase<R> {
    /// 新しいインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - 予約リポジトリ
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }

    /// 予約を取り消す
    ///
    /// # Arguments
    /// * `id` - 予約ID
    /// * `requested_by` - 操作者のメールアドレス（予約者本人であること）
    ///
    /// # Returns
    /// 取り消しによって起きた出来事
    ///
    /// # Errors
    /// - 指定されたIDの予約が見つからない場合（予約枠IDを指定した場合を含む）
    /// - 操作者が予約者でない場合
    /// - 読んだあとに予約が変更または取り消されていた場合（`RepositoryError::Conflict`）
    /// - リポジトリエラー
    pub async fn execute(
        &self,
        id: &ReservationId,
        requested_by: &EmailAddress,
    ) -> Result<ReservationEvent, ApplicationError> {
        let mut reservation = self
            .repository
            .find(id)
            .await?
            .ok_or(ApplicationError::Repository(RepositoryError::NotFound))?;

        let event = reservation.cancel(requested_by)?;
        self.repository.save(&mut reservation).await?;

        info!(
            reservation_id = %id.as_str(),
            recurring = reservation.is_recurring(),
            holder = %reservation.holder_email().as_str(),
            requested_by = %requested_by.as_str(),
            "reservation cancelled"
        );

        Ok(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::reservation::{
        OccurrenceKey, RecurrenceRule, RecurringSchedule, Reservation, Schedule,
        value_objects::{Gpu, Resource},
    };
    use crate::domain::common::TimePeriod;
    use crate::infrastructure::repositories::reservation::mock::MockReservationRepository;
    use chrono::{Duration, Utc};

    fn email(address: &str) -> EmailAddress {
        EmailAddress::new(address.to_string()).unwrap()
    }

    fn gpu() -> Vec<Resource> {
        vec![Resource::Gpu(Gpu::new(
            "Thalys".to_string(),
            0,
            "A100".to_string(),
        ))]
    }

    /// 明日から毎日2時間の定期予約
    fn daily() -> Reservation {
        let start = Utc::now() + Duration::days(1);
        Reservation::reconstruct(
            ReservationId::from_string("series".to_string()),
            email("holder@example.com"),
            gpu(),
            None,
            Schedule::Recurring(RecurringSchedule::new(
                TimePeriod::new(start, start + Duration::hours(2)).unwrap(),
                RecurrenceRule::parse(vec!["RRULE:FREQ=DAILY".to_string()]).unwrap(),
                chrono_tz::UTC,
                Default::default(),
            )),
            None,
        )
        .unwrap()
    }

    fn stored(reservation: &Reservation) -> Arc<MockReservationRepository> {
        let repository = Arc::new(MockReservationRepository::new());
        repository.put_reservation(reservation);
        repository
    }

    #[tokio::test]
    async fn cancelling_a_recurring_reservation_removes_every_slot() {
        let series = daily();
        let repository = stored(&series);

        let event = CancelReservationUseCase::new(repository.clone())
            .execute(series.id(), &email("holder@example.com"))
            .await
            .unwrap();

        assert!(
            matches!(&event, ReservationEvent::Cancelled(reservation) if reservation.is_recurring()),
            "{event:?}"
        );
        assert!(repository.find(series.id()).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn cancelling_a_one_time_reservation_removes_it() {
        let start = Utc::now() + Duration::hours(1);
        let reservation = Reservation::one_time(
            email("holder@example.com"),
            TimePeriod::new(start, start + Duration::hours(1)).unwrap(),
            gpu(),
            None,
        )
        .unwrap();
        let repository = stored(&reservation);

        CancelReservationUseCase::new(repository.clone())
            .execute(reservation.id(), &email("holder@example.com"))
            .await
            .unwrap();

        assert!(repository.find(reservation.id()).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn someone_else_cannot_cancel_the_reservation() {
        let series = daily();
        let repository = stored(&series);

        let error = CancelReservationUseCase::new(repository.clone())
            .execute(series.id(), &email("other@example.com"))
            .await
            .unwrap_err();

        assert!(
            matches!(error, ApplicationError::Unauthorized(_)),
            "{error:?}"
        );
        assert!(repository.find(series.id()).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn the_id_of_one_slot_does_not_cancel_the_reservation() {
        let series = daily();
        let repository = stored(&series);
        let first_slot = series
            .slots_in(&TimePeriod::new(Utc::now(), Utc::now() + Duration::days(2)).unwrap())
            .remove(0);
        let slot_id = series.slot_id(first_slot.key().clone());
        assert!(matches!(slot_id.key(), OccurrenceKey::At(_)));

        let error = CancelReservationUseCase::new(repository.clone())
            .execute(
                &ReservationId::from_string(slot_id.as_str().to_string()),
                &email("holder@example.com"),
            )
            .await
            .unwrap_err();

        assert!(
            matches!(
                error,
                ApplicationError::Repository(RepositoryError::NotFound)
            ),
            "{error:?}"
        );
        assert!(repository.find(series.id()).await.unwrap().is_some());
    }
}
