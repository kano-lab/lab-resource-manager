use crate::application::error::ApplicationError;
use crate::application::{Clock, SystemClock};
use crate::domain::aggregates::reservation::value_objects::ReservedSlotId;
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::{RepositoryError, ReservationRepository};
use crate::domain::read_models::ReservedSlotView;
use std::sync::Arc;
use tracing::info;

/// 進行中の予約枠を今の時点で締めるユースケース（今で終了）
///
/// 取り消しでは「誰がいつ使っていたか」まで消えてしまう。予定より早く使い終わったときに、
/// 使った分を記録として残したまま残り時間だけを他の利用者へ開くための操作である。
///
/// 締める時刻は常に現在時刻とする。任意の終了時刻を指定したい場合は
/// `UpdateReservedSlotUseCase`が担う。
pub struct EndReservedSlotEarlyUseCase<R: ReservationRepository> {
    repository: Arc<R>,
    clock: Arc<dyn Clock>,
}

impl<R: ReservationRepository> EndReservedSlotEarlyUseCase<R> {
    /// 新しいEndReservedSlotEarlyUseCaseインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - 予約リポジトリ
    pub fn new(repository: Arc<R>) -> Self {
        Self {
            repository,
            clock: Arc::new(SystemClock),
        }
    }

    /// 今の時刻を`clock`から読むようにする
    pub fn with_clock(self, clock: Arc<dyn Clock>) -> Self {
        Self { clock, ..self }
    }

    /// 予約枠を今の時点で締める
    ///
    /// # Arguments
    /// * `id` - 予約枠ID
    /// * `requested_by` - 操作者のメールアドレス（権限チェック用）
    ///
    /// # Returns
    /// 締めたあとの予約枠（終了時刻が現在時刻に更新されている）
    ///
    /// # Errors
    /// - 指定されたIDの予約枠が見つからない場合
    /// - 操作者が予約者でない場合
    /// - 予約枠がまだ始まっていない、またはすでに終わっている場合
    /// - 読んだあとに予約が変更または取り消されていた場合（`RepositoryError::Conflict`）
    /// - リポジトリエラー
    pub async fn execute(
        &self,
        id: &ReservedSlotId,
        requested_by: &EmailAddress,
    ) -> Result<ReservedSlotView, ApplicationError> {
        let mut reservation = self
            .repository
            .find(id.reservation_id())
            .await?
            .ok_or(ApplicationError::Repository(RepositoryError::NotFound))?;

        // 期間を縮める操作のため、新たに占有する時間はなく競合は起こりえない
        reservation.end_slot_early(id.key(), self.clock.now(), requested_by)?;

        self.repository.save(&mut reservation).await?;
        let slot = ReservedSlotView::of(
            &reservation,
            &reservation
                .slot(id.key())
                .ok_or(ApplicationError::Repository(RepositoryError::NotFound))?,
        );

        info!(
            reserved_slot_id = %slot.id().as_str(),
            holder = %slot.holder_email().as_str(),
            requested_by = %requested_by.as_str(),
            start = %slot.time_period().start(),
            end = %slot.time_period().end(),
            "reserved slot ended early"
        );

        Ok(slot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::clock::FixedClock;
    use crate::domain::aggregates::reservation::value_objects::{Gpu, Resource};
    use crate::domain::common::TimePeriod;
    use crate::infrastructure::repositories::reservation::mock::MockReservationRepository;
    use chrono::{DateTime, Duration, TimeZone, Utc};

    /// テストの「今」
    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 16, 2, 0, 0).unwrap()
    }

    fn usecase(
        repository: Arc<MockReservationRepository>,
    ) -> EndReservedSlotEarlyUseCase<MockReservationRepository> {
        EndReservedSlotEarlyUseCase::new(repository).with_clock(Arc::new(FixedClock::at(now())))
    }

    fn email(address: &str) -> EmailAddress {
        EmailAddress::new(address.to_string()).unwrap()
    }

    fn slot_of(holder: &str, start: DateTime<Utc>, end: DateTime<Utc>) -> ReservedSlotView {
        ReservedSlotView::of_new_one_time_reservation(
            email(holder),
            TimePeriod::new(start, end).unwrap(),
            vec![Resource::Gpu(Gpu::new(
                "Thalys".to_string(),
                0,
                "A100".to_string(),
            ))],
            None,
        )
        .unwrap()
    }

    async fn repository_with(slot: &ReservedSlotView) -> Arc<MockReservationRepository> {
        let repository = Arc::new(MockReservationRepository::new());
        repository.put_slot(slot);
        repository
    }

    #[tokio::test]
    async fn the_slot_ends_now_and_stays_in_the_repository() {
        let start = now() - Duration::hours(1);
        let slot = slot_of("holder@example.com", start, start + Duration::hours(4));
        let repository = repository_with(&slot).await;
        let usecase = usecase(repository.clone());

        let released = usecase
            .execute(slot.id(), &email("holder@example.com"))
            .await
            .unwrap();

        assert_eq!(released.time_period().start(), start);
        assert_eq!(released.time_period().end(), now(), "今の時刻で終わる");

        let stored = repository.find_slot(slot.id()).await.unwrap();
        assert_eq!(
            stored.map(|stored| stored.time_period().end()),
            Some(released.time_period().end()),
            "今で終了は予約枠を消さず、終了時刻の更新として残る"
        );
    }

    #[tokio::test]
    async fn someone_else_cannot_end_a_slot_early() {
        let start = now() - Duration::hours(1);
        let slot = slot_of("holder@example.com", start, start + Duration::hours(4));
        let repository = repository_with(&slot).await;
        let usecase = usecase(repository.clone());

        let error = usecase
            .execute(slot.id(), &email("someone@example.com"))
            .await
            .unwrap_err();

        assert!(
            matches!(error, ApplicationError::Unauthorized(_)),
            "予約者以外は締められない: {:?}",
            error
        );
        let stored = repository.find_slot(slot.id()).await.unwrap().unwrap();
        assert_eq!(
            stored.time_period().end(),
            start + Duration::hours(4),
            "拒否された操作で予約が変わってはいけない"
        );
    }

    #[tokio::test]
    async fn a_slot_that_has_not_started_is_rejected() {
        let start = now() + Duration::hours(1);
        let slot = slot_of("holder@example.com", start, start + Duration::hours(2));
        let repository = repository_with(&slot).await;
        let usecase = usecase(repository.clone());

        let error = usecase
            .execute(slot.id(), &email("holder@example.com"))
            .await
            .unwrap_err();

        assert!(
            matches!(error, ApplicationError::Reservation(_)),
            "取り消すべき予約を締めようとした: {:?}",
            error
        );
    }

    #[tokio::test]
    async fn a_missing_slot_is_reported_as_not_found() {
        let repository = Arc::new(MockReservationRepository::new());
        let usecase = usecase(repository);

        let error = usecase
            .execute(
                &ReservedSlotId::from_string("unknown".to_string()),
                &email("holder@example.com"),
            )
            .await
            .unwrap_err();

        assert!(
            matches!(
                error,
                ApplicationError::Repository(RepositoryError::NotFound)
            ),
            "存在しない予約: {:?}",
            error
        );
    }
}
