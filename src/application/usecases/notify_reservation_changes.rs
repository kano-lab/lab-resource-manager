use crate::application::ApplicationError;
use crate::domain::aggregates::reservation::{
    OneTimeReservation, RecurringReservation, Reservation, ReservationId, ReservationKind,
    ReservedSlot, ReservedSlotId, value_objects::TimePeriod,
};
use crate::domain::ports::repositories::ReservationRepository;
use crate::domain::ports::{NotificationEvent, Notifier};
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

/// 予約の変化を監視し、通知するユースケース
///
/// 監視範囲にある予約枠を前回と今回で突き合わせ、予約に何が起きたかを伝える。
/// 予約は作る・取り消すの単位であり、通知も予約を単位とする。
///
/// - 予約が作られた・変更された・取り消された → 単発予約でも定期予約でも1件
/// - 定期予約の予約枠が1つだけ変わった・取り消された・今で終了された → その予約枠について1件
/// - 時間が経って定期予約の次の予約枠が監視範囲に入ってきた → 何も伝えない
///   （予約は増えていないため）
///
/// 今で終了した予約枠は、終了時刻が縮んで監視範囲から外れる。取り消しとは異なり
/// 使った分が予約枠として残っているため、変更として伝える。
///
/// # スコープ
/// 監視するのは、現在時刻から`watch_window`先までに重なる予約枠である。
/// 予定どおり終わった予約枠は自然に監視範囲から外れ、何も伝えない。
pub struct NotifyReservationChangesUseCase<R, N>
where
    R: ReservationRepository,
    N: Notifier,
{
    repository: Arc<R>,
    notifier: N,
    /// 変更を監視する期間の長さ
    ///
    /// 前回と今回の状態を突き合わせて差分を通知するため、範囲を狭めると
    /// 範囲外に出た予約枠が取り消しとして扱われる。用途に見合う長さを与える必要がある。
    watch_window: Duration,
    previous_state: tokio::sync::Mutex<WatchedState>,
}

/// 前回のポーリングで把握した状態
struct WatchedState {
    slots: HashMap<ReservedSlotId, ReservedSlot>,
    /// 前回の監視範囲の終わり
    ///
    /// これ以降に始まる予約枠は前回の範囲に入りえなかったので、今回初めて見えても
    /// 新しく作られたとは限らない（時間の経過で範囲に入ってきただけかもしれない）。
    window_end: DateTime<Utc>,
    /// これまでに見た定期予約
    ///
    /// 監視範囲に予約枠が1つも無い時期があっても覚えておく。忘れると、間隔の長い定期予約の
    /// 次の予約枠が範囲に入るたびに、新しい定期予約として伝えてしまう。
    recurring_reservations: HashMap<ReservationId, RecurringReservation>,
}

/// 1件の予約について、前回から今回のあいだに起きた予約枠の変化
struct SlotChanges {
    kind: ReservationKind,
    /// 今回初めて見えた予約枠
    appeared: Vec<ReservedSlot>,
    /// 内容が変わった予約枠（今で終了したものを含む）
    changed: Vec<ReservedSlot>,
    /// 取り消されて見えなくなった予約枠
    cancelled: Vec<ReservedSlot>,
    /// 今回も見えている予約枠があるか
    still_has_slots: bool,
}

impl SlotChanges {
    /// 予約枠が属する予約の変化の記録を取り出す（無ければ作る）
    fn of<'a>(
        changes: &'a mut HashMap<ReservationId, SlotChanges>,
        slot: &ReservedSlot,
    ) -> &'a mut SlotChanges {
        changes
            .entry(slot.reservation_id().clone())
            .or_insert_with(|| Self {
                kind: slot.reservation_kind(),
                appeared: Vec::new(),
                changed: Vec::new(),
                cancelled: Vec::new(),
                still_has_slots: false,
            })
    }
}

