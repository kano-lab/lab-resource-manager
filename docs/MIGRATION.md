# Migration Guide: Docker to Binary Release (v1.0.0)

This guide explains how to migrate from Docker-based deployment to the new binary release with systemd.

## Overview

v1.0.0 introduces a breaking change in the deployment method:

| Item | Before (Docker) | After (v1.0.0) |
|------|-----------------|----------------|
| Deployment | Docker Compose | Binary + systemd |
| Config | `config/` | `/etc/lab-resource-manager/` |
| Data | `data/` | `/var/lib/lab-resource-manager/` |
| Env | `.env` | `/etc/default/lab-resource-manager` |
| Binary | Container | `/usr/local/bin/lab-resource-manager` |

## Prerequisites

- Root access to the server
- Backup of existing data

## Migration Steps

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

## Troubleshooting

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

If you encounter issues, please open an issue at:
https://github.com/kano-lab/lab-resource-manager/issues
