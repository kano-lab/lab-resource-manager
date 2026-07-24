use crate::domain::aggregates::identity_link::value_objects::ExternalIdentity;
use crate::domain::aggregates::resource_usage::value_objects::Resource;
use crate::domain::errors::DomainError;
use crate::domain::ports::error::PortError;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::fmt;

/// 実サーバー上で観測されたリソース利用の事実
///
/// 予約（`ResourceUsage`）とは異なり、実際に利用が観測された結果を表す。
/// `active_since`は観測アダプタ側が把握する利用開始時刻であり、
/// ポーリング間隔に依存せず継続時間を判定できるようにする。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObservedUsage {
    resource: Resource,
    external_identity: ExternalIdentity,
    active_since: DateTime<Utc>,
}

impl ObservedUsage {
    /// 新しい観測結果を作成
    pub fn new(
        resource: Resource,
        external_identity: ExternalIdentity,
        active_since: DateTime<Utc>,
    ) -> Self {
        Self {
            resource,
            external_identity,
            active_since,
        }
    }

    /// 観測対象のリソースを取得
    pub fn resource(&self) -> &Resource {
        &self.resource
    }

    /// 利用者の外部識別情報を取得
    pub fn external_identity(&self) -> &ExternalIdentity {
        &self.external_identity
    }

    /// 利用開始時刻を取得
    pub fn active_since(&self) -> DateTime<Utc> {
        self.active_since
    }
}

/// 実サーバーの利用状況を観測するポート
///
/// 監視手段（SSHポーリング、Prometheus/DCGM Exporter等）はInfrastructure層の実装に隠蔽される。
#[async_trait]
pub trait ResourceUsageObserver: Send + Sync {
    /// 現在アクティブな利用状況を取得
    async fn observe_active_usages(&self) -> Result<Vec<ObservedUsage>, ObservationError>;
}

/// 観測エラー
#[derive(Debug)]
pub enum ObservationError {
    /// 監視対象への接続失敗
    ConnectionFailure(String),
    /// 不明なエラー
    Unknown(String),
}

impl fmt::Display for ObservationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ObservationError::ConnectionFailure(msg) => write!(f, "監視対象への接続失敗: {}", msg),
            ObservationError::Unknown(msg) => write!(f, "不明な観測エラー: {}", msg),
        }
    }
}

impl std::error::Error for ObservationError {}
impl DomainError for ObservationError {}
impl PortError for ObservationError {}
