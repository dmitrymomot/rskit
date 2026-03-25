---
name: improve-sync-skill
description: Review the current conversation for mistakes made during /sync-skill and update the sync-skill to prevent them in future runs. Use when sync-skill produced wrong signatures, hallucinated APIs, missed public items, skipped modules, or broke reference formatting.
argument-hint: "[description of what went wrong]"
disable-model-invocation: true
---

# Improve Sync Skill

You just ran `/sync-skill` (or did equivalent skill reference work) and the user found
problems. Your job: figure out what went wrong and patch `.claude/skills/sync-skill/SKILL.md`
so the mistake never recurs.

$ARGUMENTS

## Step 1: Read the sync-skill

Read `.claude/skills/sync-skill/SKILL.md` in full before doing anything else. You need to
know what rules already exist so you can tell the difference between a missing rule and an
existing rule that wasn't followed or wasn't clear enough.

## Step 2: Identify what went wrong

Review this conversation. For each problem, classify it into one of three categories:

| Category         | Meaning                                                     | Fix target                  |
| ---------------- | ----------------------------------------------------------- | --------------------------- |
| **MISSING_RULE** | No rule in the sync-skill covers this case                  | Add a new rule              |
| **UNCLEAR_RULE** | A rule exists but it's ambiguous or easy to misread         | Clarify the existing rule   |
| **IGNORED_RULE** | A rule exists and is clear, the model just didn't follow it | No sync-skill change needed |

Look for:

- Hallucinated APIs — types, methods, or fields that don't exist in source
- Wrong signatures — parameters, return types, or bounds that don't match source
- Missing items — public API items that were skipped entirely
- Stale references — items that were removed from source but left in the reference
- Source files that should have been read but weren't
- Feature gate errors — wrong flag, missing flag, or flag on an always-available module
- Re-export mismatches — items missing from or extra in the Public API section
- Formatting/style drift — section structure, heading levels, or code block style changed

For each issue, write one line with severity:

```
[HIGH]   MISSING_RULE: Hallucinated DomainVerifier::lookup() — no rule requires checking impl blocks line by line
[MEDIUM] UNCLEAR_RULE: Missed timeout_ms field on DnsConfig — Phase 1 says "every pub field" but doesn't emphasize struct fields specifically
[LOW]    IGNORED_RULE: Re-export list was incomplete — Phase 4 step 3 already requires this check
```

Severity guide:

- **HIGH** — wrong information made it into a reference (hallucinated/wrong items)
- **MEDIUM** — correct information was omitted (missing items, incomplete coverage)
- **LOW** — cosmetic or process issues (formatting, ordering, redundant work)

## Step 3: Filter and plan fixes

Drop all IGNORED_RULE items — the sync-skill already covers them and adding redundant rules
just bloats the prompt.

For the remaining items, determine where the fix belongs:

- **Hard Rules section** — if it's a fundamental correctness principle
- **Phase 1 (Inventory)** — if items were missed during source reading
- **Phase 2 (Compare)** — if the two-direction comparison had a gap
- **Phase 4 (Verify)** — if a mechanical check was missing
- **Phase 5 (Update)** — if downstream artifacts were missed
- **Common issues subsection** — if it's a recurring pattern worth calling out explicitly

Before drafting any addition, grep the sync-skill for related keywords. If a closely related
rule exists, modify it rather than adding a new one.

## Step 4: Show proposed changes

For each change, show:

````
### [SEVERITY] Why: <one line explaining the miss>
Category: MISSING_RULE | UNCLEAR_RULE
Target section: <where in sync-skill>

```diff
  existing context line
- old text (if modifying)
+ new or replacement text
  existing context line
````

```

Group changes by target section. If multiple misses produce the same fix, combine them under
one entry.

## Step 5: Validate

For each proposed change, answer: **"Would this rule have prevented the original mistake?"**

If the answer is "maybe" or "no," the rule is too vague or targeting the wrong thing — rework
it before presenting. A good rule is specific enough that you can mechanically check whether
it was followed.

## Step 6: Apply with approval

Present all proposed changes to the user. Apply only after they confirm.

## Rules

- Keep sync-skill concise — it's a prompt, not a manual
- Each addition must be a concrete, actionable rule — not a vague aspiration
- If the same type of mistake happened multiple times, write ONE rule that covers all instances
- Prefer clarifying an existing rule over adding a new one — fewer rules followed well beats many rules ignored
- Don't add rules for IGNORED_RULE cases — the instructions were fine, repetition won't help
- Don't remove existing content unless it's wrong — only add or refine
```
