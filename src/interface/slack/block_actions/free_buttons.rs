//! 空き状況メッセージのボタンハンドラ
//!
//! `/free`の応答に置いた「いま」「今日」「明日」「他の日」と、
//! 「空いているものを予約」を処理します。
//!
//! 日を切り替えたときは、新しいメッセージを積まずに元のメッセージを差し替えます。
//! 積み上がると、どれがいまの状態なのか読み手が判断できなくなります。

use crate::domain::aggregates::resource_usage::value_objects::{Resource, TimePeriod};
use crate::domain::ports::notifier::Notifier;
use crate::domain::ports::repositories::ResourceUsageRepository;
use crate::domain::services::resource_usage::availability::ResourceAvailability;
use crate::interface::slack::app::SlackApp;
use crate::interface::slack::slack_client::{messages, modals};
use crate::interface::slack::utility::availability_report;
use crate::interface::slack::views::messages::availability::NOW;
use crate::interface::slack::views::modals::reserve;
use chrono::{Duration, NaiveDate, Utc};
use slack_morphism::prelude::*;
use tracing::{debug, error, info, warn};

/// 日を切り替えるボタン・メニューのクリックを処理
pub async fn handle_day_change<R, N>(
    app: &SlackApp<R, N>,
    block_actions: &SlackInteractionBlockActionsEvent,
    action: &SlackInteractionActionInfo,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    let Some(response_url) = &block_actions.response_url else {
        error!("cannot replace the availability message: no response url");
        return Ok(());
    };

    let Some(selection) = selected_day(action) else {
        warn!(action_id = %action.action_id, "the availability button carried no day");
        return Ok(());
    };

    let content = match selection {
        DaySelection::Now => availability_report::now_view(app).await?,
        DaySelection::Day(date) => availability_report::day_view(app, date).await?,
    };

    debug!(selection = ?selection, "replacing the availability message");
    messages::replace_ephemeral(app.http_client(), response_url, content).await;

    Ok(())
}

/// 表示を切り替える先
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DaySelection {
    /// いまの空き状況
    Now,
    /// ある一日の空き時間帯
    Day(NaiveDate),
}

/// ボタンやメニューが運んできた値を読む
///
/// ボタンは`value`に、セレクトメニューは選ばれた選択肢に日を持つ。
fn selected_day(action: &SlackInteractionActionInfo) -> Option<DaySelection> {
    let raw = action.value.clone().or_else(|| {
        action
            .selected_option
            .as_ref()
            .map(|option| option.value.clone())
    })?;

    if raw == NOW {
        return Some(DaySelection::Now);
    }

    raw.parse::<NaiveDate>().ok().map(DaySelection::Day)
}

