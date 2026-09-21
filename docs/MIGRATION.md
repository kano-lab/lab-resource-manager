# Migration Guide

## Storage Configuration Schema Change (v2.0.0)

v2.0.0 introduces a breaking change to the `resources.toml` configuration format: calendar
ID mappings are now separated from resource definitions for better separation of concerns.

### Configuration Format Change

The storage mapping (where reservations are persisted) is now separate from resource
definitions (what can be reserved).

**Before (v1.x):**

```toml
[[servers]]
name = "gpu-server-1"
calendar_id = "xxx@group.calendar.google.com"

[[servers.devices]]
id = 0
model = "A100 80GB PCIe"

[[rooms]]
name = "Meeting Room A"
calendar_id = "yyy@group.calendar.google.com"
```

**After (v2.0.0):**

```toml
[[servers]]
name = "gpu-server-1"

[[servers.devices]]
id = 0
model = "A100 80GB PCIe"

[[rooms]]
name = "Meeting Room A"

[storage]
type = "google_calendar"

[storage.calendars]
"gpu-server-1" = "xxx@group.calendar.google.com"
"Meeting Room A" = "yyy@group.calendar.google.com"
```

### v2.0.0 Migration Steps

1. **Backup Your Current Config**

   ```bash
   cp /etc/lab-resource-manager/resources.toml ~/resources.toml.backup
   ```

2. **Update resources.toml**

   For each server and room in your configuration:
   - Remove the `calendar_id` line from `[[servers]]` or `[[rooms]]` entries
   - Add a new `[storage]` section at the end of the file
   - Create `[storage.calendars]` with resource name -> calendar ID mappings

   **Example Migration:**

   Move this:

   ```toml
   [[servers]]
   name = "gpu-server-1"
   calendar_id = "xxx@group.calendar.google.com"
   ```

   To this:

   ```toml
   [[servers]]
   name = "gpu-server-1"

   # ... rest of servers ...

   [storage]
   type = "google_calendar"

   [storage.calendars]
   "gpu-server-1" = "xxx@group.calendar.google.com"
   ```

3. **Verify Configuration**

   Start the service to validate the new configuration:

   ```bash
   sudo systemctl restart lab-resource-manager
   sudo journalctl -u lab-resource-manager -f
   ```

   If there are mismatched mappings, you'll see a clear error message listing which
   resources lack calendar ID mappings.

4. **Validation Checklist**

   - [ ] All server names in `[[servers]]` have corresponding entries in `[storage.calendars]`
   - [ ] All room names in `[[rooms]]` have corresponding entries in `[storage.calendars]`
   - [ ] Service starts without errors: `sudo systemctl status lab-resource-manager`
   - [ ] Slack bot responds to commands
   - [ ] Calendar integration works (verify in logs)

### v2.0.0 Troubleshooting

**Error: "resources.toml の以下のリソースが storage.calendars に写像を持ちません"**

This means some servers or rooms are missing calendar ID mappings. Check:

```bash
# See which resources are configured
grep '^name = ' /etc/lab-resource-manager/resources.toml

# Verify all are in the storage.calendars section
grep -A 100 '\[storage.calendars\]' /etc/lab-resource-manager/resources.toml
```

Add any missing entries to `[storage.calendars]`.

### v2.0.0 Notes

- The `storage_config` section must specify `type = "google_calendar"` (this is currently
  the only supported backend)
