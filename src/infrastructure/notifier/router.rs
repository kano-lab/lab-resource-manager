use crate::application::{Clock, SystemClock};
use crate::domain::aggregates::reservation::{OccurrenceKey, ReservationEvent};
use crate::domain::ports::notifier::{NotificationError, Notifier};
use crate::domain::ports::repositories::IdentityLinkRepository;
use crate::infrastructure::config::{NotificationConfig, ResourceConfig};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::{Arc, Mutex};

use super::senders::{
    MockSender, SlackSender,
    sender::{NotificationContext, Sender},
    slack::SlackNotificationConfig,
};
use tracing::{error, warn};

/// 送れなかった送り先へ、送り直しを続ける長さ
///
/// これを過ぎても届かない送り先は、設定の誤り（チャンネルが無い、権限が無いなど）か、
/// 長い障害と見なして諦める。古い知らせを遅れて届けても役に立ちにくい。
fn retry_window() -> Duration {
    Duration::hours(1)
}

/// 複数の通知手段をオーケストレートし、リソースに基づいて適切な通知先にルーティングする
///
/// 各種Sender（Slack, Mock等）を保持し、通知設定の種類に応じて適切なSenderに委譲します。
///
/// # 届かなかった送り先への送り直し
///
/// 出来事は送り先ごとに独立して送る。どこかの送り先に届かなかったときは失敗を返し、
/// 呼び出し側（予約の変化の通知）が次の回に同じ出来事を送り直す。そのとき、すでに届いた
/// 送り先には送らず、届かなかった送り先にだけ送る。どの出来事をどの送り先へ届けたかは、
/// 最初に送ろうとしてから1時間覚えておく。1時間たっても届かない送り先は諦め、
/// エラーとして記録して成功を返す。
pub struct NotificationRouter {
    config: ResourceConfig,
    slack_sender: SlackSender,
    mock_sender: MockSender,
    identity_repo: Arc<dyn IdentityLinkRepository>,
    ledger: Mutex<DeliveryLedger>,
    clock: Arc<dyn Clock>,
}

impl NotificationRouter {
    /// 新しい通知ルーターを作成
    ///
    /// # Arguments
    /// * `config` - リソース設定
    /// * `identity_repo` - ID紐付けリポジトリ
    pub fn new(config: ResourceConfig, identity_repo: Arc<dyn IdentityLinkRepository>) -> Self {
        Self {
            config,
            slack_sender: SlackSender::new(),
            mock_sender: MockSender::new(),
            identity_repo,
            ledger: Mutex::new(DeliveryLedger::default()),
            clock: Arc::new(SystemClock),
        }
    }

    /// 今の時刻を`clock`から読むようにする（送り直しを諦める時刻の判断に使う）
    pub fn with_clock(self, clock: Arc<dyn Clock>) -> Self {
        Self { clock, ..self }
    }

    fn collect_notification_configs(&self, event: &ReservationEvent) -> Vec<NotificationConfig> {
        let mut configs = HashSet::new();
        for resource in event.resources() {
            let resource_configs = self.config.get_notifications_for_resource(resource);
            configs.extend(resource_configs);
        }

        configs.into_iter().collect()
    }

    async fn send_to_destination(
        &self,
        config: &NotificationConfig,
        event: &ReservationEvent,
    ) -> Result<(), NotificationError> {
        let user_email = event.holder_email();

        // IdentityLinkを取得
        // Ok(None) = IdentityLinkが未登録（正常ケース）
        // Ok(Some(_)) = IdentityLinkが存在
        // Err(_) = リポジトリエラー（DB接続障害等の異常ケース）
        let identity_link = self
            .identity_repo
            .find_by_email(user_email)
            .await
            .map_err(|e| {
                NotificationError::RepositoryError(format!(
                    "IdentityLink取得失敗 (email: {}): {}",
                    user_email.as_str(),
                    e
                ))
            })?;

        let context = NotificationContext {
            event,
            identity_link: identity_link.as_ref(),
            timezone: config.timezone(),
            customization: config.customization(),
        };

        match config {
            NotificationConfig::Slack {
                bot_token,
                channel_id,
                ..
            } => {
                let slack_config = SlackNotificationConfig {
                    bot_token: bot_token.clone(),
                    channel_id: channel_id.clone(),
                };
                self.slack_sender.send(&slack_config, context).await
            }
            NotificationConfig::Mock { .. } => self.mock_sender.send(&(), context).await,
        }
    }
}

