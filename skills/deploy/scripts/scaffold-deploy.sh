#!/usr/bin/env bash
# Creates the deploy/ directory with VPS bootstrap script.
# Usage: scaffold-deploy.sh <project_dir>
#
# Files created:
#   deploy/bootstrap.sh

set -euo pipefail

PROJECT_DIR="${1:?Usage: scaffold-deploy.sh <project_dir>}"

echo "Scaffolding deploy infrastructure"

# ── Directories ──────────────────────────────────────────────
mkdir -p "$PROJECT_DIR"/deploy

# ── deploy/bootstrap.sh ─────────────────────────────────────
cat > "$PROJECT_DIR/deploy/bootstrap.sh" << 'BASH'
#!/usr/bin/env bash
# VPS Bootstrap Script
# Run once on a fresh Debian/Ubuntu VPS as root.
# Installs Docker (Swarm mode), Caddy, Litestream, configures firewall and deploy user.
#
# Usage: bash bootstrap.sh

set -euo pipefail

echo "==> Updating system..."
apt-get update && apt-get upgrade -y
apt-get install -y curl ufw

echo "==> Configuring firewall..."
ufw default deny incoming
ufw default allow outgoing
ufw allow 22/tcp
ufw allow 80/tcp
ufw allow 443/tcp
ufw --force enable

echo "==> Creating deploy user..."
if ! id deploy &>/dev/null; then
    useradd -m -s /bin/bash deploy
fi
mkdir -p /home/deploy/.ssh
cp ~/.ssh/authorized_keys /home/deploy/.ssh/authorized_keys
chown -R deploy:deploy /home/deploy/.ssh
chmod 700 /home/deploy/.ssh
chmod 600 /home/deploy/.ssh/authorized_keys

echo "==> Installing Docker..."
if ! command -v docker &>/dev/null; then
    curl -fsSL https://get.docker.com | sh
fi
usermod -aG docker deploy
systemctl enable docker

echo "==> Initializing Docker Swarm..."
if ! docker info --format '{{.Swarm.LocalNodeState}}' 2>/dev/null | grep -q active; then
    docker swarm init
fi

echo "==> Installing Caddy..."
if ! command -v caddy &>/dev/null; then
    apt-get install -y debian-keyring debian-archive-keyring apt-transport-https
    curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' \
        | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
    curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' \
        | tee /etc/apt/sources.list.d/caddy-stable.list
    apt-get update && apt-get install -y caddy
fi
systemctl enable caddy

echo "==> Installing Litestream..."
if ! command -v litestream &>/dev/null; then
    LITESTREAM_VERSION=0.3.13
    curl -fsSL "https://github.com/benbjohnson/litestream/releases/download/v${LITESTREAM_VERSION}/litestream-v${LITESTREAM_VERSION}-linux-amd64.deb" \
        -o /tmp/litestream.deb
    dpkg -i /tmp/litestream.deb && rm /tmp/litestream.deb
fi
systemctl enable litestream

echo "==> Creating data directories..."
mkdir -p /data
mkdir -p /etc/caddy
mkdir -p /etc/litestream

echo ""
echo "============================================"
echo "  Bootstrap complete!"
echo "============================================"
echo ""
echo "Next steps:"
echo "  1. Place Caddyfile at /etc/caddy/Caddyfile"
echo "  2. Place litestream.yml at /etc/litestream.yml"
echo "  3. Create /etc/caddy/.env (if using DNS challenge):"
echo "     systemctl edit caddy → add EnvironmentFile=/etc/caddy/.env"
echo "  4. Create /etc/litestream/.env with S3 credentials:"
echo "     systemctl edit litestream → add EnvironmentFile=/etc/litestream/.env"
echo "  5. Create /data/<app>/.env.production per app"
echo "  6. Login to GHCR: su - deploy -c 'docker login ghcr.io'"
echo "  7. Deploy: docker stack deploy -c stack.yml <app>"
BASH

chmod +x "$PROJECT_DIR/deploy/bootstrap.sh"

echo "Deploy scaffold created at $PROJECT_DIR/deploy/"
