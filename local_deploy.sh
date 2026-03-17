#!/usr/bin/env bash
set -e

# Deploy CAN to TCP Gateway HA Add-on to a Home Assistant device.
#
# Usage:
#   ./local_deploy.sh [user@host]
#
# Default target: root@192.168.46.222
#
# This script:
# 1. Assembles a self-contained addon directory in /tmp/can_tcp_gateway_addon/
# 2. Cleans and copies it to /addons/can_tcp_gateway/ on the HA device via scp
# 3. Prints instructions for installing/rebuilding in HA

TARGET="${1:-root@192.168.46.222}"
ADDON_NAME="can_tcp_gateway"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR"
ADDON_DIR="$PROJECT_DIR/can2tcp"
BUILD_DIR="/tmp/${ADDON_NAME}_addon"

echo "=== Deploying $ADDON_NAME to $TARGET ==="
echo "Project dir: $PROJECT_DIR"
echo ""

# 1. Assemble the addon directory
rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR"

# Copy HA addon files, stripping the 'image:' line so HA builds locally
sed '/^image:/d' "$ADDON_DIR/config.yaml" > "$BUILD_DIR/config.yaml"
cp "$ADDON_DIR/Dockerfile" "$BUILD_DIR/"
cp "$ADDON_DIR/run.sh" "$BUILD_DIR/"

# Copy source code (excluding __pycache__ and Rust target/)
cp -r "$PROJECT_DIR/src" "$BUILD_DIR/src"
find "$BUILD_DIR/src" -type d -name '__pycache__' -exec rm -rf {} +
rm -rf "$BUILD_DIR/src/rust/target"

echo "Assembled addon in $BUILD_DIR:"
ls -la "$BUILD_DIR/"
echo ""
echo "Source files (Python):"
ls -la "$BUILD_DIR/src/python/"
echo ""
echo "Source files (Rust):"
ls -la "$BUILD_DIR/src/rust/src/"
echo ""

# 2. Clean and copy to HA device
echo "Cleaning and copying to $TARGET:/addons/$ADDON_NAME/ ..."
ssh "$TARGET" "rm -rf /addons/$ADDON_NAME && mkdir -p /addons/$ADDON_NAME"
scp -r "$BUILD_DIR/"* "$TARGET:/addons/$ADDON_NAME/"

echo ""
echo "=== Deploy complete ==="
echo ""
echo "Next steps on Home Assistant:"
echo "  1. Go to Settings → Add-ons → Add-on Store"
echo "  2. Click ⋮ (top right) → Check for updates / Reload"
echo "  3. Find 'CAN to TCP Gateway' in the Local add-ons section"
echo "  4. Click Install (first time) or Rebuild (update)"
echo "  5. Configure CAN interface and port settings"
echo "  6. Start the add-on and check logs"
echo ""
echo "Or via CLI on the HA device:"
echo "  ha store reload && ha addons install local_$ADDON_NAME   # first time"
echo "  ha addons rebuild local_$ADDON_NAME                      # update"
echo "  ha addons start local_$ADDON_NAME"
echo "  ha addons logs local_$ADDON_NAME"
