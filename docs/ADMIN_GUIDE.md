# Administrator Guide

This guide is for administrators who are deploying lab-resource-manager in their laboratory.

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

Each resource can have multiple notifier implementations configured, and different resources can specify different
notification destinations.

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
# Run the bot
cargo run --bin slackbot
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

### Binary Deployment

After a release build, binaries are generated in `target/release/`:

- `target/release/watcher` - Resource monitoring program
- `target/release/slackbot` - Slack bot
