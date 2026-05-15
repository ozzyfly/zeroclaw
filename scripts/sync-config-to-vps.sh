#!/usr/bin/env bash
# Sync local ZeroClaw config to VPS
#
# Usage:
#   ./scripts/sync-config-to-vps.sh [server-ip]
#   ./scripts/sync-config-to-vps.sh              # default: 87.99.131.162
#
set -euo pipefail

GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'
info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }

SERVER_IP="${1:-87.99.131.162}"
LOCAL_DIR="$HOME/.zeroclaw"
SSH_OPTS="-o StrictHostKeyChecking=accept-new"

if [[ ! -f "$LOCAL_DIR/config.toml" ]]; then
    error "Local config not found: $LOCAL_DIR/config.toml"
    exit 1
fi

info "Syncing local config to VPS ($SERVER_IP)..."

# Adapt config paths for VPS container
TMP_DIR=$(mktemp -d)
sed \
    -e 's|/Users/[^"]*/.zeroclaw|/zeroclaw-data/.zeroclaw|g' \
    -e 's|/Users/[^"]*open-skills|/zeroclaw-data/.zeroclaw/active-skills|g' \
    "$LOCAL_DIR/config.toml" > "$TMP_DIR/config.toml"

# Adjust gateway for VPS
if grep -q '^port = 3000$' "$TMP_DIR/config.toml"; then
    sed -i '' 's/^port = 3000$/port = 42617/' "$TMP_DIR/config.toml"
fi
sed -i '' 's/^host = "127.0.0.1"$/host = "[::]"/' "$TMP_DIR/config.toml" 2>/dev/null || true
sed -i '' 's/^allow_public_bind = false$/allow_public_bind = true/' "$TMP_DIR/config.toml" 2>/dev/null || true

# Copy files to VPS
# shellcheck disable=SC2086
scp $SSH_OPTS \
    "$TMP_DIR/config.toml" \
    root@"$SERVER_IP":/tmp/zeroclaw-sync-config.toml

# shellcheck disable=SC2086
scp $SSH_OPTS \
    "$LOCAL_DIR/.secret_key" \
    "$LOCAL_DIR/otp-secret" \
    "$LOCAL_DIR/rss_subscriptions.toml" \
    root@"$SERVER_IP":/tmp/

# Copy cron jobs database if it exists
if [[ -f "$HOME/.zeroclaw/workspace/cron/jobs.db" ]]; then
    info "Syncing cron jobs database..."
    # shellcheck disable=SC2086
    scp $SSH_OPTS \
        "$HOME/.zeroclaw/workspace/cron/jobs.db" \
        root@"$SERVER_IP":/tmp/cron-jobs.db
fi

rm -rf "$TMP_DIR"

# Install on VPS and restart
# shellcheck disable=SC2086
ssh $SSH_OPTS root@"$SERVER_IP" bash <<'REMOTE'
set -euo pipefail
VOL="/var/lib/docker/volumes/zeroclaw_zeroclaw-data/_data/.zeroclaw"

cp /tmp/zeroclaw-sync-config.toml "$VOL/config.toml"
cp /tmp/.secret_key "$VOL/.secret_key"
cp /tmp/otp-secret "$VOL/otp-secret"
cp /tmp/rss_subscriptions.toml "$VOL/rss_subscriptions.toml"

chown 65534:65534 "$VOL/config.toml" "$VOL/.secret_key" "$VOL/otp-secret" "$VOL/rss_subscriptions.toml"
chmod 600 "$VOL/.secret_key" "$VOL/otp-secret" "$VOL/config.toml"

# Install cron jobs database if synced, and fix local paths for container
if [[ -f /tmp/cron-jobs.db ]]; then
    WS="/var/lib/docker/volumes/zeroclaw_zeroclaw-data/_data/workspace/cron"
    mkdir -p "$WS"
    cp /tmp/cron-jobs.db "$WS/jobs.db"
    # Adapt macOS paths to container paths in cron commands
    if command -v sqlite3 &>/dev/null; then
        sqlite3 "$WS/jobs.db" "UPDATE cron_jobs SET command = REPLACE(command, '/Users/', '/zeroclaw-data/.zeroclaw/active-skills/') WHERE command LIKE '%/Users/%/open-skills/%';"
        sqlite3 "$WS/jobs.db" "UPDATE cron_jobs SET command = REPLACE(REPLACE(command, '/Users/', '/zeroclaw-data/'), '/.zeroclaw/active-skills/active-skills/', '/.zeroclaw/active-skills/') WHERE command LIKE '%/Users/%';"
    fi
    chown 65534:65534 "$WS/jobs.db"
    chmod 600 "$WS/jobs.db"
fi

rm -f /tmp/zeroclaw-sync-config.toml /tmp/.secret_key /tmp/otp-secret /tmp/rss_subscriptions.toml /tmp/cron-jobs.db

# Production WhatsApp is intentionally self-chat only.  The VPS guard repairs
# this block if a local config sync would reintroduce group replies or extra
# allowed numbers, before the container is restarted with the synced config.
if [[ -x /opt/zeroclaw/enforce-whatsapp-self-only.py ]]; then
    /opt/zeroclaw/enforce-whatsapp-self-only.py || true
fi

cd /opt/zeroclaw
docker compose -f docker-compose.prod.yml restart
REMOTE

info "Config synced and ZeroClaw restarted!"
