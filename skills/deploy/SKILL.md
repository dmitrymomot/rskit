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
test -d migrations && echo "HAS_MIGRATIONS=true" || echo "HAS_MIGRATIONS=false"
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

#### 4b: Generate .dockerignore

Read the .dockerignore template from `references/templates.md`. Write to `.dockerignore` in the project root. No dynamic replacement needed.

#### 4c: Generate Dockerfile

Read the Dockerfile template from `references/templates.md`. Replace:
- `{{crate_name}}` → detected crate name
- `{{port}}` → user's chosen port
- Include/exclude conditional `COPY` lines based on detected directories:
  - Include `COPY migrations/` only if `HAS_MIGRATIONS=true`
  - Include `COPY templates/` only if `HAS_TEMPLATES=true`
  - Include `COPY assets/static/` only if `HAS_STATIC=true`
  - Include `COPY emails/` only if `HAS_EMAILS=true`
- Remove any `# CONDITIONAL` comments and their `COPY` lines for directories that don't exist

Write to `Dockerfile` in the project root.

#### 4d: Generate stack.yml

Read the stack.yml template. Replace:
- `{{image}}` → `ghcr.io/<github-owner>/<repo-name>` (detect from `git remote get-url origin`)
- `{{port}}` → user's chosen port
- `{{app_name}}` → user's app name

Write to `stack.yml` in the project root.

#### 4e: Copy deploy.yml workflow

Copy the static workflow file — no dynamic replacement needed:

```bash
mkdir -p .github/workflows
cp "<skill-dir>/scripts/deploy-workflow.yml" ".github/workflows/deploy.yml"
```

#### 4f: Generate .env.production.example

Read the template. Replace:
- `{{app_name}}` → user's app name
- `{{domain}}` → user's domain
- `{{port}}` → user's chosen port

Write to `.env.production.example` in the project root.

#### 4g: Generate Caddyfile.example

Based on the TLS pattern chosen in Step 1, select the appropriate Caddyfile template from `references/templates.md`:
- "Single domain" → Caddyfile — Single Domain
- "Wildcard subdomains" → Caddyfile — Wildcard Subdomains
- "Custom user domains" → Caddyfile — On-Demand TLS
- "Wildcard + custom domains" → Caddyfile — Wildcard + On-Demand Combined

Replace `{{domain}}` and `{{port}}`.

Write to `deploy/Caddyfile.example`.

#### 4h: Generate litestream.yml.example

Read the template. Build the DB entries:
- Always include the `app.db` entry
- If `HAS_JOBS=true`, also include the `jobs.db` entry
- Remove the `# CONDITIONAL` comment and its `- path:` block if jobs are not used

Replace `{{app_name}}`, `{{s3_endpoint}}`, `{{s3_bucket}}`.

Write to `deploy/litestream.yml.example`.

#### 4i: Generate litestream.env.example

Read the litestream .env template from `references/templates.md`. Write to `deploy/litestream.env.example`. No dynamic replacement needed.

#### 4j: Generate caddy.env.example (conditional)

Only if the TLS pattern is "Wildcard subdomains" or "Wildcard + custom domains":
Read the caddy .env template from `references/templates.md`. Write to `deploy/caddy.env.example`. No dynamic replacement needed.

#### 4k: Update .gitignore

Append the following entries to `.gitignore` if not already present:

```
# Deployment secrets
.env.production
deploy/*.env
```

### Step 5: Present Results

List every file created with a brief description, then show:

```
Files generated:

  .dockerignore                   — Docker build context exclusions
  Dockerfile                      — Multi-stage Rust build (cached deps, non-root user)
  stack.yml                       — Docker Swarm stack (zero-downtime, auto-rollback, log rotation)
  .env.production.example         — Production env vars template
  .github/workflows/deploy.yml    — CI/CD: tag push → build → push to GHCR → SSH deploy
  deploy/bootstrap.sh             — VPS setup: Docker, Caddy, Litestream, firewall
  deploy/Caddyfile.example        — Caddy reverse proxy config
  deploy/litestream.yml.example   — SQLite backup to S3
  deploy/litestream.env.example   — Litestream S3 credentials template
  deploy/caddy.env.example        — Caddy env vars (if wildcard TLS)  [conditional]

Deployment workflow:

  First deploy (one-time, on VPS):
    1. Set up VPS:      scp deploy/bootstrap.sh root@<vps>: && ssh root@<vps> bash bootstrap.sh
    2. Configure VPS:   Copy deploy/*.example files to VPS, fill in values
    3. Create app dir:  ssh root@<vps> "mkdir -p /data/<app> && chown deploy:deploy /data/<app>"
    4. Copy stack.yml:  scp stack.yml deploy@<vps>:/data/<app>/stack.yml
    5. Initial deploy:  ssh deploy@<vps> "docker stack deploy -c /data/<app>/stack.yml <app>"

  Subsequent deploys (automatic via CI):
    git tag v0.1.0 && git push --tags

  Rollback:
    ssh deploy@<vps> docker service update --image ghcr.io/<owner>/<repo>:<prev-tag> <app>_app

GitHub secrets to set:
  - VPS_HOST         — Server IP or hostname
  - VPS_KNOWN_HOSTS  — Output of: ssh-keyscan <VPS_HOST>
  - DEPLOY_SSH_KEY   — SSH private key for deploy user
  - SWARM_SERVICE    — Swarm service name (e.g., <app_name>_app)
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

- `references/templates.md` — All file templates (.dockerignore, Dockerfile, stack.yml, deploy.yml, Caddyfile, litestream.yml, env examples)
- Design spec: `docs/superpowers/specs/2026-03-26-vps-deployment-design.md`