#[async_trait]
impl Notifier for NotificationRouter {
    async fn notify(&self, event: ReservationEvent) -> Result<(), NotificationError> {
        let notification_configs = self.collect_notification_configs(&event);

        if notification_configs.is_empty() {
            // 通知先が設定されていない場合は何もしない
            return Ok(());
        }

        let event = &event;
        deliver(
            &self.ledger,
            notification_configs,
            event,
            self.clock.now(),
            |destination| async move { self.send_to_destination(&destination, event).await },
        )
        .await
    }
}

/// 出来事を見分ける鍵：どの予約の、どの版についての、どの種類の出来事か
///
/// 予約は変わるたびに版が変わるので、同じ鍵の出来事は同じ変化を指す。送り直しのために
/// 呼び出し側が読み直した出来事も、同じ鍵になる。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct EventKey {
    kind: &'static str,
    reservation_id: String,
    version: Option<String>,
    slot: Option<OccurrenceKey>,
}

impl EventKey {
    fn of(event: &ReservationEvent) -> Self {
        let (kind, slot) = match event {
            ReservationEvent::Reserved(_) => ("reserved", None),
            ReservationEvent::Changed(_) => ("changed", None),
            ReservationEvent::EndedEarly(_) => ("ended_early", None),
            ReservationEvent::Cancelled(_) => ("cancelled", None),
            ReservationEvent::SlotChanged { slot, .. } => {
                ("slot_changed", Some(slot.key().clone()))
            }
            ReservationEvent::SlotEndedEarly { slot, .. } => {
                ("slot_ended_early", Some(slot.key().clone()))
            }
            ReservationEvent::SlotCancelled { slot, .. } => {
                ("slot_cancelled", Some(slot.key().clone()))
            }
        };
        let reservation = event.reservation();
        Self {
            kind,
            reservation_id: reservation.id().as_str().to_string(),
            version: reservation.version().map(|v| v.as_str().to_string()),
            slot,
        }
    }
}

/// どの出来事を、どの送り先へ届けたかの記録
#[derive(Debug, Default)]
struct DeliveryLedger {
    entries: HashMap<EventKey, Delivery>,
}

/// ひとつの出来事の届け具合
#[derive(Debug)]
struct Delivery {
    /// 最初に送ろうとした時刻
    first_attempt_at: DateTime<Utc>,
    /// 届いた送り先
    delivered: HashSet<NotificationConfig>,
}

/// 送ってみた結果
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// すべての送り先に届いた
    Delivered,
    /// 届かなかった送り先がある。次の機会に送り直す
    WillRetry,
    /// 送り直しの期間を過ぎても届かない送り先があり、諦めた
    GaveUp,
}

impl DeliveryLedger {
    /// まだ届いていない送り先
    fn not_yet_delivered(
        &self,
        key: &EventKey,
        destinations: Vec<NotificationConfig>,
    ) -> Vec<NotificationConfig> {
        match self.entries.get(key) {
            Some(delivery) => destinations
                .into_iter()
                .filter(|destination| !delivery.delivered.contains(destination))
                .collect(),
            None => destinations,
        }
    }

    /// 送ってみた結果を記録する
    fn record(
        &mut self,
        key: &EventKey,
        delivered: Vec<NotificationConfig>,
        failed: bool,
        now: DateTime<Utc>,
    ) -> Outcome {
        let delivery = self.entries.entry(key.clone()).or_insert(Delivery {
            first_attempt_at: now,
            delivered: HashSet::new(),
        });
        delivery.delivered.extend(delivered);
        let outcome = if !failed {
            Outcome::Delivered
        } else if now - delivery.first_attempt_at >= retry_window() {
            Outcome::GaveUp
        } else {
            Outcome::WillRetry
        };
        if outcome == Outcome::GaveUp {
            self.entries.remove(key);
        }
        // 送り直しの期間を過ぎた記録は忘れる。届け終えた出来事も、その間は覚えておき、
        // 読み直されて送られてきても重ねて届けない
        self.entries
            .retain(|_, delivery| now - delivery.first_attempt_at < retry_window());
        outcome
    }
}

