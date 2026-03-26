# VPS Deployment Design — Zero-Downtime with Docker Swarm + Caddy + Litestream

Production-ready deployment infrastructure for modo-based micro-SaaS apps on a single VPS. No PaaS. Zero-downtime deploys. Survives reboots.

## Architecture Overview

```
VPS (single-node Docker Swarm)
|
+-- systemd
|   +-- caddy.service        (reverse proxy, TLS termination, ports 80/443)
|   +-- litestream.service   (continuous SQLite replication to S3)
|
+-- Docker Swarm
|   +-- Stack: "myapp"       (app service, port 127.0.0.1:8080)
|   +-- Stack: "otherapp"    (app service, port 127.0.0.1:8081)
|   +-- ...
|
+-- /etc/caddy/Caddyfile     (routing config)
+-- /etc/caddy/.env          (Caddy env vars, e.g. CLOUDFLARE_API_TOKEN)
+-- /etc/litestream.yml      (DB replication config)
+-- /etc/litestream/.env     (S3 credentials)
+-- /data/<app>/             (SQLite DBs + .env.production per app)
+-- /data/caddy/             (certs, state)
```

### Design Decisions

- **Caddy + Litestream on host (systemd), apps in Docker Swarm.** Both are single Go binaries that change rarely. systemd gives boot survival and auto-restart with less indirection than containerizing them. Docker Swarm handles what it's good at: zero-downtime rolling updates for app containers.
- **One Swarm stack per app.** Each app is independently deployable. `docker stack deploy myapp` doesn't touch other apps.
- **App ports bound to 127.0.0.1.** Only Caddy can reach them. Not exposed to the internet.
- **Host volumes for data.** SQLite DBs at `/data/<app>/`. Bind-mounted into containers. No Docker named volumes — direct paths for clarity and backup simplicity.
- **.env files for secrets.** Plain files on the VPS at `/data/<app>/.env.production`. Simple, good enough for single-VPS. Docker Swarm secrets add ceremony without meaningful benefit when there's no multi-node trust boundary.
- **Centralized Litestream.** One process replicates all DBs across all apps. Adding a new app means adding its DBs to `/etc/litestream.yml` and restarting Litestream — same time you're updating Caddy anyway. Brief restart gap is harmless; WAL catches up automatically.

## CI/CD Pipeline

**Trigger:** Push a git tag matching `v*` (e.g., `v1.2.3`).

**Flow:** Tag push -> GitHub Actions builds Docker image -> pushes to GHCR with matching tag -> SSH to VPS -> pull image -> `docker service update`.

**Registry:** GitHub Container Registry (GHCR). Free for the start. Switchable to self-hosted registry later — only change is the image URL in the stack file and CI push target.

### GitHub Actions Workflow

One workflow per app repo:

```yaml
name: Deploy

on:
  push:
    tags: ["v*"]

env:
  REGISTRY: ghcr.io
  IMAGE_NAME: ${{ github.repository }}

jobs:
  build-and-deploy:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      packages: write

    steps:
      - uses: actions/checkout@v4

      - uses: docker/login-action@v3
        with:
          registry: ${{ env.REGISTRY }}
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - uses: docker/metadata-action@v5
        id: meta
        with:
          images: ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}
          tags: |
            type=semver,pattern={{version}}
            type=semver,pattern={{major}}.{{minor}}

      - uses: docker/build-push-action@v6
        with:
          context: .
          push: true
          tags: ${{ steps.meta.outputs.tags }}
          cache-from: type=gha
          cache-to: type=gha,mode=max

      - name: Deploy to VPS
        run: |
          echo "${{ secrets.DEPLOY_SSH_KEY }}" > /tmp/deploy_key
          chmod 600 /tmp/deploy_key
          ssh -o StrictHostKeyChecking=no -i /tmp/deploy_key deploy@${{ secrets.VPS_HOST }} \
            "docker pull ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}:${{ steps.meta.outputs.version }} && \
             docker service update --image ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}:${{ steps.meta.outputs.version }} ${{ secrets.SWARM_SERVICE }}"
          rm /tmp/deploy_key
```

### Secrets Per Repo

- `VPS_HOST` — server IP or hostname
- `DEPLOY_SSH_KEY` — shared private key (one key across all repos)
- `SWARM_SERVICE` — Swarm service name (e.g., `myapp_app` — follows `<stack-name>_<service-name>` convention)
- `GITHUB_TOKEN` — automatic, no setup needed

### Rollback

Same SSH command with a previous tag:

```bash
ssh deploy@vps "docker service update --image ghcr.io/you/app:1.1.0 myapp_app"
```

Instant — no rebuild. Swarm pulls the cached image.

## Dockerfile

Multi-stage build with dependency caching, non-root user, health check support:

