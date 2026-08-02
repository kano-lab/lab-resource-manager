use crate::domain::aggregates::identity_link::value_objects::{ExternalIdentity, ExternalSystem};
use crate::domain::aggregates::resource_usage::value_objects::{Gpu, Resource};
use crate::domain::ports::resource_usage_observer::{
    GpuActivity, ObservationError, ObservationSnapshot, ObservedUsage, ResourceUsageObserver,
    ServerObservation,
};
use crate::infrastructure::config::ResourceConfig;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::warn;

/// 共有ファイルシステム経由のGPU利用状況レポート（1サーバー分）
///
/// `gpu-usage-reporter`バイナリ（`src/bin/gpu-usage-reporter.rs`）がこの形式でJSONを書き出し、
/// `SharedFileResourceUsageObserver`が読み取る。両者で同じ型を共有することで、
/// スキーマ変更時のズレをコンパイラに検知させる。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuUsageReport {
    /// `config/resources.toml`の`servers[].name`と一致するサーバー名
    pub server: String,
    /// このレポートを生成した時刻（鮮度判定に使用）
    pub generated_at: DateTime<Utc>,
    /// 観測されたGPU利用プロセスの一覧
    pub processes: Vec<GpuUsageProcessEntry>,
    /// デバイスごとの稼働状況
    ///
    /// 稼働率を読み出せない環境（古い`gpu-usage-reporter`、稼働率を報告しないGPU）では
    /// 空になる。空であることは「計算していない」ではなく「計算していたかを問えない」を意味する。
    #[serde(default)]
    pub devices: Vec<GpuUsageDeviceEntry>,
}

/// 1つの(デバイス, 利用者)に集約された利用エントリ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuUsageProcessEntry {
    /// デバイス番号（`config/resources.toml`の`devices[].id`と対応）
    pub device_number: u32,
    /// 利用者のOSユーザー名
    pub os_user: String,
    /// このデバイス・利用者の組み合わせで最も古いプロセス起動時刻
    pub started_at: DateTime<Utc>,
    /// このデバイス・利用者の組み合わせが確保しているメモリ量の合計（MiB）
    ///
    /// 読み出せない環境では欠ける。欠けていることは「確保していない」ではなく
    /// 「どれだけ確保しているかを問えない」を意味する。
    #[serde(default)]
    pub used_memory_mib: Option<u64>,
}

/// 1デバイスが観測の窓のあいだにどれだけ計算していたか
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuUsageDeviceEntry {
    /// デバイス番号（`config/resources.toml`の`devices[].id`と対応）
    pub device_number: u32,
    /// 観測の窓のあいだに見た最も高い稼働率（%）
    pub peak_utilization_percent: u32,
}

/// 共有ファイルシステム上に各GPUサーバーがcronで書き出すJSONレポートを読み取る観測実装
///
/// サーバーごとにcron実行される`gpu-usage-reporter`バイナリが
/// `{directory}/{server_nameを小文字化}.json` を定期的にアトミックに書き出す前提。
/// レポートが`max_staleness`より古い場合は、cron停止等による古いデータとみなし無視する。
///
/// 個別ファイルの欠落・パース失敗・鮮度切れはエラーにせず、警告ログを出して
/// そのサーバー分のみスキップする（監視はベストエフォートであり、一部サーバーの
/// 不調で全体の突合処理を止めるべきではないため）。
pub struct SharedFileResourceUsageObserver {
    directory: PathBuf,
    resource_config: Arc<ResourceConfig>,
    max_staleness: Duration,
}

impl SharedFileResourceUsageObserver {
    /// 新しい観測実装を作成
    ///
    /// # Arguments
    /// * `directory` - 各サーバーのレポートJSONが置かれる共有ディレクトリ
    /// * `resource_config` - GPUモデル名解決に使うリソース設定
    /// * `max_staleness` - このレポートを無視し始める経過時間（cron停止の検知用）
    pub fn new(
        directory: PathBuf,
        resource_config: Arc<ResourceConfig>,
        max_staleness: Duration,
    ) -> Self {
        Self {
            directory,
            resource_config,
            max_staleness,
        }
    }

