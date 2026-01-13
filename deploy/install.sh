#!/bin/bash
set -euo pipefail

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RELEASE_DIR="$(dirname "$SCRIPT_DIR")"

# Configuration (FHS-compliant paths)
BIN_DIR="/usr/local/bin"
CONFIG_DIR="/etc/lab-resource-manager"
DATA_DIR="/var/lib/lab-resource-manager"
ENV_FILE="/etc/default/lab-resource-manager"

SERVICE_USER="lrm"
SERVICE_GROUP="lrm"
BINARY_NAME="lab-resource-manager"
SERVICE_FILE="lab-resource-manager.service"

# Check if running as root
if [[ $EUID -ne 0 ]]; then
   echo "This script must be run as root"
   exit 1
fi

# Verify required files exist
if [[ ! -f "$RELEASE_DIR/$BINARY_NAME" ]]; then
    echo "Error: Binary not found at $RELEASE_DIR/$BINARY_NAME"
    exit 1
fi

if [[ ! -f "$SCRIPT_DIR/$SERVICE_FILE" ]]; then
    echo "Error: Service file not found at $SCRIPT_DIR/$SERVICE_FILE"
    exit 1
fi

echo "Installing $BINARY_NAME..."

# Create user and group if not exists
if ! getent group "$SERVICE_GROUP" > /dev/null 2>&1; then
    groupadd --system "$SERVICE_GROUP"
    echo "Created group: $SERVICE_GROUP"
fi

if ! getent passwd "$SERVICE_USER" > /dev/null 2>&1; then
    useradd --system --gid "$SERVICE_GROUP" --no-create-home --shell /usr/sbin/nologin "$SERVICE_USER"
    echo "Created user: $SERVICE_USER"
fi

# Create directory structure
mkdir -p "$CONFIG_DIR"
mkdir -p "$DATA_DIR"

# Copy binary
cp "$RELEASE_DIR/$BINARY_NAME" "$BIN_DIR/"
chmod 755 "$BIN_DIR/$BINARY_NAME"

# Copy systemd service file
cp "$SCRIPT_DIR/$SERVICE_FILE" /etc/systemd/system/

# Set permissions
chown -R "$SERVICE_USER:$SERVICE_GROUP" "$CONFIG_DIR"
chown -R "$SERVICE_USER:$SERVICE_GROUP" "$DATA_DIR"
chmod 750 "$CONFIG_DIR"
chmod 750 "$DATA_DIR"

# Reload systemd
systemctl daemon-reload

echo ""
echo "Installation complete!"
echo ""
echo "Directory structure:"
echo "  Binary:  $BIN_DIR/$BINARY_NAME"
echo "  Config:  $CONFIG_DIR/"
echo "  Data:    $DATA_DIR/"
echo "  Env:     $ENV_FILE"
echo ""
echo "Next steps:"
echo "  1. Create environment file: $ENV_FILE"
echo "  2. Copy config files to $CONFIG_DIR/"
echo "  3. Start the service: systemctl start $BINARY_NAME"
echo "  4. Enable on boot: systemctl enable $BINARY_NAME"
echo ""
echo "Environment file format ($ENV_FILE):"
echo "  SLACK_BOT_TOKEN=xoxb-..."
echo "  SLACK_APP_TOKEN=xapp-..."
echo "  GOOGLE_SERVICE_ACCOUNT_KEY=$CONFIG_DIR/service-account.json"
echo "  RESOURCE_CONFIG=$CONFIG_DIR/resources.toml"
echo "  IDENTITY_LINKS_FILE=$DATA_DIR/identity_links.json"
echo "  GOOGLE_CALENDAR_MAPPINGS_FILE=$DATA_DIR/google_calendar_mappings.json"
echo "  RUST_LOG=info"