```dockerfile
FROM rust:1.92-slim AS builder

WORKDIR /app

# Cache dependencies
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release && rm -rf src

COPY src/ src/
RUN touch src/main.rs && cargo build --release

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -r -s /bin/false app

WORKDIR /app
RUN mkdir -p /app/data && chown app:app /app/data

COPY --from=builder /app/target/release/app /app/server
COPY config/ /app/config/
COPY templates/ /app/templates/
COPY static/ /app/static/
COPY emails/ /app/emails/
COPY migrations/ /app/migrations/

USER app

ENV APP_ENV=production
EXPOSE 8080

CMD ["/app/server"]
```

### Key details

- **Dependency caching layer:** `Cargo.toml` + `Cargo.lock` copied first, dummy `main.rs` builds deps. Source changes don't invalidate the dep cache.
- **`curl` included:** Required by Swarm health checks.
- **Non-root user:** Runs as `app` user.
- **Binary named `server`:** Runs as `/app/server`.

## Swarm Stack File

Lives in each app's repo as `stack.yml`:

```yaml
services:
  app:
    image: ghcr.io/you/myapp:latest
    ports:
      - "127.0.0.1:8080:8080"
    volumes:
      - /data/myapp:/app/data
    env_file:
      - /data/myapp/.env.production
    deploy:
      replicas: 1
      update_config:
        order: start-first
        failure_action: rollback
      rollback_config:
        order: start-first
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/_ready"]
      interval: 5s
      timeout: 3s
      retries: 3
      start_period: 10s
```

### Zero-Downtime Mechanism

`order: start-first` is the critical setting. With 1 replica, Swarm:

