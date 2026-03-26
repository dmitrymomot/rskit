# Deploy Templates Reference

Templates for generating deployment files. Placeholders use `{{name}}` syntax — the skill replaces them with user-provided values via `Write`.

## Dockerfile

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

COPY --from=builder /app/target/release/{{crate_name}} /app/server
COPY config/ /app/config/
COPY migrations/ /app/migrations/
{{#if has_templates}}
COPY templates/ /app/templates/
COPY assets/static/ /app/assets/static/
{{/if}}
{{#if has_emails}}
COPY emails/ /app/emails/
{{/if}}

USER app

ENV APP_ENV=production
EXPOSE {{port}}

CMD ["/app/server"]
```

**Assembly rules:**
- Read `Cargo.toml` to find the crate/binary name for `{{crate_name}}`
- Check if `templates/` directory exists → include templates COPY lines
- Check if `assets/static/` directory exists → include static COPY line (same condition as templates)
- Check if `emails/` directory exists → include emails COPY line
- Replace `{{port}}` with the user's chosen port (default `8080`)

## stack.yml

```yaml
services:
  app:
    image: {{image}}:latest
    ports:
      - "127.0.0.1:{{port}}:{{port}}"
    volumes:
      - /data/{{app_name}}:/app/data
    env_file:
      - /data/{{app_name}}/.env.production
    deploy:
      replicas: 1
      update_config:
        order: start-first
        failure_action: rollback
      rollback_config:
        order: start-first
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:{{port}}/_ready"]
      interval: 5s
      timeout: 3s
      retries: 3
      start_period: 10s
```

## deploy.yml (GitHub Actions)

**IMPORTANT:** This template contains GitHub Actions `${{ }}` expressions. Write them literally — do NOT replace or interpret them. They are GitHub Actions syntax, not skill placeholders.

The deploy.yml workflow file content is stored in `skills/deploy/scripts/deploy-workflow.yml` as a static file. The skill copies it directly into the project at `.github/workflows/deploy.yml` using:

```bash
mkdir -p .github/workflows
cp "<skill-dir>/scripts/deploy-workflow.yml" ".github/workflows/deploy.yml"
```

No dynamic replacement needed — the workflow uses GitHub secrets and env vars at runtime.

**GitHub secrets to document:**
- `VPS_HOST` — server IP or hostname
- `DEPLOY_SSH_KEY` — SSH private key for the `deploy` user
- `SWARM_SERVICE` — Swarm service name (`<stack>_app`, e.g., `myapp_app`)

## .env.production.example

```bash
# {{app_name}} — Production Environment
# Copy to VPS at /data/{{app_name}}/.env.production

APP_ENV=production

# Server
PORT={{port}}

# Database (paths relative to /app/data inside container)
DATABASE_PATH=data/{{app_name}}.db
JOB_DATABASE_PATH=data/{{app_name}}_jobs.db

# Cookie signing (min 64 chars — generate with: openssl rand -hex 32)
COOKIE_SECRET=

# JWT secret (min 64 chars — generate with: openssl rand -hex 32)
JWT_SECRET=

# Trusted proxy CIDR (Docker Swarm ingress network)
TRUSTED_PROXY_CIDR=10.0.0.0/8

# App URL (used for CORS, OAuth redirects)
APP_URL=https://{{domain}}

# SMTP
SMTP_HOST=
SMTP_PORT=587
SMTP_USERNAME=
SMTP_PASSWORD=
FROM_EMAIL=noreply@{{domain}}

# S3 storage (for app file uploads, NOT backups)
S3_ENDPOINT=
S3_ACCESS_KEY=
S3_SECRET_KEY=
S3_REGION=auto
S3_BUCKET=

# OAuth (optional)
GITHUB_CLIENT_ID=
GITHUB_CLIENT_SECRET=
GOOGLE_CLIENT_ID=
GOOGLE_CLIENT_SECRET=

# Sentry (optional)
SENTRY_DSN=

# DNS verification (optional)
DNS_NAMESERVER=1.1.1.1:53

# Geolocation (optional)
GEOIP_DB_PATH=data/GeoLite2-City.mmdb
```

**Assembly rules:**
- Replace `{{app_name}}` and `{{domain}}` with user values
- Replace `{{port}}` with chosen port
- Only include sections for components the app actually uses (check `Cargo.toml` features or project structure)

## Caddyfile — Single Domain

```caddy
{{domain}} {
    reverse_proxy 127.0.0.1:{{port}}
}
```

## Caddyfile — Wildcard Subdomains

```caddy
{{domain}} {
    reverse_proxy 127.0.0.1:{{port}}
}

*.{{domain}} {
    tls {
        dns cloudflare {env.CLOUDFLARE_API_TOKEN}
    }
    reverse_proxy 127.0.0.1:{{port}}
}
```

**Note:** Requires custom Caddy build with DNS challenge plugin:
```bash
go install github.com/caddyserver/xcaddy/cmd/xcaddy@latest
xcaddy build --with github.com/caddy-dns/cloudflare
sudo mv caddy /usr/bin/caddy
sudo systemctl restart caddy
```

## Caddyfile — On-Demand TLS (Custom User Domains)

```caddy
{
    on_demand_tls {
        ask http://127.0.0.1:{{port}}/api/domains/verify
    }
}

{{domain}} {
    reverse_proxy 127.0.0.1:{{port}}
}

https:// {
    tls {
        on_demand
    }
    reverse_proxy 127.0.0.1:{{port}}
}
```

**Note:** The app must implement a `GET /api/domains/verify?domain=<domain>` endpoint that returns `200` for valid domains and `404` for unknown ones.

## Caddyfile — Wildcard + On-Demand Combined

```caddy
{
    on_demand_tls {
        ask http://127.0.0.1:{{port}}/api/domains/verify
    }
}

{{domain}} {
    reverse_proxy 127.0.0.1:{{port}}
}

*.{{domain}} {
    tls {
        dns cloudflare {env.CLOUDFLARE_API_TOKEN}
    }
    reverse_proxy 127.0.0.1:{{port}}
}

https:// {
    tls {
        on_demand
    }
    reverse_proxy 127.0.0.1:{{port}}
}
```

## litestream.yml

```yaml
# Litestream replication config for {{app_name}}
# Place at /etc/litestream.yml on VPS
# S3 credentials via systemd EnvironmentFile: /etc/litestream/.env

access-key-id: ${LITESTREAM_ACCESS_KEY_ID}
secret-access-key: ${LITESTREAM_SECRET_ACCESS_KEY}

dbs:
{{#each db_files}}
  - path: /data/{{../app_name}}/{{this}}
    replicas:
      - type: s3
        bucket: {{../s3_bucket}}
        path: {{../app_name}}/{{this}}
        endpoint: {{../s3_endpoint}}
{{/each}}
```

**Assembly rules:**
- Default `db_files`: `["app.db", "jobs.db"]` (if project has job config), or `["app.db"]`
- Replace `{{app_name}}`, `{{s3_bucket}}`, `{{s3_endpoint}}` with user values
- Expand `{{#each db_files}}` into one `- path:` block per DB file

## litestream .env

```bash
# Place at /etc/litestream/.env on VPS
# Then: systemctl edit litestream
# Add: [Service]
#      EnvironmentFile=/etc/litestream/.env

LITESTREAM_ACCESS_KEY_ID=
LITESTREAM_SECRET_ACCESS_KEY=
```
