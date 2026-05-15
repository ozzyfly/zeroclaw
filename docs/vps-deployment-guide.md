# VPS Deployment Guide

Deploy ZeroClaw to a cloud VPS with Docker, Nginx reverse proxy, and automatic HTTPS.

---

## 1. Prerequisites

| Requirement | Minimum |
|---|---|
| CPU | 1 vCPU |
| RAM | 1 GB |
| Disk | 20 GB |
| OS | Ubuntu 22.04+ or Debian 12+ |
| Domain | A domain with DNS A record pointing to your VPS IP |

## 2. VPS Provider Selection

Any provider offering Ubuntu/Debian VPS works. Recommended entry-level plans:

| Provider | Plan | Monthly Cost | Notes |
|---|---|---|---|
| Hetzner | CX22 | ~€4 | 2 vCPU, 4 GB RAM, EU/US regions |
| DigitalOcean | Basic Droplet | ~$6 | 1 vCPU, 1 GB RAM, global regions |
| Linode (Akamai) | Nanode 1GB | ~$5 | 1 vCPU, 1 GB RAM |
| Vultr | Cloud Compute | ~$6 | 1 vCPU, 1 GB RAM |

Choose a region close to your users for lowest latency.

## 3. DNS Setup

1. Get your VPS public IP address from your provider's dashboard
2. Go to your domain registrar or DNS provider
3. Create an **A record**:
   - **Name**: your subdomain (e.g., `zeroclaw`) or `@` for root domain
   - **Value**: your VPS IP address (e.g., `203.0.113.42`)
   - **TTL**: 300 (5 minutes)
4. Wait for DNS propagation (usually 1–5 minutes)
5. Verify: `dig +short zeroclaw.example.com` should return your VPS IP

## 4. Deploy

### Option A: One-command deploy (recommended)

SSH into your VPS and run:

```bash
git clone https://github.com/zeroclaw-labs/zeroclaw.git
cd zeroclaw
sudo ./scripts/deploy-vps.sh
```

The script will interactively prompt for:
- **API Key** — your LLM provider API key (required)
- **Provider** — LLM provider name (default: `openrouter`)
- **Domain** — your domain name (e.g., `zeroclaw.example.com`)
- **Email** — for Let's Encrypt certificate notifications

The script is idempotent — safe to re-run if interrupted or to update configuration.

### Option B: Manual deployment

If you prefer manual control, see the individual files in `deploy/`:
- `deploy/docker-compose.prod.yml` — production Docker Compose config
- `deploy/nginx.conf` — Nginx reverse proxy template
- `deploy/zeroclaw.service` — systemd service unit
- `deploy/.env.example` — environment variable template

## 5. Verify Deployment

After the script completes:

```bash
# Check service status
systemctl status zeroclaw

# Check container health
docker compose -f /opt/zeroclaw/docker-compose.prod.yml ps

# View logs
docker compose -f /opt/zeroclaw/docker-compose.prod.yml logs -f

# Test HTTPS endpoint
curl https://YOUR_DOMAIN/health

# Test gateway status
curl https://YOUR_DOMAIN/api/status
```

## 6. Update ZeroClaw

To update to the latest version:

```bash
# Pull latest image and restart
docker compose -f /opt/zeroclaw/docker-compose.prod.yml pull
sudo systemctl restart zeroclaw
```

Or in one command:

```bash
sudo systemctl reload zeroclaw
```

Data is preserved in the `zeroclaw-data` Docker volume — updates are non-destructive.

## 7. Backup and Restore

### Backup

```bash
# Backup data volume to a tar archive
docker run --rm \
  -v zeroclaw-data:/data:ro \
  -v "$(pwd)":/backup \
  alpine tar czf /backup/zeroclaw-backup-$(date +%Y%m%d).tar.gz -C /data .
```

### Restore

```bash
# Stop ZeroClaw first
sudo systemctl stop zeroclaw

# Restore from backup
docker run --rm \
  -v zeroclaw-data:/data \
  -v "$(pwd)":/backup \
  alpine sh -c "rm -rf /data/* && tar xzf /backup/zeroclaw-backup-YYYYMMDD.tar.gz -C /data"

# Start ZeroClaw
sudo systemctl start zeroclaw
```

### Scheduled backups (optional)

Add a cron job for daily backups:

```bash
# Edit crontab
sudo crontab -e

# Add daily backup at 3 AM
0 3 * * * docker run --rm -v zeroclaw-data:/data:ro -v /opt/zeroclaw/backups:/backup alpine tar czf /backup/zeroclaw-$(date +\%Y\%m\%d).tar.gz -C /data .
```

## 8. Troubleshooting

### Port conflicts

```bash
# Check what's using port 42617
sudo ss -tlnp | grep 42617

# Check what's using port 80/443
sudo ss -tlnp | grep -E ':80|:443'
```

If another service uses port 80/443, stop it before deploying:
```bash
sudo systemctl stop apache2  # if Apache is running
```

### Certificate renewal fails

Let's Encrypt certificates auto-renew via certbot's systemd timer. To check:

```bash
# Check renewal timer
sudo systemctl status certbot.timer

# Test renewal
sudo certbot renew --dry-run

# Force renewal
sudo certbot renew --force-renewal
```

**Rate limits**: Let's Encrypt allows 5 duplicate certificates per week. For testing, use the staging environment:
```bash
sudo certbot --nginx -d YOUR_DOMAIN --staging
```

### Firewall rules

```bash
# Check UFW status and rules
sudo ufw status verbose

# If using a cloud provider firewall (e.g., DigitalOcean, Hetzner)
# ensure ports 22, 80, 443 are also allowed in the provider's dashboard
```

**Note**: Cloud provider firewalls are applied *before* UFW. You need both the provider firewall and UFW to allow traffic.

### Container won't start

```bash
# Check container logs
docker compose -f /opt/zeroclaw/docker-compose.prod.yml logs

# Check if image was pulled
docker images | grep zeroclaw

# Check .env file
cat /opt/zeroclaw/.env

# Verify API key is set
grep API_KEY /opt/zeroclaw/.env
```

### Service issues

```bash
# Check systemd service logs
journalctl -u zeroclaw -n 50 --no-pager

# Restart service
sudo systemctl restart zeroclaw

# Check Docker daemon
sudo systemctl status docker
```

## 9. Architecture Notes

This deployment uses **Docker Compose** rather than a bare-metal installation:

- **No Rust toolchain needed** on the VPS — the pre-built distroless image is used
- **Isolation** — ZeroClaw runs in an unprivileged container
- **Reproducibility** — same image runs everywhere
- **Simple updates** — `docker compose pull` fetches the latest version
- **Low resource usage** — distroless image has minimal footprint

The stack:
```
Internet → Nginx (TLS termination, port 443) → ZeroClaw container (port 42617)
```

Nginx handles HTTPS/TLS and WebSocket upgrades. The ZeroClaw container only binds to localhost (127.0.0.1), never exposed directly to the internet.
