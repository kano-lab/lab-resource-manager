use crate::application::ApplicationError;
use crate::domain::aggregates::recurring_reservation::{
    RecurringReservation, RecurringReservationId,
};
use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use crate::domain::aggregates::resource_usage::value_objects::TimePeriod;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::domain::ports::{NotificationEvent, Notifier};
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

/// 未来および進行中のリソース使用状況の変更を監視し、通知するユースケース
///
/// このユースケースは以下の変更を検知して通知します:
/// - 新規作成: 新しいリソース使用予約が追加された
/// - 更新: 既存の予約内容が変更された
/// - 削除: **未来の予約**がキャンセル/削除された
///
/// 早期終了（予定より前倒しで終了）した予約は、使った分が予約として残っているため
/// 削除ではなく更新として通知します。
///
/// # スコープ
/// このユースケースは「未来および進行中」のリソース使用のみを監視対象とします。
/// 予約期間が終了したリソースは自然に監視対象外となり、削除通知は送信されません。
///
/// # 定期予約
/// 定期予約は個々の回へ展開されて監視範囲に現れるが、利用者が行ったのは
/// 「定期予約を1件作った」「1件取り消した」という1つの行為である。回ごとに通知すると
/// 監視範囲に入る回の数だけ同じ内容が流れるため、定期予約としてまとめて1件で伝える。
///
/// - 初めて見る定期予約は、回がいくつ見えていても作成1件として伝える
/// - 既に知っている定期予約の回が、時間の経過で監視範囲に入ってきても何も伝えない
/// - 1回だけの変更・取り消しは、その回の変更・取り消しとして伝える
/// - 複数の回に及ぶ変更（時間帯の変更、途中からの打ち切りなど）は、定期予約の変更1件として伝える
/// - 定期予約そのものが取り消されたら、取り消し1件として伝える
pub struct NotifyFutureResourceUsageChangesUseCase<R, N>
where
    R: ResourceUsageRepository,
    N: Notifier,
{
    repository: Arc<R>,
    notifier: N,
    /// 変更を監視する期間の長さ
    ///
    /// 前回と今回の状態を突き合わせて差分を通知するため、範囲を狭めると
    /// 範囲外に出た予約が削除として扱われる。用途に見合う長さを与える必要がある。
    watch_window: Duration,
    previous_state: tokio::sync::Mutex<WatchedState>,
}

/// 前回のポーリングで把握した状態
struct WatchedState {
    usages: HashMap<String, ResourceUsage>,
    /// 前回の監視範囲の終わり
    ///
    /// これ以降に始まる回は前回の範囲に入りえなかったので、今回初めて見えても
    /// 新しく作られたとは限らない（時間の経過で範囲に入ってきただけかもしれない）。
    window_end: DateTime<Utc>,
    /// これまでに見た定期予約
    ///
    /// 監視範囲に回が1つも無い時期があっても覚えておく。忘れると、間隔の長い定期予約の
    /// 次の回が範囲に入るたびに新しい定期予約として伝えてしまう。
    recurring_reservations: HashMap<RecurringReservationId, RecurringReservation>,
}

/// 1件の定期予約について、前回から今回のあいだに起きた回の増減
#[derive(Default)]
struct OccurrenceChanges {
    /// 今回初めて見えた回
    appeared: Vec<ResourceUsage>,
    /// 内容が変わった回（早期終了した回を含む）
    changed: Vec<ResourceUsage>,
    /// 取り消されて見えなくなった回
    vanished: Vec<ResourceUsage>,
    /// 今回も見えている回があるか
    still_has_occurrences: bool,
}

