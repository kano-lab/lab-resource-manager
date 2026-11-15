//! 定期ジョブ
//!
//! 予約の変化の通知と実利用の観測は、利用者の操作を待たずに一定間隔で動く。
//! どの入口（Slack・MCP）が動いているかに関わらず続ける。

use super::BoxError;
use super::services::{Repository, Services, seconds};
use super::slack::SlackConnection;
use crate::application::usecases::{
    DetectIdleReservedSlotsUseCase, IdleCriteria, NotifyReservationChangesUseCase,
    ReconcileObservedUsagesUseCase,
};
use crate::infrastructure::config::AppConfig;
use crate::infrastructure::idle_reserved_slot_notifier::SlackIdleReservedSlotNotifier;
use crate::infrastructure::notifier::NotificationRouter;
use crate::infrastructure::repositories::identity_link::JsonFileIdentityLinkRepository;
use crate::infrastructure::reservation_proposal::SlackReservationProposalNotifier;
use crate::infrastructure::resource_usage_observer::SharedFileResourceUsageObserver;
use crate::infrastructure::unauthorized_usage_notifier::SlackUnauthorizedUsageNotifier;
use chrono::Duration as ChronoDuration;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

/// 予約の変更を監視する期間の長さ
///
/// 前回と今回の状態を突き合わせて差分を通知するため、この長さを縮めると
/// 範囲外に出た予約が削除として通知される。繰り返し予約は個々の予定へ展開されるので、
/// 上限なしにはできない。
const NOTIFICATION_WATCH_WINDOW_DAYS: i64 = 60;

/// `shutdown`が取り消されるまで、`interval`ごとに`tick`を繰り返す
fn spawn_periodic<F, Fut>(
    interval: Duration,
    shutdown: CancellationToken,
    tick: F,
) -> JoinHandle<()>
where
    F: Fn() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send,
{
    tokio::spawn(async move {
        loop {
            tick().await;
            tokio::select! {
                _ = tokio::time::sleep(interval) => {}
                _ = shutdown.cancelled() => break,
            }
        }
    })
}

/// 予約の変化を各リソースの通知先へ知らせるジョブを始める
///
/// 通知先は`resources.toml`にリソースごとに書かれており、Slackボットとは独立している。
///
/// # Errors
/// 起点となる予約の一覧を読めない場合
pub(super) async fn start_reservation_notifications(
    app_config: &AppConfig,
    services: &Services,
    shutdown: CancellationToken,
) -> Result<JoinHandle<()>, BoxError> {
    let notifier = NotificationRouter::new(
        services.resource_config.as_ref().clone(),
        services.identity_repo.clone(),
    );
    let notify: Arc<NotifyReservationChangesUseCase<Repository, NotificationRouter>> = Arc::new(
        NotifyReservationChangesUseCase::new(
            services.reservation_repo.clone(),
            notifier,
            ChronoDuration::days(NOTIFICATION_WATCH_WINDOW_DAYS),
        )
        .await
        .map_err(|e| format!("通知UseCaseの初期化に失敗: {}", e))?,
    );

    info!(
        interval_secs = app_config.polling_interval_secs,
        "reservation change notifications enabled"
    );
    Ok(spawn_periodic(
        Duration::from_secs(app_config.polling_interval_secs),
        shutdown,
        move || {
            let notify = notify.clone();
            async move {
                if let Err(e) = notify.poll_once().await {
                    error!(error = %e, "reservation change polling failed");
                }
            }
        },
    ))
}

/// 実利用と予約を突き合わせ、予約者へ知らせるジョブを始める
///
/// 知らせる手段はSlackのDMなので、レポートの置き場とSlackの両方が要る。
pub(super) fn start_slot_observation(
    app_config: &AppConfig,
    services: &Services,
    slack: Option<&SlackConnection>,
    shutdown: CancellationToken,
) -> Option<JoinHandle<()>> {
    let Some(observer) = services.observer.clone() else {
        info!("slot observation disabled: GPU_USAGE_REPORTS_DIR is not set");
        return None;
    };
    let Some(slack) = slack else {
        warn!(
            "slot observation disabled: it messages reservation holders on Slack, \
             but SLACK_BOT_TOKEN and SLACK_APP_TOKEN are not set"
        );
        return None;
    };

    let reconcile = build_reconcile(app_config, services, slack, observer.clone());
    let detect_idle = build_detect_idle(app_config, services, slack, observer);

    info!(
        interval_secs = app_config.polling_interval_secs,
        "slot observation enabled"
    );
    Some(spawn_periodic(
        Duration::from_secs(app_config.polling_interval_secs),
        shutdown,
        move || {
            let reconcile = reconcile.clone();
            let detect_idle = detect_idle.clone();
            async move {
                if let Err(e) = reconcile.poll_once().await {
                    error!(error = %e, "slot observation polling failed");
                }
                if let Err(e) = detect_idle.poll_once().await {
                    error!(error = %e, "idle reservation polling failed");
                }
            }
        },
    ))
}

