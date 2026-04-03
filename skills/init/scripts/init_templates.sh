#!/usr/bin/env bash
# Adds template component files to a modo project.
# Usage: init_templates.sh <project_dir>
#
# Creates: templates/base.html, templates/home.html,
#          assets/static/css/app.css, assets/src/app.css,
#          locales/en/common.yaml,
#          src/handlers/home.rs, src/routes/home.rs

set -euo pipefail

PROJECT_DIR="${1:?Usage: init_templates.sh <project_dir>}"

mkdir -p "$PROJECT_DIR"/{templates,assets/static/css,assets/static/js,assets/src,locales/en}

# ── templates/base.html ─────────────────────────────────────
cat > "$PROJECT_DIR/templates/base.html" << 'HTML'
<!doctype html>
<html lang="{{ locale }}">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta name="csrf-token" content="{{ csrf_token }}">
  <title>{% block title %}App{% endblock %}</title>
  <link rel="stylesheet" href="{{ static_url('css/app.css') }}">
  <script defer src="{{ static_url('js/alpine.min.js') }}"></script>
  <script defer src="{{ static_url('js/elements.js') }}"></script>
  {% block head %}{% endblock %}
</head>
<body class="min-h-screen bg-gray-50 text-gray-900 antialiased">
  {% for msg in flash_messages() %}
  {% for level, text in msg|items %}
  {% if level == "success" %}
  <div class="mx-4 mt-2 rounded-md bg-green-50 px-4 py-3 text-sm text-green-800" role="alert">{{ text }}</div>
  {% elif level == "error" %}
  <div class="mx-4 mt-2 rounded-md bg-red-50 px-4 py-3 text-sm text-red-800" role="alert">{{ text }}</div>
  {% elif level == "warning" %}
  <div class="mx-4 mt-2 rounded-md bg-amber-50 px-4 py-3 text-sm text-amber-800" role="alert">{{ text }}</div>
  {% else %}
  <div class="mx-4 mt-2 rounded-md bg-blue-50 px-4 py-3 text-sm text-blue-800" role="alert">{{ text }}</div>
  {% endif %}
  {% endfor %}
  {% endfor %}

  {% block content %}{% endblock %}

  <script src="{{ static_url('js/htmx.min.js') }}"></script>
  <script src="{{ static_url('js/htmx-sse.js') }}"></script>
  {% block scripts %}{% endblock %}
</body>
</html>
HTML

# ── templates/home.html ─────────────────────────────────────
cat > "$PROJECT_DIR/templates/home.html" << 'HTML'
{% extends "base.html" %}

{% block title %}{{ title }}{% endblock %}

{% block content %}
<main class="mx-auto max-w-xl px-6 py-20">
  <h1 class="text-3xl font-bold tracking-tight">{{ title }}</h1>
  <p class="mt-2 text-gray-500">Your modo app is running.</p>
  <ul class="mt-8 flex flex-col gap-3">
    <li><a href="/_ready" class="text-blue-600 hover:underline">Health check</a></li>
  </ul>
</main>
{% endblock %}
HTML

# ── assets/src/app.css (Tailwind v4 source) ─────────────────
cat > "$PROJECT_DIR/assets/src/app.css" << 'CSS'
@import "tailwindcss";
@source "../../templates/**/*.html";
CSS

# ── assets/static/css/app.css (Tailwind output) ────────────────────
# Compile with: just css
cat > "$PROJECT_DIR/assets/static/css/app.css" << 'CSS'
/* Run `just css` to compile Tailwind CSS */
CSS

if command -v tailwindcss >/dev/null 2>&1; then
    echo "Compiling Tailwind CSS..."
    (cd "$PROJECT_DIR" && tailwindcss -i assets/src/app.css -o assets/static/css/app.css --minify 2>/dev/null) || true
fi

# ── locales/en/common.yaml ──────────────────────────────────
cat > "$PROJECT_DIR/locales/en/common.yaml" << 'YAML'
app_name: My App
welcome: Welcome
home: Home
YAML

# ── src/handlers/home.rs ────────────────────────────────────
cat > "$PROJECT_DIR/src/handlers/home.rs" << 'RUST'
use modo::axum::response::Html;
use modo::{Renderer, Result};

pub async fn get(renderer: Renderer) -> Result<Html<String>> {
    renderer.html(
        "home.html",
        modo::template::context! { title => "Welcome" },
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