impl<R, N> NotifyFutureResourceUsageChangesUseCase<R, N>
where
    R: ResourceUsageRepository,
    N: Notifier,
{
    /// 新しいインスタンスを作成し、初期状態を取得する
    ///
    /// # Arguments
    /// * `repository` - リソース使用リポジトリ（Arc で共有）
    /// * `notifier` - 通知サービス
    ///
    /// # Errors
    /// リポジトリから初期状態の取得に失敗した場合
    /// * `watch_window` - 変更を監視する期間の長さ（現在時刻から先へこの長さ）
    pub async fn new(
        repository: Arc<R>,
        notifier: N,
        watch_window: Duration,
    ) -> Result<Self, ApplicationError> {
        let (usages, window_end) = Self::fetch_current_usages(&repository, watch_window).await?;

        // 起動時点で既にある定期予約は、作成済みのものとして覚えておく
        let mut recurring_reservations = HashMap::new();
        for usage in usages.values() {
            if let Some(id) = usage.recurring_reservation_id()
                && !recurring_reservations.contains_key(id)
            {
                let reservation = Self::describe(&repository, id, usage).await?;
                recurring_reservations.insert(id.clone(), reservation);
            }
        }

        Ok(Self {
            repository,
            notifier,
            watch_window,
            previous_state: tokio::sync::Mutex::new(WatchedState {
                usages,
                window_end,
                recurring_reservations,
            }),
        })
    }

    /// 一度だけポーリングを実行し、変更を検知して通知する
    ///
    /// 前回の状態と現在の状態を比較し、作成・更新・削除された予約を検知して通知します。
    ///
    /// # Errors
    /// リポジトリアクセスまたは通知送信に失敗した場合
    pub async fn poll_once(&self) -> Result<(), ApplicationError> {
        let started_at = Utc::now();
        let (current_usages, window_end) =
            Self::fetch_current_usages(&self.repository, self.watch_window).await?;
        let mut previous = self.previous_state.lock().await;

        let created = current_usages
            .keys()
            .filter(|id| !previous.usages.contains_key(*id))
            .count();
        // previousを現在時刻基準で「まだ未来」のものだけに絞る
        // (currentと同じ時間軸に合わせることで、自然な期限切れを削除と誤検知しない)
        let disappeared: Vec<&ResourceUsage> = previous
            .usages
            .iter()
            .filter(|(id, usage)| {
                usage.time_period().end() > started_at && !current_usages.contains_key(*id)
            })
            .map(|(_, usage)| usage)
            .collect();

        // 予定より早く見えなくなった予約には、取り消されたものと、早期終了で
        // 終了時刻が今より前へ縮んだものがある。後者は予約として残っており、
        // 削除ではなく更新として伝える
        let mut vanished = Vec::new();
        let mut ended_early = Vec::new();
        for usage in disappeared {
            match self.repository.find_by_id(usage.id()).await? {
                Some(stored) if stored.time_period().end() <= started_at => {
                    ended_early.push(stored)
                }
                _ => vanished.push(usage.clone()),
            }
        }
        let deleted = vanished.len();
        let released = ended_early.len();

        let mut events = Vec::new();
        let mut recurring: HashMap<RecurringReservationId, OccurrenceChanges> = HashMap::new();

        for (id, usage) in &current_usages {
            let previous_usage = previous.usages.get(id);
            match usage.recurring_reservation_id() {
                Some(series_id) => {
                    let changes = recurring.entry(series_id.clone()).or_default();
                    changes.still_has_occurrences = true;
                    match previous_usage {
                        None => changes.appeared.push(usage.clone()),
                        Some(previous_usage) if previous_usage != usage => {
                            changes.changed.push(usage.clone())
                        }
                        Some(_) => {}
                    }
                }
                None => match previous_usage {
                    None => events.push(NotificationEvent::ResourceUsageCreated(usage.clone())),
                    Some(previous_usage) if previous_usage != usage => {
                        events.push(NotificationEvent::ResourceUsageUpdated(usage.clone()))
                    }
                    Some(_) => {}
                },
            }
        }

        for usage in ended_early {
            match usage.recurring_reservation_id().cloned() {
                Some(series_id) => recurring.entry(series_id).or_default().changed.push(usage),
                None => events.push(NotificationEvent::ResourceUsageUpdated(usage)),
            }
        }

        for usage in vanished {
            match usage.recurring_reservation_id().cloned() {
                Some(series_id) => recurring.entry(series_id).or_default().vanished.push(usage),
                None => events.push(NotificationEvent::ResourceUsageDeleted(usage)),
            }
        }

        let mut recurring_reservations = previous.recurring_reservations.clone();
        for (series_id, changes) in recurring {
            if let Some(event) = self
                .summarize_recurring_changes(
                    &series_id,
                    changes,
                    previous.window_end,
                    &mut recurring_reservations,
                )
                .await?
            {
                events.push(event);
            }
        }

        let notifications = events.len();
        for event in events {
            self.notifier.notify(event).await?;
        }

        // 1周の要約。件数の推移から、取得範囲や取りこぼしの異常に気づける。
        // 定期予約は回の増減をまとめて伝えるため、通知の数は増減の数より少なくなりうる
        info!(
            reservations = current_usages.len(),
            created,
            deleted,
            released,
            notifications,
            elapsed_ms = (Utc::now() - started_at).num_milliseconds(),
            "change detection pass finished"
        );

        *previous = WatchedState {
            usages: current_usages,
            window_end,
            recurring_reservations,
        };

        Ok(())
    }

    /// 1件の定期予約に起きた回の増減を、伝えるべき1件の出来事にまとめる
    ///
    /// 伝えることがなければ`None`を返す。
    async fn summarize_recurring_changes(
        &self,
        id: &RecurringReservationId,
        changes: OccurrenceChanges,
        previous_window_end: DateTime<Utc>,
        known: &mut HashMap<RecurringReservationId, RecurringReservation>,
    ) -> Result<Option<NotificationEvent>, ApplicationError> {
        // 初めて見る定期予約は、回がいくつ見えていても作成1件として伝える
        if !known.contains_key(id) {
            let Some(earliest) = changes
                .appeared
                .iter()
                .min_by_key(|usage| usage.time_period().start())
            else {
                return Ok(None);
            };
            let reservation = Self::describe(&self.repository, id, earliest).await?;
            known.insert(id.clone(), reservation.clone());
            return Ok(Some(NotificationEvent::RecurringReservationCreated(
                reservation,
            )));
        }

        // 前回の監視範囲より先で始まる回は、時間の経過で範囲に入ってきただけである
        let newly_added: Vec<&ResourceUsage> = changes
            .appeared
            .iter()
            .filter(|usage| usage.time_period().start() < previous_window_end)
            .collect();

        let touched = newly_added.len() + changes.changed.len() + changes.vanished.len();
        if touched == 0 {
            return Ok(None);
        }

        let current = self.repository.find_recurring_reservation(id).await?;

        // 定期予約そのものが無くなり、回も残っていなければ、取り消されたということ
        if current.is_none()
            && !changes.vanished.is_empty()
            && !changes.still_has_occurrences
            && let Some(cancelled) = known.remove(id)
        {
            return Ok(Some(NotificationEvent::RecurringReservationCancelled(
                cancelled,
            )));
        }

        // 1回だけの増減は、その回の出来事として伝える（1回だけの早期終了や取り消しなど）
        if touched == 1 {
            let event = if let Some(usage) = changes.changed.first() {
                NotificationEvent::ResourceUsageUpdated(usage.clone())
            } else if let Some(usage) = changes.vanished.first() {
                NotificationEvent::ResourceUsageDeleted(usage.clone())
            } else {
                NotificationEvent::ResourceUsageCreated(newly_added[0].clone())
            };
            return Ok(Some(event));
        }

        // 複数の回に及ぶ増減は、定期予約そのものの変更として1件で伝える
        let Some(reservation) = current.or_else(|| known.get(id).cloned()) else {
            return Ok(None);
        };
        known.insert(id.clone(), reservation.clone());
        Ok(Some(NotificationEvent::RecurringReservationUpdated(
            reservation,
        )))
    }

    /// 定期予約の内容を引く
    ///
    /// 永続化層から引けない場合も、定期予約であること自体は回から分かっている。
    /// 間隔は分からないまま、手元の回から分かる範囲で組み立てる。
    async fn describe(
        repository: &R,
        id: &RecurringReservationId,
        occurrence: &ResourceUsage,
    ) -> Result<RecurringReservation, ApplicationError> {
        Ok(repository
            .find_recurring_reservation(id)
            .await?
            .unwrap_or_else(|| RecurringReservation::from_occurrence(id.clone(), occurrence, None)))
    }

    /// 監視範囲の予約と、その範囲の終わりを取得する
    async fn fetch_current_usages(
        repository: &R,
        watch_window: Duration,
    ) -> Result<(HashMap<String, ResourceUsage>, DateTime<Utc>), ApplicationError> {
        // 監視する期間は現在時刻を起点に取り直す。ポーリングごとに窓が前へ進む
        let now = Utc::now();
        let window = TimePeriod::new(now, now + watch_window)?;
        let usages = repository.find_overlapping(&window).await?;
        Ok((
            usages
                .into_iter()
                .map(|usage| (usage.id().as_str().to_string(), usage))
                .collect(),
            window.end(),
        ))
    }
}

#[cfg(test)]
#[path = "notify_future_resource_usage_changes_tests.rs"]
mod tests;
