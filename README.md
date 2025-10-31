# lab-resource-manager

[![Crates.io](https://img.shields.io/crates/v/lab-resource-manager)](https://crates.io/crates/lab-resource-manager)
[![Documentation](https://docs.rs/lab-resource-manager/badge.svg)](https://docs.rs/lab-resource-manager)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)](README.md#license)

GPU and room resource usage management and notification system.

[日本語 README](README_ja.md)

## Features

- **Resource Usage Management**: Manage GPU server and room usage schedules (default implementation: Google Calendar)
- **Slack Bot for Access Control**: Users can register their email addresses and get resource access via Slack commands
  - Support for DMs and private channels via response_url
  - Automatic user mentions in notifications
- **Identity Linking**: Map email addresses to chat user IDs for enhanced notifications
- **Multi-Destination Notifications**: Configure different notification destinations
  per resource (default implementations: Slack, Mock)
- **Flexible Device Specification**: Support for multi-device notation like `0-2,5,7-9`
- **Clean Architecture**: Designed with DDD + Hexagonal Architecture with Shared Kernel pattern
- **Extensible Design**: Repositories, notifiers, and access control services abstracted as ports

## Architecture

This project follows Clean Architecture principles:

```text
src/
├── domain/                  # Domain layer (business logic)
│   ├── aggregates/          # Aggregates (ResourceUsage, IdentityLink)
│   ├── common/              # Shared Kernel (EmailAddress, etc.)
│   ├── ports/               # Ports (Repository, Notifier, ResourceCollectionAccess traits)
│   └── errors.rs            # Domain errors
├── application/             # Application layer (use cases)
│   └── usecases/            # Notify, GrantAccess use cases
├── infrastructure/          # Infrastructure layer (external integrations)
│   ├── repositories/        # Repository implementations (Google Calendar, JSON file, etc.)
│   │   ├── resource_usage/  # ResourceUsage repository implementations
│   │   └── identity_link/   # IdentityLink repository implementations
│   ├── notifier/            # Notifier implementations (Slack, Mock, etc.)
│   ├── resource_collection_access/ # Access control service implementations (Google Calendar, etc.)
│   └── config/              # Configuration management
├── interface/               # Interface layer (adapters)
│   └── slack/               # Slack bot (Socket Mode + command handlers)
└── bin/                     # Entry points
    ├── watcher.rs           # Resource usage watcher
    └── slackbot.rs          # Slack bot for resource access management
```

## Setup

### 1. Environment Variables

```bash
cp .env.example .env
```

Edit `.env` to configure:

```env
# Repository Configuration (default implementation: Google Calendar)
GOOGLE_SERVICE_ACCOUNT_KEY=secrets/service-account.json

# Resource Configuration
RESOURCE_CONFIG=config/resources.toml

# Slack Bot Configuration (for slackbot binary)
SLACK_BOT_TOKEN=xoxb-your-bot-token-here
SLACK_APP_TOKEN=xapp-your-app-token-here
```

**Note**: Notification settings are configured in `config/resources.toml` per resource.

### 2. Repository Implementation Setup (Default: Google Calendar)

If using the Google Calendar repository:

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
calendar_id = "your-calendar-id@group.calendar.google.com"  # Repository implementation-specific ID

# Configure notification destinations per resource
[[servers.notifications]]
type = "slack"  # Notifier implementation selection
webhook_url = "https://hooks.slack.com/services/YOUR/WEBHOOK/URL"
# Optional: Configure timezone for notifications (IANA format)
# If not specified, notifications will show times in UTC
# timezone = "Asia/Tokyo"

# Optional: Add mock notifications for testing
# [[servers.notifications]]
# type = "mock"
# timezone = "America/New_York"

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
# timezone = "Europe/London"
```

Each resource can have multiple notifier implementations configured, and different
resources can specify different notification destinations.

**Timezone Configuration**: You can optionally specify a timezone for each notification
destination using IANA timezone names (e.g., `Asia/Tokyo`, `America/New_York`,
`Europe/London`). If not specified, times will be displayed in the system's local
timezone (where the bot is running). When a timezone is configured, times will be
converted to that timezone and displayed with the timezone name, making it easier to
understand local times.

## Usage

### Running the Watcher

```bash
# Default (repository implementation + configured notifications)
cargo run --bin watcher

# Use mock repository (for testing)
cargo run --bin watcher --repository mock

# Customize polling interval (default: 60 seconds)
cargo run --bin watcher --interval 30
```

### CLI Options

- `--repository <google_calendar|mock>`: Select repository implementation
- `--interval <seconds>`: Set polling interval

Notifier implementations are configured per resource in `config/resources.toml`.

### Running the Slack Bot

The Slack bot allows users to register their email addresses and get access to all resource collections:

```bash
# Run the bot
cargo run --bin slackbot
```

**Slack Commands:**

- `/register-calendar <your-email@example.com>` - Register your own email address and link to your Slack account
- `/link-user <@slack_user> <email@example.com>` - Link another user's email address to their Slack account

### Using as a Library

Add to your `Cargo.toml`:

```toml
[dependencies]
lab-resource-manager = "0.1"
```

Example code (using Google Calendar implementation):

```rust
use lab_resource_manager::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration
    let config = load_config("config/resources.toml")?;

    // Create repository implementation (using Google Calendar here)
    let repository = GoogleCalendarUsageRepository::new(
        "secrets/service-account.json",
        config.clone(),
    ).await?;

    // Create notification router (automatically handles all configured notifier implementations)
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

In resource usage titles, you can specify devices using the following formats:

- Single: `0` → Device 0
- Range: `0-2` → Devices 0, 1, 2
- Multiple: `0,2,5` → Devices 0, 2, 5
- Mixed: `0-1,6-7` → Devices 0, 1, 6, 7

The `ResourceFactory` in the domain layer handles parsing these specifications.

## Project Status

### Implemented ✅

- [x] Resource-based notification routing
- [x] Identity Linking (chat user mapping)
- [x] Slack bot for resource collection access management

### Roadmap

- [ ] Slack command for creating resource usage reservations
- [ ] Natural language resource management (LLM agent)
- [ ] Web UI for resource management

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

## Acknowledgments

Developed for laboratory resource management at Kano Lab.
