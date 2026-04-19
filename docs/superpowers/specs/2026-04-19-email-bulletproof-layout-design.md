# Email Bulletproof Layout + Heading Inlining + OTP Element

**Issue:** [modo#71](https://github.com/dmitrymomot/modo/issues/71)
**Date:** 2026-04-19
**Status:** Design approved, ready for implementation plan

## Summary

Improve default email rendering to move from ~77% to ~94% on mailpit's HTML-compatibility check, without requiring template authors to hand-inline styles. Three coordinated changes:

1. Replace the default `base` layout with a table-based XHTML Transitional shell, inline light-theme styles, and dark mode as progressive enhancement via `<style>`.
2. Add a CSS-inliner post-layout pass that inlines `<style>` rules onto elements while preserving `<style>` for `@media` queries.
3. Add an `[otp|CODE]` markdown element that renders a styled pill (HTML) or a code-on-its-own-line block (plain text), symmetric to the existing `[button|...]` syntax.

All three are additive from an API standpoint; the default HTML output changes, which is the point.

## Motivation

The current default layout relies on a `<style>` block for mobile padding and heading defaults. Gmail mobile webmail (and similar) strips `<style>`, which:

- Leaves headings at browser-default sizes (huge, off-brand).
- Kills mobile padding rules.
- Drops the dark-mode media query (minor — same clients rarely honor it anyway).

Transactional templates end up hand-inlining styles on every `<h1>` to work around this. A CSS-inliner pass + a table-based shell fixes the root cause for all downstream apps without changing how template authors write markdown.

One-time-code blocks are a common transactional-email primitive and look unprofessional as `**123456**`. A first-class `[otp|CODE]` element removes the need for inline HTML in templates.

## Non-goals

- No change to `button.rs`, `mailer.rs`, `message.rs`, `source.rs`, `cache.rs`.
- No migration path for existing custom layouts — they keep working; the inliner operates on whatever HTML `apply_layout` produces.
- No expansion of the OTP feature beyond styled code display (no timers, no resend UI, no copy buttons).
- No email-client snapshot-testing harness.

## Architecture

### Data flow

```
Template.md → [otp pre-pass] → markdown_to_html → content HTML
                                                      ↓
              layout.html  →  apply_layout  →  full HTML
                                                      ↓
                                              [css_inline::inline] (conditional, default on)
                                                      ↓
                                                 wire HTML

Template.md → [otp pre-pass] → markdown_to_text → plain text
```

### Module changes

| File | Change |
|---|---|
| `src/email/layout.rs` | New `BASE_LAYOUT` constant (table shell); updated logo section with optional `app_url` wrap |
| `src/email/markdown.rs` | Pre-pass scanning `[otp|CODE]` outside code spans/blocks; emits inline HTML for the HTML path, code-on-own-line for the text path |
| `src/email/otp.rs` | **New file.** `render_otp_html(code)` and `render_otp_text(code)` |
| `src/email/mod.rs` | Add `mod otp;` |
| `src/email/render.rs` | New `inline_css_pass` function + call site after `apply_layout`; gated by config bool |
| `src/email/config.rs` | New `EmailConfig.inline_css: bool`, default `true` |
| `src/email/README.md` | Document OTP syntax, CSS inlining, `app_url` variable |
| `Cargo.toml` | Add `css-inline` dependency; bump version |
| `.claude-plugin/plugin.json`, `.claude-plugin/marketplace.json`, module `//!` headers, `skills/dev/references/*.md`, root `README.md` | Version sync per CLAUDE.md |

## Design details

### New `BASE_LAYOUT`

XHTML 1.0 Transitional DOCTYPE (required for Outlook rendering). Fully inline light-theme styles on every structural element so clients that strip `<style>` still get correct layout. Retain a minimal `<style>` block for:

- MSO/WebKit text-size-adjust resets (matches issue's example).
- `@media (prefers-color-scheme: dark)` overrides targeting `.email-body`, `.email-card`, `.email-content`, `.email-footer`, `.email-divider`.
- `@media only screen and (max-width: 620px)` mobile padding overrides (preserved from current layout).

**Max-width:** 600px (modo's existing convention; issue suggests 560px but 600px is a more common industry default).

**Palette (inline, light):** page bg `#f4f4f5`, card `#ffffff`, body text `#18181b`, footer text `#71717a`, divider `#e4e4e7`.

**Dark overrides (in `<style>`):** page bg `#1a1a1a`, card `#2a2a2a`, content `#e4e4e7`, footer `#a1a1aa`, divider `#3f3f46`.

**Logo section conditionals** (driven by presence of template variables, same mechanism as today):

| `logo_url` | `app_url` | Rendered |
|---|---|---|
| absent | — | row omitted |
| present | absent | bare `<img>` |
| present | present | `<a href="{{app_url}}"><img></a>` |

Footer section unchanged: rendered when `footer_text` is present, omitted otherwise.

### OTP element

**Syntax:** `[otp|CODE]` where `CODE` matches `[A-Za-z0-9\-]{1,32}`. Tight character class prevents false positives in arbitrary prose; widen later only if a real use case requires it.

**HTML output** (from issue, verbatim):

```html
<table role="presentation" border="0" cellpadding="0" cellspacing="0" style="margin:8px 0 24px 0;"><tr><td style="font-family:ui-monospace,SFMono-Regular,Menlo,Monaco,Consolas,monospace;font-size:28px;font-weight:700;letter-spacing:6px;color:#18181b;background-color:#f4f4f5;padding:14px 20px;border-radius:8px;">{{code}}</td></tr></table>
```

Code is HTML-escaped via `render::escape_html` at render time.

**Plain-text output:** `\n\n{code}\n\n` — blank line above and below, mirroring paragraph block spacing.

**Parser integration:** markdown source pre-pass with a small state machine that tracks code boundaries (single-backtick spans, triple-backtick fences, 4-space indented blocks, escaped `\[`). Outside code, find `[otp|CODE]` and:

- **HTML path:** replace with the rendered OTP table HTML directly. Pulldown-cmark with `Options::ENABLE_HTML` (already enabled via `Options::all()`) passes block-level HTML through as `Event::Html`.
- **Text path:** replace with `\n\n{code}\n\n`. Existing `markdown_to_text` pass handles the surrounding flow.

This avoids hooking the link parser (which wouldn't match `[text]` without `(url)` anyway) and keeps OTP handling completely separate from the button logic in `button.rs`.

**Edge cases:**

| Input | Output |
|---|---|
| `[otp|123456]` in prose | styled pill |
| `` `[otp|123]` `` inline code | literal `[otp|123]` |
| `` ```\n[otp|123]\n``` `` fenced | literal |
| `\[otp|123]` escaped | literal `[otp|123]` |
| `[otp|]` empty | literal (scanner rejects empty code) |
| `[otp|A B]` spaces | literal (scanner requires `[A-Za-z0-9\-]`) |

### CSS-inliner pass

**Dependency:** `css-inline` crate (pure Rust, MIT). Transitively pulls `html5ever` + `selectors`, ~300KB compiled.

**Configuration:**

```rust
css_inline::CSSInliner::options()
    .keep_style_tags(true)
    .load_remote_stylesheets(false)
    .build()
```

- `keep_style_tags(true)` — critical for preserving `@media` rules for clients that honor them. Inliner still copies matched declarations to element `style=""` for clients that strip `<style>`.
- `load_remote_stylesheets(false)` — never fetch external CSS at send time.

**Integration:** one call site in `render.rs` after `apply_layout`:

```rust
let html = apply_layout(&layout, &content, &vars);
let html = if inline_css { inline_css_pass(&html)? } else { html };
```

**Failure mode:** malformed HTML bubbles up as `Error::internal("css inline failed: ...")`. Generated layouts are well-formed; this surfaces only if a user ships a broken custom layout, which is the correct time to surface it.

**Performance:** ~50–200µs per small email on typical hardware. Email is sent in background jobs, so overhead is irrelevant.

### Config

New field on `EmailConfig`:

```rust
#[serde(default = "default_inline_css")]
pub inline_css: bool,

fn default_inline_css() -> bool { true }
```

YAML surface (backward-compatible; field optional):

```yaml
email:
  inline_css: true   # default; set false to skip post-layout CSS inlining
```

Value threads from `EmailConfig` into whatever struct holds the layout map / renderer, as a plain `bool`. No trait plumbing, no Arc.

### Template variables

`app_url` joins `logo_url` and `footer_text` as supported layout variables. Set per-message via the existing `message::Message` vars API. No config change.

## Backward compatibility

- **Default layout HTML changes** — intentional. Downstream snapshot tests will diff; PR description will call this out.
- **CSS-inliner default-on** could surprise a user with a weird custom layout. Opt out via `inline_css: false`.
- **OTP syntax additive** — `[otp|CODE]` previously rendered as literal text (not a link), so no content collision.
- **Config additive** — missing `inline_css` field defaults to `true`; existing configs keep working.

## Testing

### Unit tests

**`src/email/layout.rs`** (updated + new):
- `base_layout_has_content_placeholder` (kept)
- `base_layout_has_dark_mode` (kept)
- `base_layout_has_max_width` (kept, still 600px)
- `base_layout_is_xhtml_transitional` (new)
- `apply_layout_logo_wraps_in_link_when_app_url_present` (new)
- `apply_layout_logo_bare_when_only_logo_url_present` (new)
- Existing `apply_layout_logo_section_when_var_present` updated to assert both shapes.

**`src/email/otp.rs`** (new file):
- `render_html_basic`
- `render_html_escapes_code`
- `render_text_format` (asserts `\n\n{code}\n\n`)

**`src/email/markdown.rs`** (new):
- `html_otp_basic` — renders pill
- `html_otp_in_code_span` — literal
- `html_otp_in_code_block` — literal
- `html_otp_escaped` — literal
- `html_otp_empty_code` — literal
- `html_otp_with_space` — literal
- `text_otp_basic` — asserts `\n\n{code}\n\n`
- `text_otp_in_code_span` — literal

**`src/email/render.rs`** (new):
- `inline_css_inlines_style_rules`
- `inline_css_preserves_media_queries`
- `inline_css_inline_attr_wins_over_style`
- `inline_css_disabled_leaves_html_untouched`
- `inline_css_end_to_end` — markdown `# Heading` through full pipeline emits `<h1 style="...">` with light colors inline and `<style>` `@media` blocks preserved.

### Integration test

**`tests/email_rendering.rs`** (new or extended):
- Full markdown → HTML pipeline with heading, button, OTP, custom layout.
- Assert: `<h1>` has inline `style=""`, button unchanged, OTP pill present, `<style>` retained and contains `@media (prefers-color-scheme: dark)`.

No env-var tests, no `serial_test` needed.

## Versioning

Feature release (additive). Bump per CLAUDE.md's sync rule:

- `Cargo.toml`
- `.claude-plugin/plugin.json`
- `.claude-plugin/marketplace.json`
- All `//!` module headers (at minimum `src/email/mod.rs`, `src/lib.rs`)
- All `src/**/README.md`
- `skills/dev/references/*.md`
- Root `README.md`

Exact version picked during implementation, based on current Cargo.toml value.

## Open questions

None — all resolved during brainstorming.
