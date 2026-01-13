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
    GoogleCalendarUsageRepository, JsonFileIdentityLinkRepository, NotificationRouter,
    NotifyFutureResourceUsageChangesUseCase, load_config,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting resource usage watcher (basic example)");

    // Load resource configuration
    let config_path =
        std::env::var("RESOURCE_CONFIG").unwrap_or_else(|_| "config/resources.toml".to_string());

    let config = load_config(&config_path)?;
    println!("✅ Configuration loaded");

    // Create repository
    let service_account_key = std::env::var("GOOGLE_SERVICE_ACCOUNT_KEY")
        .expect("GOOGLE_SERVICE_ACCOUNT_KEY must be set");

    let id_mappings_path = std::env::var("GOOGLE_CALENDAR_MAPPINGS_FILE")
        .unwrap_or_else(|_| "data/google_calendar_mappings.json".to_string());

    let repository = Arc::new(
        GoogleCalendarUsageRepository::new(
            &service_account_key,
            config.clone(),
            PathBuf::from(&id_mappings_path),
        )
        .await?,
    );
    println!("✅ Google Calendar repository initialized");

    // Create identity link repository
    let identity_links_path = std::env::var("IDENTITY_LINKS_FILE")
        .unwrap_or_else(|_| "data/identity_links.json".to_string());
    let identity_repo = Arc::new(JsonFileIdentityLinkRepository::new(PathBuf::from(
        identity_links_path,
    )));

    // Create notification router (uses configured notification destinations and identity_repo)
    let notifier = NotificationRouter::new(config, identity_repo);
    println!("✅ Notification router initialized (using configured destinations)");

    // Create use case
    let usecase = NotifyFutureResourceUsageChangesUseCase::new(repository, notifier).await?;

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
