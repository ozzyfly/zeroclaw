#!/usr/bin/env bash
# ZeroClaw VPS Deployment Script
#
# Automates deploying ZeroClaw to a fresh Ubuntu 22.04+ / Debian 12+ VPS.
# This script is idempotent — safe to re-run.
#
# Usage:
#   sudo ./scripts/deploy-vps.sh
#
# Prerequisites:
#   - Root access on an Ubuntu 22.04+ or Debian 12+ VPS
#   - A domain name with DNS A record pointing to this server's IP
#
# What it does:
#   1. Installs Docker and Docker Compose plugin
#   2. Creates a dedicated zeroclaw system user
#   3. Configures UFW firewall (SSH, HTTP, HTTPS only)
#   4. Deploys ZeroClaw production Docker Compose stack
#   5. Installs Nginx reverse proxy with Let's Encrypt TLS
#   6. Installs systemd service for auto-start/restart
#
set -euo pipefail

# ── Colours ──────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }

DEPLOY_DIR="/opt/zeroclaw"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# ── Step 0: Check root and OS ───────────────────────────────────
check_prerequisites() {
    if [[ $EUID -ne 0 ]]; then
        error "This script must be run as root (use sudo)"
        exit 1
    fi

    if [[ -f /etc/os-release ]]; then
        # shellcheck source=/dev/null
        . /etc/os-release
        case "$ID" in
            ubuntu)
                if [[ "${VERSION_ID%%.*}" -lt 22 ]]; then
                    error "Ubuntu 22.04 or later required (found $VERSION_ID)"
                    exit 1
                fi
                ;;
            debian)
                if [[ "${VERSION_ID%%.*}" -lt 12 ]]; then
                    error "Debian 12 or later required (found $VERSION_ID)"
                    exit 1
                fi
                ;;
            *)
                error "Unsupported OS: $ID. This script supports Ubuntu 22.04+ and Debian 12+."
                exit 1
                ;;
        esac
        info "OS check passed: $PRETTY_NAME"
    else
        error "Cannot determine OS version (/etc/os-release not found)"
        exit 1
    fi
}

# ── Step 1: Install Docker ──────────────────────────────────────
install_docker() {
    if command -v docker &>/dev/null && docker compose version &>/dev/null; then
        info "Docker and Docker Compose already installed, skipping"
        return 0
    fi

    info "Installing Docker..."
    apt-get update -qq
    apt-get install -y -qq ca-certificates curl gnupg

    install -m 0755 -d /etc/apt/keyrings
    if [[ ! -f /etc/apt/keyrings/docker.asc ]]; then
        curl -fsSL https://download.docker.com/linux/"$ID"/gpg -o /etc/apt/keyrings/docker.asc
        chmod a+r /etc/apt/keyrings/docker.asc
    fi

    if [[ ! -f /etc/apt/sources.list.d/docker.list ]]; then
        echo \
            "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/$ID \
            $(. /etc/os-release && echo "$VERSION_CODENAME") stable" \
            > /etc/apt/sources.list.d/docker.list
        apt-get update -qq
    fi

    apt-get install -y -qq docker-ce docker-ce-cli containerd.io docker-compose-plugin
    systemctl enable --now docker
    info "Docker installed successfully"
}

# ── Step 2: Create zeroclaw user ─────────────────────────────────
create_user() {
    if id zeroclaw &>/dev/null; then
        info "User 'zeroclaw' already exists, skipping"
    else
        info "Creating system user 'zeroclaw'..."
        useradd --system --create-home --home-dir /home/zeroclaw --shell /usr/sbin/nologin zeroclaw
        info "User 'zeroclaw' created"
    fi

    if groups zeroclaw | grep -q '\bdocker\b'; then
        info "User 'zeroclaw' already in docker group"
    else
        usermod -aG docker zeroclaw
        info "Added 'zeroclaw' to docker group"
    fi
}

# ── Step 3: Configure UFW firewall ───────────────────────────────
configure_firewall() {
    if ! command -v ufw &>/dev/null; then
        info "Installing UFW..."
        apt-get install -y -qq ufw
    fi

    if ufw status | grep -q "Status: active"; then
        info "UFW already active, ensuring rules..."
    else
        info "Configuring UFW firewall..."
    fi

    ufw --force reset >/dev/null 2>&1
    ufw default deny incoming >/dev/null
    ufw default allow outgoing >/dev/null
    ufw allow ssh >/dev/null
    ufw allow http >/dev/null
    ufw allow https >/dev/null
    ufw --force enable >/dev/null
    info "UFW configured: SSH, HTTP, HTTPS allowed"
}

