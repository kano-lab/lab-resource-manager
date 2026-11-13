use crate::application::error::ApplicationError;
use crate::application::{Clock, SystemClock};
use crate::domain::aggregates::reservation::{
    OccurrenceKey, Reservation,
    value_objects::{ReservedSlotId, Resource},
};
use crate::domain::common::EmailAddress;
use crate::domain::common::TimePeriod;
use crate::domain::ports::repositories::{ReservationRepository, ReservedSlotQuery};
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
pub struct AcceptReservationProposalUseCase<R: ReservationRepository + ReservedSlotQuery> {
    repository: Arc<R>,
    conflict_checker: ResourceConflictChecker,
    /// この機会について確定した予約のID
    ///
    /// 受諾ボタンは押し直せるため、同じ機会への受諾が連続して届く。ロックを兼ねており、
    /// ひとつの機会に対する処理は同時に1つしか進まない。
    settled_sessions: tokio::sync::Mutex<HashMap<SessionKey, ReservedSlotId>>,
    clock: Arc<dyn Clock>,
}

impl<R: ReservationRepository + ReservedSlotQuery> AcceptReservationProposalUseCase<R> {
    /// 新しいユースケースインスタンスを作成
    pub fn new(repository: Arc<R>) -> Self {
        Self {
            repository,
            conflict_checker: ResourceConflictChecker::new(),
            settled_sessions: tokio::sync::Mutex::new(HashMap::new()),
            clock: Arc::new(SystemClock),
        }
    }

    /// 今の時刻を`clock`から読むようにする
    pub fn with_clock(self, clock: Arc<dyn Clock>) -> Self {
        Self { clock, ..self }
    }

