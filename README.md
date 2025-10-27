# lab-resource-manager

[![Crates.io](https://img.shields.io/crates/v/lab-resource-manager)](https://crates.io/crates/lab-resource-manager)
[![Documentation](https://docs.rs/lab-resource-manager/badge.svg)](https://docs.rs/lab-resource-manager)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)](README.md#license)

GPU and room resource management system with Google Calendar and Slack integration.

[日本語 README](README_ja.md)

## Features

- **Google Calendar Integration**: Manage GPU server and room reservations via calendar
- **Multi-Destination Notifications**: Configure different notification destinations per resource (Slack, Mock, etc.)
- **Flexible Device Specification**: Support for multi-device notation like `0-2,5,7-9`
- **Clean Architecture**: Designed with DDD + Hexagonal Architecture
- **Mock Implementations**: Built-in mock repository and notifier for testing

## Architecture

This project follows Clean Architecture principles:

```
src/
├── domain/           # Domain layer (business logic)
│   ├── aggregates/   # Aggregates (ResourceUsage)
│   ├── ports/        # Ports (Repository, Notifier traits)
│   └── errors.rs     # Domain errors
├── application/      # Application layer (use cases)
│   └── usecases/     # NotifyResourceUsageChangesUseCase
├── infrastructure/   # Infrastructure layer (external integrations)
│   ├── repositories/ # Google Calendar implementation
│   ├── notifier/     # Slack implementation
│   └── config/       # Configuration management
└── bin/              # Entry points
    └── watcher.rs    # Main watcher binary
```

**Key Design Patterns:**
- **DDD Factory Pattern**: Device specification parsing (`ResourceFactory`)
- **Repository Pattern**: Abstract data access via traits
- **Hexagonal Architecture**: Ports and Adapters for external dependencies

## Setup

### 1. Environment Variables

```bash
cp .env.example .env
```

Edit `.env` to configure:

```env
GOOGLE_SERVICE_ACCOUNT_KEY=secrets/service-account.json
CONFIG_PATH=config/resources.toml
```

**Note**: Notification settings (Slack webhook URLs, etc.) are configured in `config/resources.toml` per resource.

### 2. Google Calendar API Setup

1. Create a project in [Google Cloud Console](https://console.cloud.google.com/)
2. Enable Google Calendar API
3. Create a service account and download JSON key
4. Place the key as `secrets/service-account.json`
5. Share your calendar with the service account email

### 3. Resource Configuration

Define GPU servers and rooms in `config/resources.toml`:

```toml
[[servers]]
name = "Thalys"
calendar_id = "your-calendar-id@group.calendar.google.com"

# Configure notification destinations per resource
[[servers.notifications]]
type = "slack"
webhook_url = "https://hooks.slack.com/services/YOUR/WEBHOOK/URL"

# Optional: Add mock notifications for testing
# [[servers.notifications]]
# type = "mock"

[[servers.devices]]
id = 0
model = "A100 80GB PCIe"

[[servers.devices]]
id = 1
model = "A100 80GB PCIe"

[[rooms]]
name = "Meeting Room A"
calendar_id = "room-calendar-id@group.calendar.google.com"

[[rooms.notifications]]
type = "slack"
webhook_url = "https://hooks.slack.com/services/YOUR/ROOM/WEBHOOK"
```

Each resource can have multiple notification destinations, and different resources can notify different channels.

## Usage

### Running the Watcher

```bash
# Default (Google Calendar + configured notifications)
cargo run --bin watcher

# Use mock repository (for testing)
cargo run --bin watcher --repository mock

# Customize polling interval (default: 60 seconds)
cargo run --bin watcher --interval 30
```

### CLI Options

- `--repository <google_calendar|mock>`: Select data source
- `--interval <seconds>`: Set polling interval

Notification destinations are configured per resource in `config/resources.toml`.

### Using as a Library

Add to your `Cargo.toml`:

```toml
[dependencies]
lab-resource-manager = "0.1"
```

Example code:

```rust
use lab_resource_manager::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration
    let config = load_config("config/resources.toml")?;

    // Create repository and notifier
    let repository = GoogleCalendarUsageRepository::new(
        "secrets/service-account.json",
        config.clone(),
    ).await?;

    // NotificationRouter automatically handles all configured notification types
    let notifier = NotificationRouter::new(config);

    // Create and run use case
    let usecase = NotifyResourceUsageChangesUseCase::new(repository, notifier).await?;
    usecase.poll_once().await?;

    Ok(())
}
```

See [examples/](examples/) for more usage patterns.

## Development

### Running Tests

```bash
# All tests
cargo test

# Specific module
cargo test resource_factory

# With output
cargo test -- --nocapture
```

### Building

```bash
# Development build
cargo build

# Release build
cargo build --release
```

### Code Quality

```bash
cargo check
cargo clippy
cargo fmt
```

## Device Specification Format

In calendar event titles, you can specify devices using the following formats:

- Single: `0` → Device 0
- Range: `0-2` → Devices 0, 1, 2
- Multiple: `0,2,5` → Devices 0, 2, 5
- Mixed: `0-1,6-7` → Devices 0, 1, 6, 7

The `ResourceFactory` in the domain layer handles parsing these specifications.

## Project Status

### Implemented ✅

- [x] Resource-based notification routing

### Roadmap

- [ ] Slack command for creating calendar reservations
- [ ] Slack user mapping
- [ ] Natural language resource management (LLM agent)

## License

Licensed under either of

 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

## Acknowledgments

Developed for laboratory resource management at Kano Lab.
