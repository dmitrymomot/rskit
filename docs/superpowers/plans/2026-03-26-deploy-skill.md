# Deploy Skill Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a modo plugin skill at `skills/deploy/` that scaffolds production-ready VPS deployment infrastructure into a modo project — Dockerfile, Swarm stack, CI/CD workflow, bootstrap script, Caddy config, and Litestream config.

**Architecture:** Interactive skill using `AskUserQuestion` to gather deployment parameters (app name, domain, TLS pattern, port, DB names, S3 backup config), then generates files via bash scripts for static boilerplate and `Write` for dynamic content. Follows the same pattern as the existing `skills/init/` skill.

**Tech Stack:** Bash scripts, YAML/Dockerfile/Caddyfile templates, GitHub Actions workflow, Docker Swarm, Caddy, Litestream.

---

## File Structure

```
skills/deploy/
├── SKILL.md                        # Skill definition with interactive workflow
├── references/
│   └── templates.md                # All file templates for dynamic generation
└── scripts/
    ├── scaffold-deploy.sh          # Creates deploy/ directory and static files
    └── deploy-workflow.yml         # GitHub Actions workflow (static, copied as-is)
```

**Files generated into the user's project when the skill runs:**

```
<project>/
├── Dockerfile                      # Multi-stage Rust build (or updates existing)
├── stack.yml                       # Docker Swarm stack with zero-downtime config
├── .env.production.example         # Template for VPS runtime env vars
├── .github/workflows/deploy.yml    # Tag-triggered CI/CD to GHCR + SSH deploy
└── deploy/
    ├── bootstrap.sh                # VPS initial setup (Docker, Caddy, Litestream, firewall)
    ├── Caddyfile.example           # Caddy config with the chosen TLS pattern
    └── litestream.yml.example      # Litestream replication config for app DBs
```

---

### Task 1: Create scaffold-deploy.sh

**Files:**
- Create: `skills/deploy/scripts/scaffold-deploy.sh`

This script creates the `deploy/` directory and writes the static bootstrap script. The bootstrap script is the same regardless of app config — it's the VPS-side one-time setup.

- [ ] **Step 1: Create the script**

```bash
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
```

Write this to `skills/deploy/scripts/scaffold-deploy.sh`.

- [ ] **Step 2: Create the GitHub Actions workflow file**

This is a static file copied as-is into projects. Write the following to `skills/deploy/scripts/deploy-workflow.yml`:

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

- [ ] **Step 3: Verify files exist**

Run: `ls -la skills/deploy/scripts/`
Expected: both `scaffold-deploy.sh` and `deploy-workflow.yml` exist

- [ ] **Step 4: Commit**

```bash
git add skills/deploy/scripts/
git commit -m "feat(skill): add deploy scaffold script and CI workflow template"
```

---

### Task 2: Create references/templates.md

**Files:**
- Create: `skills/deploy/references/templates.md`

This file contains all the file templates that the skill assembles dynamically based on user answers. Each template is a fenced code block with placeholder variables that the skill's `Write` tool fills in.

- [ ] **Step 1: Create the templates reference file**

The file must contain these template sections:

1. **Dockerfile template** — Multi-stage Rust build. Placeholders: `{{crate_name}}` for the binary name (from `Cargo.toml` `[[bin]]` or package name). Conditional `COPY` lines for `templates/`, `static/`, `emails/` (only if those directories exist).

2. **stack.yml template** — Docker Swarm stack. Placeholders: `{{image}}` (e.g., `ghcr.io/owner/repo`), `{{port}}` (e.g., `8080`), `{{app_name}}` (e.g., `myapp` — used for data dir path), `{{stack_name}}` for service naming context.

3. **deploy.yml workflow template** — GitHub Actions. Placeholders: `{{swarm_service}}` default value comment.

4. **.env.production.example template** — All env vars the production config needs. Placeholder: `{{app_name}}` for comments.

5. **Caddyfile templates** — Three variants:
   - `single-domain`: Placeholder `{{domain}}`, `{{port}}`
   - `wildcard`: Placeholder `{{domain}}`, `{{port}}`
   - `on-demand`: Placeholder `{{domain}}`, `{{port}}`, `{{verify_endpoint}}`

