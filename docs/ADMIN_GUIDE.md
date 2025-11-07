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
webhook_url = "https://hooks.slack.com/services/YOUR/ROOM/WEBHOOK"
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

## Docker Deployment

### Prerequisites

Ensure proper file permissions for secrets:

```bash
# Set secure permissions for service account key
chmod 600 secrets/service-account.json

# Recommended: Set permissions for all secrets
chmod 600 secrets/*
```

**Security Note**: The `secrets/` directory is mounted read-only in containers, but host file permissions are preserved through Docker volume mounts. Always ensure sensitive files have appropriate permissions (600 or 400) on the host system.

### Building and Running with Docker Compose

```bash
# Build and start both services
docker-compose up -d

# View logs
docker-compose logs -f

# Stop services
docker-compose down
```

The Docker setup uses a multi-stage build with separate optimized images for each service:

- **Base image**: `ubuntu:24.04` (required for GLIBC 2.38 support)
- **Service-specific stages**: Each service (watcher/slackbot) gets only its binary
- **Shared builder**: Single build stage compiles both binaries efficiently

### Standalone Docker Usage

```bash
# Build watcher image
docker build --target watcher -t lab-resource-manager:watcher .

# Build slackbot image
docker build --target slackbot -t lab-resource-manager:slackbot .

# Run watcher
docker run -v ./config:/app/config:ro \
           -v ./data:/app/data \
           -v ./secrets:/app/secrets:ro \
           --env-file .env \
           lab-resource-manager:watcher

# Run slackbot
docker run -v ./config:/app/config:ro \
           -v ./data:/app/data \
           -v ./secrets:/app/secrets:ro \
           --env-file .env \
           lab-resource-manager:slackbot
```

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
