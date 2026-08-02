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
name = "gpu-server-1"
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
| `full` (default) | gpu-server-1 / A100 80GB PCIe / GPU:0 |
| `compact` | gpu-server-1 0,1,2 |
| `server_only` | gpu-server-1 |

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

To use `SharedFileResourceUsageObserver`, you need to register the `gpu-usage-reporter`
binary as a cron job on each GPU server. This binary ships alongside the main
`lab-resource-manager` binary in the release archive (e.g.,
`lab-resource-manager-x86_64-unknown-linux-musl.tar.gz`).

**Prerequisites:**

- A shared directory (e.g., NFS) readable/writable from every monitored server
- `nvidia-smi`, `getconf`, and `getent` available on each server (all standard on typical
  Linux setups). `gpu-usage-reporter` itself is a musl static build with no other runtime
  dependencies.

**Setup steps:**

This binary is the "reporting side" of the setup. Deploy it to **every monitored server
individually**, including the one that runs the LRM binary itself, each with its own
`--server-name`.

1. Deploy `gpu-usage-reporter` to every monitored server.
2. Register it in each server's crontab, using that server's own name (example: every minute):

   ```cron
   # crontab on gpu-server-1
   * * * * * /usr/local/bin/gpu-usage-reporter \
       --server-name gpu-server-1 --output-dir /mnt/shared/lrm-gpu-status

   # crontab on gpu-server-2
   * * * * * /usr/local/bin/gpu-usage-reporter \
       --server-name gpu-server-2 --output-dir /mnt/shared/lrm-gpu-status
   ```

   `--output-dir` must point to the same shared directory from every server (see
   Prerequisites). `--server-name` must match a `servers[].name` value in
   `config/resources.toml`, using **that server's own name** — pointing it at another
   server's name overwrites that other server's report with the wrong data.

3. The resulting `{lowercased server name}.json` schema:

```json
{
  "server": "gpu-server-1",
  "generated_at": "2026-07-24T12:00:00+00:00",
  "processes": [
    {"device_number": 0, "os_user": "alice", "started_at": "2026-07-24T10:00:00+00:00"}
  ]
}
```

Files older than a configured staleness threshold (5 minutes by default) are ignored, so a
stopped cron job doesn't cause stale usage data to be mistaken for still-active usage.

**Enabling the feature on the main binary side:** the reconciliation loop (comparing
observed usage against reservations, proposing a post-hoc reservation via Slack DM, and
notifying on unauthorized usage) is disabled by default. Set `GPU_USAGE_REPORTS_DIR` to
the same shared directory used by `gpu-usage-reporter` above to enable it:

| Environment variable | Default | Purpose |
|-----------------------|---------|---------|
| `GPU_USAGE_REPORTS_DIR` | (unset = feature disabled) | Shared directory read by `SharedFileResourceUsageObserver` |
| `GPU_USAGE_MAX_STALENESS_SECS` | `300` | How old a report can be before it's ignored |
| `UNRESERVED_USAGE_THRESHOLD_SECS` | `600` | How long unreserved usage must continue before a proposal is sent |
| `RESERVATION_PROPOSAL_DURATION_CANDIDATES_HOURS` | `1,2,3,5,8` | Comma-separated hour candidates offered in the Slack DM |

When enabled, a user whose OS account is linked (`/link-user`) and who also has a linked
Slack account will receive a DM with buttons for each duration candidate; clicking one
creates the reservation retroactively starting from when usage was first observed.

Usage that conflicts with someone else's existing reservation (i.e. someone is using a
resource reserved by someone else) sends a DM directly to the person actually using it — not
a broadcast to the resource's notification channel — asking them to stop or make their own
reservation. This DM requires the actual user's OS account to be linked to an email address
that also has a linked Slack account; if the OS account observed on the server isn't linked
to anyone, the event is skipped entirely (whether it's unauthorized use is undecidable
without knowing who it is), so linking every server user via `/link-user` is required for
this detection to work.

**Current limitation**: this wiring has been verified with unit/integration tests using
mock adapters, but sending an actual Slack DM (via `conversations.open`/`chat.postMessage`)
and running `gpu-usage-reporter` against a real GPU server have not been verified in a live
environment yet. The Slack app needs the `im:write` and `chat:write` scopes for the DM to
work.

### 6. MCP Server Setup (Optional)

Agents such as Claude Code can view, create, update, and cancel reservations through an
embedded MCP (Model Context Protocol) server that runs inside the main
`lab-resource-manager` process. No new binary or systemd service is needed — the HTTP/SSE
listener is started as an additional task within the existing process. The Google service
account key continues to exist only on the single host running LRM.

**Prerequisite**: the lab's servers and members' machines must be able to reach each other
over the same LAN. Exposing it to the internet is out of scope.

**Environment variables:**

