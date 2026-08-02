//! 空き状況の問い合わせとメッセージ組み立て
//!
//! `/free`コマンドと、その応答に置いたボタンの両方から使います。
//! どちらの入口から来ても同じ見え方になるよう、調べる範囲と表示の組み立てを1箇所に置きます。

use crate::domain::aggregates::resource_usage::value_objects::TimePeriod;
use crate::domain::common::EmailAddress;
use crate::domain::ports::notifier::Notifier;
use crate::domain::ports::repositories::{IdentityLinkRepository, ResourceUsageRepository};
use crate::domain::services::resource_usage::availability::{
    AvailabilityState, ResourceAvailability,
};
use crate::infrastructure::config::ResourceConfig;
use crate::infrastructure::notifier::formatter::{start_of_day, to_zoned};
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::utility::user_resolver;
use crate::interface::slack::views::messages::availability as availability_message;
use crate::interface::slack::views::messages::availability::days::{self, DayOption};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use slack_morphism::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

/// 空きを調べる範囲の長さ
///
/// いまの状態を答えるだけなら現在時刻だけで足りるが、「いつまで空いているか」
/// 「ふさがっているものはいつ空くか」を伝えるには先の予約も要る。
/// 選べる日の範囲もこれに従う。データのない日を選ばせないため。
pub const LOOKAHEAD_DAYS: i64 = 7;

/// いまの空き状況メッセージを組み立てる
pub async fn now_view<R, N>(
    app: &SlackApp<R, N>,
) -> Result<SlackMessageContent, Box<dyn std::error::Error + Send + Sync>>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    let now = Utc::now();
    let timezone = display_timezone(app.resource_config());
    let window = TimePeriod::new(now, now + Duration::days(LOOKAHEAD_DAYS))?;

    let availabilities = availabilities_in(app, &window).await?;
    let owner_displays =
        resolve_owner_displays(&availabilities, now, &app.repositories().identity_link).await;

    Ok(availability_message::build(
        &availabilities,
        now,
        timezone.as_deref(),
        &owner_displays,
        &day_options(now, timezone.as_deref()),
    ))
}

/// ある一日の空き時間帯メッセージを組み立てる
///
/// 今日を指定した場合、過ぎた時間は範囲に含めない。
pub async fn day_view<R, N>(
    app: &SlackApp<R, N>,
    date: NaiveDate,
) -> Result<SlackMessageContent, Box<dyn std::error::Error + Send + Sync>>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    let now = Utc::now();
    let timezone = display_timezone(app.resource_config());
    let window = day_window(date, now, timezone.as_deref())
        .ok_or_else(|| format!("{}の範囲を求められませんでした", date))?;

    let availabilities = availabilities_in(app, &window).await?;

    Ok(availability_message::day::build(
        &availabilities,
        &window,
        date,
        today_in(now, timezone.as_deref()),
        &day_options(now, timezone.as_deref()),
        timezone.as_deref(),
    ))
}

/// 指定した範囲における全リソースの空き
pub async fn availabilities_in<R, N>(
    app: &SlackApp<R, N>,
    window: &TimePeriod,
) -> Result<Vec<ResourceAvailability>, Box<dyn std::error::Error + Send + Sync>>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    let resources = app.resource_config().all_resources();

    Ok(app
        .usecases()
        .check_resource_availability
        .execute(&resources, window)
        .await?)
}

/// 選べる日の一覧（今日から先読みの長さぶん）
pub fn day_options(now: DateTime<Utc>, timezone: Option<&str>) -> Vec<DayOption> {
    days::options_from(today_in(now, timezone), LOOKAHEAD_DAYS as usize)
}

/// 表示タイムゾーンでの今日
pub fn today_in(now: DateTime<Utc>, timezone: Option<&str>) -> NaiveDate {
    to_zoned(now, timezone).date_naive()
}

/// ある一日の範囲
///
/// 今日であれば現在時刻から始める。過ぎた時間の空きを見せても選べない。
fn day_window(date: NaiveDate, now: DateTime<Utc>, timezone: Option<&str>) -> Option<TimePeriod> {
    let start = start_of_day(date, timezone)?.max(now);
    let end = start_of_day(date.succ_opt()?, timezone)?;

    TimePeriod::new(start, end).ok()
}

