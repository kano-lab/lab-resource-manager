use crate::application::error::ApplicationError;
use crate::domain::ports::{ResourceUsageObserver, ServerObservation};
use chrono::Duration;
use std::sync::Arc;

/// 実利用の監視の動き方
///
/// どれも運用者が設定した値であり、状態を読むときの尺度になる
/// （「42分前のレポート」が古いかどうかは`max_staleness`次第）。
#[derive(Debug, Clone, Copy)]
pub struct MonitoringSettings {
    /// 実利用と予約を突き合わせる間隔
    pub polling_interval: Duration,
    /// 予約者のプロセスを観測できない予約を予約者へ知らせるまでの時間
    pub idle_threshold: Duration,
    /// GPUを押さえたまま計算が走らない予約を予約者へ知らせるまでの時間
    pub held_gpu_threshold: Duration,
    /// レポートを鮮度切れとみなす経過時間
    pub max_staleness: Duration,
}

/// 実利用の監視がいまどう動いているか
#[derive(Debug, Clone)]
pub enum MonitoringStatus {
    /// 監視そのものが無効
    ///
    /// レポートの置き場が設定されていない場合、実利用は一切観測されない。
    /// 未予約利用の提案も未使用予約のお知らせも起こらない。
    Disabled,
    /// 監視は動いている
    Enabled {
        /// サーバーごとの観測状態（サーバー名順）
        servers: Vec<(String, ServerObservation)>,
        /// 監視の動き方
        settings: MonitoringSettings,
    },
}

/// 実利用の監視の稼働状況を答えるユースケース
///
/// 監視が止まっていても、予約と実利用の突合が黙るだけで誰にも伝わらない。
/// 運用者が「いま監視は効いているか」を確かめられるようにする。
pub struct DescribeMonitoringUseCase {
    observer: Option<Arc<dyn ResourceUsageObserver>>,
    settings: MonitoringSettings,
}

impl DescribeMonitoringUseCase {
    /// 新しいユースケースインスタンスを作成
    ///
    /// # Arguments
    /// * `observer` - 実利用の観測（監視が無効なら`None`）
    /// * `settings` - 監視の動き方
    pub fn new(
        observer: Option<Arc<dyn ResourceUsageObserver>>,
        settings: MonitoringSettings,
    ) -> Self {
        Self { observer, settings }
    }

    /// いまの稼働状況を取得
    ///
    /// 状態は保存された記録ではなく、この場で観測し直した結果を返す。
    /// 「さっきまで動いていた」ではなく「いま動いているか」が知りたいことだからである。
    ///
    /// # Errors
    /// 観測に失敗した場合
    pub async fn execute(&self) -> Result<MonitoringStatus, ApplicationError> {
        let Some(observer) = &self.observer else {
            return Ok(MonitoringStatus::Disabled);
        };

        let snapshot = observer.observe_active_usages().await?;
        let servers = snapshot
            .servers()
            .into_iter()
            .map(|(name, observation)| (name.to_string(), observation.clone()))
            .collect();

        Ok(MonitoringStatus::Enabled {
            servers,
            settings: self.settings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::resource_usage_observer::MockResourceUsageObserver;

    fn settings() -> MonitoringSettings {
        MonitoringSettings {
            polling_interval: Duration::seconds(60),
            idle_threshold: Duration::minutes(30),
            held_gpu_threshold: Duration::hours(1),
            max_staleness: Duration::minutes(5),
        }
    }

    #[tokio::test]
    async fn monitoring_that_was_never_set_up_reports_itself_as_disabled() {
        let usecase = DescribeMonitoringUseCase::new(None, settings());

        let status = usecase.execute().await.unwrap();

        assert!(
            matches!(status, MonitoringStatus::Disabled),
            "観測が無効なことは、サーバーが1台もないこととは違う"
        );
    }

    #[tokio::test]
    async fn a_running_monitor_reports_each_server() {
        let observer = Arc::new(MockResourceUsageObserver::new());
        observer.set_snapshot(crate::domain::ports::ObservationSnapshot::new(
            Vec::new(),
            std::collections::HashMap::from([
                ("Thalys".to_string(), ServerObservation::Missing),
                (
                    "Freccia".to_string(),
                    ServerObservation::Observed {
                        generated_at: chrono::Utc::now(),
                    },
                ),
            ]),
        ));
        let usecase = DescribeMonitoringUseCase::new(Some(observer), settings());

        let status = usecase.execute().await.unwrap();

        let MonitoringStatus::Enabled { servers, .. } = status else {
            panic!("監視は動いているはず: {status:?}");
        };
        assert_eq!(
            servers
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            vec!["Freccia", "Thalys"],
            "読み手が探しやすいよう並び順を決めておく"
        );
    }
}
