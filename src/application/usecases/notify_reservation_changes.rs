use crate::application::{ApplicationError, Clock, SystemClock};
use crate::domain::aggregates::reservation::{Reservation, ReservationId};
use crate::domain::common::TimePeriod;
use crate::domain::ports::Notifier;
use crate::domain::ports::repositories::ReservationRepository;
use crate::domain::services::ReservationChangeInterpreter;
use chrono::{DateTime, Duration, Utc};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::{error, info};

/// 予約の変化を監視し、通知するユースケース
///
/// 監視範囲にある予約を一定間隔で読み、前回からの変化を予約に起きた出来事として
/// 予約チャンネルへ知らせる。変化をどの出来事と読むかはドメインサービス
/// （`ReservationChangeInterpreter`）が決める。このユースケースは、前回の状態を持ち、
/// 読み取った出来事を送ることに責務を持つ。
///
/// カレンダーは人の手でも直接編集されるため、アプリの操作ではなく保存先の変化を見る。
///
/// # スコープ
/// 監視するのは、現在時刻から`watch_window`先までに予約枠が重なる予約である。
/// 予定どおり終わった予約は自然に監視範囲から外れ、何も伝えない。
pub struct NotifyReservationChangesUseCase<R, N>
where
    R: ReservationRepository,
    N: Notifier,
{
    repository: Arc<R>,
    notifier: N,
    interpreter: ReservationChangeInterpreter,
    /// 変更を監視する期間の長さ
    ///
    /// 前回と今回の状態を突き合わせて差分を通知するため、範囲を狭めると
    /// 範囲外に出た予約が取り消しとして扱われる。用途に見合う長さを与える必要がある。
    watch_window: Duration,
    previous_reservations: tokio::sync::Mutex<HashMap<ReservationId, Reservation>>,
    clock: Arc<dyn Clock>,
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
        Self::with_clock(repository, notifier, watch_window, Arc::new(SystemClock)).await
    }

    /// 今の時刻を`clock`から読むインスタンスを作成し、初期状態を取得する
    ///
    /// # Errors
    /// リポジトリから初期状態の取得に失敗した場合
    pub async fn with_clock(
        repository: Arc<R>,
        notifier: N,
        watch_window: Duration,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, ApplicationError> {
        let reservations =
            Self::fetch_current_reservations(&repository, watch_window, clock.now()).await?;
        Ok(Self {
            repository,
            notifier,
            interpreter: ReservationChangeInterpreter::new(),
            watch_window,
            previous_reservations: tokio::sync::Mutex::new(reservations),
            clock,
        })
    }

    /// 一度だけポーリングを実行し、変化を検知して通知する
    ///
    /// 出来事はすべて送ろうとする。送れなかった出来事の予約は前回の姿のまま覚えておき、
    /// 次のポーリングで同じ変化として読み直して送り直す。送れた出来事は送り直さない。
    ///
    /// # Errors
    /// リポジトリアクセスに失敗した場合、または送れなかった出来事があった場合（最初の失敗）
    pub async fn poll_once(&self) -> Result<(), ApplicationError> {
        let started = std::time::Instant::now();
        let now = self.clock.now();
        let current_reservations =
            Self::fetch_current_reservations(&self.repository, self.watch_window, now).await?;
        let mut previous_reservations = self.previous_reservations.lock().await;

        let changes = self
            .interpreter
            .interpret(
                self.repository.as_ref(),
                &previous_reservations,
                &current_reservations,
                now,
            )
            .await?;

        let notifications = changes.events.len();
        let mut unsent: HashSet<ReservationId> = HashSet::new();
        let mut first_failure = None;
        for event in changes.events {
            let reservation_id = event.reservation().id().clone();
            if let Err(e) = self.notifier.notify(event).await {
                error!(
                    reservation_id = %reservation_id.as_str(),
                    error = %e,
                    "announcing a reservation change failed; it is sent again on the next pass"
                );
                unsent.insert(reservation_id);
                first_failure.get_or_insert(e);
            }
        }

        // 1周の要約。件数の推移から、取得範囲や取りこぼしの異常に気づける
        info!(
            reservations = current_reservations.len(),
            appeared = changes.appeared,
            disappeared = changes.disappeared,
            notifications,
            elapsed_ms = started.elapsed().as_millis(),
            "change detection pass finished"
        );

        // 見えなくなったが続いている予約も覚えておく。予約枠が監視期間に戻ってきたときに、
        // 新しい予約と読まないため
        let mut known = current_reservations;
        for reservation in changes.carried_over {
            known.insert(reservation.id().clone(), reservation);
        }
        // 送れなかった変化は、前回の姿に戻しておくと次回また同じ変化として読める
        for id in unsent {
            match previous_reservations.get(&id) {
                Some(before) => known.insert(id, before.clone()),
                None => known.remove(&id),
            };
        }
        *previous_reservations = known;

        match first_failure {
            Some(e) => Err(e.into()),
            None => Ok(()),
        }
    }

    /// 監視範囲の予約を取得する
    async fn fetch_current_reservations(
        repository: &R,
        watch_window: Duration,
        now: DateTime<Utc>,
    ) -> Result<HashMap<ReservationId, Reservation>, ApplicationError> {
        // 監視する期間は現在時刻を起点に取り直す。ポーリングごとに窓が前へ進む
        let window = TimePeriod::new(now, now + watch_window)?;
        let reservations = repository.find_overlapping(&window).await?;
        Ok(reservations
            .into_iter()
            .map(|reservation| (reservation.id().clone(), reservation))
            .collect())
    }
}

#[cfg(test)]
#[path = "notify_reservation_changes_tests.rs"]
mod tests;
