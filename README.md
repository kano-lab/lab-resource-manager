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
    └── lab-resource-manager.rs  # Slack bot for resource access management
```

## Setup

For detailed setup instructions, see the [Administrator Guide](docs/ADMIN_GUIDE.md).

## Installation

### From Binary Release (Recommended)

Download the latest release from [GitHub Releases](https://github.com/kano-lab/lab-resource-manager/releases):

```bash
# Download and extract
curl -LO https://github.com/kano-lab/lab-resource-manager/releases/latest/download/lab-resource-manager-x86_64-unknown-linux-gnu.tar.gz
tar -xzf lab-resource-manager-x86_64-unknown-linux-gnu.tar.gz

# Install (requires root)
sudo bash deploy/install.sh
```

This installs:

- Binary to `/usr/local/bin/lab-resource-manager`
- Config directory at `/etc/lab-resource-manager/`
- Data directory at `/var/lib/lab-resource-manager/`
- systemd service file

### From Source

```bash
cargo build --release
```

### Migration from Docker

If upgrading from a Docker-based deployment, see [Migration Guide](docs/MIGRATION.md).

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
cargo run --bin lab-resource-manager
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
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration
    let config = load_config("config/resources.toml")?;

    // Create repository implementation (using Google Calendar here)
    let repository = GoogleCalendarUsageRepository::new(
        "secrets/service-account.json",
        config.clone(),
        PathBuf::from("data/google_calendar_mappings.json"),
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
