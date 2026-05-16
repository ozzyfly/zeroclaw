# Hetzner Cloud Deployment Guide

Deploy ZeroClaw to Hetzner Cloud with a single command using the `hetzner-deploy.sh` script.

---

## 1. Overview

The `hetzner-deploy.sh` script automates the full deployment flow:

1. Creates a Hetzner Cloud server (CX22: 2 vCPU, 4 GB RAM, Ubuntu 24.04)
2. Configures firewall (SSH, HTTP, HTTPS only)
3. Uploads SSH keys for secure access
4. Copies deployment files and runs the VPS deployment script remotely
5. Sets up Docker, Nginx reverse proxy, Let's Encrypt TLS, and systemd service

## 2. Prerequisites

### Hetzner Cloud Account

Sign up at [cloud.hetzner.com](https://cloud.hetzner.com). A credit card or PayPal is required.

### Install hcloud CLI

```bash
# macOS
brew install hcloud

# Ubuntu / Debian
apt install hcloud-cli

# Other platforms
# Download from https://github.com/hetznercloud/cli/releases
```

### Generate API Token

1. Log in to [Hetzner Cloud Console](https://console.hetzner.cloud)
2. Select your project (or create one)
3. Go to **Security** → **API Tokens**
4. Click **Generate API Token**
5. Name: `zeroclaw`, Permissions: **Read & Write**
6. Copy the token (shown only once)

### Other Requirements

- A domain name with access to DNS settings
- An LLM provider API key (OpenRouter, Anthropic, OpenAI, etc.)

## 3. Quick Start

```bash
export HCLOUD_TOKEN="<your-hetzner-api-token>"
cd zeroclaw
./scripts/hetzner-deploy.sh
```

The script will prompt for:

| Prompt | Default | Required |
|---|---|---|
| Server name | `zeroclaw` | No |
| Domain name | — | Yes |
| API Key (LLM) | — | Yes |
| Provider | `openrouter` | No |
| Email (Let's Encrypt) | — | Yes |
| Location | `fsn1` | No |

Available locations: `fsn1` (Falkenstein), `nbg1` (Nuremberg), `hel1` (Helsinki), `ash` (Ashburn, US).

## 4. DNS Setup

After the script outputs the server IP:

1. Go to your DNS provider (Cloudflare, Namecheap, etc.)
2. Create an **A record**:
   - Name: your subdomain (e.g., `zeroclaw`)
   - Value: the server IP from the script output
   - TTL: 300
3. Wait 1–5 minutes for propagation
4. If the script completed before DNS was ready, re-run it — it will detect the existing server and retry TLS certificate issuance

## 5. Verify Deployment

```bash
# Test HTTPS endpoint
curl https://YOUR_DOMAIN/health

# Check service status
ssh root@<server-ip> systemctl status zeroclaw

# View container logs
ssh root@<server-ip> docker compose -f /opt/zeroclaw/docker-compose.prod.yml logs -f
```

## 6. Server Management

### SSH Access

```bash
ssh root@<server-ip>
```

### Create Snapshot (Backup)

```bash
hcloud server create-image --type snapshot --description "zeroclaw-backup-$(date +%Y%m%d)" <server-name>
```

### Resize Server

```bash
# Upgrade to CX32 (4 vCPU, 8 GB RAM)
hcloud server change-type --keep-disk <server-name> cx32
```

Note: Server must be powered off first (`hcloud server poweroff <server-name>`).

### Update ZeroClaw

```bash
ssh root@<server-ip> 'docker compose -f /opt/zeroclaw/docker-compose.prod.yml pull && systemctl restart zeroclaw'
```

## 7. Cost Control

| Resource | Cost |
|---|---|
| CX22 server | ~€4.49/month |
| Snapshots | €0.0119/GB/month |
| Traffic | 20 TB included |
| IPv4 address | Included |
| Firewall | Free |

**Tips:**

- Delete unused snapshots: `hcloud image delete <image-id>`
- Power off when not needed: `hcloud server poweroff <name>` (still billed for disk)
- Delete server completely to stop all charges: `hcloud server delete <name>`

## 8. Cleanup / Destroy

To completely remove ZeroClaw from Hetzner and stop all charges:

```bash
# Delete the server
hcloud server delete <server-name>

# Delete the firewall
hcloud firewall delete zeroclaw-fw

# Delete the SSH key from Hetzner
hcloud ssh-key delete <server-name>-key

# Remove local hcloud context (optional)
hcloud context delete zeroclaw
```

## 9. Troubleshooting

### SSH connection refused

- Server may still be booting — wait 30–60 seconds and retry
- Verify firewall rules: `hcloud firewall describe zeroclaw-fw`
- Check SSH key is attached: `hcloud server describe <name>`

### hcloud authentication error

- Verify token: `hcloud server list` (should list servers without error)
- Token may be expired — regenerate in Hetzner Console → Security → API Tokens
- Check active context: `hcloud context active`

### Server creation quota limit

- New Hetzner accounts are limited to ~5 servers
- Request increase via Hetzner support ticket
- Delete unused servers to free quota: `hcloud server list`

### Deployment fails mid-way

- The script is idempotent — safe to re-run
- Check remote logs: `ssh root@<ip> journalctl -u zeroclaw -n 50`
- Check Docker: `ssh root@<ip> docker compose -f /opt/zeroclaw/docker-compose.prod.yml logs`
- Verify `.env` file: `ssh root@<ip> cat /opt/zeroclaw/.env`
