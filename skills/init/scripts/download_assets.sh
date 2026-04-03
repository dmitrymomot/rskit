#!/usr/bin/env bash
# Downloads vendored JS assets for a modo project with templates.
# Usage: download_assets.sh <project_dir>
#
# Downloads: htmx.min.js, htmx-sse.js, alpine.min.js, elements.js

set -euo pipefail

PROJECT_DIR="${1:?Usage: download_assets.sh <project_dir>}"

mkdir -p "$PROJECT_DIR"/assets/static/js

echo "Downloading htmx.min.js..."
curl -sL https://unpkg.com/htmx.org@2/dist/htmx.min.js -o "$PROJECT_DIR/assets/static/js/htmx.min.js"

echo "Downloading htmx-sse.js..."
curl -sL https://unpkg.com/htmx-ext-sse@2/sse.js -o "$PROJECT_DIR/assets/static/js/htmx-sse.js"

echo "Downloading alpine.min.js..."
curl -sL https://unpkg.com/alpinejs@3/dist/cdn.min.js -o "$PROJECT_DIR/assets/static/js/alpine.min.js"

echo "Downloading elements.js..."
curl -sL https://unpkg.com/@tailwindplus/elements@1/dist/index.js -o "$PROJECT_DIR/assets/static/js/elements.js"

echo "Static JS assets downloaded to $PROJECT_DIR/assets/static/js/"
