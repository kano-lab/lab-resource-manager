//! Mock usage example
//!
//! This example demonstrates using mock implementations for testing
//! without requiring actual Google Calendar or Slack credentials.
//!
//! Run with:
//! ```bash
//! cargo run --example mock_usage
//! ```

use lab_resource_manager::{MockNotifier, MockUsageRepository, NotifyResourceUsageChangesUseCase};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting resource usage watcher (mock example)");
    println!("This example uses mock implementations - no credentials required!\n");

    // Create mock repository and notifier
    let repository = MockUsageRepository::new();
    let notifier = MockNotifier::new();

    println!("✅ Mock repository and notifier initialized");

    // Create use case
    let usecase = NotifyResourceUsageChangesUseCase::new(repository, notifier);

    // Poll once to demonstrate
    println!("📊 Polling for changes...\n");
    usecase.poll_once().await?;

    println!("\n✅ Example completed successfully!");
    println!("💡 Note: Mock repository returns empty results by default.");
    println!("   To see actual notifications, add items to MockUsageRepository.");

    Ok(())
}