/// いま使用中の予約について、予約者の表示名を解決する
///
/// 解決できなかった予約者は結果に含めない（ビューがメールアドレスで表示する）。
pub async fn resolve_owner_displays(
    availabilities: &[ResourceAvailability],
    now: DateTime<Utc>,
    identity_repo: &Arc<dyn IdentityLinkRepository>,
) -> HashMap<EmailAddress, String> {
    let mut owners: Vec<EmailAddress> = Vec::new();

    for availability in availabilities {
        if let AvailabilityState::Busy(busy) = availability.state_at(now)
            && !owners.contains(busy.owner_email())
        {
            owners.push(busy.owner_email().clone());
        }
    }

    let mut displays = HashMap::with_capacity(owners.len());
    for owner in owners {
        let display = user_resolver::resolve_display_name(&owner, identity_repo).await;
        displays.insert(owner, display);
    }

    displays
}

/// 表示に使うタイムゾーン
///
/// リソースをまたいで一覧するため、リソースごとの設定ではなく設定全体で
/// ひとつに決める。どこにも指定がなければボットのローカル時刻で表示される。
pub fn display_timezone(config: &ResourceConfig) -> Option<String> {
    config
        .servers
        .iter()
        .flat_map(|server| &server.notifications)
        .chain(config.rooms.iter().flat_map(|room| &room.notifications))
        .find_map(|notification| notification.timezone())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::config::{
        DeviceConfig, NotificationConfig, RoomConfig, ServerConfig,
    };
    use chrono::TimeZone;

    const TOKYO: Option<&str> = Some("Asia/Tokyo");

    fn slack_notification(timezone: Option<&str>) -> NotificationConfig {
        NotificationConfig::Slack {
            bot_token: "xoxb-test".to_string(),
            channel_id: "C0TEST".to_string(),
            timezone: timezone.map(str::to_string),
            templates: None,
            format: None,
        }
    }

    fn config(
        server_notifications: Vec<NotificationConfig>,
        room_notifications: Vec<NotificationConfig>,
    ) -> ResourceConfig {
        ResourceConfig {
            servers: vec![ServerConfig {
                name: "gpu-server-1".to_string(),
                calendar_id: "cal".to_string(),
                devices: vec![DeviceConfig {
                    id: 0,
                    model: "A100 80GB PCIe".to_string(),
                }],
                notifications: server_notifications,
            }],
            rooms: vec![RoomConfig {
                name: "Meeting Room A".to_string(),
                calendar_id: "cal-room".to_string(),
                notifications: room_notifications,
            }],
        }
    }

    /// 日本時間8月2日14:00
    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 2, 5, 0, 0).unwrap()
    }

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, day).unwrap()
    }

    #[test]
    fn the_timezone_comes_from_the_configuration() {
        let config = config(vec![slack_notification(Some("Asia/Tokyo"))], vec![]);

        assert_eq!(display_timezone(&config), Some("Asia/Tokyo".to_string()));
    }

    #[test]
    fn a_timezone_set_only_on_a_room_still_applies() {
        let config = config(
            vec![slack_notification(None)],
            vec![slack_notification(Some("Asia/Tokyo"))],
        );

        assert_eq!(
            display_timezone(&config),
            Some("Asia/Tokyo".to_string()),
            "一覧はリソースをまたぐので、どこに書かれていても拾う"
        );
    }

    #[test]
    fn no_timezone_anywhere_falls_back_to_the_local_clock() {
        let config = config(vec![slack_notification(None)], vec![]);

        assert_eq!(display_timezone(&config), None);
    }

    #[test]
    fn today_is_read_in_the_display_timezone() {
        assert_eq!(
            today_in(now(), TOKYO),
            date(2),
            "UTCでは8月2日5時、日本時間では同じ日の14時"
        );
    }

    #[test]
    fn a_future_day_covers_that_whole_day() {
        let window = day_window(date(3), now(), TOKYO).unwrap();

        assert_eq!(
            to_zoned(window.start(), TOKYO)
                .format("%m-%d %H:%M")
                .to_string(),
            "08-03 00:00"
        );
        assert_eq!(
            to_zoned(window.end(), TOKYO)
                .format("%m-%d %H:%M")
                .to_string(),
            "08-04 00:00"
        );
    }

    #[test]
    fn today_starts_from_now_instead_of_midnight() {
        let window = day_window(date(2), now(), TOKYO).unwrap();

        assert_eq!(
            window.start(),
            now(),
            "過ぎた時間の空きを見せても選びようがない"
        );
    }

    #[test]
    fn the_selectable_days_start_from_today() {
        let options = day_options(now(), TOKYO);

        assert_eq!(options.len(), LOOKAHEAD_DAYS as usize);
        assert_eq!(options[0].date, date(2));
        assert_eq!(
            options.last().unwrap().date,
            date(8),
            "調べる範囲より先の日は選ばせない"
        );
    }
}