    /// 提案を受諾して予約を確定する
    ///
    /// 同じ観測セッション（同一の予約者・利用開始時刻・リソース集合）に対する予約が
    /// 既に存在する場合は、その期間を指定された長さに更新する。存在しない場合は新規作成する。
    ///
    /// # Arguments
    /// * `holder_email` - 予約者
    /// * `resources` - 実利用が観測されたリソース
    /// * `active_since` - 実利用の開始時刻。どの機会への受諾かを見分けるために使う
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
        holder_email: EmailAddress,
        resources: Vec<Resource>,
        active_since: DateTime<Utc>,
        duration: Duration,
    ) -> Result<ReservedSlotId, ApplicationError> {
        // 予約は受諾した時点から始める。実利用の開始時刻を起点にすると、長く使い続けて
        // いる利用者では作った瞬間に終わっている予約になり、リソースを押さえられない。
        // 事後予約が果たす役割は、これから先を他の人に使われないようにすることである。
        let accepted_at = self.clock.now();
        let time_period = TimePeriod::new(accepted_at, accepted_at + duration)?;
        let session_key = Self::session_key(&holder_email, &resources, active_since);

        // 同じ機会への受諾を直列化する。連打で書き込みが検索へ反映される前に次の受諾が
        // 走ると、同じ機会の予約が二重に作られてしまう。
        let mut settled_sessions = self.settled_sessions.lock().await;

        let existing_slot = match settled_sessions.get(&session_key) {
            // このプロセスで既に確定させた予約は、検索の反映を待たずIDで引く
            Some(settled_id) => Some(settled_id.clone()),
            // プロセスが入れ替わった場合に備え、進行中の事後予約から同じ機会のものを探す
            None => {
                self.find_settled_reservation_in_progress(&holder_email, &resources, accepted_at)
                    .await?
            }
        };
        let existing = match existing_slot {
            Some(slot_id) => self
                .repository
                .find(slot_id.reservation_id())
                .await?
                .map(|reservation| (reservation, slot_id)),
            None => None,
        };

        // 既存予約の期間を更新する場合、自分自身は競合対象から外す
        self.conflict_checker
            .check_conflicts(
                self.repository.as_ref(),
                &time_period,
                &resources,
                existing.as_ref().map(|(_, slot_id)| slot_id),
            )
            .await?;

        let slot_id = match existing {
            // 同じ機会への受諾の押し直しは、確定済みの予約枠の時間を変える
            Some((mut reservation, slot_id)) => {
                reservation.reschedule_slot(slot_id.key(), time_period, &holder_email)?;
                self.repository.save(&mut reservation).await?;
                slot_id
            }
            None => {
                let mut reservation = Reservation::one_time(
                    holder_email,
                    time_period,
                    resources,
                    Some(POST_HOC_RESERVATION_NOTES.to_string()),
                )?;
                self.repository.save(&mut reservation).await?;
                reservation.slot_id(OccurrenceKey::Only)
            }
        };
        settled_sessions.insert(session_key, slot_id.clone());

        Ok(slot_id)
    }

    /// ひとつの機会を表すキーを作る
    ///
    /// リソースの並びは呼び出し側に依存するため、表示名で並べ替えて正規化する。
    fn session_key(
        holder_email: &EmailAddress,
        resources: &[Resource],
        active_since: DateTime<Utc>,
    ) -> SessionKey {
        let mut normalized: Vec<String> = resources
            .iter()
            .map(|resource| resource.to_string())
            .collect();
        normalized.sort();

        (holder_email.as_str().to_string(), active_since, normalized)
    }

    /// 同じ機会について既に確定している事後予約を探す
    ///
    /// 受諾した時点を開始とするため、開始時刻では同じ機会かを判別できない。
    /// 進行中の事後予約のうち、予約者とリソース集合が一致するものを同じ機会とみなす。
    /// 同じ利用者が同じリソースの組について事後予約を二重に持つことはないため、
    /// これで取り違えは起こらない。
    async fn find_settled_reservation_in_progress(
        &self,
        holder_email: &EmailAddress,
        resources: &[Resource],
        now: DateTime<Utc>,
    ) -> Result<Option<ReservedSlotId>, ApplicationError> {
        let requested: HashSet<&Resource> = resources.iter().collect();

        // 進行中かどうかを見るため、現在時刻を含む最小の期間を問う
        let in_progress = TimePeriod::new(now, now + Duration::seconds(1))?;
        let candidates = self
            .repository
            .find_slots_held_by(holder_email, &in_progress)
            .await?;

        Ok(candidates
            .into_iter()
            .find(|slot| {
                slot.notes().map(String::as_str) == Some(POST_HOC_RESERVATION_NOTES)
                    && slot.resources().iter().collect::<HashSet<_>>() == requested
            })
            .map(|slot| slot.id().clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::clock::FixedClock;
    use crate::domain::aggregates::reservation::ReservationId;
    use crate::domain::aggregates::reservation::value_objects::Gpu;
    use crate::domain::ports::repositories::RepositoryError;
    use crate::domain::read_models::ReservedSlotView;
    use crate::infrastructure::repositories::reservation::mock::MockReservationRepository;
    use async_trait::async_trait;
    use chrono::TimeZone;
    use std::sync::Mutex as StdMutex;

    /// テストの「今」（受諾ボタンが押される時刻）
    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 16, 1, 0, 0).unwrap()
    }

    fn usecase<R: ReservationRepository + ReservedSlotQuery>(
        repository: Arc<R>,
    ) -> AcceptReservationProposalUseCase<R> {
        AcceptReservationProposalUseCase::new(repository)
            .with_clock(Arc::new(FixedClock::at(now())))
    }

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
        repository: &MockReservationRepository,
        holder: &str,
        resources: Vec<Resource>,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> ReservedSlotId {
        let slot = ReservedSlotView::of_new_one_time_reservation(
            email(holder),
            TimePeriod::new(start, end).unwrap(),
            resources,
            Some(POST_HOC_RESERVATION_NOTES.to_string()),
        )
        .unwrap();
        repository.put_slot(&slot);
        slot.id().clone()
    }

    /// 保存されている全予約（終了済みを含む）
    async fn all_reservations(repository: &MockReservationRepository) -> Vec<ReservedSlotView> {
        let far_past = now() - Duration::days(365);
        let far_future = now() + Duration::days(365);
        repository
            .find_slots_overlapping(&TimePeriod::new(far_past, far_future).unwrap())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn accepting_twice_for_the_same_session_updates_the_period_instead_of_adding() {
        let repository = Arc::new(MockReservationRepository::new());
        let active_since = now() - Duration::hours(1);
        let existing_id = save_post_hoc_reservation(
            &repository,
            "holder@example.com",
            vec![gpu(0), gpu(1)],
            active_since,
            active_since + Duration::hours(2),
        )
        .await;

        let usecase = usecase(repository.clone());
        let returned_id = usecase
            .execute(
                email("holder@example.com"),
                vec![gpu(0), gpu(1)],
                active_since,
                Duration::hours(12),
            )
            .await
            .unwrap();

        assert_eq!(returned_id, existing_id, "既存予約が更新されるべき");

        let reservations = all_reservations(&repository).await;
        assert_eq!(reservations.len(), 1, "予約が増えてはいけない");
        let period = reservations[0].time_period();
        assert_eq!(
            period.end() - period.start(),
            Duration::hours(12),
            "受諾された長さに更新されるべき"
        );
    }

    #[tokio::test]
    async fn an_ended_post_hoc_reservation_is_left_alone_and_a_new_one_is_created() {
        let repository = Arc::new(MockReservationRepository::new());
        // 以前の受諾で作った予約が既に終わっている状況
        let active_since = now() - Duration::hours(5);
        save_post_hoc_reservation(
            &repository,
            "holder@example.com",
            vec![gpu(0)],
            active_since,
            now() - Duration::hours(1),
        )
        .await;

        let usecase = usecase(repository.clone());
        usecase
            .execute(
                email("holder@example.com"),
                vec![gpu(0)],
                active_since,
                Duration::hours(12),
            )
            .await
            .unwrap();

        // 終わった予約を延長するのは記録の書き換えになる。これから先の分を別に作る
        let all = all_reservations(&repository).await;
        assert_eq!(all.len(), 2, "終了済みの予約は残し、新しい予約を作るべき");
    }

    #[tokio::test]
    async fn accepting_shorter_duration_shrinks_the_existing_reservation() {
        let repository = Arc::new(MockReservationRepository::new());
        let active_since = now() - Duration::hours(1);
        save_post_hoc_reservation(
            &repository,
            "holder@example.com",
            vec![gpu(0)],
            active_since,
            active_since + Duration::hours(12),
        )
        .await;

        let usecase = usecase(repository.clone());
        usecase
            .execute(
                email("holder@example.com"),
                vec![gpu(0)],
                active_since,
                Duration::hours(2),
            )
            .await
            .unwrap();

        let reservations = all_reservations(&repository).await;
        assert_eq!(reservations.len(), 1);
        let period = reservations[0].time_period();
        assert_eq!(
            period.end() - period.start(),
            Duration::hours(2),
            "短い候補を押した場合は期間が縮むべき"
        );
    }

    #[tokio::test]
    async fn creates_a_new_reservation_when_no_session_matches() {
        let repository = Arc::new(MockReservationRepository::new());
        let active_since = now() - Duration::minutes(30);

        let usecase = usecase(repository.clone());
        usecase
            .execute(
                email("holder@example.com"),
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
        let repository = Arc::new(MockReservationRepository::new());
        let active_since = now() - Duration::hours(1);
        save_post_hoc_reservation(
            &repository,
            "someone-else@example.com",
            vec![gpu(0)],
            active_since,
            active_since + Duration::hours(6),
        )
        .await;

        let usecase = usecase(repository.clone());
        let result = usecase
            .execute(
                email("holder@example.com"),
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
        let repository = Arc::new(MockReservationRepository::new());
        let active_since = now() - Duration::hours(1);
        save_post_hoc_reservation(
            &repository,
            "holder@example.com",
            vec![gpu(0)],
            active_since,
            active_since + Duration::hours(2),
        )
        .await;

        // 同じ利用者・同じ開始時刻でも、別のGPUを使い始めた場合は別の予約になる
        let usecase = usecase(repository.clone());
        usecase
            .execute(
                email("holder@example.com"),
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
        visible: StdMutex<Vec<Reservation>>,
        /// 保存されたがまだ検索から見えない予約
        pending: StdMutex<Vec<Reservation>>,
    }

    impl EventuallyConsistentRepository {
        fn new() -> Self {
            Self {
                visible: StdMutex::new(Vec::new()),
                pending: StdMutex::new(Vec::new()),
            }
        }

        fn all(&self) -> Vec<Reservation> {
            let mut all = self.visible.lock().unwrap().clone();
            all.extend(self.pending.lock().unwrap().iter().cloned());
            all
        }

        /// 検索から見える予約枠の写し
        fn visible_slots(&self, time_period: &TimePeriod) -> Vec<ReservedSlotView> {
            self.visible
                .lock()
                .unwrap()
                .iter()
                .flat_map(|reservation| {
                    reservation
                        .slots_in(time_period)
                        .iter()
                        .map(|slot| ReservedSlotView::of(reservation, slot))
                        .collect::<Vec<_>>()
                })
                .collect()
        }
    }

    #[async_trait]
    impl ReservedSlotQuery for EventuallyConsistentRepository {
        async fn find_slots_overlapping(
            &self,
            time_period: &TimePeriod,
        ) -> Result<Vec<ReservedSlotView>, RepositoryError> {
            Ok(self.visible_slots(time_period))
        }

        async fn find_slots_held_by(
            &self,
            holder_email: &EmailAddress,
            time_period: &TimePeriod,
        ) -> Result<Vec<ReservedSlotView>, RepositoryError> {
            Ok(self
                .visible_slots(time_period)
                .into_iter()
                .filter(|slot| slot.holder_email() == holder_email)
                .collect())
        }
    }

    #[async_trait]
    impl ReservationRepository for EventuallyConsistentRepository {
        async fn find(&self, id: &ReservationId) -> Result<Option<Reservation>, RepositoryError> {
            // IDを指定した取得は書き込み直後でも一貫して読める
            Ok(self
                .all()
                .into_iter()
                .find(|reservation| reservation.id() == id))
        }

        async fn find_overlapping(
            &self,
            _time_period: &TimePeriod,
        ) -> Result<Vec<Reservation>, RepositoryError> {
            unimplemented!("このテストでは使用しない")
        }

        async fn save(&self, reservation: &mut Reservation) -> Result<(), RepositoryError> {
            let mut pending = self.pending.lock().unwrap();
            match pending.iter_mut().find(|p| p.id() == reservation.id()) {
                Some(existing) => *existing = reservation.clone(),
                None => pending.push(reservation.clone()),
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn pressing_the_same_button_twice_in_a_row_creates_only_one_reservation() {
        let repository = Arc::new(EventuallyConsistentRepository::new());
        let usecase = Arc::new(usecase(repository.clone()));
        let active_since = now() - Duration::minutes(5);

        // 1回目の書き込みが検索に反映される前に2回目が走る（ボタン連打）
        let first = {
            let usecase = usecase.clone();
            tokio::spawn(async move {
                usecase
                    .execute(
                        email("holder@example.com"),
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
                        email("holder@example.com"),
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
        let usecase = usecase(repository.clone());
        let active_since = now() - Duration::minutes(5);

        let first = usecase
            .execute(
                email("holder@example.com"),
                vec![gpu(5)],
                active_since,
                Duration::hours(1),
            )
            .await
            .unwrap();

        // 反映される前に別の長さを選び直す
        let second = usecase
            .execute(
                email("holder@example.com"),
                vec![gpu(5)],
                active_since,
                Duration::hours(12),
            )
            .await
            .unwrap();

        assert_eq!(first, second, "同じ予約を指すべき");
        let all = repository.all();
        assert_eq!(all.len(), 1, "予約は増えない");
        let period = all[0]
            .slot(&OccurrenceKey::Only)
            .unwrap()
            .time_period()
            .clone();
        assert_eq!(
            period.end() - period.start(),
            Duration::hours(12),
            "選び直した長さに更新されるべき"
        );
    }
    #[tokio::test]
    async fn the_reservation_starts_when_the_button_is_pressed_not_when_the_usage_began() {
        let repository = Arc::new(MockReservationRepository::new());
        let usecase = usecase(repository.clone());
        // 6日間使い続けている状況。利用開始を起点にすると、押した時点で既に終わった予約になる
        let active_since = now() - Duration::days(6);

        usecase
            .execute(
                email("holder@example.com"),
                vec![gpu(0)],
                active_since,
                Duration::hours(1),
            )
            .await
            .unwrap();

        let all = all_reservations(&repository).await;
        assert_eq!(all.len(), 1);
        let period = all[0].time_period();
        assert_eq!(
            period,
            &TimePeriod::new(now(), now() + Duration::hours(1)).unwrap(),
            "押した時刻から、受諾した長さだけ押さえる（利用開始の{active_since}からではない）"
        );
    }

    #[tokio::test]
    async fn an_earlier_reservation_of_the_same_user_does_not_block_acceptance() {
        let repository = Arc::new(MockReservationRepository::new());
        // 利用開始から現在までの間に、自分の予約が既にあった（切れた直後に提案が来る状況）
        let active_since = now() - Duration::days(6);
        save_post_hoc_reservation(
            &repository,
            "holder@example.com",
            vec![gpu(0)],
            active_since,
            now() - Duration::hours(1),
        )
        .await;

        let usecase = usecase(repository.clone());
        let result = usecase
            .execute(
                email("holder@example.com"),
                vec![gpu(0)],
                active_since,
                Duration::hours(1),
            )
            .await;

        assert!(
            result.is_ok(),
            "過去に自分の予約があっても、これから先の予約は取れるべき: {result:?}"
        );
        assert_eq!(
            all_reservations(&repository).await.len(),
            2,
            "過去の予約は残し、新しい予約を足すべき"
        );
    }
}
