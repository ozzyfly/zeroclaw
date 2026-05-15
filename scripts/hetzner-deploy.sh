#!/usr/bin/env bash
# ZeroClaw Hetzner Cloud Deployment Script
#
# Provisions a Hetzner Cloud server and deploys ZeroClaw automatically.
# Uses the hcloud CLI to create infrastructure, then runs deploy-vps.sh remotely.
#
# Prerequisites:
#   - hcloud CLI installed (brew install hcloud / apt install hcloud-cli)
#   - HCLOUD_TOKEN environment variable set
#   - A domain name with DNS access
#   - An LLM provider API key
#
# Usage:
#   export HCLOUD_TOKEN="<your-token>"
#   ./scripts/hetzner-deploy.sh
#
# This script is idempotent — safe to re-run.
#
set -euo pipefail

# ── Colours ──────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SSH_OPTS="-o StrictHostKeyChecking=accept-new -o ConnectTimeout=5"

# ── Step 0: Prerequisites ───────────────────────────────────────
check_prerequisites() {
    if ! command -v hcloud &>/dev/null; then
        error "hcloud CLI not found. Install it:"
        echo "  macOS:   brew install hcloud"
        echo "  Linux:   apt install hcloud-cli"
        echo "  Manual:  https://github.com/hetznercloud/cli/releases"
        exit 1
    fi

    if [[ -z "${HCLOUD_TOKEN:-}" ]]; then
        error "HCLOUD_TOKEN not set."
        echo "Create an API token at: Hetzner Cloud Console → Security → API Tokens → Generate"
        echo "Then run: export HCLOUD_TOKEN=\"<your-token>\""
        exit 1
    fi

    # Verify token works
    if ! hcloud server list &>/dev/null 2>&1; then
        error "HCLOUD_TOKEN is invalid or expired. Check your token."
        exit 1
    fi

    info "Prerequisites OK"
}

# ── Step 1: Configuration prompts ────────────────────────────────
prompt_config() {
    echo ""
    echo "─── Hetzner Cloud Configuration ───"
    echo ""

    read -rp "Server name [zeroclaw]: " SERVER_NAME
    SERVER_NAME="${SERVER_NAME:-zeroclaw}"

    read -rp "Domain name (e.g., zeroclaw.example.com): " DOMAIN
    if [[ -z "$DOMAIN" ]]; then
        error "Domain name is required"
        exit 1
    fi

    read -rp "API Key for LLM provider (required): " API_KEY
    if [[ -z "$API_KEY" ]]; then
        error "API_KEY is required"
        exit 1
    fi

    read -rp "LLM Provider [openrouter]: " PROVIDER
    PROVIDER="${PROVIDER:-openrouter}"

    read -rp "Email for Let's Encrypt: " EMAIL
    if [[ -z "$EMAIL" ]]; then
        error "Email is required for Let's Encrypt"
        exit 1
    fi

    echo ""
    echo "Available locations: fsn1 (Falkenstein), nbg1 (Nuremberg), hel1 (Helsinki), ash (Ashburn, US)"
    read -rp "Server location [fsn1]: " LOCATION
    LOCATION="${LOCATION:-fsn1}"
}

# ── Step 2: SSH key handling ─────────────────────────────────────
setup_ssh_key() {
    # Find local SSH public key
    if [[ -f "$HOME/.ssh/id_ed25519.pub" ]]; then
        SSH_PUB_KEY="$HOME/.ssh/id_ed25519.pub"
    elif [[ -f "$HOME/.ssh/id_rsa.pub" ]]; then
        SSH_PUB_KEY="$HOME/.ssh/id_rsa.pub"
    else
        info "No SSH key found. Generating ed25519 key..."
        ssh-keygen -t ed25519 -f "$HOME/.ssh/id_ed25519" -N "" -q
        SSH_PUB_KEY="$HOME/.ssh/id_ed25519.pub"
        info "SSH key generated: $SSH_PUB_KEY"
    fi

    SSH_KEY_NAME="${SERVER_NAME}-key"

    # Upload to Hetzner if not already present
    if hcloud ssh-key describe "$SSH_KEY_NAME" &>/dev/null 2>&1; then
        info "SSH key '$SSH_KEY_NAME' already exists on Hetzner, skipping"
    else
        info "Uploading SSH key to Hetzner..."
        hcloud ssh-key create --name "$SSH_KEY_NAME" --public-key-from-file "$SSH_PUB_KEY"
        info "SSH key '$SSH_KEY_NAME' uploaded"
    fi
}

