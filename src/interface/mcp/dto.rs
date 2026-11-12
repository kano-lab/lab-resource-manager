//! MCPツールの入出力DTO
//!
//! ドメインエンティティはserde非依存のため、JSON出力専用のDTOをここに定義する。

use crate::domain::aggregates::reservation::value_objects::Resource;
use crate::domain::common::TimePeriod;
use crate::domain::read_models::ReservedSlotView;
use crate::domain::services::reservation::availability::ResourceAvailability;
use crate::infrastructure::config::ResourceConfig;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 予約枠の出力用DTO
#[derive(Debug, Serialize)]
pub struct ReservedSlotDto {
    /// 予約枠ID（`get_reserved_slot`・`update_reserved_slot`・`end_reserved_slot_early`・`cancel_reserved_slot`に指定できる）
    pub id: String,
    pub holder_email: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub resources: Vec<String>,
    pub notes: Option<String>,
}

impl From<&ReservedSlotView> for ReservedSlotDto {
    fn from(slot: &ReservedSlotView) -> Self {
        Self {
            id: slot.id().as_str().to_string(),
            holder_email: slot.holder_email().as_str().to_string(),
            start: slot.time_period().start(),
            end: slot.time_period().end(),
            resources: slot.resources().iter().map(|r| r.to_string()).collect(),
            notes: slot.notes().cloned(),
        }
    }
}

/// 予約枠IDだけを受け取るツールの引数
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReservedSlotIdParams {
    /// 予約枠ID
    pub id: String,
}

/// `create_reservation`の引数
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateReservationParams {
    /// リソース種別（"gpu" または "room"）
    pub resource_type: String,
    /// GPU予約時のサーバー名
    #[serde(default)]
    pub server: Option<String>,
    /// GPU予約時のデバイス番号（省略時はサーバーの全デバイス）
    #[serde(default)]
    pub device_numbers: Option<Vec<u32>>,
    /// 部屋予約時の部屋名
    #[serde(default)]
    pub room: Option<String>,
    /// 開始時刻（UTC）
    pub start: DateTime<Utc>,
    /// 終了時刻（UTC）
    pub end: DateTime<Utc>,
    /// 備考
    #[serde(default)]
    pub notes: Option<String>,
}

/// `update_reserved_slot`の引数
#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateReservedSlotParams {
    /// 予約枠ID
    pub id: String,
    /// 新しい開始時刻（endと同時に指定、省略時は時間変更なし）
    #[serde(default)]
    pub start: Option<DateTime<Utc>>,
    /// 新しい終了時刻（startと同時に指定、省略時は時間変更なし）
    #[serde(default)]
    pub end: Option<DateTime<Utc>>,
    /// 新しい備考
    #[serde(default)]
    pub notes: Option<String>,
}

/// `list_resources`の出力：予約できるリソースの一覧
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ResourceCatalogDto {
    /// GPUサーバー（`create_reservation`の`server`と`device_numbers`に指定できる値）
    pub servers: Vec<ServerDto>,
    /// 部屋（`create_reservation`の`room`に指定できる値）
    pub rooms: Vec<String>,
}

/// GPUサーバーと、その予約できるデバイス
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ServerDto {
    pub name: String,
    pub devices: Vec<DeviceDto>,
}

/// GPUデバイス
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct DeviceDto {
    pub device_number: u32,
    pub model: String,
}

impl From<&ResourceConfig> for ResourceCatalogDto {
    fn from(config: &ResourceConfig) -> Self {
        Self {
            servers: config
                .servers
                .iter()
                .map(|server| ServerDto {
                    name: server.name.clone(),
                    devices: server
                        .devices
                        .iter()
                        .map(|device| DeviceDto {
                            device_number: device.id,
                            model: device.model.clone(),
                        })
                        .collect(),
                })
                .collect(),
            rooms: config.rooms.iter().map(|room| room.name.clone()).collect(),
        }
    }
}

