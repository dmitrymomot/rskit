---
name: modo-feedback
allowed-tools: Bash, AskUserQuestion
description: "Quickly create a GitHub issue for the modo project — report bugs, request features, or suggest improvements for the modo Rust framework, modo-dev plugin skills, or marketplace. Use this skill when the user wants to file an issue, report a bug, request a feature, suggest an improvement, open an issue, submit feedback, say 'something is broken', 'this should work differently', 'I wish modo had...', or wants to create a GitHub issue about anything related to modo."
---

# modo-feedback — Create GitHub Issues

Create a GitHub issue on [dmitrymomot/modo](https://github.com/dmitrymomot/modo) with minimal effort from the user. The whole point of this skill is that the conversation already contains the context — errors, unexpected behavior, feature gaps, things the user struggled with. Your job is to mine that context, draft a complete issue, and get a quick approval.

The user should not have to re-explain what just happened. They lived it. You were there.

## Prerequisites

Verify `gh` CLI is authenticated:
```bash
gh auth status 2>&1 | head -3
```

If not authenticated, tell the user to run `! gh auth login` and stop.

## Workflow

### Step 1: Analyze the Session

Before asking anything, scan the conversation for:

- **Errors and failures** — compiler errors, test failures, unexpected panics, wrong HTTP status codes
- **Workarounds the user had to apply** — anything that felt like fighting the framework
- **Feature gaps** — things the user wanted to do but couldn't, or had to build from scratch
- **Confusing APIs** — places where the user got stuck because the interface wasn't intuitive
- **Documentation gaps** — questions that required reading source code because docs were missing
- **Plugin/skill issues** — incorrect skill guidance, missing references, wrong code patterns suggested

From this analysis, determine:

1. **Issue type**: bug, feature request, improvement, or documentation
2. **Component**: framework (Rust crate), plugin skills, or marketplace/packaging
3. **Title**: concise summary, under 80 characters
4. **Body**: structured write-up with all relevant details from the session

### Step 2: Draft and Present

Present the complete issue to the user as a single `AskUserQuestion` for review:

- header: "GitHub Issue Draft"
- question: Show the full issue in the question text, formatted as:

```
**Type:** <Bug / Feature request / Improvement / Documentation>
**Labels:** <label1>, <label2>

**Title:** <title>

**Body:**
<full issue body — see templates below>

---
Edit any part of this, or approve as-is.
```
- options:
  - **"Create issue"** — Looks good, submit it
  - **"Edit title"** — I want to change the title
  - **"Edit body"** — I want to change the body
  - **"Cancel"** — Don't create anything

If the user picks "Edit title" or "Edit body", use a follow-up `AskUserQuestion` with free text input to collect their edit, apply it, and present the updated draft again.

### Body Templates

Use the appropriate template based on issue type. Fill in every section from the conversation context — don't leave placeholders for the user to fill.

**Bug report:**
```markdown
## Bug Report

**Component:** <component name>

### What happened
<Concrete description extracted from the session — include error messages, wrong behavior, code that failed>

### Expected behavior
<What should have happened instead>

### Context
<Relevant code snippets, config, or commands from the session. Keep it focused — only what someone needs to reproduce or understand the issue>
```

**Feature request / Improvement:**
```markdown
## Feature Request

**Component:** <component name>

### Description
<What the user wants, written clearly for someone who wasn't in the conversation>

### Motivation
<Why this matters — what was the user trying to do? What workaround did they use? Why is the current approach insufficient?>

### Suggested approach
<If the conversation discussed a possible implementation, include it. Otherwise omit this section.>
```

**Documentation:**
```markdown
## Documentation

**Component:** <component name>

### What's missing or wrong
<What the user couldn't find or what was inaccurate>

### Suggested improvement
<What should be documented, or how existing docs should be corrected>
```

### Labels

Map type and component to GitHub labels:

| Issue type      | Label           |
|-----------------|-----------------|
| Bug             | `bug`           |
| Feature request | `enhancement`   |
| Improvement     | `enhancement`   |
| Documentation   | `documentation` |

| Component              | Label       |
|------------------------|-------------|
| Framework (Rust crate) | `framework` |
| Plugin skills          | `plugin`    |
| Marketplace/packaging  | `plugin`    |

### Step 3: Create the Issue

Once the user approves, create it:

```bash
gh issue create \
  --repo dmitrymomot/modo \
  --title "<title>" \
  --label "<label1>,<label2>" \
  --body "$(cat <<'ISSUE_EOF'
<body content>
ISSUE_EOF
)"
```

If labels don't exist in the repo yet, retry without them and note which need to be created.

### Step 4: Done

Show the issue URL returned by `gh issue create`.
