#!/bin/bash
# Sound Guard — Pi Zero installer
# Usage: curl -sL https://raw.githubusercontent.com/mhear22/pitraincamera/rewrite/install.sh | bash
set -e

BOLD='\033[1m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
RED='\033[0;31m'
NC='\033[0m'

log()  { echo -e "${GREEN}[sound-guard]${NC} $1"; }
warn() { echo -e "${YELLOW}[sound-guard]${NC} $1"; }
die()  { echo -e "${RED}[sound-guard]${NC} $1"; exit 1; }

# --- Config ---
INSTALL_DIR="${SOUND_GUARD_DIR:-/opt/sound-guard}"
SERVER_URL="${SOUND_GUARD_SERVER:-http://192.168.1.66:3002/api/events}"
THRESHOLD="${SOUND_GUARD_THRESHOLD:--10}"
COOLDOWN="${SOUND_GUARD_COOLDOWN:-5}"

log "Sound Guard installer for Raspberry Pi"
log "Install dir: ${INSTALL_DIR}"
log "Server: ${SERVER_URL}"

# --- Check arch ---
ARCH=$(uname -m)
if [[ "$ARCH" != "armv6l" && "$ARCH" != "armv7l" && "$ARCH" != "aarch64" ]]; then
    warn "Not running on ARM (detected: ${ARCH}). This is meant for Raspberry Pi."
    warn "Continuing anyway..."
fi

# --- Install system deps ---
log "Installing system dependencies..."
sudo apt-get update -qq
sudo apt-get install -y -qq \
    build-essential \
    pkg-config \
    libasound2-dev \
    libssl-dev \
    git \
    curl \
    2>/dev/null || warn "Some packages may have failed (non-critical on some OS versions)"

# --- Install Rust if not present ---
if ! command -v cargo &>/dev/null; then
    log "Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

# --- Install ARM target ---
log "Adding ARM target..."
rustup target add arm-unknown-linux-gnueabihf 2>/dev/null || true

# --- Check for cross-compilation tools ---
if ! command -v arm-linux-gnueabihf-gcc &>/dev/null; then
    log "Installing cross-compilation toolchain..."
    sudo apt-get install -y -qq gcc-arm-linux-gnueabihf 2>/dev/null || {
        warn "Cross-compiler not available. Building natively (slower on Pi Zero but works)."
    }
fi

# --- Clone repo ---
if [[ -d "${INSTALL_DIR}" ]]; then
    log "Updating existing install at ${INSTALL_DIR}..."
    cd "${INSTALL_DIR}"
    git fetch origin rewrite
    git checkout rewrite
    git reset --hard origin/rewrite
else
    log "Cloning repository..."
    git clone -b rewrite https://github.com/mhear22/pitraincamera.git "${INSTALL_DIR}"
    cd "${INSTALL_DIR}"
fi

# --- Build client ---
log "Building Rust client (this takes a few minutes on Pi)..."
cd client-rust

if command -v arm-linux-gnueabihf-gcc &>/dev/null; then
    export TARGET_CC=arm-linux-gnueabihf-gcc
    export CARGO_TARGET_ARM_UNKNOWN_LINUX_GNUEABIHF_LINKER=arm-linux-gnueabihf-gcc
fi

cargo build --release --target arm-unknown-linux-gnueabihf 2>&1 || \
    cargo build --release 2>&1 || die "Build failed"

# Find the binary
if [[ -f "target/arm-unknown-linux-gnueabihf/release/sound-guard-client" ]]; then
    BINARY="target/arm-unknown-linux-gnueabihf/release/sound-guard-client"
else
    BINARY="target/release/sound-guard-client"
fi

sudo cp "${BINARY}" /usr/local/bin/sound-guard-client
sudo chmod +x /usr/local/bin/sound-guard-client
log "Binary installed to /usr/local/bin/sound-guard-client"

# --- Install config ---
log "Writing config..."
sudo tee /etc/sound-guard.env > /dev/null <<EOF
SERVER_URL=${SERVER_URL}
THRESHOLD_DB=${THRESHOLD}
COOLDOWN_SEC=${COOLDOWN}
SAMPLE_RATE=44100
PRE_BUFFER_SEC=1.0
POST_BUFFER_SEC=1.0
EOF

# --- Install systemd service ---
log "Installing systemd service..."
sudo tee /etc/systemd/system/sound-guard.service > /dev/null <<EOF
[Unit]
Description=Sound Guard — Audio Monitor
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
EnvironmentFile=/etc/sound-guard.env
ExecStart=/usr/local/bin/sound-guard-client
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl daemon-reload
sudo systemctl enable sound-guard
sudo systemctl start sound-guard

log "✅ Sound Guard installed and running!"
log ""
log "Commands:"
log "  sudo systemctl status sound-guard   — check status"
log "  sudo journalctl -u sound-guard -f   — live logs"
log "  sudo systemctl restart sound-guard  — restart"
log "  sudo systemctl stop sound-guard     — stop"
log ""
log "Config file: /etc/sound-guard.env"
log "Dashboard: ${SERVER_URL%/api/events}"
log ""
log "Settings can be changed remotely from the dashboard — no Pi restart needed."