/// `check_availability`の引数
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CheckAvailabilityParams {
    /// 調べる期間の開始時刻（UTC）
    pub start: DateTime<Utc>,
    /// 調べる期間の終了時刻（UTC）
    pub end: DateTime<Utc>,
    /// このGPUサーバーに絞る（roomとともに省略すると全リソースが対象）
    #[serde(default)]
    pub server: Option<String>,
    /// この部屋に絞る（serverとともに省略すると全リソースが対象）
    #[serde(default)]
    pub room: Option<String>,
}

/// `check_availability`が対象とするリソースを決める
///
/// 絞り込みの指定がなければ全リソース、あれば指定されたサーバーの全デバイスと部屋。
///
/// # Errors
/// 指定されたサーバーまたは部屋が設定に無い場合
pub fn resources_to_check(
    config: &ResourceConfig,
    server: Option<&str>,
    room: Option<&str>,
) -> Result<Vec<Resource>, String> {
    if server.is_none() && room.is_none() {
        return Ok(config.all_resources());
    }

    if let Some(server) = server
        && config.get_server(server).is_none()
    {
        return Err(format!("サーバー {server} が見つかりません"));
    }
    if let Some(room) = room
        && !config.rooms.iter().any(|candidate| candidate.name == room)
    {
        return Err(format!("部屋 {room} が見つかりません"));
    }

    Ok(config
        .all_resources()
        .into_iter()
        .filter(|resource| match resource {
            Resource::Gpu(gpu) => server == Some(gpu.server()),
            Resource::Room { name } => room == Some(name.as_str()),
        })
        .collect())
}

