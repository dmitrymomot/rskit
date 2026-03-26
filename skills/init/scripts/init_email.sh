#!/usr/bin/env bash
# Adds email component files to a modo project.
# Usage: init_email.sh <project_dir>
#
# Creates: emails/layouts/ directory, emails/welcome.md

set -euo pipefail

PROJECT_DIR="${1:?Usage: init_email.sh <project_dir>}"

mkdir -p "$PROJECT_DIR"/emails/layouts

# ── emails/welcome.md ───────────────────────────────────────
cat > "$PROJECT_DIR/emails/welcome.md" << 'MARKDOWN'
---
subject: Welcome — let's get started!
layout: base
---

# Welcome, {{ name }}!

Thanks for signing up. We're excited to have you on board.

## Getting Started

1. **Complete your profile** — add your details to personalize your experience
2. **Explore the dashboard** — familiarize yourself with the interface

## Need Help?

Just reply to this email — we're happy to help.

---

If you didn't create this account, please ignore this email.
MARKDOWN

echo "Email component added"