/// 送り先の説明（ログ用。トークンは含めない）
fn describe(destination: &NotificationConfig) -> String {
    match destination {
        NotificationConfig::Slack { channel_id, .. } => format!("slack:{channel_id}"),
        NotificationConfig::Mock { .. } => "mock".to_string(),
    }
}

/// 出来事を、まだ届いていない送り先へ送る
///
/// 届かなかった送り先があれば最初の失敗を返し、次に同じ出来事が来たときにその送り先へ
/// 送り直す。送り直しの期間を過ぎたら諦め、エラーとして記録して成功を返す。
async fn deliver<F, Fut>(
    ledger: &Mutex<DeliveryLedger>,
    destinations: Vec<NotificationConfig>,
    event: &ReservationEvent,
    now: DateTime<Utc>,
    send: F,
) -> Result<(), NotificationError>
where
    F: Fn(NotificationConfig) -> Fut,
    Fut: Future<Output = Result<(), NotificationError>>,
{
    let key = EventKey::of(event);
    let pending = ledger.lock().unwrap().not_yet_delivered(&key, destinations);

    let mut delivered = Vec::new();
    let mut undelivered = Vec::new();
    let mut first_failure = None;
    for destination in pending {
        match send(destination.clone()).await {
            Ok(()) => delivered.push(destination),
            Err(e) => {
                warn!(
                    destination = %describe(&destination),
                    reservation_id = %key.reservation_id,
                    event = key.kind,
                    error = %e,
                    "sending a notification to one destination failed; it is sent again next time"
                );
                undelivered.push(describe(&destination));
                first_failure.get_or_insert(e);
            }
        }
    }

    let outcome = ledger
        .lock()
        .unwrap()
        .record(&key, delivered, first_failure.is_some(), now);
    match (outcome, first_failure) {
        (Outcome::GaveUp, Some(e)) => {
            error!(
                destinations = ?undelivered,
                reservation_id = %key.reservation_id,
                event = key.kind,
                error = %e,
                "a notification could not be delivered for an hour; giving up"
            );
            Ok(())
        }
        (_, Some(e)) => Err(e),
        (_, None) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::reservation::value_objects::{Gpu, Resource};
    use crate::domain::aggregates::reservation::{Reservation, Version};
    use crate::domain::common::{EmailAddress, TimePeriod};
    use chrono::TimeZone;

    fn at(minute: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 16, 1, 0, 0).unwrap() + Duration::minutes(minute)
    }

    /// 見分けのつく送り先（時間帯の違うモック）
    fn destination(name: &str) -> NotificationConfig {
        NotificationConfig::Mock {
            timezone: Some(name.to_string()),
            templates: None,
            format: None,
        }
    }

    fn reserved_at_version(version: &str) -> ReservationEvent {
        let mut reservation = Reservation::one_time(
            EmailAddress::new("holder@example.com".to_string()).unwrap(),
            TimePeriod::new(at(60), at(120)).unwrap(),
            vec![Resource::Gpu(Gpu::new(
                "Thalys".to_string(),
                0,
                "A100".to_string(),
            ))],
            None,
        )
        .unwrap();
        reservation.record_version(Some(Version::new(version.to_string())));
        ReservationEvent::Reserved(reservation)
    }

    /// 送り先ごとに、何回失敗させるかを決めておける送り手
    #[derive(Default)]
    struct Script {
        failures_left: Mutex<HashMap<NotificationConfig, u32>>,
        sent: Mutex<Vec<NotificationConfig>>,
    }

    impl Script {
        fn failing(destination: &NotificationConfig, times: u32) -> Self {
            let script = Self::default();
            script
                .failures_left
                .lock()
                .unwrap()
                .insert(destination.clone(), times);
            script
        }

        async fn send(&self, destination: NotificationConfig) -> Result<(), NotificationError> {
            let mut failures_left = self.failures_left.lock().unwrap();
            if let Some(left) = failures_left.get_mut(&destination)
                && *left > 0
            {
                *left -= 1;
                return Err(NotificationError::SendFailure(
                    "Slackが応答しない".to_string(),
                ));
            }
            self.sent.lock().unwrap().push(destination);
            Ok(())
        }

        /// これまでに届けた送り先を取り出し、記録を空にする
        fn take_sent(&self) -> Vec<NotificationConfig> {
            std::mem::take(&mut self.sent.lock().unwrap())
        }
    }

    async fn deliver_with(
        ledger: &Mutex<DeliveryLedger>,
        script: &Script,
        event: &ReservationEvent,
        now: DateTime<Utc>,
    ) -> Result<(), NotificationError> {
        deliver(
            ledger,
            vec![destination("a"), destination("b")],
            event,
            now,
            |destination| script.send(destination),
        )
        .await
    }

    #[tokio::test]
    async fn a_destination_that_missed_the_event_gets_it_next_time_and_the_other_does_not() {
        let ledger = Mutex::new(DeliveryLedger::default());
        let script = Script::failing(&destination("b"), 1);
        let event = reserved_at_version("\"1\"");

        let first = deliver_with(&ledger, &script, &event, at(0)).await;
        let sent_first = script.take_sent();
        let second = deliver_with(&ledger, &script, &event, at(1)).await;
        let sent_second = script.take_sent();

        assert!(
            matches!(first, Err(NotificationError::SendFailure(_))),
            "届かなかった送り先があれば失敗を返し、送り直してもらう: {first:?}"
        );
        assert_eq!(sent_first, vec![destination("a")]);
        assert!(second.is_ok(), "{second:?}");
        assert_eq!(
            sent_second,
            vec![destination("b")],
            "届かなかった送り先にだけ送り直す"
        );
    }

    #[tokio::test]
    async fn an_event_already_delivered_everywhere_is_not_sent_again() {
        // 同じ予約の別の出来事が失敗して、呼び出し側がこの出来事も読み直して送ってくる
        let ledger = Mutex::new(DeliveryLedger::default());
        let script = Script::default();
        let event = reserved_at_version("\"1\"");
        deliver_with(&ledger, &script, &event, at(0)).await.unwrap();
        script.take_sent();

        let again = deliver_with(&ledger, &script, &event, at(1)).await;

        assert!(again.is_ok(), "{again:?}");
        assert!(script.take_sent().is_empty(), "重ねて届けない");
    }

    #[tokio::test]
    async fn a_new_change_of_the_same_reservation_goes_to_every_destination() {
        let ledger = Mutex::new(DeliveryLedger::default());
        let script = Script::default();
        deliver_with(&ledger, &script, &reserved_at_version("\"1\""), at(0))
            .await
            .unwrap();
        script.take_sent();

        deliver_with(&ledger, &script, &reserved_at_version("\"2\""), at(1))
            .await
            .unwrap();

        assert_eq!(
            script.take_sent(),
            vec![destination("a"), destination("b")],
            "版が変われば別の変化"
        );
    }

    #[tokio::test]
    async fn a_destination_that_misses_the_event_for_an_hour_is_given_up() {
        let ledger = Mutex::new(DeliveryLedger::default());
        let script = Script::failing(&destination("b"), u32::MAX);
        let event = reserved_at_version("\"1\"");

        let first = deliver_with(&ledger, &script, &event, at(0)).await;
        let before_an_hour = deliver_with(&ledger, &script, &event, at(59)).await;
        let after_an_hour = deliver_with(&ledger, &script, &event, at(60)).await;

        assert!(first.is_err());
        assert!(before_an_hour.is_err(), "1時間までは送り直してもらう");
        assert!(
            after_an_hour.is_ok(),
            "1時間たっても届かなければ諦め、送り直しを求めない: {after_an_hour:?}"
        );
        assert_eq!(
            script.take_sent(),
            vec![destination("a")],
            "届いた送り先には、送り直しのあいだも重ねて送らない"
        );
    }

    #[tokio::test]
    async fn what_was_delivered_is_remembered_only_for_an_hour() {
        let ledger = Mutex::new(DeliveryLedger::default());
        let script = Script::default();
        deliver_with(&ledger, &script, &reserved_at_version("\"1\""), at(0))
            .await
            .unwrap();
        deliver_with(&ledger, &script, &reserved_at_version("\"2\""), at(61))
            .await
            .unwrap();

        assert_eq!(
            ledger.lock().unwrap().entries.len(),
            1,
            "1時間を過ぎた記録は忘れる（覚えておくものが増え続けない）"
        );
    }
}