6. **litestream.yml template** — Placeholders: `{{app_name}}`, `{{db_files}}` (list of DB filenames), `{{s3_endpoint}}`, `{{s3_bucket}}`.

Write the complete file content. Each template section uses a heading (`## Dockerfile`, `## stack.yml`, etc.) and contains one or more fenced code blocks with the complete file content including placeholders.

Here is the exact content to write:

````markdown
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
````

Write this to `skills/deploy/references/templates.md`.

- [ ] **Step 2: Commit**

```bash
git add skills/deploy/references/templates.md
git commit -m "feat(skill): add deploy templates reference with all file templates"
```

---

### Task 3: Create SKILL.md

**Files:**
- Create: `skills/deploy/SKILL.md`

This is the main skill definition — frontmatter + interactive workflow.

- [ ] **Step 1: Create the skill file**

Write the following content to `skills/deploy/SKILL.md`:

```markdown
---
name: modo-deploy
allowed-tools: Read, Write, Edit, Glob, Grep, Bash, AskUserQuestion
description: "Set up production deployment for a modo app — generates Dockerfile, Docker Swarm stack, GitHub Actions CI/CD workflow, VPS bootstrap script, Caddy reverse proxy config, and Litestream SQLite backup config. Use this skill when the user wants to deploy a modo app, set up production infrastructure, configure CI/CD, add Docker support, set up a VPS, configure Caddy, configure Litestream backups, or says things like 'deploy', 'production setup', 'set up VPS', 'add deployment', 'CI/CD', 'zero-downtime', 'docker swarm', or 'deploy to server'."
---

# modo-deploy — Production Deployment Setup

This skill generates production-ready VPS deployment infrastructure for a modo app. It uses `AskUserQuestion` to gather deployment parameters, then creates all necessary files.

**Architecture:** Caddy (systemd) as reverse proxy with automatic TLS. Docker Swarm for zero-downtime app deploys. Litestream (systemd) for continuous SQLite backup to S3. GitHub Actions for CI/CD with tag-triggered deploys to GHCR.

## Prerequisites

Before running this skill, verify:
1. The project has `Cargo.toml` with a modo dependency
2. The project has `config/production.yaml`
3. The project has a health check endpoint (modo's `/_ready` is included by default)

Check these by running:
```bash
test -f Cargo.toml && grep -q "modo" Cargo.toml && echo "OK: modo project" || echo "WARN: not a modo project"
test -f config/production.yaml && echo "OK: production config exists" || echo "WARN: no production config"
```

If either check fails, warn the user but continue — the files can be added later.

## Workflow

### Step 1: Gather Deployment Parameters

Use `AskUserQuestion` to ask these questions. Ask in a single call with multiple questions:

**Question 1 — App name:**
- header: "App Name"
- question: "What is the app name? (Used for data directories, stack name, and service naming)"
- suggestion: Suggest the crate name from `Cargo.toml` `[package] name`
- Free text input

**Question 2 — Domain:**
- header: "Domain"
- question: "What domain will this app be served on?"
- suggestion: "example.com"
- Free text input

**Question 3 — TLS pattern:**
- header: "TLS Pattern"
- question: "Which TLS/domain pattern does this app need?"
- options:
  - **"Single domain"** — One domain, automatic Let's Encrypt cert
  - **"Wildcard subdomains"** — Main domain + `*.domain.com` (requires DNS challenge plugin for Caddy)
  - **"Custom user domains"** — On-demand TLS for user-provided domains (app needs a verify endpoint)
  - **"Wildcard + custom domains"** — Both wildcard subdomains and custom user domains
- multiSelect: false

**Question 4 — Port:**
- header: "Port"
- question: "Which port should the app listen on inside the container? (Must match your production.yaml server.port)"
- suggestion: "8080"
- Free text input

### Step 2: Gather Backup Parameters

Use `AskUserQuestion`:

**Question 1 — S3 backup endpoint:**
- header: "Backup S3 Endpoint"
- question: "S3-compatible endpoint URL for Litestream backups (e.g., https://s3.us-east-1.amazonaws.com, https://s3.eu-central-003.backblazeb2.com)"
- Free text input

**Question 2 — S3 backup bucket:**
- header: "Backup S3 Bucket"
- question: "S3 bucket name for backups"
- suggestion: "backups"
- Free text input

### Step 3: Detect Project Structure

Before generating files, detect what the project has:

```bash
# Detect crate name
grep -m1 '^name' Cargo.toml | sed 's/.*= *"//;s/"//'

