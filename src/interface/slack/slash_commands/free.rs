//! /free コマンドハンドラ

use crate::domain::aggregates::resource_usage::value_objects::TimePeriod;
use crate::domain::common::EmailAddress;
use crate::domain::ports::notifier::Notifier;
use crate::domain::ports::repositories::{IdentityLinkRepository, ResourceUsageRepository};
use crate::domain::services::resource_usage::availability::{
    AvailabilityState, ResourceAvailability,
};
use crate::infrastructure::config::ResourceConfig;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::utility::user_resolver;
use crate::interface::slack::views::messages::availability as availability_message;
use chrono::{DateTime, Duration, Utc};
use slack_morphism::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};

/// 空きの算出で先読みする長さ
///
/// いまの状態を答えるだけなら現在時刻だけで足りるが、「いつまで空いているか」
/// 「ふさがっているものはいつ空くか」を伝えるには先の予約も要る。
/// 長くするほどカレンダーから取得する予定が増えるため、待たずに済む範囲に留める。
const LOOKAHEAD_DAYS: i64 = 7;

/// /free スラッシュコマンドを処理
///
/// いま空いているリソースを、本人にのみ見えるエフェメラルメッセージで返す。
pub async fn handle<R, N>(
    app: &SlackApp<R, N>,
    event: SlackCommandEvent,
) -> Result<SlackCommandEventResponse, Box<dyn std::error::Error + Send + Sync>>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    debug!(user = %event.user_id, "checking resource availability");

    let now = Utc::now();
    let window = TimePeriod::new(now, now + Duration::days(LOOKAHEAD_DAYS))?;

    let resources = app.resource_config().all_resources();
    let availabilities = app
        .usecases()
        .check_resource_availability
        .execute(&resources, &window)
        .await?;

    let free_count = availabilities
        .iter()
        .filter(|availability| availability.is_free_at(now))
        .count();
    info!(
        slack_user = %event.user_id,
        free = free_count,
        total = availabilities.len(),
        "reporting resource availability"
    );

    let owner_displays =
        resolve_owner_displays(&availabilities, now, &app.repositories().identity_link).await;
    let timezone = display_timezone(app.resource_config());

    let content =
        availability_message::build(&availabilities, now, timezone.as_deref(), &owner_displays);

    Ok(SlackCommandEventResponse::new(content))
}

/// いま使用中の予約について、予約者の表示名を解決する
///
/// 解決できなかった予約者は結果に含めない（ビューがメールアドレスで表示する）。
async fn resolve_owner_displays(
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
fn display_timezone(config: &ResourceConfig) -> Option<String> {
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
}