/// 「空いているものを予約」ボタンのクリックを処理
///
/// いま空いているGPUのうち、最も多く空いているサーバーを選んだ状態で予約モーダルを開く。
/// 複数のサーバーに空きがあるとき、どれを開いても選び直しは起こる。
/// 選び直しがいちばん少なくて済むサーバーを選ぶ。
pub async fn handle_reserve<R, N>(
    app: &SlackApp<R, N>,
    block_actions: &SlackInteractionBlockActionsEvent,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    R: ResourceUsageRepository + Send + Sync + 'static,
    N: Notifier + Send + Sync + 'static,
{
    let now = Utc::now();
    let window = TimePeriod::new(
        now,
        now + Duration::days(availability_report::LOOKAHEAD_DAYS),
    )?;
    let availabilities = availability_report::availabilities_in(app, &window).await?;

    let Some((server, devices)) = server_with_most_free_devices(&availabilities, now) else {
        if let Some(response_url) = &block_actions.response_url {
            messages::send_ephemeral(
                app.http_client(),
                response_url,
                "いま空いているGPUがありません。".to_string(),
            )
            .await;
        }
        return Ok(());
    };

    info!(
        server = %server,
        devices = ?devices,
        "opening the reservation modal with the free devices chosen"
    );

    let modal = reserve::create_reserve_modal(
        app.resource_config(),
        &reserve::ReserveModalParams {
            selected_server: Some(&server),
            selected_devices: &devices,
            ..Default::default()
        },
    );

    modals::open(
        app.slack_client(),
        app.bot_token(),
        &block_actions.trigger_id,
        modal,
    )
    .await?;

    Ok(())
}

/// いま空いているGPUが最も多いサーバーと、その空きデバイス番号
///
/// 同数のサーバーが複数あれば、設定に先に書かれているほうを選ぶ。
fn server_with_most_free_devices(
    availabilities: &[ResourceAvailability],
    now: chrono::DateTime<Utc>,
) -> Option<(String, Vec<u32>)> {
    let mut servers: Vec<(String, Vec<u32>)> = Vec::new();

    for availability in availabilities {
        let Resource::Gpu(gpu) = availability.resource() else {
            continue;
        };
        if !availability.is_free_at(now) {
            continue;
        }

        match servers.iter_mut().find(|(name, _)| name == gpu.server()) {
            Some((_, devices)) => devices.push(gpu.device_number()),
            None => servers.push((gpu.server().to_string(), vec![gpu.device_number()])),
        }
    }

    // 空きが同数なら先に書かれているサーバーを残す（`max_by_key`は同値のとき後を採る）
    let mut most_free: Option<(String, Vec<u32>)> = None;
    for (name, devices) in servers {
        let is_better = most_free
            .as_ref()
            .is_none_or(|(_, current)| devices.len() > current.len());

        if is_better {
            most_free = Some((name, devices));
        }
    }

    most_free
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
    use crate::domain::aggregates::resource_usage::value_objects::Gpu;
    use crate::domain::common::EmailAddress;
    use crate::domain::services::resource_usage::availability::calculate;
    use chrono::{DateTime, TimeZone};

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 2, hour, 0, 0).unwrap()
    }

    fn window() -> TimePeriod {
        TimePeriod::new(at(9), at(21)).unwrap()
    }

    fn gpu(server: &str, device_number: u32) -> Resource {
        Resource::Gpu(Gpu::new(
            server.to_string(),
            device_number,
            "A100 80GB PCIe".to_string(),
        ))
    }

    fn reservation(resources: Vec<Resource>) -> ResourceUsage {
        ResourceUsage::new(
            EmailAddress::new("sato@example.com".to_string()).unwrap(),
            TimePeriod::new(at(9), at(18)).unwrap(),
            resources,
            None,
        )
        .unwrap()
    }

    fn availabilities(
        resources: &[Resource],
        usages: &[ResourceUsage],
    ) -> Vec<ResourceAvailability> {
        calculate(resources, &window(), usages)
    }

    #[test]
    fn the_server_with_the_most_free_devices_wins() {
        let resources = vec![
            gpu("gpu-server-1", 0),
            gpu("gpu-server-1", 1),
            gpu("gpu-server-2", 0),
            gpu("gpu-server-2", 1),
            gpu("gpu-server-2", 2),
        ];
        let usages = vec![reservation(vec![gpu("gpu-server-1", 1)])];

        let (server, devices) =
            server_with_most_free_devices(&availabilities(&resources, &usages), at(12)).unwrap();

        assert_eq!(server, "gpu-server-2");
        assert_eq!(devices, vec![0, 1, 2]);
    }

    #[test]
    fn a_tie_goes_to_the_server_listed_first() {
        let resources = vec![gpu("gpu-server-1", 0), gpu("gpu-server-2", 0)];

        let (server, _) =
            server_with_most_free_devices(&availabilities(&resources, &[]), at(12)).unwrap();

        assert_eq!(server, "gpu-server-1");
    }

    #[test]
    fn a_fully_booked_lab_offers_no_server() {
        let resources = vec![gpu("gpu-server-1", 0)];
        let usages = vec![reservation(vec![gpu("gpu-server-1", 0)])];

        assert_eq!(
            server_with_most_free_devices(&availabilities(&resources, &usages), at(12)),
            None
        );
    }

    #[test]
    fn rooms_do_not_count_as_free_gpus() {
        let resources = vec![Resource::Room {
            name: "Meeting Room A".to_string(),
        }];

        assert_eq!(
            server_with_most_free_devices(&availabilities(&resources, &[]), at(12)),
            None,
            "GPUを選んだ状態で開くモーダルに、部屋は渡せない"
        );
    }
}