    fn report_path(&self, server_name: &str) -> PathBuf {
        self.directory
            .join(format!("{}.json", server_name.to_lowercase()))
    }

    /// 1サーバー分のレポートを読む
    ///
    /// 利用状況を把握できなかった場合、その理由まで返す。読めなかったことと
    /// 「使われていない」ことを呼び出し側が取り違えないようにするため、
    /// 空の一覧では表さない。
    async fn read_server_report(&self, server_name: &str) -> ServerReading {
        let path = self.report_path(server_name);

        let content = match tokio::fs::read_to_string(&path).await {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return ServerReading::unavailable(ServerObservation::Missing);
            }
            Err(e) => {
                warn!(path = %path.display(), error = %e, "reading a usage report failed");
                return ServerReading::unavailable(ServerObservation::Unreadable);
            }
        };

        let report: GpuUsageReport = match serde_json::from_str(&content) {
            Ok(report) => report,
            Err(e) => {
                warn!(path = %path.display(), error = %e, "parsing a usage report failed");
                return ServerReading::unavailable(ServerObservation::Unreadable);
            }
        };

        if Utc::now() - report.generated_at > self.max_staleness {
            warn!(
                path = %path.display(),
                generated_at = %report.generated_at,
                max_staleness_secs = self.max_staleness.num_seconds(),
                "the usage report is too old; ignoring it"
            );
            return ServerReading::unavailable(ServerObservation::Stale {
                generated_at: report.generated_at,
            });
        }

        ServerReading {
            observation: ServerObservation::Observed {
                generated_at: report.generated_at,
            },
            usages: self.build_observed_usages(&report),
            gpu_activities: self.build_gpu_activities(&report),
        }
    }

    /// レポートのデバイス欄を、設定に載っているデバイスの稼働状況として読む
    fn build_gpu_activities(&self, report: &GpuUsageReport) -> Vec<((String, u32), GpuActivity)> {
        let Some(server_config) = self.resource_config.get_server(&report.server) else {
            return Vec::new();
        };

        report
            .devices
            .iter()
            .filter(|device| {
                server_config
                    .devices
                    .iter()
                    .any(|configured| configured.id == device.device_number)
            })
            .map(|device| {
                (
                    (report.server.clone(), device.device_number),
                    GpuActivity::new(device.peak_utilization_percent),
                )
            })
            .collect()
    }

    fn build_observed_usages(&self, report: &GpuUsageReport) -> Vec<ObservedUsage> {
        let Some(server_config) = self.resource_config.get_server(&report.server) else {
            warn!(
                server = %report.server,
                "the usage report is for a server that is not configured; ignoring it"
            );
            return Vec::new();
        };

        report
            .processes
            .iter()
            .filter_map(|process| {
                let model = server_config
                    .devices
                    .iter()
                    .find(|d| d.id == process.device_number)?
                    .model
                    .clone();

                let resource = Resource::Gpu(Gpu::new(
                    report.server.clone(),
                    process.device_number,
                    model,
                ));
                let identity = ExternalIdentity::new(
                    ExternalSystem::Os {
                        server: report.server.clone(),
                    },
                    process.os_user.clone(),
                );

                let observed = ObservedUsage::new(resource, identity, process.started_at);
                Some(match process.used_memory_mib {
                    Some(used_memory_mib) => observed.with_used_memory(used_memory_mib),
                    None => observed,
                })
            })
            .collect()
    }
}

/// 1サーバー分のレポートから読み取れたこと
struct ServerReading {
    observation: ServerObservation,
    usages: Vec<ObservedUsage>,
    gpu_activities: Vec<((String, u32), GpuActivity)>,
}

impl ServerReading {
    /// 利用状況を把握できなかったときの読み取り結果
    fn unavailable(observation: ServerObservation) -> Self {
        Self {
            observation,
            usages: Vec::new(),
            gpu_activities: Vec::new(),
        }
    }
}