# ── Step 4: Prompt for configuration ─────────────────────────────
prompt_config() {
    echo ""
    echo "─── ZeroClaw Configuration ───"
    echo ""

    if [[ -f "$DEPLOY_DIR/.env" ]]; then
        info "Existing .env found at $DEPLOY_DIR/.env"
        read -rp "Overwrite existing configuration? [y/N]: " overwrite
        if [[ "$overwrite" != "y" && "$overwrite" != "Y" ]]; then
            info "Keeping existing configuration"
            # Source existing config for domain/email
            # shellcheck source=/dev/null
            . "$DEPLOY_DIR/.env"
            DOMAIN="${DOMAIN:-}"
            EMAIL="${EMAIL:-}"
            return 0
        fi
    fi

    read -rp "API Key (required): " API_KEY
    if [[ -z "$API_KEY" ]]; then
        error "API_KEY is required"
        exit 1
    fi

    read -rp "LLM Provider [openrouter]: " PROVIDER
    PROVIDER="${PROVIDER:-openrouter}"

    read -rp "Domain name (e.g., zeroclaw.example.com): " DOMAIN
    if [[ -z "$DOMAIN" ]]; then
        error "Domain name is required"
        exit 1
    fi

    read -rp "Email for Let's Encrypt certificates: " EMAIL
    if [[ -z "$EMAIL" ]]; then
        error "Email is required for Let's Encrypt"
        exit 1
    fi

    read -rp "Model override (leave empty for default): " ZEROCLAW_MODEL

    # Write .env
    mkdir -p "$DEPLOY_DIR"
    cat > "$DEPLOY_DIR/.env" <<EOF
API_KEY=$API_KEY
PROVIDER=$PROVIDER
DOMAIN=$DOMAIN
EMAIL=$EMAIL
ZEROCLAW_MODEL=${ZEROCLAW_MODEL:-}
ZEROCLAW_GATEWAY_PORT=42617
EOF
    chmod 600 "$DEPLOY_DIR/.env"
    chown zeroclaw:docker "$DEPLOY_DIR/.env"
    info "Configuration saved to $DEPLOY_DIR/.env"
}