# Check for existing directories
test -d templates && echo "HAS_TEMPLATES=true" || echo "HAS_TEMPLATES=false"
test -d emails && echo "HAS_EMAILS=true" || echo "HAS_EMAILS=false"
test -d assets/static && echo "HAS_STATIC=true" || echo "HAS_STATIC=false"

# Check for job config in production.yaml
grep -q "^job:" config/production.yaml 2>/dev/null && echo "HAS_JOBS=true" || echo "HAS_JOBS=false"

# Check for existing deploy files
test -f Dockerfile && echo "DOCKERFILE_EXISTS=true" || echo "DOCKERFILE_EXISTS=false"
test -f stack.yml && echo "STACKFILE_EXISTS=true" || echo "STACKFILE_EXISTS=false"
test -f .github/workflows/deploy.yml && echo "DEPLOY_YML_EXISTS=true" || echo "DEPLOY_YML_EXISTS=false"
```

If any deploy files already exist, warn the user before overwriting:
> "Found existing `Dockerfile` / `stack.yml` / `deploy.yml`. Overwrite? (y/n)"

### Step 4: Generate Files

Read `references/templates.md` for all templates.

#### 4a: Run scaffold script

```bash
bash "<skill-dir>/scripts/scaffold-deploy.sh" "<project_dir>"
```

This creates `deploy/bootstrap.sh`.

#### 4b: Generate Dockerfile

Read the Dockerfile template from `references/templates.md`. Replace:
- `{{crate_name}}` → detected crate name
- `{{port}}` → user's chosen port
- Include/exclude conditional `COPY` lines based on detected directories

Write to `Dockerfile` in the project root.

#### 4c: Generate stack.yml

Read the stack.yml template. Replace:
- `{{image}}` → `ghcr.io/<github-owner>/<repo-name>` (detect from `git remote get-url origin`)
- `{{port}}` → user's chosen port
- `{{app_name}}` → user's app name

Write to `stack.yml` in the project root.

#### 4d: Copy deploy.yml workflow

Copy the static workflow file — no dynamic replacement needed:

```bash
mkdir -p .github/workflows
cp "<skill-dir>/scripts/deploy-workflow.yml" ".github/workflows/deploy.yml"
```

#### 4e: Generate .env.production.example

Read the template. Replace:
- `{{app_name}}` → user's app name
- `{{domain}}` → user's domain
- `{{port}}` → user's chosen port

Write to `.env.production.example` in the project root.

#### 4f: Generate Caddyfile.example

Based on the TLS pattern chosen in Step 1, select the appropriate Caddyfile template from `references/templates.md`:
- "Single domain" → Caddyfile — Single Domain
- "Wildcard subdomains" → Caddyfile — Wildcard Subdomains
- "Custom user domains" → Caddyfile — On-Demand TLS
- "Wildcard + custom domains" → Caddyfile — Wildcard + On-Demand Combined

Replace `{{domain}}` and `{{port}}`.

Write to `deploy/Caddyfile.example`.

#### 4g: Generate litestream.yml.example

Read the template. Build the DB file list:
- Always include `{{app_name}}.db`
- If `HAS_JOBS=true`, also include `{{app_name}}_jobs.db`

Replace `{{app_name}}`, `{{s3_endpoint}}`, `{{s3_bucket}}`, and expand the DB entries.

Write to `deploy/litestream.yml.example`.

### Step 5: Present Results

List every file created with a brief description, then show:

```
Files generated:

  Dockerfile                      — Multi-stage Rust build (cached deps, non-root user)
  stack.yml                       — Docker Swarm stack (zero-downtime, auto-rollback)
  .env.production.example         — Production env vars template
  .github/workflows/deploy.yml    — CI/CD: tag push → build → push to GHCR → SSH deploy
  deploy/bootstrap.sh             — VPS setup: Docker, Caddy, Litestream, firewall
  deploy/Caddyfile.example        — Caddy reverse proxy config
  deploy/litestream.yml.example   — SQLite backup to S3

