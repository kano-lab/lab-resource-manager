#!/bin/bash
set -euo pipefail

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RELEASE_DIR="$(dirname "$SCRIPT_DIR")"

# Configuration
INSTALL_DIR="/opt/lab-resource-manager"
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
mkdir -p "$INSTALL_DIR"/{config,data,secrets}

# Copy binary
cp "$RELEASE_DIR/$BINARY_NAME" "$INSTALL_DIR/"
chmod 755 "$INSTALL_DIR/$BINARY_NAME"

# Copy systemd service file
cp "$SCRIPT_DIR/$SERVICE_FILE" /etc/systemd/system/

# Set permissions
chown -R "$SERVICE_USER:$SERVICE_GROUP" "$INSTALL_DIR"
chmod 750 "$INSTALL_DIR"
chmod 700 "$INSTALL_DIR/secrets"

# Reload systemd
systemctl daemon-reload

echo ""
echo "Installation complete!"
echo ""
echo "Next steps:"
echo "  1. Copy your .env file to $INSTALL_DIR/.env"
echo "  2. Copy config files to $INSTALL_DIR/config/"
echo "  3. Copy secrets to $INSTALL_DIR/secrets/"
echo "  4. Start the service: systemctl start $BINARY_NAME"
echo "  5. Enable on boot: systemctl enable $BINARY_NAME"
