use crate::application::error::ApplicationError;
use crate::domain::aggregates::resource_usage::{
    entity::ResourceUsage,
    value_objects::{Resource, TimePeriod, UsageId},
};
use crate::domain::common::EmailAddress;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::domain::services::ResourceConflictChecker;
use chrono::{DateTime, Duration, Utc};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// 実利用検知にもとづく事後予約の備考
pub const POST_HOC_RESERVATION_NOTES: &str = "GPU実利用検知による事後予約";

/// ひとつの機会を表すキー（予約者・利用開始時刻・正規化したリソース名）
type SessionKey = (String, DateTime<Utc>, Vec<String>);

/// 事後予約の提案を受諾するユースケース
///
/// 提案の受諾は「予約を新しく作る」ではなく「この観測セッションの予約を、この長さにする」
/// という意味を持つ。利用者は提案メッセージの時間候補ボタンを押し直せるため、
/// 同じ観測セッションに対する受諾が複数回起きても予約は1件に保たれる必要がある。
pub struct AcceptReservationProposalUseCase<R: ResourceUsageRepository> {
    repository: Arc<R>,
    conflict_checker: ResourceConflictChecker,
    /// この機会について確定した予約のID
    ///
    /// 受諾ボタンは押し直せるため、同じ機会への受諾が連続して届く。ロックを兼ねており、
    /// ひとつの機会に対する処理は同時に1つしか進まない。
    settled_sessions: tokio::sync::Mutex<HashMap<SessionKey, UsageId>>,
}

impl<R: ResourceUsageRepository> AcceptReservationProposalUseCase<R> {
    /// 新しいユースケースインスタンスを作成
    pub fn new(repository: Arc<R>) -> Self {
        Self {
            repository,
            conflict_checker: ResourceConflictChecker::new(),
            settled_sessions: tokio::sync::Mutex::new(HashMap::new()),
        }
    }

    /// 提案を受諾して予約を確定する
    ///
    /// 同じ観測セッション（同一の予約者・利用開始時刻・リソース集合）に対する予約が
    /// 既に存在する場合は、その期間を指定された長さに更新する。存在しない場合は新規作成する。
    ///
    /// # Arguments
    /// * `owner_email` - 予約者
    /// * `resources` - 実利用が観測されたリソース
    /// * `active_since` - 利用開始時刻（予約の開始時刻になる）
    /// * `duration` - 受諾された利用時間
    ///
    /// # Returns
    /// 作成または更新された予約のID
    ///
    /// # Errors
    /// - 他の予約と競合する場合
    /// - リポジトリエラー
    pub async fn execute(
        &self,
        owner_email: EmailAddress,
        resources: Vec<Resource>,
        active_since: DateTime<Utc>,
        duration: Duration,
    ) -> Result<UsageId, ApplicationError> {
        let time_period = TimePeriod::new(active_since, active_since + duration)?;
        let session_key = Self::session_key(&owner_email, &resources, active_since);

        // 同じ機会への受諾を直列化する。連打で書き込みが検索へ反映される前に次の受諾が
        // 走ると、同じ機会の予約が二重に作られてしまう。
        let mut settled_sessions = self.settled_sessions.lock().await;

        let existing = match settled_sessions.get(&session_key) {
            // このプロセスで既に確定させた予約は、検索の反映を待たずIDで引く
            Some(settled_id) => self.repository.find_by_id(settled_id).await?,
            None => {
                self.find_same_session_reservation(
                    &owner_email,
                    &resources,
                    active_since,
                    &time_period,
                )
                .await?
            }
        };

        // 既存予約の期間を更新する場合、自分自身は競合対象から外す
        self.conflict_checker
            .check_conflicts(
                self.repository.as_ref(),
                &time_period,
                &resources,
                existing.as_ref().map(|usage| usage.id()),
            )
            .await?;

        let usage = match existing {
            Some(existing) => ResourceUsage::reconstruct(
                existing.id().clone(),
                owner_email,
                time_period,
                resources,
                existing.notes().cloned(),
            )?,
            None => ResourceUsage::new(
                owner_email,
                time_period,
                resources,
                Some(POST_HOC_RESERVATION_NOTES.to_string()),
            )?,
        };

        self.repository.save(&usage).await?;
        settled_sessions.insert(session_key, usage.id().clone());

        Ok(usage.id().clone())
    }