# ── Step 3: Firewall ─────────────────────────────────────────────
setup_firewall() {
    local fw_name="zeroclaw-fw"

    if hcloud firewall describe "$fw_name" &>/dev/null 2>&1; then
        info "Firewall '$fw_name' already exists, skipping"
        return 0
    fi

    info "Creating firewall '$fw_name'..."
    hcloud firewall create --name "$fw_name"

    # Allow SSH
    hcloud firewall add-rule "$fw_name" \
        --direction in --protocol tcp --port 22 \
        --source-ips 0.0.0.0/0 --source-ips ::/0 \
        --description "SSH"

    # Allow HTTP
    hcloud firewall add-rule "$fw_name" \
        --direction in --protocol tcp --port 80 \
        --source-ips 0.0.0.0/0 --source-ips ::/0 \
        --description "HTTP"

    # Allow HTTPS
    hcloud firewall add-rule "$fw_name" \
        --direction in --protocol tcp --port 443 \
        --source-ips 0.0.0.0/0 --source-ips ::/0 \
        --description "HTTPS"

    info "Firewall '$fw_name' created with SSH/HTTP/HTTPS rules"
}

# ── Step 4: Detect server type ────────────────────────────────────
detect_server_type() {
    # Try preferred types in order; not all are available in every location
    local candidates=("cx22" "cpx21" "cx21" "cpx11" "cax11")
    local available
    available=$(hcloud server-type list -o noheader -o columns=name 2>/dev/null || true)

    for candidate in "${candidates[@]}"; do
        if echo "$available" | grep -qw "$candidate"; then
            SERVER_TYPE="$candidate"
            return 0
        fi
    done

    # Fallback: pick first shared type with >= 2 cores
    SERVER_TYPE=$(hcloud server-type list -o noheader -o columns=name,cores 2>/dev/null \
        | awk '$2 >= 2 { print $1; exit }' || true)

    if [[ -z "$SERVER_TYPE" ]]; then
        error "No suitable server type found. Check available types with: hcloud server-type list"
        exit 1
    fi
}

# ── Step 5: Server creation ──────────────────────────────────────
create_server() {
    if hcloud server describe "$SERVER_NAME" &>/dev/null 2>&1; then
        SERVER_IP=$(hcloud server ip "$SERVER_NAME")
        warn "Server '$SERVER_NAME' already exists (IP: $SERVER_IP)"
        read -rp "Deploy to existing server? [Y/n]: " proceed
        if [[ "$proceed" == "n" || "$proceed" == "N" ]]; then
            info "Aborted."
            exit 0
        fi
        return 0
    fi

    detect_server_type
    info "Creating server '$SERVER_NAME' ($SERVER_TYPE, Ubuntu 24.04, $LOCATION)..."
    hcloud server create \
        --name "$SERVER_NAME" \
        --type "$SERVER_TYPE" \
        --image ubuntu-24.04 \
        --location "$LOCATION" \
        --ssh-key "$SSH_KEY_NAME" \
        --firewall zeroclaw-fw

    info "Server '$SERVER_NAME' created"
}

# ── Step 5: Wait for server ──────────────────────────────────────
wait_for_server() {
    info "Waiting for server to be ready..."

    local max_wait=120
    local waited=0
    while [[ $waited -lt $max_wait ]]; do
        local status
        status=$(hcloud server describe "$SERVER_NAME" -o format='{{.Status}}' 2>/dev/null || echo "unknown")
        if [[ "$status" == "running" ]]; then
            break
        fi
        sleep 2
        waited=$((waited + 2))
    done

    if [[ $waited -ge $max_wait ]]; then
        error "Server did not reach 'running' state within ${max_wait}s"
        exit 1
    fi

    SERVER_IP=$(hcloud server ip "$SERVER_NAME")
    info "Server running at $SERVER_IP"
}

# ── Step 6: Wait for SSH ─────────────────────────────────────────
wait_for_ssh() {
    info "Waiting for SSH to become available..."

    local max_wait=60
    local waited=0
    while [[ $waited -lt $max_wait ]]; do
        # shellcheck disable=SC2086
        if ssh $SSH_OPTS root@"$SERVER_IP" echo ok &>/dev/null 2>&1; then
            info "SSH connection established"
            return 0
        fi
        sleep 3
        waited=$((waited + 3))
    done

    error "SSH not available after ${max_wait}s. Check firewall and server status."
    exit 1
}

