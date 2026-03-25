#!/usr/bin/env bash
# Adds email component files to a modo project.
# Usage: init_email.sh <project_dir>
#
# Creates: emails/welcome.md

set -euo pipefail

PROJECT_DIR="${1:?Usage: init_email.sh <project_dir>}"

mkdir -p "$PROJECT_DIR"/emails

# ── emails/welcome.md ───────────────────────────────────────
cat > "$PROJECT_DIR/emails/welcome.md" << 'MARKDOWN'
# Welcome

Hello {{ name }},

Thanks for signing up! We're glad to have you.

— The Team
MARKDOWN

echo "Email component added"