#[async_trait]
impl ResourceUsageObserver for SharedFileResourceUsageObserver {
    async fn observe_active_usages(&self) -> Result<ObservationSnapshot, ObservationError> {
        let mut observed = Vec::new();
        let mut servers = HashMap::new();
        let mut gpu_activities = HashMap::new();

        for server in &self.resource_config.servers {
            let reading = self.read_server_report(&server.name).await;
            observed.extend(reading.usages);
            gpu_activities.extend(reading.gpu_activities);
            servers.insert(server.name.clone(), reading.observation);
        }

        Ok(ObservationSnapshot::new(observed, servers).with_gpu_activities(gpu_activities))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::config::{DeviceConfig, ServerConfig};

    fn test_resource_config() -> Arc<ResourceConfig> {
        Arc::new(ResourceConfig {
            servers: vec![ServerConfig {
                name: "Thalys".to_string(),
                calendar_id: "dummy-calendar-id".to_string(),
                devices: vec![DeviceConfig {
                    id: 0,
                    model: "A100".to_string(),
                }],
                notifications: vec![],
            }],
            rooms: vec![],
        })
    }

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!("lrm_test_{}", uuid::Uuid::new_v4()))
    }

    async fn write_report(dir: &std::path::Path, filename: &str, content: &str) {
        tokio::fs::create_dir_all(dir).await.unwrap();
        tokio::fs::write(dir.join(filename), content).await.unwrap();
    }

    #[tokio::test]
    async fn a_server_without_a_report_is_not_covered() {
        let dir = temp_dir();
        let observer =
            SharedFileResourceUsageObserver::new(dir, test_resource_config(), Duration::minutes(5));

        let snapshot = observer.observe_active_usages().await.unwrap();

        assert!(snapshot.usages().is_empty());
        assert!(
            !snapshot.covers("Thalys"),
            "レポートがないサーバーの利用状況は分からない"
        );
        assert_eq!(
            snapshot.servers(),
            vec![("Thalys", &ServerObservation::Missing)],
            "届いていないことが理由として分かる"
        );
    }

    #[tokio::test]
    async fn test_valid_report_is_parsed() {
        let dir = temp_dir();
        let report_generated_at = Utc::now();
        let generated_at = report_generated_at;
        let started_at = generated_at - Duration::minutes(20);
        write_report(
            &dir,
            "thalys.json",
            &format!(
                r#"{{"server": "Thalys", "generated_at": "{}", "processes": [
                    {{"device_number": 0, "os_user": "kkawaguchi", "started_at": "{}"}}
                ]}}"#,
                generated_at.to_rfc3339(),
                started_at.to_rfc3339(),
            ),
        )
        .await;

        let observer = SharedFileResourceUsageObserver::new(
            dir.clone(),
            test_resource_config(),
            Duration::minutes(5),
        );
        let snapshot = observer.observe_active_usages().await.unwrap();
        let observed = snapshot.usages();

        assert_eq!(observed.len(), 1);
        assert!(snapshot.covers("Thalys"));
        assert_eq!(
            snapshot.servers(),
            vec![(
                "Thalys",
                &ServerObservation::Observed {
                    generated_at: report_generated_at
                }
            )],
            "いつ時点の状況なのかが分かる"
        );
        assert_eq!(observed[0].external_identity().user_id(), "kkawaguchi");
        assert_eq!(observed[0].active_since(), started_at);
        match observed[0].resource() {
            Resource::Gpu(gpu) => {
                assert_eq!(gpu.server(), "Thalys");
                assert_eq!(gpu.device_number(), 0);
                assert_eq!(gpu.model(), "A100");
            }
            other => panic!("unexpected resource: {:?}", other),
        }

        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn a_stale_report_leaves_the_server_uncovered() {
        let dir = temp_dir();
        let stale_generated_at = Utc::now() - Duration::minutes(30);
        let generated_at = stale_generated_at;
        let started_at = generated_at - Duration::minutes(20);
        write_report(
            &dir,
            "thalys.json",
            &format!(
                r#"{{"server": "Thalys", "generated_at": "{}", "processes": [
                    {{"device_number": 0, "os_user": "kkawaguchi", "started_at": "{}"}}
                ]}}"#,
                generated_at.to_rfc3339(),
                started_at.to_rfc3339(),
            ),
        )
        .await;

        let observer = SharedFileResourceUsageObserver::new(
            dir.clone(),
            test_resource_config(),
            Duration::minutes(5),
        );
        let snapshot = observer.observe_active_usages().await.unwrap();

        assert!(snapshot.usages().is_empty());
        assert!(
            !snapshot.covers("Thalys"),
            "監視が止まっている間の沈黙を「使われていない」と読ませない"
        );
        assert_eq!(
            snapshot.servers(),
            vec![(
                "Thalys",
                &ServerObservation::Stale {
                    generated_at: stale_generated_at
                }
            )],
            "届いてはいるが古い、と区別できる"
        );

        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn test_unconfigured_device_is_filtered_out() {
        let dir = temp_dir();
        let generated_at = Utc::now();
        write_report(
            &dir,
            "thalys.json",
            &format!(
                r#"{{"server": "Thalys", "generated_at": "{}", "processes": [
                    {{"device_number": 99, "os_user": "kkawaguchi", "started_at": "{}"}}
                ]}}"#,
                generated_at.to_rfc3339(),
                generated_at.to_rfc3339(),
            ),
        )
        .await;

        let observer = SharedFileResourceUsageObserver::new(
            dir.clone(),
            test_resource_config(),
            Duration::minutes(5),
        );
        let snapshot = observer.observe_active_usages().await.unwrap();

        assert!(snapshot.usages().is_empty());
        assert!(
            snapshot.covers("Thalys"),
            "レポートは読めているので、そのサーバーの利用状況は把握できている"
        );

        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn a_report_carries_how_hard_each_device_was_working() {
        let dir = temp_dir();
        let generated_at = Utc::now();
        write_report(
            &dir,
            "thalys.json",
            &format!(
                r#"{{"server": "Thalys", "generated_at": "{}", "processes": [], "devices": [
                    {{"device_number": 0, "peak_utilization_percent": 3}}
                ]}}"#,
                generated_at.to_rfc3339(),
            ),
        )
        .await;

        let observer = SharedFileResourceUsageObserver::new(
            dir.clone(),
            test_resource_config(),
            Duration::minutes(5),
        );
        let snapshot = observer.observe_active_usages().await.unwrap();

        let gpu = Gpu::new("Thalys".to_string(), 0, "A100".to_string());
        let activity = snapshot
            .gpu_activity_of(&gpu)
            .expect("レポートに載っているデバイスの稼働状況は読み取れる");
        assert_eq!(activity.peak_utilization_percent(), 3);

        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn a_report_without_device_activity_leaves_it_unknown() {
        let dir = temp_dir();
        let generated_at = Utc::now();
        // 稼働状況を報告しない`gpu-usage-reporter`が書いたレポート
        write_report(
            &dir,
            "thalys.json",
            &format!(
                r#"{{"server": "Thalys", "generated_at": "{}", "processes": [
                    {{"device_number": 0, "os_user": "kkawaguchi", "started_at": "{}"}}
                ]}}"#,
                generated_at.to_rfc3339(),
                generated_at.to_rfc3339(),
            ),
        )
        .await;

        let observer = SharedFileResourceUsageObserver::new(
            dir.clone(),
            test_resource_config(),
            Duration::minutes(5),
        );
        let snapshot = observer.observe_active_usages().await.unwrap();

        let gpu = Gpu::new("Thalys".to_string(), 0, "A100".to_string());
        assert!(
            snapshot.gpu_activity_of(&gpu).is_none(),
            "報告のないことを、計算していないことの証拠にしてはいけない"
        );
        assert_eq!(
            snapshot.usages().len(),
            1,
            "稼働状況が無くても、誰が乗っているかは読み取れる"
        );

        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn a_report_that_cannot_be_parsed_is_reported_as_unreadable() {
        let dir = temp_dir();
        write_report(&dir, "thalys.json", "{ this is not json").await;

        let observer = SharedFileResourceUsageObserver::new(
            dir.clone(),
            test_resource_config(),
            Duration::minutes(5),
        );
        let snapshot = observer.observe_active_usages().await.unwrap();

        assert!(!snapshot.covers("Thalys"));
        assert_eq!(
            snapshot.servers(),
            vec![("Thalys", &ServerObservation::Unreadable)],
            "壊れたレポートは、届いていないこととは別の手当てが要る"
        );

        tokio::fs::remove_dir_all(&dir).await.ok();
    }
}