1. Starts a new container with the new image
2. Waits for it to pass the health check (`/_ready` — modo's built-in readiness endpoint that checks DB pool connectivity)
3. Routes traffic to the new container
4. Sends SIGTERM to the old container (modo's `run!` macro handles graceful shutdown)
5. Old container drains in-flight requests and exits

`failure_action: rollback` — if the new container fails health checks, Swarm automatically rolls back to the previous image.

### SQLite During Rolling Update

`start-first` briefly runs two containers with access to the same SQLite volume. At micro-SaaS traffic levels this is safe — the old container handles zero or near-zero requests during the overlap window since Swarm routes new traffic to the healthy new container immediately.

### Port Allocation Convention

Each app gets a unique port on `127.0.0.1`:
- `myapp` -> `8080`
- `otherapp` -> `8081`
- `thirdapp` -> `8082`

## Caddy Configuration

Three TLS patterns depending on the app's needs. All coexist in one Caddyfile.

### Pattern 1: Single Domain

```caddy
myapp.com {
    reverse_proxy 127.0.0.1:8080
}
```

### Pattern 2: Main Domain + Wildcard Subdomains

Requires DNS challenge plugin (e.g., `caddy-dns/cloudflare`). Stock Caddy from apt must be replaced with a custom build via `xcaddy build --with github.com/caddy-dns/cloudflare`.

```caddy
myapp.com {
    reverse_proxy 127.0.0.1:8080
}

*.myapp.com {
    tls {
        dns cloudflare {env.CLOUDFLARE_API_TOKEN}
    }
    reverse_proxy 127.0.0.1:8080
}
```

### Pattern 3: Custom User Domains (On-Demand TLS)

```caddy
{
    on_demand_tls {
        ask http://127.0.0.1:8080/api/domains/verify
    }
}

https:// {
    tls {
        on_demand
    }
    reverse_proxy 127.0.0.1:8080
}
```

The app's `/api/domains/verify` endpoint returns `200` if the domain is valid, `404` if not. Caddy only provisions a cert for approved domains.

### Combined Example (Multiple Apps)

```caddy
{
    on_demand_tls {
        ask http://127.0.0.1:8080/api/domains/verify
    }
}

# App 1: main domain + wildcard + custom domains
myapp.com {
    reverse_proxy 127.0.0.1:8080
}

*.myapp.com {
    tls {
        dns cloudflare {env.CLOUDFLARE_API_TOKEN}
    }
    reverse_proxy 127.0.0.1:8080
}

# App 2: single domain
otherapp.com {
    reverse_proxy 127.0.0.1:8081
}
```

### Environment Variables

Caddy reads env vars from a systemd override:

```bash
systemctl edit caddy
```

```ini
[Service]
EnvironmentFile=/etc/caddy/.env
```

`/etc/caddy/.env`:
```
CLOUDFLARE_API_TOKEN=your-token-here
```

## Litestream Configuration

`/etc/litestream.yml`:

```yaml
access-key-id: ${LITESTREAM_ACCESS_KEY_ID}
secret-access-key: ${LITESTREAM_SECRET_ACCESS_KEY}

dbs:
  - path: /data/myapp/app.db
    replicas:
      - type: s3
        bucket: backups
        path: myapp/app.db
        endpoint: https://s3.provider.com

  - path: /data/myapp/jobs.db
    replicas:
      - type: s3
        bucket: backups
        path: myapp/jobs.db
        endpoint: https://s3.provider.com
```

S3 credentials in systemd override:

```bash
systemctl edit litestream
```

```ini
[Service]
EnvironmentFile=/etc/litestream/.env
```

`/etc/litestream/.env`:
```
LITESTREAM_ACCESS_KEY_ID=your-key
LITESTREAM_SECRET_ACCESS_KEY=your-secret
```

### Adding a New App's DBs

1. Add DB entries to `/etc/litestream.yml`
2. `systemctl restart litestream`
3. Litestream catches up on any writes that happened during restart via WAL — no data loss

### S3 Provider

Any S3-compatible endpoint. Isolated from the app's S3 storage (different provider/account). Configured via env vars — provider-agnostic.

## VPS Bootstrap Script

Run once on a fresh Debian/Ubuntu VPS as root:

```bash
#!/usr/bin/env bash
set -euo pipefail

# --- System ---
apt-get update && apt-get upgrade -y
apt-get install -y curl ufw

# --- Firewall ---
ufw default deny incoming
ufw default allow outgoing
ufw allow 22/tcp
ufw allow 80/tcp
ufw allow 443/tcp
ufw --force enable

# --- Deploy user ---
useradd -m -s /bin/bash deploy
mkdir -p /home/deploy/.ssh
cp ~/.ssh/authorized_keys /home/deploy/.ssh/authorized_keys
chown -R deploy:deploy /home/deploy/.ssh

# --- Docker ---
curl -fsSL https://get.docker.com | sh
usermod -aG docker deploy
systemctl enable docker
docker swarm init

# --- Caddy ---
apt-get install -y debian-keyring debian-archive-keyring apt-transport-https
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' \
  | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' \
  | tee /etc/apt/sources.list.d/caddy-stable.list
apt-get update && apt-get install -y caddy
systemctl enable caddy

# --- Litestream ---
LITESTREAM_VERSION=0.3.13
curl -fsSL "https://github.com/benbjohnson/litestream/releases/download/v${LITESTREAM_VERSION}/litestream-v${LITESTREAM_VERSION}-linux-amd64.deb" \
  -o /tmp/litestream.deb
dpkg -i /tmp/litestream.deb && rm /tmp/litestream.deb
systemctl enable litestream

# --- Data directories ---
mkdir -p /data/caddy
mkdir -p /etc/caddy
mkdir -p /etc/litestream

echo ""
echo "Bootstrap complete. Next steps:"
echo "  1. Place Caddyfile at /etc/caddy/Caddyfile"
echo "  2. Place litestream.yml at /etc/litestream.yml"
echo "  3. Create /etc/caddy/.env (if using DNS challenge)"
echo "  4. Create /etc/litestream/.env with S3 credentials"
echo "  5. Create /data/<app>/.env.production per app"
echo "  6. Run: su - deploy -c 'docker login ghcr.io'"
echo "  7. Deploy: docker stack deploy -c stack.yml <app>"
```

For wildcard certs, replace stock Caddy with custom build after install:

```bash
go install github.com/caddyserver/xcaddy/cmd/xcaddy@latest
xcaddy build --with github.com/caddy-dns/cloudflare
mv caddy /usr/bin/caddy
systemctl restart caddy
```

## Disaster Recovery

Full restore procedure for a dead VPS:

1. Provision new VPS, run bootstrap script
2. Restore each DB: `litestream restore -o /data/myapp/app.db s3://backups/myapp/app.db`
3. Place `.env.production` files (from your secrets management — 1Password, etc.)
4. Place Caddyfile, litestream.yml, env files
5. `docker login ghcr.io` as deploy user
6. `docker stack deploy -c stack.yml myapp` for each app
7. Update DNS to new VPS IP
8. Caddy re-provisions TLS certs automatically

## Adding a New App — Complete Checklist

1. Create `stack.yml` and `Dockerfile` in the app repo (copy from template)
2. Add GitHub Actions workflow (copy from template)
3. Set `VPS_HOST` and `DEPLOY_SSH_KEY` secrets in the new repo
4. On VPS: `mkdir -p /data/newapp` and place `.env.production`
5. On VPS: add Caddy route to `/etc/caddy/Caddyfile`, run `caddy reload --config /etc/caddy/Caddyfile`
6. On VPS: add DB paths to `/etc/litestream.yml`, run `systemctl restart litestream`
7. Push first tag — CI builds, pushes, deploys

## Boot Survival

- Docker daemon: enabled via systemd (`systemctl enable docker`) — starts on boot
- Docker Swarm: reconciles all service states after Docker starts — all app containers come up automatically
- Caddy: enabled via systemd — starts on boot, resumes TLS cert management
- Litestream: enabled via systemd — starts on boot, resumes replication from WAL

No manual intervention needed after a reboot.