Deployment workflow:
  1. Set up VPS:     scp deploy/bootstrap.sh root@<vps>: && ssh root@<vps> bash bootstrap.sh
  2. Configure VPS:  Copy deploy/*.example files to VPS, fill in values
  3. First deploy:   git tag v0.1.0 && git push --tags
  4. Rollback:       ssh deploy@<vps> docker service update --image ghcr.io/<owner>/<repo>:<prev-tag> <stack>_app

GitHub secrets to set:
  - VPS_HOST        — Server IP or hostname
  - DEPLOY_SSH_KEY  — SSH private key for deploy user
  - SWARM_SERVICE   — Swarm service name (e.g., <app_name>_app)
```

If the TLS pattern is "Wildcard subdomains" or "Wildcard + custom domains", also mention:
```
Note: Wildcard certs require a custom Caddy build with a DNS challenge plugin.
On the VPS, after running bootstrap.sh:
  go install github.com/caddyserver/xcaddy/cmd/xcaddy@latest
  xcaddy build --with github.com/caddy-dns/cloudflare
  sudo mv caddy /usr/bin/caddy
  sudo systemctl restart caddy
```

If the TLS pattern is "Custom user domains" or "Wildcard + custom domains", also mention:
```
Note: On-demand TLS requires your app to implement a domain verification endpoint.
Caddy will call GET /api/domains/verify?domain=<domain> — return 200 for valid, 404 for unknown.
```

## References

- `references/templates.md` — All file templates (Dockerfile, stack.yml, deploy.yml, Caddyfile, litestream.yml)
- Design spec: `docs/superpowers/specs/2026-03-26-vps-deployment-design.md`
```

Write this to `skills/deploy/SKILL.md`.

- [ ] **Step 2: Commit**

```bash
git add skills/deploy/SKILL.md
git commit -m "feat(skill): add deploy skill definition with interactive workflow"
```

---

### Task 4: Verify skill structure and commit

**Files:**
- Verify: `skills/deploy/SKILL.md`
- Verify: `skills/deploy/references/templates.md`
- Verify: `skills/deploy/scripts/scaffold-deploy.sh`

- [ ] **Step 1: Verify all files exist**

Run: `find skills/deploy -type f | sort`

Expected output:
```
skills/deploy/SKILL.md
skills/deploy/references/templates.md
skills/deploy/scripts/deploy-workflow.yml
skills/deploy/scripts/scaffold-deploy.sh
```

- [ ] **Step 2: Verify SKILL.md frontmatter is valid**

Run: `head -5 skills/deploy/SKILL.md`

Expected: YAML frontmatter with `name: modo-deploy`, `allowed-tools:`, and `description:`.

- [ ] **Step 3: Verify scaffold script is executable**

Run: `test -x skills/deploy/scripts/scaffold-deploy.sh && echo "OK" || echo "NOT EXECUTABLE"`

Expected: `OK`

- [ ] **Step 4: Verify templates reference has all sections**

Run: `grep "^## " skills/deploy/references/templates.md`

Expected output should include:
```
## Dockerfile
## stack.yml
## deploy.yml (GitHub Actions)
## .env.production.example
## Caddyfile — Single Domain
## Caddyfile — Wildcard Subdomains
## Caddyfile — On-Demand TLS (Custom User Domains)
## Caddyfile — Wildcard + On-Demand Combined
## litestream.yml
## litestream .env
```

- [ ] **Step 5: Final commit (if any unstaged changes)**

```bash
git status
# If any unstaged changes:
git add skills/deploy/
git commit -m "feat(skill): complete deploy skill with templates and bootstrap script"
```