| Environment variable | Default | Purpose |
|-----------------------|---------|---------|
| `MCP_LISTEN_ADDR` | (unset = feature disabled) | HTTP/SSE listen address for the MCP server (e.g. `0.0.0.0:8787`) |
| `MCP_TOKENS_FILE` | `/var/lib/lab-resource-manager/mcp_tokens.json` | Persistence file for MCP access tokens |
| `MCP_ALLOWED_HOSTS` | (required; startup fails if unset while `MCP_LISTEN_ADDR` is set) | Comma-separated list of `Host` header values to accept (e.g. `<LRM host>:8787,192.168.1.10:8787`) |
| `MCP_TLS_CERT_FILE` | (unset = TLS disabled) | Path to the MCP server's TLS certificate (PEM). Must be set together with `MCP_TLS_KEY_FILE` |
| `MCP_TLS_KEY_FILE` | (unset = TLS disabled) | Path to the MCP server's TLS private key (PEM). Must be set together with `MCP_TLS_CERT_FILE` |

Set `MCP_ALLOWED_HOSTS` to the actual reachable host name/IP and port combination that
members will use in their client configuration. If `MCP_LISTEN_ADDR` is set (MCP enabled),
`MCP_ALLOWED_HOSTS` is required — leaving it unset causes startup to fail with an error
(fail-closed, so a missed setting can't silently ship unprotected).

Setting only one of `MCP_TLS_CERT_FILE`/`MCP_TLS_KEY_FILE` is a startup error. TLS is
recommended but not required — leaving both unset falls back to plain HTTP with a warning
that the Bearer token will be sent unencrypted.

**Setting up TLS (recommended):**

To avoid sending Bearer tokens in plaintext over the LAN, TLS is strongly recommended.
Create a self-signed internal CA once, then issue a server certificate from it for the LRM
host:

```bash
# 1. Create an internal CA (one-time)
openssl req -x509 -newkey rsa:4096 -keyout ca.key -out ca.crt -days 3650 -nodes \
  -subj "/CN=lab-resource-manager internal CA"

# 2. Issue a server certificate for the LRM host, signed by that CA
#    (include the actual host name/IP in the SAN)
openssl req -newkey rsa:2048 -keyout mcp.key -out mcp.csr -nodes -subj "/CN=<LRM host>"
openssl x509 -req -in mcp.csr -CA ca.crt -CAkey ca.key -CAcreateserial \
  -out mcp.crt -days 825 \
  -extfile <(echo "subjectAltName=DNS:<LRM host>,IP:<LRM host IP>")
```

```bash
MCP_TLS_CERT_FILE=/etc/lab-resource-manager/mcp.crt
MCP_TLS_KEY_FILE=/etc/lab-resource-manager/mcp.key
```

**Client-side trust**: certificate trust is a matter of the OS-level trust store, not
per-application configuration, so `ca.crt` doesn't need to be handed out to every member's
laptop individually. If your team already runs agent sessions (e.g. Claude Code) from a
shared machine — such as a shared GPU server members SSH into — registering `ca.crt` in
that machine's system trust store once is enough: every process running there
(regardless of which HTTP library the MCP client uses internally) then automatically
trusts it:

```bash
# Run on each shared machine where your team's agent sessions run (Debian/Ubuntu example)
sudo cp ca.crt /usr/local/share/ca-certificates/lab-resource-manager.crt
sudo update-ca-certificates
```

Trusting a CA is a passive client-side setting and grants no one any new access (actual
authorization is still enforced by the Bearer token). If members later want to connect
from their own laptops too, add the same `ca.crt` there as well.

**Authentication: the `/mcp-token` command**

Members run the following in Slack to obtain their own personal access token:

```text
/mcp-token
```

The token is shown in an ephemeral message visible only to the requester, sent to the
email address linked to their Slack account (via `/link-user` or `/register-calendar`).
Re-running the command issues a new token and revokes the previous one (one token per
person).

**Example member-side configuration (`.mcp.json`):**

```json
{
  "mcpServers": {
    "lab-resource-manager": {
      "url": "https://<LRM host>:8787/mcp",
      "headers": {
        "Authorization": "Bearer <issued token>"
      }
    }
  }
}
```

(Use `http://` instead of `https://` if TLS is not configured.)

**Available tools:**

| Tool | Description |
|------|--------------|
| `list_all_reservations` | List all future (including ongoing) reservations |
| `list_my_reservations` | List reservations owned by the caller |
| `get_reservation` | Get a reservation's details by ID |
| `create_reservation` | Create a new reservation (GPU server or room) |
| `update_reservation` | Update the time or notes of a reservation you own |
| `cancel_reservation` | Cancel a reservation you own |

Write tools (create/update/cancel) treat the email address linked to the caller's Bearer
token as the owner. Updating or cancelling someone else's reservation is rejected by the
same existing authorization rule used by `/reserve` (owner only).

TLS itself (handshake succeeding with a self-signed certificate, rejection when the CA
isn't trusted, and the auth middleware still requiring a Bearer token independently of the
TLS layer) has been verified in the development environment.

**Current limitation**: connecting over HTTP/SSE from another machine on the LAN,
exercising `/mcp-token` against a real Slack workspace, and confirming that a real MCP
client (e.g. Claude Code) actually trusts a CA registered on a shared machine, have not
been verified — the development Docker sandbox cannot exercise any of these.

### 7. Web Timeline Setup (Optional, Experimental)

Browse reservations as a resource-by-time timeline. Comparing GPU availability side by side
is what a calendar UI is poor at, and this page fills that gap. As with the MCP server, an
HTTP listener starts as an extra task inside the existing `lab-resource-manager` process —
no separate binary or systemd service is needed.

See the [live demo](https://kano-lab.github.io/lab-resource-manager/) for what it looks like
(the data there is fictional).

**This feature is experimental.** The environment variable names, the URL layout and the
shape of the page may change in a minor release. The web framework underneath (Topcoat) is
itself a 0.x release that expects breaking changes. Take that into account before building
it into an operational procedure. The library's public API is unaffected — only `serve` and
the `ReservationQuery` port are exposed, and neither names a framework type.

**It is read-only.** Creating, updating and cancelling reservations stay with Slack and MCP.

**There is no authentication.** Anyone who can reach the URL can see everyone's
reservations. The same assumptions as MCP apply: lab servers and members' machines share a
LAN, and exposure to the internet is out of scope. Bind `WEB_LISTEN_ADDR` to an address
reachable only from the LAN. To limit what is exposed, owners are shown by the local part of
their email address (the part before `@`) only. Reservation notes, however, are shown
verbatim, so members should keep anything sensitive out of them.

**Requires the `web` feature at build time:**

```bash
cargo build --release --features web --bin lab-resource-manager
```

It is off by default so that people using this crate as a library from crates.io don't take
on a web framework dependency. The binaries attached to GitHub releases have it enabled.

**Environment variables:**

| Variable | Default | Purpose |
|----------|---------|---------|
| `WEB_LISTEN_ADDR` | (unset = feature disabled) | Listen address for the web page (e.g. `0.0.0.0:8080`) |
| `WEB_TIMEZONE` | `Asia/Tokyo` | Timezone used to display times (IANA timezone name) |

Topcoat's router cannot be nested under a path prefix, so give the web page a different port
from the MCP server.

**Reading the page:**

- Resources (each server's GPUs, then rooms) run down the page; time runs across it.
  Resources with no reservations keep their row, so you can see what is free
- The range — 1 day, 3 days, 1 week, 1 month — is selectable at the top right, or via
  `?days=` in the URL (capped at 60). The view starts at midnight of the current day
- Reservation blocks are coloured per owner. Hovering one shows the exact times and notes
- The red vertical line marks the current time
- Reservations that overlap on the same resource are stacked so both stay visible
- Reservations for resources missing from the configuration are collected into a row at the
  bottom — dropping them silently would make the page claim a resource is free

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

Administrators can link another person's identity to their email address:

```text
/link-user
```

This opens a modal where a radio button switches between two link targets:

- **Slack User**: select a Slack user and enter their email address. Grants Google Calendar
  access, same as before.
- **OS Username**: select the GPU server and enter the person's OS username together with
  their email address. This is what makes the unreserved-usage-proposal and
  unauthorized-usage-DM features described above able to identify who is actually running a
  process — without it, an observed OS username can never be resolved to a person.

### Logs

Logs go to journald as structured records. `RUST_LOG` controls the level for everything the
service emits.

```bash
# Follow the log
sudo journalctl -u lab-resource-manager -f

# Read fields instead of formatted lines
sudo journalctl -u lab-resource-manager -o json | jq 'select(.MESSAGE | contains("reservation created"))'
```

Each polling pass ends with a one-line summary, which is the quickest way to notice something
is off:

```text
reconcile pass finished
  observed=6 reservations_in_progress=5 matched_to_a_reservation=5
  unreserved_sessions=1 failures=0 elapsed_ms=412
change detection pass finished
  reservations=23 created=0 deleted=0 elapsed_ms=380
```

A count that jumps or an `elapsed_ms` that grows points at a problem well before anything
visibly breaks.

Set `RUST_LOG=debug` to also see per-step detail (form parsing, modal handling, individual
Slack API calls). This is verbose and meant for investigating a specific problem, not for
normal operation.

**Personal information in logs**: logs contain the email addresses and OS usernames of the
people who make reservations. This is deliberate — it is what makes it possible to answer who
booked or cancelled something. Access is limited to whoever can read journald (the `adm` and
`systemd-journal` groups). If you forward these logs to an external collector, that personal
information leaves the host, so decide whether that is acceptable first.

**Tracing a message a member asks about**: every message the service posts is logged with the
`channel` and `ts` that Slack assigned it, which together identify that message. Given a Slack
permalink of the form `.../archives/<channel>/p<digits>`, the `ts` is those digits with a dot
inserted before the last six.

```bash
# find the log line for a message the member linked to
sudo journalctl -u lab-resource-manager | grep 'ts=1785207600.123456'
```

The line also carries the reservation id and recipient, so a report of "this notification looks
wrong" can be traced back to the reservation it came from. Ephemeral messages (the private
replies to slash commands and buttons) are the exception: Slack returns no identifier for them,
so they cannot be located this way.

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
