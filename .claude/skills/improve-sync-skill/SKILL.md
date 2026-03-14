---
name: improve-sync-skill
description: Review the current conversation for mistakes made during /sync-skill and update the sync-skill to prevent them in future runs.
argument-hint: "[description of what went wrong]"
disable-model-invocation: true
---

# Improve Sync Skill

You just ran `/sync-skill` (or did equivalent skill reference work) and the user found
problems — missed inconsistencies, wrong patterns, bad examples, incomplete coverage, etc.

Your job: figure out what went wrong and patch `.claude/skills/sync-skill/SKILL.md` so the
mistake never recurs.

## Step 1: Identify what went wrong

Review this conversation. Look for:

- Inconsistencies the user caught that you missed
- Patterns you used that the user corrected (e.g. wrong API style, outdated idioms)
- Source files you should have read but didn't
- Claims you made that turned out to be wrong
- Sections you forgot to update
- New crate features you overlooked

$ARGUMENTS

For each issue, write one line: `MISS: <what happened> — <why it happened>`

## Step 2: Draft a fix for the sync-skill

Read `.claude/skills/sync-skill/SKILL.md` first.

For each miss, determine where the fix belongs:

- **Principles section** — if it's a general approach problem
- **Process steps** — if a step was skipped or incomplete
- **Step 4 checklist** — if a verification check was missing
- **Step 5 common issues** — if a known failure pattern wasn't listed
- **New section** — if an entirely new concern emerged

Write the minimal diff. One line per lesson. Don't bloat the skill.

## Step 3: Show proposed changes

For each addition, show:

```
### Why: <one line explaining the miss>

\`\`\`diff
+ <the addition>
\`\`\`
```

## Step 4: Apply with approval

Ask the user to confirm before editing `.claude/skills/sync-skill/SKILL.md`.

## Rules

- Keep sync-skill concise — it's a prompt, not a manual
- Each addition should be a concrete, actionable rule — not a vague aspiration
- If the same type of mistake happened multiple times, write ONE rule that covers all instances
- Don't remove existing content unless it's wrong — only add or refine
