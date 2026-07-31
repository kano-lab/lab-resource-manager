//! MCPツールの入出力DTO
//!
//! ドメインエンティティはserde非依存のため、JSON出力専用のDTOをここに定義する。

use crate::domain::aggregates::resource_usage::entity::ResourceUsage;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 予約情報の出力用DTO
#[derive(Debug, Serialize)]
pub struct ReservationDto {
    pub id: String,
    pub owner_email: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub resources: Vec<String>,
    pub notes: Option<String>,
}

impl From<&ResourceUsage> for ReservationDto {
    fn from(usage: &ResourceUsage) -> Self {
        Self {
            id: usage.id().as_str().to_string(),
            owner_email: usage.owner_email().as_str().to_string(),
            start: usage.time_period().start(),
            end: usage.time_period().end(),
            resources: usage.resources().iter().map(|r| r.to_string()).collect(),
            notes: usage.notes().cloned(),
        }
    }
}

/// `get_reservation`の引数
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetReservationParams {
    /// 予約ID
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

/// `update_reservation`の引数
#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateReservationParams {
    /// 予約ID
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

/// `cancel_reservation`の引数
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CancelReservationParams {
    /// 予約ID
    pub id: String,
}

/// `release_reservation_early`の引数
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReleaseReservationEarlyParams {
    /// 予約ID
    pub id: String,
}