- All resource names (servers and rooms) must have exactly one calendar ID mapping
- The application startup will fail with a clear error if mappings are incomplete
- Warning logs appear for extra calendars defined but not used (these can be ignored if
  you're planning to add resources later)

---

# Migration Guide: Docker to Binary Release (v1.0.0)

This guide explains how to migrate from Docker-based deployment to the new binary release
with systemd.

## Overview

v1.0.0 introduces a breaking change in the deployment method:

| Item       | Before (Docker) | After (v1.0.0)          |
|------------|-----------------|-------------------------|
| Deployment | Docker Compose  | Binary + systemd        |
| Config     | `config/`       | `/etc/lab-resource-manager/` |
| Data       | `data/`         | `/var/lib/lab-resource-manager/` |
| Env        | `.env`          | `/etc/default/lab-resource-manager` |
| Binary     | Container       | `/usr/local/bin/lab-resource-manager` |

## Prerequisites

- Root access to the server
- Backup of existing data

## v1.0.0 Migration Steps

### 1. Stop Docker Container

```bash
cd /path/to/lab-resource-manager
docker compose down
```

### 2. Backup Existing Data

```bash
# Create backup directory
mkdir -p ~/lrm-backup

# Backup data files
cp data/identity_links.json ~/lrm-backup/
cp data/google_calendar_mappings.json ~/lrm-backup/

# Backup config
cp config/resources.toml ~/lrm-backup/

# Backup environment
cp .env ~/lrm-backup/
```

### 3. Download and Install New Release

```bash
# Download the release
curl -LO https://github.com/kano-lab/lab-resource-manager/releases/download/v1.0.0/lab-resource-manager-x86_64-unknown-linux-gnu.tar.gz

# Extract
tar -xzf lab-resource-manager-x86_64-unknown-linux-gnu.tar.gz

# Run installer (as root)
sudo bash deploy/install.sh
```

### 4. Migrate Data Files

```bash
# Copy data files to new location
sudo cp ~/lrm-backup/identity_links.json /var/lib/lab-resource-manager/
sudo cp ~/lrm-backup/google_calendar_mappings.json /var/lib/lab-resource-manager/

# Copy config files
sudo cp ~/lrm-backup/resources.toml /etc/lab-resource-manager/

# Copy service account key (adjust path as needed)
sudo cp /path/to/service-account.json /etc/lab-resource-manager/

# Set ownership
sudo chown -R lrm:lrm /var/lib/lab-resource-manager/
sudo chown -R lrm:lrm /etc/lab-resource-manager/
```

### 5. Create Environment File

Convert your `.env` to the new format:

```bash
sudo tee /etc/default/lab-resource-manager << 'EOF'
SLACK_BOT_TOKEN=xoxb-your-token
SLACK_APP_TOKEN=xapp-your-token
GOOGLE_SERVICE_ACCOUNT_KEY=/etc/lab-resource-manager/service-account.json
RESOURCE_CONFIG=/etc/lab-resource-manager/resources.toml
IDENTITY_LINKS_FILE=/var/lib/lab-resource-manager/identity_links.json
GOOGLE_CALENDAR_MAPPINGS_FILE=/var/lib/lab-resource-manager/google_calendar_mappings.json
RUST_LOG=info
EOF

# Secure the file
sudo chmod 600 /etc/default/lab-resource-manager
```

### 6. Start the Service

```bash
# Start the service
sudo systemctl start lab-resource-manager

# Check status
sudo systemctl status lab-resource-manager

# View logs
sudo journalctl -u lab-resource-manager -f

# Enable on boot
sudo systemctl enable lab-resource-manager
```

### 7. Verify Operation

1. Check Slack bot responds to commands
2. Verify calendar integration works
3. Monitor logs for any errors

### 8. Clean Up (Optional)

After confirming everything works:

```bash
# Remove Docker resources
docker compose down --rmi all --volumes

# Remove old files
rm -rf /path/to/lab-resource-manager  # Old Docker deployment directory
```

## Rollback

If issues occur, you can rollback to Docker:

```bash
# Stop systemd service
sudo systemctl stop lab-resource-manager
sudo systemctl disable lab-resource-manager

# Restore Docker deployment
cd /path/to/lab-resource-manager-backup
docker compose up -d
```

## v1.0.0 Troubleshooting

### Service fails to start

Check logs:

```bash
sudo journalctl -u lab-resource-manager -e
```

Common issues:

- Missing environment variables in `/etc/default/lab-resource-manager`
- Incorrect file permissions
- Missing data files

### Permission denied errors

Ensure correct ownership:

```bash
sudo chown -R lrm:lrm /var/lib/lab-resource-manager/
sudo chown -R lrm:lrm /etc/lab-resource-manager/
```

### Cannot find configuration

Verify paths in `/etc/default/lab-resource-manager` are absolute paths and files exist.

## Support

If you encounter issues, please open an issue at
<https://github.com/kano-lab/lab-resource-manager/issues>
