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

### 4. Notification Message Customization (Optional)

You can customize notification message templates and formatting:

```toml
[[servers.notifications]]
type = "slack"
bot_token = "xoxb-YOUR-BOT-TOKEN"
channel_id = "C01234567..."
timezone = "Asia/Tokyo"

# Message templates (optional)
[servers.notifications.templates]
created = "{user} is using {resource} at {time}"
updated = "{user} changed reservation: {resource} {time}"
deleted = "{user} cancelled reservation: {resource}"
conflict = "{user} already reserved {resource} at {time}"

# Format settings (optional)
[servers.notifications.format]
resource_style = "compact"   # Resource display style
time_style = "smart"         # Time display style
date_format = "md"           # Date format
```

`conflict` is used when a reservation request is rejected due to a conflict.
Unlike `created`/`updated`/`deleted`, its placeholders refer to the
**existing** reservation that caused the conflict, not the request that was
rejected: `{user}` is the existing reservation's owner, `{time}` is its
period, and `{resource}` is the specific resource that conflicted. It is
shown to the user who attempted the reservation, using the format/template
settings of the conflicting resource's notification config (not a
resource-wide broadcast).

If a single reservation request covers multiple resources (e.g. reserving
an entire server) and more than one of them conflicts — possibly with
different existing reservations — the template is rendered once per
conflicting resource and the resulting messages are shown together, so no
conflict is silently dropped.

**Placeholders:**

| Placeholder | Description |
|-------------|-------------|
| `{user}` | User name/Slack mention |
| `{resource}` | Resource information |
| `{time}` | Time period |
| `{notes}` | Notes section with heading (expands to `\n\n📝 備考\n...` if present, empty if absent) |
| `{resource_label}` | Resource label (e.g., 💻 予約GPU; label text is in Japanese) |

**resource_style options:**

| Value | Example Output |
|-------|----------------|
| `full` (default) | Thalys / A100 80GB PCIe / GPU:0 |
| `compact` | Thalys 0,1,2 |
| `server_only` | Thalys |

**time_style options:**

| Value | Example Output |
|-------|----------------|
| `full` (default) | 2024-01-15 19:00 - 2024-01-15 21:00 (Asia/Tokyo) |
| `smart` | 1/15 19:00-21:00 (omits end date if same day) |
| `relative` | 今日 19:00-21:00, 明日 10:00-12:00 |

**date_format options:**

| Value | Example Output |
|-------|----------------|
| `ymd` (default) | 2024-01-15 |
| `md` | 1/15 |
| `md_japanese` | 1月15日 |

### 5. GPU Usage Observer Adapter Setup (Optional, Experimental)

The monitoring component that cross-references actual GPU usage against reservations
(e.g., detecting unreserved usage) is pluggable via the `ResourceUsageObserver` port, so
it can be adapted to each lab's infrastructure. Two implementations are currently
available:

| Implementation | Purpose |
|-----------------|---------|
| Mock | Testing/development only (always returns empty results) |
| `SharedFileResourceUsageObserver` | Reads GPU usage per server via a shared filesystem |

To use `SharedFileResourceUsageObserver`, you need to register `scripts/gpu_usage_reporter.py`
as a cron job on each GPU server.

**Prerequisites:**

- A shared directory (e.g., NFS) readable/writable from every monitored server (e.g., Thalys, Freccia, Lyria)
- Python3 and `nvidia-smi` available on each server (no `pip install` needed, standard library only)

**Setup steps:**

This script is the "reporting side" of the setup. Deploy it to **every monitored server
individually** — not just the one running the LRM binary (e.g., Thalys) — including
Freccia and Lyria, each with its own `--server-name`.

1. Deploy `scripts/gpu_usage_reporter.py` to every monitored server.
2. Register it in each server's crontab, using that server's own name (example: every minute):

```cron
# crontab on Thalys
* * * * * /usr/bin/python3 /path/to/gpu_usage_reporter.py \
    --server-name Thalys --output-dir /mnt/shared/lrm-gpu-status

# crontab on Freccia
* * * * * /usr/bin/python3 /path/to/gpu_usage_reporter.py \
    --server-name Freccia --output-dir /mnt/shared/lrm-gpu-status

# crontab on Lyria
* * * * * /usr/bin/python3 /path/to/gpu_usage_reporter.py \
    --server-name Lyria --output-dir /mnt/shared/lrm-gpu-status
```

`--output-dir` must point to the same shared directory from every server (see
Prerequisites). `--server-name` must match a `servers[].name` value in
`config/resources.toml`, using **that server's own name** — pointing it at another
server's name overwrites that other server's report with the wrong data.

3. The resulting `{lowercased server name}.json` schema:

```json
{
  "server": "Thalys",
  "generated_at": "2026-07-24T12:00:00+00:00",
  "processes": [
    {"device_number": 0, "os_user": "kkawaguchi", "started_at": "2026-07-24T10:00:00+00:00"}
  ]
}
```

Files older than a configured staleness threshold (5 minutes by default) are ignored, so a
stopped cron job doesn't cause stale usage data to be mistaken for still-active usage.

**Current limitation**: Only the JSON output from this script has been verified working.
The main binary's periodic read of this file to cross-reference reservations, and the
Slack notification of detected mismatches, are still under development — `AppConfig` does
not yet have environment variables for this observer.

## Running the System

### Service Management

```bash
# Start the service
sudo systemctl start lab-resource-manager

# Stop the service
sudo systemctl stop lab-resource-manager

# Check status
sudo systemctl status lab-resource-manager

# View logs
sudo journalctl -u lab-resource-manager -f

# Enable on boot
sudo systemctl enable lab-resource-manager
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

## Installation

Download the latest release from [GitHub Releases](https://github.com/kano-lab/lab-resource-manager/releases) and run:

```bash
# Extract and install
tar -xzf lab-resource-manager-x86_64-unknown-linux-gnu.tar.gz
sudo bash deploy/install.sh
```

This installs:

- `/usr/local/bin/lab-resource-manager` - Main binary
- `/etc/lab-resource-manager/` - Configuration directory
- `/var/lib/lab-resource-manager/` - Data directory
- `/etc/systemd/system/lab-resource-manager.service` - systemd service

See [Migration Guide](MIGRATION.md) if upgrading from Docker deployment.
