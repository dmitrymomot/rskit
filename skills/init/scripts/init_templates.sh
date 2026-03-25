#!/usr/bin/env bash
# Adds template component files to a modo project.
# Usage: init_templates.sh <project_dir>
#
# Creates: templates/base.html, templates/home.html, static/.gitkeep,
#          src/handlers/home.rs, src/routes/home.rs

set -euo pipefail

PROJECT_DIR="${1:?Usage: init_templates.sh <project_dir>}"

mkdir -p "$PROJECT_DIR"/{templates,static}

# ── templates/base.html ─────────────────────────────────────
cat > "$PROJECT_DIR/templates/base.html" << 'HTML'
<!doctype html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>{% block title %}App{% endblock %}</title>
</head>
<body>
    {% for msg in flash_messages() %}
    <div class="flash flash-{{ msg.level }}">{{ msg.message }}</div>
    {% endfor %}

    {% block content %}{% endblock %}
</body>
</html>
HTML

# ── templates/home.html ─────────────────────────────────────
cat > "$PROJECT_DIR/templates/home.html" << 'HTML'
{% extends "base.html" %}

{% block title %}{{ title }}{% endblock %}

{% block content %}
<h1>{{ title }}</h1>
<p>Welcome to your new modo app!</p>
{% endblock %}
HTML

# ── static/.gitkeep ─────────────────────────────────────────
touch "$PROJECT_DIR/static/.gitkeep"

# ── src/handlers/home.rs ────────────────────────────────────
cat > "$PROJECT_DIR/src/handlers/home.rs" << 'RUST'
use modo::axum::response::Html;
use modo::{Flash, Renderer, Result};

pub async fn get(renderer: Renderer, flash: Flash) -> Result<Html<String>> {
    let messages = flash.messages();
    renderer.html(
        "home.html",
        modo::template::context! { title => "Welcome", messages => messages },
    )
}
RUST

# ── src/routes/home.rs ──────────────────────────────────────
cat > "$PROJECT_DIR/src/routes/home.rs" << 'RUST'
use modo::axum::routing::get;
use modo::axum::Router;

use crate::handlers;

pub fn router() -> Router<modo::service::AppState> {
    Router::new().route("/", get(handlers::home::get))
}
RUST

echo "Templates component added"
