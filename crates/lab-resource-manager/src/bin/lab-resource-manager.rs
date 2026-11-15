//! lab-resource-manager の常駐プロセス
//!
//! GPUと部屋の予約を監視して通知し、SlackボットとMCPサーバーから予約を受け付ける。
//! 依存の組み立てと各処理の起動は`bootstrap`が受け持つ。

use lab_resource_manager::bootstrap;
use lab_resource_manager::infrastructure::config::{load_config, load_from_env};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // tracingの初期化（RUST_LOGで制御、未設定時はinfo）
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // rustls暗号化プロバイダの初期化
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    let app_config = load_from_env()?;
    let (resource_config, storage_config) = load_config(&app_config.resource_config_path)?;

    bootstrap::run(app_config, resource_config, storage_config)
        .await
        .map_err(|e| -> Box<dyn std::error::Error> { e })
}
