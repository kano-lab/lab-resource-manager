//! Mock usage example
//!
//! This example demonstrates using mock implementations for testing
//! without requiring actual Google Calendar or Slack credentials.
//!
//! Run with:
//! ```bash
//! cargo run --example mock_usage
//! ```

use lab_resource_manager::{
    MockUsageRepository, NotificationRouter, NotifyResourceUsageChangesUseCase, load_config,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting resource usage watcher (mock example)");
    println!("This example uses mock implementations - no credentials required!\n");

    // Load configuration
    let config = load_config("config/resources.toml")?;

    // Create mock repository and notification router
    let repository = MockUsageRepository::new();
    let notifier = NotificationRouter::new(config);

    println!("✅ Mock repository and notification router initialized");

    // Create use case
    let usecase = NotifyResourceUsageChangesUseCase::new(repository, notifier).await?;

    // Poll once to demonstrate
    println!("📊 Polling for changes...\n");
    usecase.poll_once().await?;

    println!("\n✅ Example completed successfully!");
    println!("💡 Note: Mock repository returns empty results by default.");
    println!(
        "   To see actual notifications, configure mock notifications in config/resources.toml"
    );

    Ok(())
}
