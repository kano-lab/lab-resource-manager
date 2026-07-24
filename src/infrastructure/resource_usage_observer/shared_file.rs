use crate::domain::aggregates::identity_link::value_objects::{ExternalIdentity, ExternalSystem};
use crate::domain::aggregates::resource_usage::value_objects::{Gpu, Resource};
use crate::domain::ports::resource_usage_observer::{
    ObservationError, ObservedUsage, ResourceUsageObserver,
};
use crate::infrastructure::config::ResourceConfig;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
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

    async fn read_server_report(&self, server_name: &str) -> Vec<ObservedUsage> {
        let path = self.report_path(server_name);

        let content = match tokio::fs::read_to_string(&path).await {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
            Err(e) => {
                warn!("観測ファイル読み込み失敗 ({}): {}", path.display(), e);
                return Vec::new();
            }
        };

        let report: GpuUsageReport = match serde_json::from_str(&content) {
            Ok(report) => report,
            Err(e) => {
                warn!("観測ファイルのパース失敗 ({}): {}", path.display(), e);
                return Vec::new();
            }
        };

        if Utc::now() - report.generated_at > self.max_staleness {
            warn!(
                "観測ファイルが古いため無視します ({}): generated_at={}",
                path.display(),
                report.generated_at
            );
            return Vec::new();
        }

        self.build_observed_usages(&report)
    }

    fn build_observed_usages(&self, report: &GpuUsageReport) -> Vec<ObservedUsage> {
        let Some(server_config) = self.resource_config.get_server(&report.server) else {
            warn!(
                "未設定のサーバーからの観測データを無視します: {}",
                report.server
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

                Some(ObservedUsage::new(resource, identity, process.started_at))
            })
            .collect()
    }
}

#[async_trait]
impl ResourceUsageObserver for SharedFileResourceUsageObserver {
    async fn observe_active_usages(&self) -> Result<Vec<ObservedUsage>, ObservationError> {
        let mut observed = Vec::new();
        for server in &self.resource_config.servers {
            observed.extend(self.read_server_report(&server.name).await);
        }
        Ok(observed)
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
    async fn test_missing_file_returns_empty() {
        let dir = temp_dir();
        let observer =
            SharedFileResourceUsageObserver::new(dir, test_resource_config(), Duration::minutes(5));

        let observed = observer.observe_active_usages().await.unwrap();
        assert!(observed.is_empty());
    }

    #[tokio::test]
    async fn test_valid_report_is_parsed() {
        let dir = temp_dir();
        let generated_at = Utc::now();
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
        let observed = observer.observe_active_usages().await.unwrap();

        assert_eq!(observed.len(), 1);
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
    async fn test_stale_report_is_ignored() {
        let dir = temp_dir();
        let generated_at = Utc::now() - Duration::minutes(30);
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
        let observed = observer.observe_active_usages().await.unwrap();

        assert!(observed.is_empty());

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
        let observed = observer.observe_active_usages().await.unwrap();

        assert!(observed.is_empty());

        tokio::fs::remove_dir_all(&dir).await.ok();
    }
}