# ── Step 7: Copy deployment files ────────────────────────────────
copy_files() {
    info "Copying deployment files to server..."

    local deploy_dir="$SCRIPT_DIR/deploy"
    local scripts_dir="$SCRIPT_DIR/scripts"

    if [[ ! -d "$deploy_dir" ]]; then
        error "Deploy directory not found at $deploy_dir"
        error "Ensure the vps-deployment files exist (deploy/docker-compose.prod.yml, etc.)"
        exit 1
    fi

    # Create remote directory
    # shellcheck disable=SC2086
    ssh $SSH_OPTS root@"$SERVER_IP" "mkdir -p /tmp/zeroclaw-deploy"

    # Copy files
    # shellcheck disable=SC2086
    scp $SSH_OPTS \
        "$deploy_dir/docker-compose.prod.yml" \
        "$deploy_dir/nginx.conf" \
        "$deploy_dir/zeroclaw.service" \
        "$deploy_dir/.env.example" \
        "$scripts_dir/deploy-vps.sh" \
        root@"$SERVER_IP":/tmp/zeroclaw-deploy/

    info "Files copied to server"
}

# ── Step 8: Remote deployment ────────────────────────────────────
remote_deploy() {
    info "Starting remote deployment on $SERVER_IP..."

    # shellcheck disable=SC2086,SC2029
    ssh $SSH_OPTS root@"$SERVER_IP" bash -s -- \
        "$API_KEY" "$PROVIDER" "$DOMAIN" "$EMAIL" <<'REMOTE_SCRIPT'
set -euo pipefail

DEPLOY_API_KEY="$1"
DEPLOY_PROVIDER="$2"
DEPLOY_DOMAIN="$3"
DEPLOY_EMAIL="$4"

# Set up repo structure
mkdir -p /opt/zeroclaw-repo/deploy /opt/zeroclaw-repo/scripts
cp /tmp/zeroclaw-deploy/docker-compose.prod.yml /opt/zeroclaw-repo/deploy/
cp /tmp/zeroclaw-deploy/nginx.conf /opt/zeroclaw-repo/deploy/
cp /tmp/zeroclaw-deploy/zeroclaw.service /opt/zeroclaw-repo/deploy/
cp /tmp/zeroclaw-deploy/.env.example /opt/zeroclaw-repo/deploy/
cp /tmp/zeroclaw-deploy/deploy-vps.sh /opt/zeroclaw-repo/scripts/
chmod +x /opt/zeroclaw-repo/scripts/deploy-vps.sh

# Pre-create .env so deploy-vps.sh finds existing config
mkdir -p /opt/zeroclaw
cat > /opt/zeroclaw/.env <<EOF
API_KEY=$DEPLOY_API_KEY
PROVIDER=$DEPLOY_PROVIDER
DOMAIN=$DEPLOY_DOMAIN
EMAIL=$DEPLOY_EMAIL
ZEROCLAW_GATEWAY_PORT=42617
EOF
chmod 600 /opt/zeroclaw/.env

# Run the VPS deployment script
cd /opt/zeroclaw-repo
export DEBIAN_FRONTEND=noninteractive
./scripts/deploy-vps.sh

# Cleanup
rm -rf /tmp/zeroclaw-deploy
REMOTE_SCRIPT

    info "Remote deployment complete"
}

# ── Step 9: Summary ──────────────────────────────────────────────
show_summary() {
    echo ""
    echo "╔══════════════════════════════════════╗"
    echo "║   ZeroClaw Deployed on Hetzner!      ║"
    echo "╚══════════════════════════════════════╝"
    echo ""
    info "Server IP:    $SERVER_IP"
    info "HTTPS URL:    https://$DOMAIN"
    info "SSH access:   ssh root@$SERVER_IP"
    echo ""
    info "Manage:"
    echo "  systemctl status zeroclaw"
    echo "  docker compose -f /opt/zeroclaw/docker-compose.prod.yml logs -f"
    echo ""
    info "Update:"
    echo "  ssh root@$SERVER_IP 'docker compose -f /opt/zeroclaw/docker-compose.prod.yml pull && systemctl restart zeroclaw'"
    echo ""
    info "Destroy:"
    echo "  hcloud server delete $SERVER_NAME"
    echo "  hcloud firewall delete zeroclaw-fw"
    echo "  hcloud ssh-key delete $SSH_KEY_NAME"
    echo ""
    warn "Remember to set your DNS A record: $DOMAIN → $SERVER_IP"
    echo ""
}

# ── Main ─────────────────────────────────────────────────────────
main() {
    echo ""
    echo "╔══════════════════════════════════════╗"
    echo "║  ZeroClaw Hetzner Cloud Deployer     ║"
    echo "╚══════════════════════════════════════╝"
    echo ""

    check_prerequisites
    prompt_config
    setup_ssh_key
    setup_firewall
    create_server
    wait_for_server
    wait_for_ssh
    copy_files
    remote_deploy
    show_summary
}

main "$@"
