use crate::application::error::ApplicationError;
use crate::domain::aggregates::reservation::value_objects::ReservedSlotId;
use crate::domain::common::EmailAddress;
use crate::domain::common::TimePeriod;
use crate::domain::ports::repositories::{
    RepositoryError, ReservationRepository, ReservedSlotQuery,
};
use crate::domain::services::ResourceConflictChecker;
use std::sync::Arc;
use tracing::info;

/// 予約枠を更新するユースケース
pub struct UpdateSlotUseCase<R: ReservationRepository + ReservedSlotQuery> {
    repository: Arc<R>,
    conflict_checker: ResourceConflictChecker,
}

impl<R: ReservationRepository + ReservedSlotQuery> UpdateSlotUseCase<R> {
    /// 新しいUpdateSlotUseCaseインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - 予約を読み書きし、他の予約枠との競合を確かめるリポジトリ
    pub fn new(repository: Arc<R>) -> Self {
        Self {
            repository,
            conflict_checker: ResourceConflictChecker::new(),
        }
    }

    /// 予約枠を更新
    ///
    /// # Arguments
    /// * `id` - 予約枠ID
    /// * `requested_by` - 操作者のメールアドレス（予約者本人であること）
    /// * `new_time_period` - 新しい使用期間（Noneの場合は変更なし）
    /// * `new_notes` - 新しい備考（Noneの場合は変更なし）
    ///
    /// 変えるものが何も指定されていなければ、何もしない。
    ///
    /// # Returns
    /// 更新成功
    ///
    /// # Errors
    /// - 指定されたIDの予約枠が見つからない場合
    /// - 操作者が予約者でない場合
    /// - 新しい時間枠が競合する場合
    /// - 読んだあとに予約が変更または取り消されていた場合（`RepositoryError::Conflict`）
    /// - リポジトリエラー
    pub async fn execute(
        &self,
        id: &ReservedSlotId,
        requested_by: &EmailAddress,
        new_time_period: Option<TimePeriod>,
        new_notes: Option<String>,
    ) -> Result<(), ApplicationError> {
        if new_time_period.is_none() && new_notes.is_none() {
            return Ok(());
        }

        let mut reservation = self
            .repository
            .find(id.reservation_id())
            .await?
            .ok_or(ApplicationError::Repository(RepositoryError::NotFound))?;

        if let Some(new_period) = new_time_period {
            // 予約者でなければここで断る。競合の判定より先に行い、他人の予約枠の
            // 重なり具合を予約者以外へ見せない
            reservation.reschedule_slot(id.key(), new_period.clone(), requested_by)?;
            // 新しい期間が他の予約枠と重ならないか（自分自身は除く）
            self.conflict_checker
                .check_conflicts(
                    self.repository.as_ref(),
                    &new_period,
                    reservation.resources(),
                    Some(id),
                )
                .await?;
        }

        if let Some(notes) = new_notes {
            reservation.change_slot_notes(id.key(), notes, requested_by)?;
        }

        self.repository.save(&mut reservation).await?;
        let slot = reservation
            .slot(id.key())
            .ok_or(ApplicationError::Repository(RepositoryError::NotFound))?;

        info!(
            slot_id = %id.as_str(),
            holder = %reservation.holder_email().as_str(),
            start = %slot.time_period().start(),
            end = %slot.time_period().end(),
            "reserved slot updated"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::reservation::value_objects::{Gpu, Resource};
    use crate::domain::read_models::SlotView;
    use crate::infrastructure::repositories::reservation::mock::MockReservationRepository;
    use chrono::{DateTime, Duration, Utc};

    fn email(address: &str) -> EmailAddress {
        EmailAddress::new(address.to_string()).unwrap()
    }

    fn slot_of(holder: &str, start: DateTime<Utc>, end: DateTime<Utc>) -> SlotView {
        SlotView::of_new_one_time_reservation(
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

    #[tokio::test]
    async fn the_holder_can_move_the_slot_and_rewrite_its_notes() {
        let start = Utc::now() + Duration::hours(1);
        let slot = slot_of("holder@example.com", start, start + Duration::hours(1));
        let repository = Arc::new(MockReservationRepository::new());
        repository.put_slot(&slot);
        let later =
            TimePeriod::new(start + Duration::hours(2), start + Duration::hours(4)).unwrap();

        UpdateSlotUseCase::new(repository.clone())
            .execute(
                slot.id(),
                &email("holder@example.com"),
                Some(later.clone()),
                Some("学習の続き".to_string()),
            )
            .await
            .unwrap();

        let stored = repository.find_slot(slot.id()).await.unwrap().unwrap();
        assert_eq!(stored.time_period(), &later);
        assert_eq!(stored.notes().map(String::as_str), Some("学習の続き"));
    }

    #[tokio::test]
    async fn someone_else_is_refused_before_learning_about_conflicts() {
        let start = Utc::now() + Duration::hours(1);
        let mine = slot_of("holder@example.com", start, start + Duration::hours(1));
        let theirs = slot_of(
            "other@example.com",
            start + Duration::hours(2),
            start + Duration::hours(3),
        );
        let repository = Arc::new(MockReservationRepository::new());
        repository.put_slot(&mine);
        repository.put_slot(&theirs);
        let onto_theirs = theirs.time_period().clone();

        let error = UpdateSlotUseCase::new(repository.clone())
            .execute(
                mine.id(),
                &email("intruder@example.com"),
                Some(onto_theirs),
                None,
            )
            .await
            .unwrap_err();

        assert!(
            matches!(error, ApplicationError::Unauthorized(_)),
            "予約者でない人には、他の予約枠との重なりを答えない: {error:?}"
        );
        let stored = repository.find_slot(mine.id()).await.unwrap().unwrap();
        assert_eq!(stored, mine, "断った操作は予約枠を変えない");
    }

    #[tokio::test]
    async fn moving_onto_another_slot_is_refused_as_a_conflict() {
        let start = Utc::now() + Duration::hours(1);
        let mine = slot_of("holder@example.com", start, start + Duration::hours(1));
        let theirs = slot_of(
            "other@example.com",
            start + Duration::hours(2),
            start + Duration::hours(3),
        );
        let repository = Arc::new(MockReservationRepository::new());
        repository.put_slot(&mine);
        repository.put_slot(&theirs);

        let error = UpdateSlotUseCase::new(repository.clone())
            .execute(
                mine.id(),
                &email("holder@example.com"),
                Some(theirs.time_period().clone()),
                None,
            )
            .await
            .unwrap_err();

        assert!(
            matches!(error, ApplicationError::ResourceConflict { .. }),
            "{error:?}"
        );
        let stored = repository.find_slot(mine.id()).await.unwrap().unwrap();
        assert_eq!(stored, mine);
    }
}