/// 期間（開始・終了）の出力用DTO
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct PeriodDto {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl From<&TimePeriod> for PeriodDto {
    fn from(period: &TimePeriod) -> Self {
        Self {
            start: period.start(),
            end: period.end(),
        }
    }
}

/// 予約枠で埋まっている区間の出力用DTO
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct BusyPeriodDto {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    /// 押さえている予約枠のID
    pub reserved_slot_id: String,
    /// 押さえている予約枠の予約者
    pub holder_email: String,
}

/// あるリソースの、調べた期間における空きの出力用DTO
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ResourceAvailabilityDto {
    /// リソースの表示名
    pub resource: String,
    /// GPUの場合のサーバー名（`create_reservation`の`server`に指定できる）
    pub server: Option<String>,
    /// GPUの場合のデバイス番号（`create_reservation`の`device_numbers`に指定できる）
    pub device_number: Option<u32>,
    /// 部屋の場合の部屋名（`create_reservation`の`room`に指定できる）
    pub room: Option<String>,
    /// 調べた期間の全体が空いているか
    pub entirely_free: bool,
    /// 空いている区間（開始順）
    pub free_periods: Vec<PeriodDto>,
    /// 予約枠で埋まっている区間（開始順）。期間の外にはみ出す予約枠は期間内の部分だけを示す
    pub busy_periods: Vec<BusyPeriodDto>,
}

impl From<&ResourceAvailability> for ResourceAvailabilityDto {
    fn from(availability: &ResourceAvailability) -> Self {
        let (server, device_number, room) = match availability.resource() {
            Resource::Gpu(gpu) => (
                Some(gpu.server().to_string()),
                Some(gpu.device_number()),
                None,
            ),
            Resource::Room { name } => (None, None, Some(name.clone())),
        };

        Self {
            resource: availability.resource().to_string(),
            server,
            device_number,
            room,
            entirely_free: availability.is_entirely_free(),
            free_periods: availability
                .free_periods()
                .iter()
                .map(PeriodDto::from)
                .collect(),
            busy_periods: availability
                .busy_periods()
                .iter()
                .map(|busy| BusyPeriodDto {
                    start: busy.period().start(),
                    end: busy.period().end(),
                    reserved_slot_id: busy.slot_id().as_str().to_string(),
                    holder_email: busy.holder_email().as_str().to_string(),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::reservation::value_objects::Gpu;
    use crate::domain::common::EmailAddress;
    use crate::domain::services::reservation::availability;
    use crate::infrastructure::config::{DeviceConfig, RoomConfig, ServerConfig};
    use chrono::{Duration, TimeZone};

    fn config() -> ResourceConfig {
        let server = |name: &str, devices: &[u32]| ServerConfig {
            name: name.to_string(),
            devices: devices
                .iter()
                .map(|id| DeviceConfig {
                    id: *id,
                    model: "A100".to_string(),
                })
                .collect(),
            notifications: vec![],
        };
        ResourceConfig {
            servers: vec![server("Thalys", &[0, 1]), server("Freccia", &[0])],
            rooms: vec![RoomConfig {
                name: "会議室A".to_string(),
                notifications: vec![],
            }],
        }
    }

    fn names(resources: &[Resource]) -> Vec<String> {
        resources
            .iter()
            .map(|resource| resource.to_string())
            .collect()
    }

    #[test]
    fn the_catalog_lists_what_can_be_named_when_reserving() {
        let catalog = ResourceCatalogDto::from(&config());

        assert_eq!(
            catalog
                .servers
                .iter()
                .map(|server| (
                    server.name.as_str(),
                    server
                        .devices
                        .iter()
                        .map(|device| device.device_number)
                        .collect::<Vec<_>>()
                ))
                .collect::<Vec<_>>(),
            vec![("Thalys", vec![0, 1]), ("Freccia", vec![0])]
        );
        assert_eq!(catalog.rooms, vec!["会議室A".to_string()]);
    }

    #[test]
    fn without_a_filter_every_resource_is_checked() {
        let resources = resources_to_check(&config(), None, None).unwrap();

        assert_eq!(
            resources.len(),
            4,
            "GPU3台と部屋1つ: {:?}",
            names(&resources)
        );
    }

    #[test]
    fn a_server_filter_narrows_to_the_devices_of_that_server() {
        let resources = resources_to_check(&config(), Some("Thalys"), None).unwrap();

        assert_eq!(resources.len(), 2, "{:?}", names(&resources));
        assert!(
            resources
                .iter()
                .all(|resource| matches!(resource, Resource::Gpu(gpu) if gpu.server() == "Thalys"))
        );
    }

    #[test]
    fn a_server_and_a_room_can_be_checked_together() {
        let resources = resources_to_check(&config(), Some("Freccia"), Some("会議室A")).unwrap();

        assert_eq!(resources.len(), 2, "{:?}", names(&resources));
    }

    #[test]
    fn an_unknown_server_or_room_is_rejected_rather_than_reported_as_free() {
        assert!(resources_to_check(&config(), Some("Unknown"), None).is_err());
        assert!(resources_to_check(&config(), None, Some("どこか")).is_err());
    }

    #[test]
    fn availability_shows_who_holds_the_resource_and_when_it_is_free() {
        let start = Utc.with_ymd_and_hms(2026, 11, 12, 0, 0, 0).unwrap();
        let window = TimePeriod::new(start, start + Duration::hours(6)).unwrap();
        let gpu = Resource::Gpu(Gpu::new("Thalys".to_string(), 0, "A100".to_string()));
        let slot = ReservedSlotView::of_new_one_time_reservation(
            EmailAddress::new("holder@example.com".to_string()).unwrap(),
            TimePeriod::new(start + Duration::hours(1), start + Duration::hours(2)).unwrap(),
            vec![gpu.clone()],
            None,
        )
        .unwrap();

        let calculated = availability::calculate(&[gpu], &window, std::slice::from_ref(&slot));
        let dto = ResourceAvailabilityDto::from(&calculated[0]);

        assert_eq!(dto.server.as_deref(), Some("Thalys"));
        assert_eq!(dto.device_number, Some(0));
        assert_eq!(dto.room, None);
        assert!(!dto.entirely_free);
        assert_eq!(
            dto.busy_periods,
            vec![BusyPeriodDto {
                start: start + Duration::hours(1),
                end: start + Duration::hours(2),
                reserved_slot_id: slot.id().as_str().to_string(),
                holder_email: "holder@example.com".to_string(),
            }]
        );
        assert_eq!(
            dto.free_periods,
            vec![
                PeriodDto {
                    start,
                    end: start + Duration::hours(1)
                },
                PeriodDto {
                    start: start + Duration::hours(2),
                    end: start + Duration::hours(6)
                },
            ]
        );
    }
}
