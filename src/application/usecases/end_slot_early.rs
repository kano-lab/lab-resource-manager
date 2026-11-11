use crate::application::error::ApplicationError;
use crate::domain::aggregates::reservation::ReservedSlot;
use crate::domain::aggregates::reservation::value_objects::ReservedSlotId;
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::{RepositoryError, ReservationRepository};
use crate::domain::services::{AuthorizationPolicy, ReservationAuthorizationPolicy};
use chrono::Utc;
use std::sync::Arc;
use tracing::info;

/// 進行中の予約を今の時点で締めるユースケース
///
/// 取り消しでは「誰がいつ使っていたか」まで消えてしまう。予定より早く使い終わったときに、
/// 使った分を記録として残したまま残り時間だけを他の利用者へ開くための操作である。
///
/// 締める時刻は常に現在時刻とする。任意の終了時刻を指定したい場合は
/// `UpdateSlotUseCase`が担う。
pub struct EndSlotEarlyUseCase<R: ReservationRepository> {
    repository: Arc<R>,
    authorization_policy: ReservationAuthorizationPolicy,
}

impl<R: ReservationRepository> EndSlotEarlyUseCase<R> {
    /// 新しいEndSlotEarlyUseCaseインスタンスを作成
    ///
    /// # Arguments
    /// * `repository` - ReservedSlotリポジトリ
    pub fn new(repository: Arc<R>) -> Self {
        Self {
            repository,
            authorization_policy: ReservationAuthorizationPolicy::new(),
        }
    }

    /// 予約を今の時点で締める
    ///
    /// # Arguments
    /// * `id` - 使用予定ID
    /// * `requested_by` - 操作者のメールアドレス（権限チェック用）
    ///
    /// # Returns
    /// 締めたあとの予約（終了時刻が現在時刻に更新されている）
    ///
    /// # Errors
    /// - 指定されたIDの予約が見つからない場合
    /// - 所有者が一致しない場合
    /// - 予約がまだ始まっていない、またはすでに終わっている場合
    /// - リポジトリエラー
    pub async fn execute(
        &self,
        id: &ReservedSlotId,
        requested_by: &EmailAddress,
    ) -> Result<ReservedSlot, ApplicationError> {
        let mut usage = self
            .repository
            .find_slot(id)
            .await?
            .ok_or(ApplicationError::Repository(RepositoryError::NotFound))?;

        self.authorization_policy
            .authorize_update(requested_by, &usage)
            .map_err(|e| ApplicationError::Unauthorized(e.to_string()))?;

        // 期間を縮める操作のため、新たに占有する時間はなく競合は起こりえない
        usage.end_early(Utc::now())?;

        self.repository.save_slot(&usage).await?;

        info!(
            usage_id = %usage.id().as_str(),
            owner = %usage.holder_email().as_str(),
            requested_by = %requested_by.as_str(),
            start = %usage.time_period().start(),
            end = %usage.time_period().end(),
            "reservation released early"
        );

        Ok(usage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::reservation::value_objects::{Gpu, Resource, TimePeriod};
    use crate::infrastructure::repositories::reservation::mock::MockReservationRepository;
    use chrono::{DateTime, Duration};

    fn email(address: &str) -> EmailAddress {
        EmailAddress::new(address.to_string()).unwrap()
    }

    fn usage_of(owner: &str, start: DateTime<Utc>, end: DateTime<Utc>) -> ReservedSlot {
        ReservedSlot::new(
            email(owner),
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

    async fn repository_with(usage: &ReservedSlot) -> Arc<MockReservationRepository> {
        let repository = Arc::new(MockReservationRepository::new());
        repository.save_slot(usage).await.unwrap();
        repository
    }

    #[tokio::test]
    async fn the_reservation_ends_now_and_stays_in_the_repository() {
        let start = Utc::now() - Duration::hours(1);
        let usage = usage_of("owner@example.com", start, start + Duration::hours(4));
        let repository = repository_with(&usage).await;
        let usecase = EndSlotEarlyUseCase::new(repository.clone());

        let released = usecase
            .execute(usage.id(), &email("owner@example.com"))
            .await
            .unwrap();

        assert_eq!(released.time_period().start(), start);
        assert!(released.time_period().end() < start + Duration::hours(4));

        let stored = repository.find_slot(usage.id()).await.unwrap();
        assert_eq!(
            stored.map(|stored| stored.time_period().end()),
            Some(released.time_period().end()),
            "早期終了は予約を消さず、終了時刻の更新として残る"
        );
    }

    #[tokio::test]
    async fn someone_else_cannot_release_a_reservation() {
        let start = Utc::now() - Duration::hours(1);
        let usage = usage_of("owner@example.com", start, start + Duration::hours(4));
        let repository = repository_with(&usage).await;
        let usecase = EndSlotEarlyUseCase::new(repository.clone());

        let error = usecase
            .execute(usage.id(), &email("someone@example.com"))
            .await
            .unwrap_err();

        assert!(
            matches!(error, ApplicationError::Unauthorized(_)),
            "所有者以外は締められない: {:?}",
            error
        );
        let stored = repository.find_slot(usage.id()).await.unwrap().unwrap();
        assert_eq!(
            stored.time_period().end(),
            start + Duration::hours(4),
            "拒否された操作で予約が変わってはいけない"
        );
    }

    #[tokio::test]
    async fn a_reservation_that_has_not_started_is_rejected() {
        let start = Utc::now() + Duration::hours(1);
        let usage = usage_of("owner@example.com", start, start + Duration::hours(2));
        let repository = repository_with(&usage).await;
        let usecase = EndSlotEarlyUseCase::new(repository.clone());

        let error = usecase
            .execute(usage.id(), &email("owner@example.com"))
            .await
            .unwrap_err();

        assert!(
            matches!(error, ApplicationError::ReservedSlot(_)),
            "取り消すべき予約を締めようとした: {:?}",
            error
        );
    }

    #[tokio::test]
    async fn a_missing_reservation_is_reported_as_not_found() {
        let repository = Arc::new(MockReservationRepository::new());
        let usecase = EndSlotEarlyUseCase::new(repository);

        let error = usecase
            .execute(
                &ReservedSlotId::from_string("unknown".to_string()),
                &email("owner@example.com"),
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