    /// ひとつの機会を表すキーを作る
    ///
    /// リソースの並びは呼び出し側に依存するため、表示名で並べ替えて正規化する。
    fn session_key(
        owner_email: &EmailAddress,
        resources: &[Resource],
        active_since: DateTime<Utc>,
    ) -> SessionKey {
        let mut normalized: Vec<String> = resources
            .iter()
            .map(|resource| resource.to_string())
            .collect();
        normalized.sort();

        (owner_email.as_str().to_string(), active_since, normalized)
    }

    /// 同じ観測セッションに対して既に作られた予約を探す
    ///
    /// 受諾しようとしている期間と重なる予約のうち、予約者・利用開始時刻・リソース集合が
    /// 一致するものを同一セッションとみなす。終了時刻が過去になっていても対象に含める
    /// 必要があるため、進行中・今後の予約に限定する検索は使わない。
    async fn find_same_session_reservation(
        &self,
        owner_email: &EmailAddress,
        resources: &[Resource],
        active_since: DateTime<Utc>,
        time_period: &TimePeriod,
    ) -> Result<Option<ResourceUsage>, ApplicationError> {
        let requested: HashSet<&Resource> = resources.iter().collect();

        let candidates = self.repository.find_overlapping(time_period).await?;

        Ok(candidates.into_iter().find(|usage| {
            usage.owner_email() == owner_email
                && usage.time_period().start() == active_since
                && usage.resources().iter().collect::<HashSet<_>>() == requested
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::resource_usage::value_objects::Gpu;
    use crate::domain::ports::repositories::RepositoryError;
    use crate::infrastructure::repositories::resource_usage::mock::MockUsageRepository;
    use async_trait::async_trait;
    use std::sync::Mutex as StdMutex;

    fn gpu(device_number: u32) -> Resource {
        Resource::Gpu(Gpu::new(
            "Thalys".to_string(),
            device_number,
            "A100".to_string(),
        ))
    }

    fn email(address: &str) -> EmailAddress {
        EmailAddress::new(address.to_string()).unwrap()
    }

    /// 事後予約として保存済みの予約を作る
    async fn save_post_hoc_reservation(
        repository: &MockUsageRepository,
        owner: &str,
        resources: Vec<Resource>,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> UsageId {
        let usage = ResourceUsage::new(
            email(owner),
            TimePeriod::new(start, end).unwrap(),
            resources,
            Some(POST_HOC_RESERVATION_NOTES.to_string()),
        )
        .unwrap();
        repository.save(&usage).await.unwrap();
        usage.id().clone()
    }

    /// 保存されている全予約（終了済みを含む）
    async fn all_reservations(repository: &MockUsageRepository) -> Vec<ResourceUsage> {
        let far_past = Utc::now() - Duration::days(365);
        let far_future = Utc::now() + Duration::days(365);
        repository
            .find_overlapping(&TimePeriod::new(far_past, far_future).unwrap())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn accepting_twice_for_the_same_session_updates_the_period_instead_of_adding() {
        let repository = Arc::new(MockUsageRepository::new());
        let active_since = Utc::now() - Duration::hours(1);
        let existing_id = save_post_hoc_reservation(
            &repository,
            "owner@example.com",
            vec![gpu(0), gpu(1)],
            active_since,
            active_since + Duration::hours(2),
        )
        .await;

        let usecase = AcceptReservationProposalUseCase::new(repository.clone());
        let returned_id = usecase
            .execute(
                email("owner@example.com"),
                vec![gpu(0), gpu(1)],
                active_since,
                Duration::hours(12),
            )
            .await
            .unwrap();

        assert_eq!(returned_id, existing_id, "既存予約が更新されるべき");

        let reservations = all_reservations(&repository).await;
        assert_eq!(reservations.len(), 1, "予約が増えてはいけない");
        assert_eq!(
            reservations[0].time_period().end(),
            active_since + Duration::hours(12),
            "受諾された長さに更新されるべき"
        );
    }

    #[tokio::test]
    async fn accepting_updates_even_when_the_existing_reservation_already_ended() {
        let repository = Arc::new(MockUsageRepository::new());
        // 2時間の受諾後に時間が経ち、既存予約の終了時刻が過去になっている状況
        let active_since = Utc::now() - Duration::hours(5);
        save_post_hoc_reservation(
            &repository,
            "owner@example.com",
            vec![gpu(0)],
            active_since,
            active_since + Duration::hours(2),
        )
        .await;

        let usecase = AcceptReservationProposalUseCase::new(repository.clone());
        usecase
            .execute(
                email("owner@example.com"),
                vec![gpu(0)],
                active_since,
                Duration::hours(12),
            )
            .await
            .unwrap();

        let reservations = all_reservations(&repository).await;
        assert_eq!(
            reservations.len(),
            1,
            "終了済みの予約も同一セッションとして扱い、更新するべき"
        );
    }

    #[tokio::test]
    async fn accepting_shorter_duration_shrinks_the_existing_reservation() {
        let repository = Arc::new(MockUsageRepository::new());
        let active_since = Utc::now() - Duration::hours(1);
        save_post_hoc_reservation(
            &repository,
            "owner@example.com",
            vec![gpu(0)],
            active_since,
            active_since + Duration::hours(12),
        )
        .await;

        let usecase = AcceptReservationProposalUseCase::new(repository.clone());
        usecase
            .execute(
                email("owner@example.com"),
                vec![gpu(0)],
                active_since,
                Duration::hours(2),
            )
            .await
            .unwrap();

        let reservations = all_reservations(&repository).await;
        assert_eq!(reservations.len(), 1);
        assert_eq!(
            reservations[0].time_period().end(),
            active_since + Duration::hours(2),
            "短い候補を押した場合は期間が縮むべき"
        );
    }

    #[tokio::test]
    async fn creates_a_new_reservation_when_no_session_matches() {
        let repository = Arc::new(MockUsageRepository::new());
        let active_since = Utc::now() - Duration::minutes(30);

        let usecase = AcceptReservationProposalUseCase::new(repository.clone());
        usecase
            .execute(
                email("owner@example.com"),
                vec![gpu(0), gpu(1)],
                active_since,
                Duration::hours(3),
            )
            .await
            .unwrap();

        let reservations = all_reservations(&repository).await;
        assert_eq!(reservations.len(), 1);
        assert_eq!(
            reservations[0].notes().map(String::as_str),
            Some(POST_HOC_RESERVATION_NOTES),
            "事後予約として備考が入るべき"
        );
        assert_eq!(
            reservations[0].resources().len(),
            2,
            "観測された全リソースが1件の予約になるべき"
        );
    }

    #[tokio::test]
    async fn rejects_when_another_user_holds_an_overlapping_reservation() {
        let repository = Arc::new(MockUsageRepository::new());
        let active_since = Utc::now() - Duration::hours(1);
        save_post_hoc_reservation(
            &repository,
            "someone-else@example.com",
            vec![gpu(0)],
            active_since,
            active_since + Duration::hours(6),
        )
        .await;

        let usecase = AcceptReservationProposalUseCase::new(repository.clone());
        let result = usecase
            .execute(
                email("owner@example.com"),
                vec![gpu(0)],
                active_since,
                Duration::hours(3),
            )
            .await;

        assert!(
            matches!(result, Err(ApplicationError::ResourceConflict { .. })),
            "他人の予約と重なる場合は競合として拒否するべき, got {result:?}"
        );
        assert_eq!(
            all_reservations(&repository).await.len(),
            1,
            "予約は増えない"
        );
    }

    #[tokio::test]
    async fn a_different_resource_set_is_treated_as_a_separate_session() {
        let repository = Arc::new(MockUsageRepository::new());
        let active_since = Utc::now() - Duration::hours(1);
        save_post_hoc_reservation(
            &repository,
            "owner@example.com",
            vec![gpu(0)],
            active_since,
            active_since + Duration::hours(2),
        )
        .await;

        // 同じ利用者・同じ開始時刻でも、別のGPUを使い始めた場合は別の予約になる
        let usecase = AcceptReservationProposalUseCase::new(repository.clone());
        usecase
            .execute(
                email("owner@example.com"),
                vec![gpu(1)],
                active_since,
                Duration::hours(2),
            )
            .await
            .unwrap();

        assert_eq!(all_reservations(&repository).await.len(), 2);
    }
    /// 保存がすぐには検索結果に現れないリポジトリ
    ///
    /// Google Calendarへ書き込んだ直後の検索に、その予約がまだ現れないことがある。
    /// ボタン連打で実際に重複予約が生まれたのはこの性質が原因なので、テストでも再現する。
    struct EventuallyConsistentRepository {
        /// 検索から見える予約
        visible: StdMutex<Vec<ResourceUsage>>,
        /// 保存されたがまだ検索から見えない予約
        pending: StdMutex<Vec<ResourceUsage>>,
    }

    impl EventuallyConsistentRepository {
        fn new() -> Self {
            Self {
                visible: StdMutex::new(Vec::new()),
                pending: StdMutex::new(Vec::new()),
            }
        }

        fn all(&self) -> Vec<ResourceUsage> {
            let mut all = self.visible.lock().unwrap().clone();
            all.extend(self.pending.lock().unwrap().iter().cloned());
            all
        }
    }

    #[async_trait]
    impl ResourceUsageRepository for EventuallyConsistentRepository {
        async fn find_by_id(&self, id: &UsageId) -> Result<Option<ResourceUsage>, RepositoryError> {
            // IDを指定した取得は書き込み直後でも一貫して読める
            Ok(self.all().into_iter().find(|usage| usage.id() == id))
        }

        async fn find_future(&self) -> Result<Vec<ResourceUsage>, RepositoryError> {
            Ok(self.visible.lock().unwrap().clone())
        }

        async fn find_overlapping(
            &self,
            time_period: &TimePeriod,
        ) -> Result<Vec<ResourceUsage>, RepositoryError> {
            Ok(self
                .visible
                .lock()
                .unwrap()
                .iter()
                .filter(|usage| usage.time_period().overlaps_with(time_period))
                .cloned()
                .collect())
        }

        async fn find_by_owner(
            &self,
            owner_email: &EmailAddress,
        ) -> Result<Vec<ResourceUsage>, RepositoryError> {
            Ok(self
                .visible
                .lock()
                .unwrap()
                .iter()
                .filter(|usage| usage.owner_email() == owner_email)
                .cloned()
                .collect())
        }

        async fn save(&self, usage: &ResourceUsage) -> Result<(), RepositoryError> {
            let mut pending = self.pending.lock().unwrap();
            if let Some(existing) = pending.iter_mut().find(|p| p.id() == usage.id()) {
                *existing = usage.clone();
            } else {
                pending.push(usage.clone());
            }
            Ok(())
        }

        async fn delete(&self, _id: &UsageId) -> Result<(), RepositoryError> {
            unimplemented!("このテストでは使用しない")
        }
    }

    #[tokio::test]
    async fn pressing_the_same_button_twice_in_a_row_creates_only_one_reservation() {
        let repository = Arc::new(EventuallyConsistentRepository::new());
        let usecase = Arc::new(AcceptReservationProposalUseCase::new(repository.clone()));
        let active_since = Utc::now() - Duration::minutes(5);

        // 1回目の書き込みが検索に反映される前に2回目が走る（ボタン連打）
        let first = {
            let usecase = usecase.clone();
            tokio::spawn(async move {
                usecase
                    .execute(
                        email("owner@example.com"),
                        vec![gpu(5)],
                        active_since,
                        Duration::hours(1),
                    )
                    .await
            })
        };
        let second = {
            let usecase = usecase.clone();
            tokio::spawn(async move {
                usecase
                    .execute(
                        email("owner@example.com"),
                        vec![gpu(5)],
                        active_since,
                        Duration::hours(1),
                    )
                    .await
            })
        };

        let first = first.await.unwrap();
        let second = second.await.unwrap();

        assert!(first.is_ok(), "1回目は成功するべき: {first:?}");
        assert!(
            second.is_ok(),
            "2回目も競合エラーにせず受け付けるべき: {second:?}"
        );
        assert_eq!(repository.all().len(), 1, "連打しても予約は1件であるべき");
    }

    #[tokio::test]
    async fn pressing_a_longer_duration_after_the_first_press_updates_the_same_reservation() {
        let repository = Arc::new(EventuallyConsistentRepository::new());
        let usecase = AcceptReservationProposalUseCase::new(repository.clone());
        let active_since = Utc::now() - Duration::minutes(5);

        let first = usecase
            .execute(
                email("owner@example.com"),
                vec![gpu(5)],
                active_since,
                Duration::hours(1),
            )
            .await
            .unwrap();

        // 反映される前に別の長さを選び直す
        let second = usecase
            .execute(
                email("owner@example.com"),
                vec![gpu(5)],
                active_since,
                Duration::hours(12),
            )
            .await
            .unwrap();

        assert_eq!(first, second, "同じ予約を指すべき");
        let all = repository.all();
        assert_eq!(all.len(), 1, "予約は増えない");
        assert_eq!(
            all[0].time_period().end(),
            active_since + Duration::hours(12),
            "選び直した長さに更新されるべき"
        );
    }
}
