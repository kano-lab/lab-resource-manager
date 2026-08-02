use crate::domain::aggregates::identity_link::value_objects::ExternalIdentity;
use crate::domain::aggregates::resource_usage::value_objects::{Gpu, Resource};
use crate::domain::errors::DomainError;
use crate::domain::ports::error::PortError;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
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
    used_memory_mib: Option<u64>,
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
            used_memory_mib: None,
        }
    }

    /// この利用が確保しているメモリ量を添える
    ///
    /// 報告できる観測手段とそうでない観測手段があるため、別に足せるようにしている。
    pub fn with_used_memory(self, used_memory_mib: u64) -> Self {
        Self {
            used_memory_mib: Some(used_memory_mib),
            ..self
        }
    }

    /// この利用が確保しているメモリ量（MiB）
    ///
    /// `None`は「確保していない」ではなく「どれだけ確保しているかを問えない」を意味する。
    pub fn used_memory_mib(&self) -> Option<u64> {
        self.used_memory_mib
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

/// GPU1台が観測の窓のあいだにどれだけ計算していたか
///
/// プロセスが乗っていることと、計算が走っていることは別である。メモリだけを確保して
/// 待機する使い方（常駐させた推論サーバー、開いたままのノートブック、止まったジョブ）では、
/// プロセスの有無からは計算しているかどうかが分からない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuActivity {
    peak_utilization_percent: u32,
}

impl GpuActivity {
    /// 新しい観測結果を作成
    ///
    /// # Arguments
    /// * `peak_utilization_percent` - 観測の窓のあいだに見た最も高い稼働率
    pub fn new(peak_utilization_percent: u32) -> Self {
        Self {
            peak_utilization_percent,
        }
    }

    /// 観測の窓のあいだに見た最も高い稼働率
    ///
    /// 平均ではなく最大を持つ。稼働が途切れがちな使い方（対話的な作業）を
    /// 平均で均すと、実際には使われている予約を使われていないと読んでしまう。
    pub fn peak_utilization_percent(&self) -> u32 {
        self.peak_utilization_percent
    }
}

/// 1サーバーの利用状況をいま把握できているか
///
/// 把握できていないときは、その理由まで持つ。理由は運用者が手を打つ手がかりになる
/// （届いていない＝収集そのものが止まっている、古い＝収集が滞っている）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerObservation {
    /// 把握できている
    Observed {
        /// もとになったレポートが作られた時刻
        generated_at: DateTime<Utc>,
    },
    /// レポートが届いていない
    Missing,
    /// レポートが古く、いまの利用状況としては使えない
    Stale {
        /// 使えないと判断したレポートが作られた時刻
        generated_at: DateTime<Utc>,
    },
    /// レポートを読めない、または解釈できない
    Unreadable,
}

impl ServerObservation {
    /// このサーバーの利用状況を今このとき把握できているか
    pub fn is_current(&self) -> bool {
        matches!(self, Self::Observed { .. })
    }
}

/// 一度の観測で分かったことのまとまり
///
/// 「利用が観測されなかった」ことと「そのサーバーを観測できなかった」ことは異なる。
/// 監視が止まっているサーバーの沈黙を「誰も使っていない」と読むと、実際には
/// 使われているリソースを空きとみなしてしまう。どのサーバーについて語れるのかを
/// 利用の一覧と一緒に返し、読み手が両者を区別できるようにする。
#[derive(Debug, Clone, Default)]
pub struct ObservationSnapshot {
    usages: Vec<ObservedUsage>,
    servers: HashMap<String, ServerObservation>,
    gpu_activities: HashMap<(String, u32), GpuActivity>,
}

impl ObservationSnapshot {
    /// 新しい観測結果を作成
    ///
    /// # Arguments
    /// * `usages` - 観測された利用の一覧
    /// * `servers` - サーバーごとの観測状態
    pub fn new(usages: Vec<ObservedUsage>, servers: HashMap<String, ServerObservation>) -> Self {
        Self {
            usages,
            servers,
            gpu_activities: HashMap::new(),
        }
    }

    /// GPUごとの稼働状況を添えた観測結果を返す
    ///
    /// 稼働状況を報告できる観測手段とそうでない観測手段があるため、利用の一覧とは
    /// 別に足せるようにしている。添えられていないGPUについては、計算しているか
    /// どうかを問えない。
    pub fn with_gpu_activities(self, gpu_activities: HashMap<(String, u32), GpuActivity>) -> Self {
        Self {
            gpu_activities,
            ..self
        }
    }

    /// 観測された利用の一覧を取得
    pub fn usages(&self) -> &[ObservedUsage] {
        &self.usages
    }

    /// このGPUが観測の窓のあいだにどれだけ計算していたか
    ///
    /// `None`は「計算していない」ではなく「計算していたかを問えない」を意味する。
    /// 稼働状況を報告しない観測手段や、稼働率を読み出せないGPUがある。
    pub fn gpu_activity_of(&self, gpu: &Gpu) -> Option<&GpuActivity> {
        self.gpu_activities
            .get(&(gpu.server().to_string(), gpu.device_number()))
    }

    /// そのサーバーの利用状況を今このとき把握できているか
    ///
    /// `false`のサーバーについては、利用の一覧に現れないことが
    /// 「使われていない」ことを意味しない。
    pub fn covers(&self, server: &str) -> bool {
        self.servers
            .get(server)
            .is_some_and(ServerObservation::is_current)
    }

    /// サーバーごとの観測状態をサーバー名順に取得
    ///
    /// 監視が動いているかを人に見せるための入り口。並び順を決めておくことで、
    /// 表示のたびにサーバーが入れ替わって読みにくくなるのを防ぐ。
    pub fn servers(&self) -> Vec<(&str, &ServerObservation)> {
        let mut servers: Vec<(&str, &ServerObservation)> = self
            .servers
            .iter()
            .map(|(name, observation)| (name.as_str(), observation))
            .collect();
        servers.sort_by_key(|(name, _)| *name);
        servers
    }

    /// 利用の有無を把握できたサーバーの数
    pub fn observed_server_count(&self) -> usize {
        self.servers
            .values()
            .filter(|observation| observation.is_current())
            .count()
    }
}

/// 実サーバーの利用状況を観測するポート
///
/// 監視手段（SSHポーリング、Prometheus/DCGM Exporter等）はInfrastructure層の実装に隠蔽される。
#[async_trait]
pub trait ResourceUsageObserver: Send + Sync {
    /// 現在アクティブな利用状況を取得
    async fn observe_active_usages(&self) -> Result<ObservationSnapshot, ObservationError>;
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
