# Administrator Guide

This guide is for administrators who are deploying lab-resource-manager in their laboratory.

## Setup

### 1. Environment Variables

For systemd deployment, create `/etc/default/lab-resource-manager`:

```env
# Repository Configuration (default implementation: Google Calendar)
GOOGLE_SERVICE_ACCOUNT_KEY=/etc/lab-resource-manager/service-account.json

# Resource Configuration
RESOURCE_CONFIG=/etc/lab-resource-manager/resources.toml

# Data files
IDENTITY_LINKS_FILE=/var/lib/lab-resource-manager/identity_links.json
GOOGLE_CALENDAR_MAPPINGS_FILE=/var/lib/lab-resource-manager/google_calendar_mappings.json

# Slack Bot Configuration
SLACK_BOT_TOKEN=xoxb-your-bot-token-here
SLACK_APP_TOKEN=xapp-your-app-token-here

# Logging
RUST_LOG=info
```

For development, you can set these as shell environment variables.

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
bot_token = "xoxb-YOUR-BOT-TOKEN"
channel_id = "C01234567..."
# Optional: Configure timezone for notifications (IANA format)
# If not specified, notifications will show times in the system's local timezone
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
bot_token = "xoxb-YOUR-BOT-TOKEN"
channel_id = "C01234567..."
# timezone = "Europe/London"
```

Each resource can have multiple notifier implementations configured, and different resources can specify different
notification destinations.

**Timezone Configuration**: You can optionally specify a timezone for each notification
destination using IANA timezone names (e.g., `Asia/Tokyo`, `America/New_York`,
`Europe/London`). If not specified, times will be displayed in the system's local
timezone (where the bot is running). When a timezone is configured, times will be
converted to that timezone and displayed with the timezone name, making it easier to
understand local times.

## Running the System

### Running the Watcher (Resource Monitor)

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
# Run the bot (development)
cargo run --bin lab-resource-manager

# Run as systemd service (production)
sudo systemctl start lab-resource-manager
```

### Administrator Commands

Administrators can register other users' email addresses:

```text
/link-user <@slack_user> <email@example.com>
```

**Example:**

```text
/link-user @bob bob@example.com
```

This command links the specified Slack user with an email address and grants access to Google Calendar resources.

## Building

### Development Build

```bash
cargo build
```

### Release Build

```bash
cargo build --release
```

### Production Deployment

For production deployment, use the binary release with systemd:

```bash
# Download and install (see README for details)
sudo bash deploy/install.sh
```

This installs:

- `/usr/local/bin/lab-resource-manager` - Main binary
- `/etc/lab-resource-manager/` - Configuration directory
- `/var/lib/lab-resource-manager/` - Data directory
- `/etc/systemd/system/lab-resource-manager.service` - systemd service

See [Migration Guide](MIGRATION.md) if upgrading from Docker deployment.
