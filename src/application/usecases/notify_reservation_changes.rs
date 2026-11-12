use crate::application::ApplicationError;
use crate::domain::aggregates::reservation::{
    OneTimeReservation, Reservation, ReservedSlot, ReservedSlotId, value_objects::TimePeriod,
};
use crate::domain::ports::repositories::ReservationRepository;
use crate::domain::ports::{NotificationEvent, Notifier};
use chrono::{Duration, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

/// 予約の変化を監視し、通知するユースケース
///
/// 監視範囲にある予約枠を前回と今回で突き合わせ、予約に何が起きたかを伝える。
///
/// - 予約枠が現れた → 予約が作られた
/// - 予約枠の内容が変わった → 予約が変更された
/// - 予約枠が予定より早く見えなくなった → 予約が取り消された
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
    previous_slots: tokio::sync::Mutex<HashMap<ReservedSlotId, ReservedSlot>>,
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
        let slots = Self::fetch_current_slots(&repository, watch_window).await?;
        Ok(Self {
            repository,
            notifier,
            watch_window,
            previous_slots: tokio::sync::Mutex::new(slots),
        })
    }

    /// 一度だけポーリングを実行し、変化を検知して通知する
    ///
    /// # Errors
    /// リポジトリアクセスまたは通知送信に失敗した場合
    pub async fn poll_once(&self) -> Result<(), ApplicationError> {
        let started_at = Utc::now();
        let current_slots = Self::fetch_current_slots(&self.repository, self.watch_window).await?;
        let mut previous_slots = self.previous_slots.lock().await;

        let one_time =
            |slot: &ReservedSlot| Reservation::OneTime(OneTimeReservation::from_slot(slot));
        let mut events = Vec::new();

        let mut appeared = 0;
        for (id, slot) in &current_slots {
            match previous_slots.get(id) {
                None => {
                    appeared += 1;
                    events.push(NotificationEvent::ReservationCreated(one_time(slot)));
                }
                Some(previous_slot) if previous_slot != slot => {
                    events.push(NotificationEvent::ReservationUpdated(one_time(slot)))
                }
                Some(_) => {}
            }
        }

        // 予定より早く見えなくなった予約枠には、取り消されたものと、今で終了して
        // 終了時刻が今より前へ縮んだものがある。後者は予約枠として残っており、
        // 取り消しではなく変更として伝える
        let mut disappeared = 0;
        for (id, slot) in previous_slots.iter() {
            if slot.time_period().end() <= started_at || current_slots.contains_key(id) {
                continue;
            }
            disappeared += 1;
            match self.repository.find_slot(id).await? {
                Some(stored) if stored.time_period().end() <= started_at => {
                    events.push(NotificationEvent::ReservationUpdated(one_time(&stored)))
                }
                _ => events.push(NotificationEvent::ReservationCancelled(one_time(slot))),
            }
        }

        let notifications = events.len();
        for event in events {
            self.notifier.notify(event).await?;
        }

        // 1周の要約。件数の推移から、取得範囲や取りこぼしの異常に気づける
        info!(
            slots = current_slots.len(),
            appeared,
            disappeared,
            notifications,
            elapsed_ms = (Utc::now() - started_at).num_milliseconds(),
            "change detection pass finished"
        );

        *previous_slots = current_slots;

        Ok(())
    }

    /// 監視範囲の予約枠を取得する
    async fn fetch_current_slots(
        repository: &R,
        watch_window: Duration,
    ) -> Result<HashMap<ReservedSlotId, ReservedSlot>, ApplicationError> {
        // 監視する期間は現在時刻を起点に取り直す。ポーリングごとに窓が前へ進む
        let now = Utc::now();
        let window = TimePeriod::new(now, now + watch_window)?;
        let slots = repository.find_slots_overlapping(&window).await?;
        Ok(slots
            .into_iter()
            .map(|slot| (slot.id().clone(), slot))
            .collect())
    }
}

#[cfg(test)]
#[path = "notify_reservation_changes_tests.rs"]
mod tests;
