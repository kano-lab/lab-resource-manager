use crate::application::error::ApplicationError;
use crate::domain::aggregates::reservation::{Reservation, ReservationId, value_objects::Resource};
use crate::domain::common::EmailAddress;
use crate::domain::common::TimePeriod;
use crate::domain::ports::repositories::{ReservationRepository, ReservedSlotQuery};
use crate::domain::services::ResourceConflictChecker;
use std::sync::Arc;
use tracing::info;

/// 単発予約を作るユースケース
pub struct CreateReservationUseCase<R: ReservationRepository + ReservedSlotQuery> {
    repository: Arc<R>,
    conflict_checker: ResourceConflictChecker,
}

impl<R: ReservationRepository + ReservedSlotQuery> CreateReservationUseCase<R> {
    /// 新しいインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - 予約を保存し、既存の予約枠との競合を確かめるリポジトリ
    pub fn new(repository: Arc<R>) -> Self {
        Self {
            repository,
            conflict_checker: ResourceConflictChecker::new(),
        }
    }

    /// 単発予約を作る
    ///
    /// # Arguments
    /// * `holder_email` - 予約者のメールアドレス
    /// * `time_period` - 期間
    /// * `resources` - 押さえるリソース
    /// * `notes` - 備考（オプション）
    ///
    /// # Returns
    /// 作られた単発予約のID（その予約枠のIDと同じ）
    ///
    /// # Errors
    /// - 指定期間に、同じリソースを押さえる予約枠がある場合
    /// - リポジトリエラー
    pub async fn execute(
        &self,
        holder_email: EmailAddress,
        time_period: TimePeriod,
        resources: Vec<Resource>,
        notes: Option<String>,
    ) -> Result<ReservationId, ApplicationError> {
        self.conflict_checker
            .check_conflicts(self.repository.as_ref(), &time_period, &resources, None)
            .await?;

        let mut reservation = Reservation::one_time(holder_email, time_period, resources, notes)?;
        self.repository.save(&mut reservation).await?;

        info!(
            reservation_id = %reservation.id().as_str(),
            holder = %reservation.holder_email().as_str(),
            resources = %reservation
                .resources()
                .iter()
                .map(|resource| resource.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            "reservation created"
        );

        Ok(reservation.id().clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::reservation::ReservedSlotId;
    use crate::domain::aggregates::reservation::value_objects::Gpu;
    use crate::infrastructure::repositories::reservation::mock::MockReservationRepository;
    use chrono::{Duration, Utc};

    fn email(address: &str) -> EmailAddress {
        EmailAddress::new(address.to_string()).unwrap()
    }

    fn gpu() -> Resource {
        Resource::Gpu(Gpu::new("Thalys".to_string(), 0, "A100".to_string()))
    }

    #[tokio::test]
    async fn a_new_one_time_reservation_shows_up_as_one_slot_with_the_same_id() {
        let repository = Arc::new(MockReservationRepository::new());
        let start = Utc::now() + Duration::hours(1);
        let period = TimePeriod::new(start, start + Duration::hours(2)).unwrap();

        let id = CreateReservationUseCase::new(repository.clone())
            .execute(
                email("holder@example.com"),
                period.clone(),
                vec![gpu()],
                None,
            )
            .await
            .unwrap();

        let slot = repository
            .find_slot(&ReservedSlotId::from_string(id.as_str().to_string()))
            .await
            .unwrap()
            .expect("単発予約の予約枠は予約と同じIDで読める");
        assert_eq!(slot.time_period(), &period);
        assert_eq!(slot.holder_email(), &email("holder@example.com"));
    }

    #[tokio::test]
    async fn a_reservation_that_overlaps_an_existing_slot_is_refused() {
        let repository = Arc::new(MockReservationRepository::new());
        let usecase = CreateReservationUseCase::new(repository.clone());
        let start = Utc::now() + Duration::hours(1);
        let period = TimePeriod::new(start, start + Duration::hours(2)).unwrap();
        usecase
            .execute(
                email("first@example.com"),
                period.clone(),
                vec![gpu()],
                None,
            )
            .await
            .unwrap();

        let error = usecase
            .execute(
                email("second@example.com"),
                period.clone(),
                vec![gpu()],
                None,
            )
            .await
            .unwrap_err();

        assert!(
            matches!(error, ApplicationError::ResourceConflict { .. }),
            "{error:?}"
        );
        assert_eq!(
            repository
                .find_slots_overlapping(&period)
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn a_reservation_may_start_exactly_when_another_ends_but_not_a_moment_earlier() {
        let repository = Arc::new(MockReservationRepository::new());
        let usecase = CreateReservationUseCase::new(repository.clone());
        let ten = Utc::now() + Duration::hours(1);
        let twelve = ten + Duration::hours(2);
        let fourteen = ten + Duration::hours(4);
        usecase
            .execute(
                email("first@example.com"),
                TimePeriod::new(ten, twelve).unwrap(),
                vec![gpu()],
                None,
            )
            .await
            .unwrap();

        let a_minute_early = usecase
            .execute(
                email("second@example.com"),
                TimePeriod::new(twelve - Duration::minutes(1), fourteen).unwrap(),
                vec![gpu()],
                None,
            )
            .await;
        let right_after = usecase
            .execute(
                email("second@example.com"),
                TimePeriod::new(twelve, fourteen).unwrap(),
                vec![gpu()],
                None,
            )
            .await;

        assert!(
            matches!(
                a_minute_early,
                Err(ApplicationError::ResourceConflict { .. })
            ),
            "{a_minute_early:?}"
        );
        assert!(right_after.is_ok(), "続けて予約できる: {right_after:?}");
    }
}