type Reconcile = ReconcileObservedUsagesUseCase<
    Repository,
    SharedFileResourceUsageObserver,
    JsonFileIdentityLinkRepository,
    SlackReservationProposalNotifier,
    SlackUnauthorizedUsageNotifier,
>;

fn build_reconcile(
    app_config: &AppConfig,
    services: &Services,
    slack: &SlackConnection,
    observer: Arc<SharedFileResourceUsageObserver>,
) -> Arc<Reconcile> {
    Arc::new(ReconcileObservedUsagesUseCase::new(
        services.reservation_repo.clone(),
        observer,
        services.identity_repo.clone(),
        SlackReservationProposalNotifier::new(
            slack.client.clone(),
            slack.bot_token.clone(),
            services.identity_repo.clone(),
        ),
        SlackUnauthorizedUsageNotifier::new(
            slack.client.clone(),
            slack.bot_token.clone(),
            services.identity_repo.clone(),
        ),
        seconds(app_config.unreserved_usage_threshold_secs),
        app_config.reservation_proposal_duration_candidates.clone(),
    ))
}

type DetectIdle = DetectIdleReservedSlotsUseCase<
    Repository,
    SharedFileResourceUsageObserver,
    SlackIdleReservedSlotNotifier,
>;

fn build_detect_idle(
    app_config: &AppConfig,
    services: &Services,
    slack: &SlackConnection,
    observer: Arc<SharedFileResourceUsageObserver>,
) -> Arc<DetectIdle> {
    Arc::new(DetectIdleReservedSlotsUseCase::new(
        services.reservation_repo.clone(),
        observer,
        SlackIdleReservedSlotNotifier::new(
            slack.client.clone(),
            slack.bot_token.clone(),
            services.identity_repo.clone(),
        ),
        IdleCriteria {
            absent_threshold: seconds(app_config.idle_reservation_threshold_secs),
            held_threshold: seconds(app_config.idle_held_gpu_threshold_secs),
            computing_utilization_percent: app_config.computing_gpu_utilization_percent,
            held_notices: app_config.idle_held_gpu_notices,
            observation_gap_tolerance: seconds(app_config.gpu_usage_max_staleness_secs),
        },
        services.idle_notices.clone(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const INTERVAL: Duration = Duration::from_secs(60);

    fn counting_job(shutdown: CancellationToken) -> (JoinHandle<()>, Arc<AtomicUsize>) {
        let ticks = Arc::new(AtomicUsize::new(0));
        let counter = ticks.clone();
        let handle = spawn_periodic(INTERVAL, shutdown, move || {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        });
        (handle, ticks)
    }

    #[tokio::test(start_paused = true)]
    async fn a_job_runs_once_per_interval() {
        let shutdown = CancellationToken::new();
        let (_handle, ticks) = counting_job(shutdown.clone());

        tokio::time::sleep(INTERVAL * 2 + Duration::from_secs(1)).await;

        assert_eq!(
            ticks.load(Ordering::SeqCst),
            3,
            "始めた直後と、間隔ごとに1回ずつ"
        );
        shutdown.cancel();
    }

    #[tokio::test(start_paused = true)]
    async fn a_job_stops_without_waiting_out_its_interval() {
        let shutdown = CancellationToken::new();
        let (handle, ticks) = counting_job(shutdown.clone());
        tokio::time::sleep(Duration::from_secs(1)).await;

        let started = tokio::time::Instant::now();
        shutdown.cancel();
        handle.await.unwrap();

        assert!(
            started.elapsed() < INTERVAL,
            "終了の合図を受けたら、次の回を待たずに止まる"
        );
        assert_eq!(ticks.load(Ordering::SeqCst), 1);
    }
}
