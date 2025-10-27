//! Basic usage example of lab-resource-manager
//!
//! This example demonstrates how to use the library to monitor
//! Google Calendar events and send Slack notifications.
//!
//! Run with:
//! ```bash
//! cargo run --example basic_usage
//! ```

use lab_resource_manager::{
    GoogleCalendarUsageRepository, NotificationRouter, NotifyResourceUsageChangesUseCase,
    load_config,
};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting resource usage watcher (basic example)");

    // Load resource configuration
    let dotenv_path = dotenv::dotenv().ok();
    let project_root = dotenv_path
        .as_ref()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

    let config_path =
        std::env::var("CONFIG_PATH").unwrap_or_else(|_| "config/resources.toml".to_string());
    let absolute_config_path = project_root.join(&config_path);

    let config = load_config(
        absolute_config_path
            .to_str()
            .expect("Failed to convert path"),
    )?;
    println!("✅ Configuration loaded");

    // Create repository
    let service_account_key = std::env::var("GOOGLE_SERVICE_ACCOUNT_KEY")
        .expect("GOOGLE_SERVICE_ACCOUNT_KEY must be set");
    let absolute_key_path = project_root.join(&service_account_key);

    let repository = GoogleCalendarUsageRepository::new(
        absolute_key_path.to_str().expect("Failed to convert path"),
        config.clone(),
    )
    .await?;
    println!("✅ Google Calendar repository initialized");

    // Create notification router (uses configured notification destinations)
    let notifier = NotificationRouter::new(config);
    println!("✅ Notification router initialized (using configured destinations)");

    // Create use case
    let usecase = NotifyResourceUsageChangesUseCase::new(repository, notifier);

    // Run polling loop
    let interval = Duration::from_secs(60);
    println!("🔍 Starting monitoring loop (interval: {:?})", interval);
    println!("Press Ctrl+C to stop\n");

    loop {
        match usecase.poll_once().await {
            Ok(_) => {
                println!("✅ Poll completed successfully");
            }
            Err(e) => {
                eprintln!("❌ Polling error: {}", e);
            }
        }

        tokio::time::sleep(interval).await;
    }
}