# ── Step 5: Verify DNS ──────────────────────────────────────────
verify_dns() {
    if ! command -v dig &>/dev/null; then
        apt-get install -y -qq dnsutils
    fi

    local vps_ip
    vps_ip=$(curl -sf https://api.ipify.org || curl -sf https://ifconfig.me || true)

    if [[ -z "$vps_ip" ]]; then
        warn "Could not determine VPS public IP. Skipping DNS check."
        return 0
    fi

    local dns_ip
    dns_ip=$(dig +short "$DOMAIN" | tail -n1)

    if [[ -z "$dns_ip" ]]; then
        error "Domain '$DOMAIN' does not resolve to any IP address."
        error "Please create a DNS A record pointing $DOMAIN to $vps_ip"
        exit 1
    fi

    if [[ "$dns_ip" != "$vps_ip" ]]; then
        error "DNS mismatch: '$DOMAIN' resolves to $dns_ip but this VPS IP is $vps_ip"
        error "Please update your DNS A record to point to $vps_ip"
        exit 1
    fi

    info "DNS verified: $DOMAIN → $vps_ip"
}

# ── Step 6: Copy deployment files ────────────────────────────────
copy_deploy_files() {
    mkdir -p "$DEPLOY_DIR"

    info "Copying deployment files to $DEPLOY_DIR..."

    # Copy from repository deploy/ directory if available, else from script dir
    local source_dir="$SCRIPT_DIR/deploy"
    if [[ ! -d "$source_dir" ]]; then
        error "Deploy directory not found at $source_dir"
        error "Run this script from the zeroclaw repository root"
        exit 1
    fi

    cp "$source_dir/docker-compose.prod.yml" "$DEPLOY_DIR/docker-compose.prod.yml"

    chown -R zeroclaw:docker "$DEPLOY_DIR"
    chmod 755 "$DEPLOY_DIR"
    info "Deployment files copied to $DEPLOY_DIR"
}

# ── Step 7: Install Nginx and Certbot ────────────────────────────
install_nginx() {
    if command -v nginx &>/dev/null; then
        info "Nginx already installed, skipping"
    else
        info "Installing Nginx..."
        apt-get install -y -qq nginx
    fi

    if command -v certbot &>/dev/null; then
        info "Certbot already installed, skipping"
    else
        info "Installing Certbot..."
        apt-get install -y -qq certbot python3-certbot-nginx
    fi
}

# ── Step 8: Configure Nginx ─────────────────────────────────────
configure_nginx() {
    local nginx_conf="/etc/nginx/sites-available/zeroclaw"
    local cert_path="/etc/letsencrypt/live/$DOMAIN/fullchain.pem"

    info "Configuring Nginx reverse proxy for $DOMAIN..."

    # Disable default site
    rm -f /etc/nginx/sites-enabled/default

    if [[ -f "$cert_path" ]]; then
        # Certs exist — install full SSL config from template
        local source_conf="$SCRIPT_DIR/deploy/nginx.conf"
        if [[ ! -f "$source_conf" ]]; then
            error "Nginx config template not found at $source_conf"
            exit 1
        fi
        sed "s/YOUR_DOMAIN/$DOMAIN/g" "$source_conf" > "$nginx_conf"
    else
        # No certs yet — install HTTP-only config so Certbot can verify domain
        cat > "$nginx_conf" <<NGINX_HTTP
server {
    listen 80;
    listen [::]:80;
    server_name $DOMAIN;

    location / {
        proxy_pass http://127.0.0.1:42617;
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;
    }
}
NGINX_HTTP
    fi

    # Enable site
    ln -sf "$nginx_conf" /etc/nginx/sites-enabled/zeroclaw

    # Test config
    if nginx -t 2>/dev/null; then
        systemctl reload nginx
        info "Nginx configured for $DOMAIN"
    else
        error "Nginx configuration test failed"
        nginx -t
        exit 1
    fi
}

# ── Step 9: Obtain TLS certificate ──────────────────────────────
obtain_certificate() {
    local cert_path="/etc/letsencrypt/live/$DOMAIN/fullchain.pem"

    if [[ -f "$cert_path" ]]; then
        info "TLS certificate already exists for $DOMAIN, skipping"
        return 0
    fi

    info "Obtaining Let's Encrypt TLS certificate for $DOMAIN..."
    certbot --nginx \
        -d "$DOMAIN" \
        --non-interactive \
        --agree-tos \
        -m "$EMAIL" \
        --redirect

    if [[ -f "$cert_path" ]]; then
        info "TLS certificate obtained for $DOMAIN"

        # Now install the full SSL config from template
        local nginx_conf="/etc/nginx/sites-available/zeroclaw"
        local source_conf="$SCRIPT_DIR/deploy/nginx.conf"
        if [[ -f "$source_conf" ]]; then
            sed "s/YOUR_DOMAIN/$DOMAIN/g" "$source_conf" > "$nginx_conf"
            nginx -t 2>/dev/null && systemctl reload nginx
            info "Nginx upgraded to full SSL configuration"
        fi
    else
        error "Failed to obtain TLS certificate"
        error "Check that port 80 is accessible and DNS is correct"
        exit 1
    fi
}

# ── Step 10: Install systemd service ─────────────────────────────
install_service() {
    local service_src="$SCRIPT_DIR/deploy/zeroclaw.service"
    local service_dst="/etc/systemd/system/zeroclaw.service"

    if [[ ! -f "$service_src" ]]; then
        error "Service unit file not found at $service_src"
        exit 1
    fi

    info "Installing systemd service..."
    cp "$service_src" "$service_dst"
    systemctl daemon-reload
    systemctl enable zeroclaw
    info "Systemd service installed and enabled"
}

# ── Step 11: Start ZeroClaw ──────────────────────────────────────
start_zeroclaw() {
    info "Starting ZeroClaw..."
    systemctl start zeroclaw

    # Wait for health check
    info "Waiting for ZeroClaw to become healthy..."
    local max_wait=60
    local waited=0
    while [[ $waited -lt $max_wait ]]; do
        if curl -sf "http://127.0.0.1:42617/health" >/dev/null 2>&1; then
            info "ZeroClaw is healthy!"
            break
        fi
        sleep 2
        waited=$((waited + 2))
    done

    if [[ $waited -ge $max_wait ]]; then
        warn "Health check did not pass within ${max_wait}s"
        warn "Check logs with: docker compose -f $DEPLOY_DIR/docker-compose.prod.yml logs"
    fi
}

# ── Step 12: Verify deployment ───────────────────────────────────
verify_deployment() {
    echo ""
    echo "─── Deployment Summary ───"
    echo ""

    if curl -sf "https://$DOMAIN/health" >/dev/null 2>&1; then
        info "HTTPS endpoint accessible at https://$DOMAIN/health"
    else
        warn "HTTPS endpoint not yet reachable (may need a few moments)"
    fi

    echo ""
    info "ZeroClaw deployed to: https://$DOMAIN"
    info "Service status:       systemctl status zeroclaw"
    info "View logs:            docker compose -f $DEPLOY_DIR/docker-compose.prod.yml logs -f"
    info "Restart:              systemctl restart zeroclaw"
    info "Update:               docker compose -f $DEPLOY_DIR/docker-compose.prod.yml pull && systemctl restart zeroclaw"
    echo ""
}

# ── Main ─────────────────────────────────────────────────────────
main() {
    echo ""
    echo "╔══════════════════════════════════════╗"
    echo "║   ZeroClaw VPS Deployment Script     ║"
    echo "╚══════════════════════════════════════╝"
    echo ""

    check_prerequisites
    install_docker
    create_user
    configure_firewall
    prompt_config
    verify_dns
    copy_deploy_files
    install_nginx
    configure_nginx
    obtain_certificate
    install_service
    start_zeroclaw
    verify_deployment
}

main "$@"
