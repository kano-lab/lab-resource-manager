use crate::application::ApplicationError;
use crate::domain::aggregates::reservation::{ReservedSlot, ReservedSlotId};
use crate::domain::common::TimePeriod;
use crate::domain::ports::Notifier;
use crate::domain::ports::repositories::{ReservationRepository, ReservedSlotRepository};
use crate::domain::services::{ReservationChangeInterpreter, WatchedReservations};
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

/// 予約の変化を監視し、通知するユースケース
///
/// 監視範囲にある予約枠を一定間隔で読み、前回からの変化を予約に起きた出来事として
/// 予約チャンネルへ知らせる。変化をどの出来事と読むかはドメインサービス
/// （`ReservationChangeInterpreter`）が決める。このユースケースは、前回の状態を持ち、
/// 読み取った出来事を送ることに責務を持つ。
///
/// カレンダーは人の手でも直接編集されるため、アプリの操作ではなく保存先の変化を見る。
/// 定期予約は、予約枠がいくつあっても1件の予約として伝わる。
///
/// # スコープ
/// 監視するのは、現在時刻から`watch_window`先までに重なる予約枠である。
/// 予定どおり終わった予約枠は自然に監視範囲から外れ、何も伝えない。
pub struct NotifyReservationChangesUseCase<R, N>
where
    R: ReservationRepository + ReservedSlotRepository,
    N: Notifier,
{
    repository: Arc<R>,
    notifier: N,
    interpreter: ReservationChangeInterpreter,
    /// 変更を監視する期間の長さ
    ///
    /// 前回と今回の状態を突き合わせて差分を通知するため、範囲を狭めると
    /// 範囲外に出た予約枠が取り消しとして扱われる。用途に見合う長さを与える必要がある。
    watch_window: Duration,
    previous: tokio::sync::Mutex<WatchedReservations>,
}

impl<R, N> NotifyReservationChangesUseCase<R, N>
where
    R: ReservationRepository + ReservedSlotRepository,
    N: Notifier,
{
    /// 新しいインスタンスを作成し、初期状態を取得する
    ///
    /// 起動時点で既にある予約は、作成済みのものとして扱う（通知しない）。
    ///
    /// # Arguments
    /// * `repository` - 予約枠リポジトリ
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
        let interpreter = ReservationChangeInterpreter::new();
        let recurring_reservations = interpreter
            .recurring_reservations_of(repository.as_ref(), &slots)
            .await?;
        Ok(Self {
            repository,
            notifier,
            interpreter,
            watch_window,
            previous: tokio::sync::Mutex::new(WatchedReservations {
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
        let mut previous = self.previous.lock().await;

        let changes = self
            .interpreter
            .interpret(
                self.repository.as_ref(),
                &previous,
                &current_slots,
                started_at,
            )
            .await?;

        let notifications = changes.events.len();
        for event in changes.events {
            self.notifier.notify(event).await?;
        }

        // 1周の要約。件数の推移から、取得範囲や取りこぼしの異常に気づける。
        // 定期予約は予約枠の増減をまとめて伝えるため、通知の数は増減の数より少なくなりうる
        info!(
            slots = current_slots.len(),
            appeared = changes.appeared,
            disappeared = changes.disappeared,
            notifications,
            elapsed_ms = (Utc::now() - started_at).num_milliseconds(),
            "change detection pass finished"
        );

        *previous = WatchedReservations {
            slots: current_slots,
            window_end,
            recurring_reservations: changes.recurring_reservations,
        };

        Ok(())
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