impl<R, N> NotifyReservationChangesUseCase<R, N>
where
    R: ReservationRepository,
    N: Notifier,
{
    /// 新しいインスタンスを作成し、初期状態を取得する
    ///
    /// 起動時点で既にある予約は、作成済みのものとして扱う（通知しない）。
    ///
    /// # Arguments
    /// * `repository` - 予約リポジトリ
    /// * `notifier` - 通知サービス
    /// * `watch_window` - 変更を監視する期間の長さ（現在時刻から先へこの長さ）
    ///
    /// # Errors
    /// リポジトリから初期状態の取得に失敗した場合
    pub async fn new(
        repository: Arc<R>,
        notifier: N,
        watch_window: Duration,
    ) -> Result<Self, ApplicationError> {
        let (slots, window_end) = Self::fetch_current_slots(&repository, watch_window).await?;

        let mut recurring_reservations = HashMap::new();
        for slot in slots.values() {
            if slot.reservation_kind() == ReservationKind::Recurring
                && !recurring_reservations.contains_key(slot.reservation_id())
            {
                let reservation = Self::describe_recurring(&repository, slot).await?;
                recurring_reservations.insert(slot.reservation_id().clone(), reservation);
            }
        }

        Ok(Self {
            repository,
            notifier,
            watch_window,
            previous_state: tokio::sync::Mutex::new(WatchedState {
                slots,
                window_end,
                recurring_reservations,
            }),
        })
    }

    /// 一度だけポーリングを実行し、変化を検知して通知する
    ///
    /// # Errors
    /// リポジトリアクセスまたは通知送信に失敗した場合
    pub async fn poll_once(&self) -> Result<(), ApplicationError> {
        let started_at = Utc::now();
        let (current_slots, window_end) =
            Self::fetch_current_slots(&self.repository, self.watch_window).await?;
        let mut previous = self.previous_state.lock().await;

        let mut changes: HashMap<ReservationId, SlotChanges> = HashMap::new();

        let mut appeared = 0;
        for (id, slot) in &current_slots {
            let entry = SlotChanges::of(&mut changes, slot);
            entry.still_has_slots = true;
            match previous.slots.get(id) {
                None => {
                    appeared += 1;
                    entry.appeared.push(slot.clone());
                }
                Some(previous_slot) if previous_slot != slot => entry.changed.push(slot.clone()),
                Some(_) => {}
            }
        }

        // 予定より早く見えなくなった予約枠には、取り消されたものと、今で終了して
        // 終了時刻が今より前へ縮んだものがある。後者は予約枠として残っており、
        // 取り消しではなく変更として伝える
        let mut disappeared = 0;
        for (id, slot) in &previous.slots {
            if slot.time_period().end() <= started_at || current_slots.contains_key(id) {
                continue;
            }
            disappeared += 1;
            match self.repository.find_slot(id).await? {
                Some(stored) if stored.time_period().end() <= started_at => {
                    SlotChanges::of(&mut changes, &stored).changed.push(stored)
                }
                _ => SlotChanges::of(&mut changes, slot)
                    .cancelled
                    .push(slot.clone()),
            }
        }

        let mut recurring_reservations = previous.recurring_reservations.clone();
        let mut events = Vec::new();
        for (reservation_id, slot_changes) in changes {
            let event = match slot_changes.kind {
                ReservationKind::OneTime => Self::summarize_one_time(slot_changes),
                ReservationKind::Recurring => {
                    self.summarize_recurring(
                        &reservation_id,
                        slot_changes,
                        previous.window_end,
                        &mut recurring_reservations,
                    )
                    .await?
                }
            };
            events.extend(event);
        }

        let notifications = events.len();
        for event in events {
            self.notifier.notify(event).await?;
        }

        // 1周の要約。件数の推移から、取得範囲や取りこぼしの異常に気づける。
        // 定期予約は予約枠の増減をまとめて伝えるため、通知の数は増減の数より少なくなりうる
        info!(
            slots = current_slots.len(),
            appeared,
            disappeared,
            notifications,
            elapsed_ms = (Utc::now() - started_at).num_milliseconds(),
            "change detection pass finished"
        );

        *previous = WatchedState {
            slots: current_slots,
            window_end,
            recurring_reservations,
        };

        Ok(())
    }

    /// 単発予約に起きた変化を、伝えるべき出来事にまとめる
    ///
    /// 単発予約は予約枠をちょうど1つ持つので、予約枠の変化がそのまま予約の変化である。
    fn summarize_one_time(changes: SlotChanges) -> Option<NotificationEvent> {
        let one_time =
            |slot: &ReservedSlot| Reservation::OneTime(OneTimeReservation::from_slot(slot));

        if let Some(slot) = changes.appeared.first() {
            Some(NotificationEvent::ReservationCreated(one_time(slot)))
        } else if let Some(slot) = changes.changed.first() {
            Some(NotificationEvent::ReservationUpdated(one_time(slot)))
        } else {
            changes
                .cancelled
                .first()
                .map(|slot| NotificationEvent::ReservationCancelled(one_time(slot)))
        }
    }

    /// 定期予約に起きた予約枠の変化を、伝えるべき1件の出来事にまとめる
    ///
    /// 伝えることがなければ`None`を返す。
    async fn summarize_recurring(
        &self,
        id: &ReservationId,
        changes: SlotChanges,
        previous_window_end: DateTime<Utc>,
        known: &mut HashMap<ReservationId, RecurringReservation>,
    ) -> Result<Option<NotificationEvent>, ApplicationError> {
        // 初めて見る定期予約は、予約枠がいくつ見えていても作成1件として伝える
        if !known.contains_key(id) {
            let Some(earliest) = changes
                .appeared
                .iter()
                .min_by_key(|slot| slot.time_period().start())
            else {
                return Ok(None);
            };
            let reservation = Self::describe_recurring(&self.repository, earliest).await?;
            known.insert(id.clone(), reservation.clone());
            return Ok(Some(NotificationEvent::ReservationCreated(
                Reservation::Recurring(reservation),
            )));
        }

        // 前回の監視範囲より先で始まる予約枠は、時間の経過で範囲に入ってきただけである
        let newly_added: Vec<&ReservedSlot> = changes
            .appeared
            .iter()
            .filter(|slot| slot.time_period().start() < previous_window_end)
            .collect();

        let touched = newly_added.len() + changes.changed.len() + changes.cancelled.len();
        if touched == 0 {
            return Ok(None);
        }

        let current = match self.repository.find_reservation(id).await? {
            Some(Reservation::Recurring(reservation)) => Some(reservation),
            _ => None,
        };

        // 定期予約そのものが無くなり、予約枠も残っていなければ、取り消されたということ
        if current.is_none()
            && !changes.cancelled.is_empty()
            && !changes.still_has_slots
            && let Some(cancelled) = known.remove(id)
        {
            return Ok(Some(NotificationEvent::ReservationCancelled(
                Reservation::Recurring(cancelled),
            )));
        }

        // 予約枠が1つだけ変わったなら、その予約枠の出来事として伝える
        if touched == 1 {
            let event = if let Some(slot) = changes.changed.first() {
                NotificationEvent::SlotUpdated(slot.clone())
            } else if let Some(slot) = changes.cancelled.first() {
                NotificationEvent::SlotCancelled(slot.clone())
            } else {
                NotificationEvent::SlotUpdated(newly_added[0].clone())
            };
            return Ok(Some(event));
        }

        // 複数の予約枠に及ぶ変化は、定期予約そのものの変更として1件で伝える
        let Some(reservation) = current.or_else(|| known.get(id).cloned()) else {
            return Ok(None);
        };
        known.insert(id.clone(), reservation.clone());
        Ok(Some(NotificationEvent::ReservationUpdated(
            Reservation::Recurring(reservation),
        )))
    }

    /// 予約枠が属する定期予約の内容を引く
    ///
    /// 永続化層から引けない場合も、定期予約であること自体は予約枠から分かっている。
    /// 繰り返しの規則は分からないまま、手元の予約枠から分かる範囲で組み立てる。
    async fn describe_recurring(
        repository: &R,
        slot: &ReservedSlot,
    ) -> Result<RecurringReservation, ApplicationError> {
        Ok(
            match repository.find_reservation(slot.reservation_id()).await? {
                Some(Reservation::Recurring(reservation)) => reservation,
                _ => {
                    RecurringReservation::from_first_slot(slot.reservation_id().clone(), slot, None)
                }
            },
        )
    }

    /// 監視範囲の予約枠と、その範囲の終わりを取得する
    async fn fetch_current_slots(
        repository: &R,
        watch_window: Duration,
    ) -> Result<(HashMap<ReservedSlotId, ReservedSlot>, DateTime<Utc>), ApplicationError> {
        // 監視する期間は現在時刻を起点に取り直す。ポーリングごとに窓が前へ進む
        let now = Utc::now();
        let window = TimePeriod::new(now, now + watch_window)?;
        let slots = repository.find_slots_overlapping(&window).await?;
        Ok((
            slots
                .into_iter()
                .map(|slot| (slot.id().clone(), slot))
                .collect(),
            window.end(),
        ))
    }
}

#[cfg(test)]
#[path = "notify_reservation_changes_tests.rs"]
mod tests;
