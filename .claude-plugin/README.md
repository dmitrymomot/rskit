# modo

Claude Code plugin for building applications with the [modo](https://github.com/dmitrymomot/modo) Rust web framework.

## Skills

### dev (`/modo:dev`)

Development reference for modo v2. Covers handlers, routing, middleware, database (raw sqlx), sessions, auth (OAuth, JWT, password, TOTP), RBAC, templates, SSE, jobs, cron, email, storage, webhooks, DNS verification, geolocation, multi-tenancy, flash messages, configuration, and testing.

**Triggers:** "build a modo app", "add a handler", "create a route", "set up database", "configure sessions", "add OAuth", "set up JWT auth", "add templates", etc.

Includes 16 reference files covering every framework module.

### init (`/modo:init`)

Interactive project scaffolding. Generates a complete, ready-to-run modo v2 application with Cargo.toml, config YAML, main.rs wiring, routes, handlers, migrations, justfile, Dockerfile, docker-compose, .env, .gitignore, and CI workflow.

**Triggers:** "new project", "init app", "create service", "scaffold", "bootstrap", "set up a modo project from scratch".

Supports 4 app type presets (API service, Web application, Full stack, Custom) and 13 optional components.

## Installation

Via marketplace:
```
/plugin marketplace add dmitrymomot/modo
/plugin install modo@dmitrymomot-modo
/reload-plugins
```

Or locally:
```bash
claude --plugin-dir /path/to/modo
```

## License

Apache-2.0
