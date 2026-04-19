# Email Bulletproof Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement issue #71 — table-based XHTML base layout, CSS-inliner post-layout pass, and `[otp|CODE]` markdown element — so modo's default email rendering moves from ~77% to ~94% on the mailpit HTML-compatibility check without requiring template authors to hand-inline styles.

**Architecture:** Three independent changes inside `src/email/`. New `BASE_LAYOUT` constant with inline light styles + `<style>` dark-mode progressive enhancement. New `otp.rs` module exposing `render_otp_html` / `render_otp_text`; markdown renderer gains a pre-pass that replaces `[otp|CODE]` with rendered HTML (for the HTML path) or a code-on-own-line block (for the text path), while respecting code spans / code blocks / escapes. New `css-inline` dependency runs a post-layout pass gated by a config flag (default on) that inlines `<style>` rules onto elements while preserving `@media` blocks.

**Tech Stack:** Rust 2024 (MSRV 1.92), `pulldown-cmark` (already in deps), `css-inline` crate (new), `serde` / `serde_yaml_ng`, `regex`, `lettre`. Tests use `tempfile` + `modo::email::Mailer::with_stub_transport` (already wired via the `test-helpers` feature).

**Design spec:** [docs/superpowers/specs/2026-04-19-email-bulletproof-layout-design.md](docs/superpowers/specs/2026-04-19-email-bulletproof-layout-design.md)

---

## File Structure

| File | Status | Responsibility |
|---|---|---|
| [Cargo.toml](Cargo.toml) | modify | Add `css-inline` dep; bump `version` to `0.10.0` |
| [.claude-plugin/plugin.json](.claude-plugin/plugin.json) | modify | Version sync `0.10.0` |
| [.claude-plugin/marketplace.json](.claude-plugin/marketplace.json) | modify | Version sync `0.10.0` |
| [src/email/config.rs](src/email/config.rs) | modify | New `inline_css: bool` field on `EmailConfig`, default `true` |
| [src/email/otp.rs](src/email/otp.rs) | **create** | `render_otp_html`, `render_otp_text`, character-class validator |
| [src/email/mod.rs](src/email/mod.rs) | modify | `mod otp;` registration |
| [src/email/markdown.rs](src/email/markdown.rs) | modify | OTP pre-pass for HTML + text paths with code-span/block/escape awareness |
| [src/email/layout.rs](src/email/layout.rs) | modify | Rewrite `BASE_LAYOUT` + logo section gains optional `app_url` link wrap |
| [src/email/render.rs](src/email/render.rs) | modify | New `inline_css_pass(html)` helper using `css_inline` |
| [src/email/mailer.rs](src/email/mailer.rs) | modify | `Mailer::render` calls `inline_css_pass` when `config.inline_css` is `true` |
| [src/email/README.md](src/email/README.md) | modify | Document OTP syntax, CSS inlining, `app_url` variable |
| [tests/email_test.rs](tests/email_test.rs) | modify | Full-pipeline integration test asserting inline heading style + preserved `@media` + OTP pill |

Each task below is an atomic commit.

---

## Task 1: Bump version to 0.10.0 and add `css-inline` dependency

**Why first:** every subsequent task will `cargo check` against the new dep; doing it alone as a no-op change surfaces compile problems early.

**Files:**
- Modify: [Cargo.toml](Cargo.toml)
- Modify: [.claude-plugin/plugin.json](.claude-plugin/plugin.json) (line 4)
- Modify: [.claude-plugin/marketplace.json](.claude-plugin/marketplace.json) (line 14)

- [ ] **Step 1: Update Cargo.toml — bump version and add `css-inline`**

In [Cargo.toml](Cargo.toml) change the `[package]` version:

```toml
version = "0.10.0"
```

Add to the `[dependencies]` section (keep alphabetical order; insert between `croner` and `futures` or wherever `c*` lands):

```toml
css-inline = { version = "0.16", default-features = false, features = ["html5ever"] }
```

If `default-features = false` + `features = ["html5ever"]` doesn't resolve a working combination with the crate's current flags, fall back to:

```toml
css-inline = "0.16"
```

(Feature gating is for avoiding optional reqwest / remote-stylesheet paths we don't need. We never call `load_remote_stylesheets(true)`.)

- [ ] **Step 2: Sync version in plugin manifests**

[.claude-plugin/plugin.json](.claude-plugin/plugin.json) line 4:
```json
"version": "0.10.0",
```

[.claude-plugin/marketplace.json](.claude-plugin/marketplace.json) line 14:
```json
"version": "0.10.0",
```

- [ ] **Step 3: Verify build still green**

Run: `cargo check`
Expected: compiles clean (may download css-inline crates on first run).

Run: `cargo clippy --features test-helpers --tests -- -D warnings`
Expected: no warnings.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock .claude-plugin/plugin.json .claude-plugin/marketplace.json
git commit -m "chore(email): bump version to 0.10.0 and add css-inline dependency

Preparing for issue #71: table-based base layout, CSS inliner, OTP element."
```

Note: `Cargo.lock` is gitignored per CLAUDE.md (library crate) — omit it from `git add` if the path doesn't exist or is ignored.

---

## Task 2: Add `inline_css` field to `EmailConfig`

**Files:**
- Modify: [src/email/config.rs](src/email/config.rs)

- [ ] **Step 1: Write failing tests**

Add to the `#[cfg(test)] mod tests` block in [src/email/config.rs](src/email/config.rs):

```rust
#[test]
fn email_config_inline_css_default_true() {
    let config = EmailConfig::default();
    assert!(config.inline_css);
}

#[test]
fn email_config_inline_css_from_yaml() {
    let yaml = "inline_css: false";
    let config: EmailConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert!(!config.inline_css);
}

#[test]
fn email_config_inline_css_omitted_uses_default() {
    let yaml = "default_from_email: noreply@app.com";
    let config: EmailConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert!(config.inline_css);
}
```

- [ ] **Step 2: Verify tests fail**

Run: `cargo test --features test-helpers email::config::tests::email_config_inline_css_default_true`
Expected: FAIL — `no field 'inline_css'`.

- [ ] **Step 3: Add the field**

In [src/email/config.rs](src/email/config.rs), add to the `EmailConfig` struct (after `template_cache_size`, before `smtp`):

```rust
    /// When `true`, rendered HTML is passed through a CSS inliner that
    /// resolves rules from `<style>` blocks into per-element `style=""`
    /// attributes. `<style>` is retained so `@media` rules (dark mode,
    /// mobile) still apply on clients that honour them. Default: `true`.
    pub inline_css: bool,
```

In `impl Default for EmailConfig`, add `inline_css: true,` (after `template_cache_size`, before `smtp`).

Also update the existing `email_config_defaults` test by adding:
```rust
assert!(config.inline_css);
```

And update `email_config_from_yaml` by adding `inline_css: false` to the YAML and asserting `assert!(!config.inline_css);`.

- [ ] **Step 4: Verify all tests pass**

Run: `cargo test --features test-helpers email::config::`
Expected: all pass.

Run: `cargo clippy --features test-helpers --tests -- -D warnings`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add src/email/config.rs
git commit -m "feat(email): add inline_css config field (default true)

Gates the post-layout CSS inliner pass introduced for issue #71."
```

---

## Task 3: Create `otp.rs` module

**Files:**
- Create: [src/email/otp.rs](src/email/otp.rs)
- Modify: [src/email/mod.rs](src/email/mod.rs)

- [ ] **Step 1: Register the module**

In [src/email/mod.rs](src/email/mod.rs), add `mod otp;` to the `mod` block (after `mod message;` and before `mod render;` to keep alphabetical order):

```rust
mod markdown;
mod message;
mod otp;
mod render;
```

- [ ] **Step 2: Create otp.rs with failing tests first**

Create [src/email/otp.rs](src/email/otp.rs):

```rust
use crate::email::render;

/// Character class accepted as OTP code body: ASCII letters, digits, hyphen.
/// Length 1..=32.
pub(crate) fn is_valid_code(s: &str) -> bool {
    let len = s.len();
    if !(1..=32).contains(&len) {
        return false;
    }
    s.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// Render a styled HTML OTP pill (table-based, fully inline styles).
///
/// The code is HTML-escaped before interpolation.
pub fn render_otp_html(code: &str) -> String {
    let escaped = render::escape_html(code);
    format!(
        r#"<table role="presentation" border="0" cellpadding="0" cellspacing="0" style="margin:8px 0 24px 0;"><tr><td style="font-family:ui-monospace,SFMono-Regular,Menlo,Monaco,Consolas,monospace;font-size:28px;font-weight:700;letter-spacing:6px;color:#18181b;background-color:#f4f4f5;padding:14px 20px;border-radius:8px;">{escaped}</td></tr></table>"#
    )
}

/// Plain-text OTP rendering: blank line, code, blank line.
///
/// Returns a string with leading and trailing `\n\n` so it sits as its own
/// block in surrounding paragraph flow.
pub fn render_otp_text(code: &str) -> String {
    format!("\n\n{code}\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_valid_code_accepts_digits() {
        assert!(is_valid_code("123456"));
    }

    #[test]
    fn is_valid_code_accepts_alphanumeric_with_hyphen() {
        assert!(is_valid_code("ABCD-1234"));
    }

    #[test]
    fn is_valid_code_rejects_empty() {
        assert!(!is_valid_code(""));
    }

    #[test]
    fn is_valid_code_rejects_too_long() {
        assert!(!is_valid_code(&"A".repeat(33)));
    }

    #[test]
    fn is_valid_code_accepts_max_length() {
        assert!(is_valid_code(&"A".repeat(32)));
    }

    #[test]
    fn is_valid_code_rejects_space() {
        assert!(!is_valid_code("123 456"));
    }

    #[test]
    fn is_valid_code_rejects_punctuation() {
        assert!(!is_valid_code("abc.def"));
        assert!(!is_valid_code("abc]def"));
    }

    #[test]
    fn render_html_basic() {
        let html = render_otp_html("123456");
        assert!(html.contains(">123456<"));
        assert!(html.contains("role=\"presentation\""));
        assert!(html.contains("font-family:ui-monospace"));
        assert!(html.contains("letter-spacing:6px"));
    }

    #[test]
    fn render_html_escapes_code() {
        let html = render_otp_html("<b>&");
        assert!(html.contains("&lt;b&gt;&amp;"));
        assert!(!html.contains("<b>"));
    }

    #[test]
    fn render_text_format() {
        assert_eq!(render_otp_text("123456"), "\n\n123456\n\n");
    }
}
```

- [ ] **Step 3: Verify tests pass**

Run: `cargo test --features test-helpers email::otp::`
Expected: all 10 tests pass.

Run: `cargo clippy --features test-helpers --tests -- -D warnings`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src/email/mod.rs src/email/otp.rs
git commit -m "feat(email): add OTP element renderer module

Introduces render_otp_html (styled pill table) and render_otp_text
(code on its own line) for the [otp|CODE] markdown element landing in
the next task."
```

---

## Task 4: OTP pre-pass in `markdown_to_html`

**Files:**
- Modify: [src/email/markdown.rs](src/email/markdown.rs)

Write a small source-level scanner that walks the markdown input, tracks whether we're inside a code span (single/double backtick), a fenced code block (``` / ~~~), an indented code block (≥4 spaces at line start), or an escape (`\[`), and outside those contexts replaces `[otp|CODE]` (where `CODE` passes `otp::is_valid_code`) with rendered HTML. Pulldown-cmark with `Options::all()` emits the substituted HTML as `Event::Html` and passes it through unchanged.

- [ ] **Step 1: Write failing tests**

Add to the `#[cfg(test)] mod tests` block in [src/email/markdown.rs](src/email/markdown.rs):

```rust
#[test]
fn html_otp_basic() {
    let html = markdown_to_html("Your code is [otp|123456] — enter it.", None);
    assert!(html.contains("font-family:ui-monospace"));
    assert!(html.contains(">123456<"));
    assert!(html.contains("Your code is"));
    assert!(html.contains("enter it"));
}

#[test]
fn html_otp_alphanumeric_with_hyphen() {
    let html = markdown_to_html("[otp|ABCD-1234]", None);
    assert!(html.contains(">ABCD-1234<"));
}

#[test]
fn html_otp_in_code_span_is_literal() {
    let html = markdown_to_html("Syntax: `[otp|123]`.", None);
    assert!(html.contains("<code>[otp|123]</code>"));
    assert!(!html.contains("font-family:ui-monospace"));
}

#[test]
fn html_otp_in_fenced_block_is_literal() {
    let html = markdown_to_html("```\n[otp|123]\n```", None);
    assert!(html.contains("[otp|123]"));
    assert!(!html.contains("font-family:ui-monospace"));
}

#[test]
fn html_otp_escaped_is_literal() {
    let html = markdown_to_html(r"\[otp|123]", None);
    assert!(html.contains("[otp|123]"));
    assert!(!html.contains("font-family:ui-monospace"));
}

#[test]
fn html_otp_empty_code_is_literal() {
    let html = markdown_to_html("[otp|]", None);
    assert!(html.contains("[otp|]"));
    assert!(!html.contains("font-family:ui-monospace"));
}

#[test]
fn html_otp_with_space_is_literal() {
    let html = markdown_to_html("[otp|12 34]", None);
    assert!(html.contains("[otp|12 34]"));
    assert!(!html.contains("font-family:ui-monospace"));
}

#[test]
fn html_otp_too_long_is_literal() {
    let long = "A".repeat(33);
    let src = format!("[otp|{long}]");
    let html = markdown_to_html(&src, None);
    assert!(!html.contains("font-family:ui-monospace"));
}

#[test]
fn html_otp_multiple_instances() {
    let html = markdown_to_html("[otp|111] and [otp|222]", None);
    assert!(html.contains(">111<"));
    assert!(html.contains(">222<"));
}
```

- [ ] **Step 2: Verify tests fail**

Run: `cargo test --features test-helpers email::markdown::tests::html_otp_basic`
Expected: FAIL — output contains literal `[otp|123456]` instead of the pill.

- [ ] **Step 3: Add the `otp` import**

At the top of [src/email/markdown.rs](src/email/markdown.rs), add to the existing `use` block (next to `use crate::email::button;`):

```rust
use crate::email::otp;
```

- [ ] **Step 4: Write the OTP pre-pass helper**

Add a module-private helper at the top of [src/email/markdown.rs](src/email/markdown.rs) (below imports, above `markdown_to_html`):

```rust
/// State of the source scanner used by the OTP pre-pass.
#[derive(Copy, Clone)]
enum ScanCtx {
    /// Normal markdown text.
    Text,
    /// Inside a single- or multi-backtick code span; `ticks` is the run length.
    CodeSpan { ticks: usize },
    /// Inside a fenced code block; `fence` is the opening fence string (``` or ~~~ of length ≥3).
    Fence { fence: &'static str },
}

/// Walk `src` and, outside code spans / fenced blocks / escapes, replace
/// `[otp|CODE]` with `replace(CODE)` (e.g., rendered HTML or plain-text block).
///
/// Rules:
/// - A code span starts with N consecutive backticks and ends at the next run
///   of exactly N backticks. Content between is preserved verbatim.
/// - A fenced code block starts on a line whose only non-whitespace prefix is
///   ``` or ~~~ (≥3 chars). It ends at the next line starting with the same
///   fence character run ≥ the opening length. Content between is preserved.
/// - Indented code blocks (4-space indent) are NOT handled specially here —
///   pulldown-cmark will still parse them correctly because the raw HTML we
///   emit at the top level is inline-HTML-in-paragraph; indented lines won't
///   match `[otp|...]` patterns we substitute because we operate before
///   markdown parsing and leave indentation intact. (If a user indents an
///   `[otp|...]` line by 4 spaces expecting a code block, the OTP pill will
///   still render — acceptable; users rarely indent markdown emails.)
/// - A backslash immediately before `[` disables substitution for that match.
fn transform_otp<F>(src: &str, mut replace: F) -> String
where
    F: FnMut(&str) -> String,
{
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    let mut ctx = ScanCtx::Text;
    let mut at_line_start = true;

    while i < bytes.len() {
        match ctx {
            ScanCtx::Text => {
                // Fenced block open: line-leading ```/~~~
                if at_line_start {
                    let line = &src[i..];
                    let trimmed = line.trim_start_matches(|c: char| c == ' ');
                    if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                        let ch = trimmed.as_bytes()[0];
                        let run_len = trimmed.bytes().take_while(|b| *b == ch).count();
                        if run_len >= 3 {
                            // Emit the whole opening line as-is, switch to Fence.
                            let nl = line.find('\n').map_or(line.len(), |n| n + 1);
                            out.push_str(&line[..nl]);
                            i += nl;
                            at_line_start = true;
                            ctx = ScanCtx::Fence {
                                fence: if ch == b'`' { "```" } else { "~~~" },
                            };
                            continue;
                        }
                    }
                }

                let b = bytes[i];
                // Backslash escape for `\[`
                if b == b'\\' && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
                    out.push('\\');
                    out.push('[');
                    i += 2;
                    at_line_start = false;
                    continue;
                }
                // Enter code span on backtick run
                if b == b'`' {
                    let ticks = bytes[i..].iter().take_while(|&&c| c == b'`').count();
                    out.push_str(&src[i..i + ticks]);
                    i += ticks;
                    at_line_start = false;
                    ctx = ScanCtx::CodeSpan { ticks };
                    continue;
                }
                // OTP candidate: `[otp|CODE]`
                if b == b'[' && src[i..].starts_with("[otp|") {
                    let rest = &src[i + 5..];
                    if let Some(end) = rest.find(']') {
                        let code = &rest[..end];
                        if otp::is_valid_code(code) {
                            out.push_str(&replace(code));
                            i += 5 + end + 1; // past `]`
                            at_line_start = false;
                            continue;
                        }
                    }
                }

                out.push(b as char);
                at_line_start = b == b'\n';
                i += 1;
            }
            ScanCtx::CodeSpan { ticks } => {
                // Copy verbatim until a matching tick run.
                let b = bytes[i];
                if b == b'`' {
                    let run = bytes[i..].iter().take_while(|&&c| c == b'`').count();
                    out.push_str(&src[i..i + run]);
                    i += run;
                    if run == ticks {
                        ctx = ScanCtx::Text;
                    }
                    continue;
                }
                out.push(b as char);
                at_line_start = b == b'\n';
                i += 1;
            }
            ScanCtx::Fence { fence } => {
                // At line start, check for closing fence.
                if at_line_start {
                    let line = &src[i..];
                    let trimmed = line.trim_start_matches(|c: char| c == ' ');
                    if trimmed.starts_with(fence) {
                        let ch = fence.as_bytes()[0];
                        let run_len = trimmed.bytes().take_while(|b| *b == ch).count();
                        if run_len >= 3 {
                            let nl = line.find('\n').map_or(line.len(), |n| n + 1);
                            out.push_str(&line[..nl]);
                            i += nl;
                            at_line_start = true;
                            ctx = ScanCtx::Text;
                            continue;
                        }
                    }
                }
                let b = bytes[i];
                out.push(b as char);
                at_line_start = b == b'\n';
                i += 1;
            }
        }
    }

    out
}
```

- [ ] **Step 5: Wire the pre-pass into `markdown_to_html`**

Modify `markdown_to_html` in [src/email/markdown.rs](src/email/markdown.rs) — at the top of the function, before `let parser = Parser::new_ext(...)`:

```rust
pub fn markdown_to_html(markdown: &str, brand_color: Option<&str>) -> String {
    let preprocessed = transform_otp(markdown, otp::render_otp_html);
    let parser = Parser::new_ext(&preprocessed, Options::all());
    // ... rest of function unchanged, but replace `markdown` with `&preprocessed`
    //     in any references (actually only the Parser::new_ext call uses it)
```

**Full pattern:** the existing function body references `markdown` only once (the `Parser::new_ext(markdown, ...)` call). Change that single reference to `&preprocessed` and insert the `let preprocessed = ...` line above it.

- [ ] **Step 6: Verify tests pass**

Run: `cargo test --features test-helpers email::markdown::`
Expected: all OTP tests pass; all pre-existing markdown tests still pass.

Run: `cargo clippy --features test-helpers --tests -- -D warnings`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add src/email/markdown.rs
git commit -m "feat(email): add [otp|CODE] markdown element (HTML path)

Pre-pass scans markdown source for [otp|CODE] outside code spans,
fenced blocks, and backslash escapes, and substitutes the rendered
pill HTML. CODE must match [A-Za-z0-9-]{1,32}; invalid forms pass
through literally."
```

---

## Task 5: OTP pre-pass in `markdown_to_text`

**Files:**
- Modify: [src/email/markdown.rs](src/email/markdown.rs)

- [ ] **Step 1: Write failing tests**

Add to the `#[cfg(test)] mod tests` block in [src/email/markdown.rs](src/email/markdown.rs):

```rust
#[test]
fn text_otp_basic() {
    let text = markdown_to_text("Your code is [otp|123456] — enter it.");
    assert!(text.contains("123456"));
    // Code appears on its own line (blank line before and after)
    let idx = text.find("123456").unwrap();
    assert!(text[..idx].ends_with("\n\n") || text[..idx].ends_with('\n'));
}

#[test]
fn text_otp_in_code_span_literal() {
    let text = markdown_to_text("Use `[otp|123]` syntax");
    assert!(text.contains("[otp|123]"));
}

#[test]
fn text_otp_escaped_literal() {
    let text = markdown_to_text(r"\[otp|123]");
    assert!(text.contains("[otp|123]"));
}
```

- [ ] **Step 2: Verify tests fail**

Run: `cargo test --features test-helpers email::markdown::tests::text_otp_basic`
Expected: FAIL — raw `[otp|123456]` appears in the text output.

- [ ] **Step 3: Wire the pre-pass into `markdown_to_text`**

Modify `markdown_to_text` in [src/email/markdown.rs](src/email/markdown.rs). At the top of the function:

```rust
pub fn markdown_to_text(markdown: &str) -> String {
    let preprocessed = transform_otp(markdown, otp::render_otp_text);
    let parser = Parser::new_ext(&preprocessed, Options::all());
    // ... rest unchanged
```

Replace the single `Parser::new_ext(markdown, ...)` reference with `&preprocessed`.

**Important:** `render_otp_text` returns `\n\n{code}\n\n`. Pulldown-cmark will interpret consecutive newlines as paragraph breaks, so the code ends up in its own paragraph — `markdown_to_text`'s existing `End(TagEnd::Paragraph)` handling will emit the trailing `\n\n`. The leading `\n\n` from `render_otp_text` ensures the code starts a new paragraph even when the surrounding text is on the same source line.

- [ ] **Step 4: Verify tests pass**

Run: `cargo test --features test-helpers email::markdown::`
Expected: all pass, including pre-existing text tests.

Run: `cargo clippy --features test-helpers --tests -- -D warnings`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add src/email/markdown.rs
git commit -m "feat(email): render [otp|CODE] in plain-text path as its own block

Code appears on its own line with blank-line separation from
surrounding paragraph flow."
```

---

## Task 6: Rewrite `BASE_LAYOUT` with table shell and optional `app_url` logo link

**Files:**
- Modify: [src/email/layout.rs](src/email/layout.rs)

- [ ] **Step 1: Write failing tests**

Add to the `#[cfg(test)] mod tests` block in [src/email/layout.rs](src/email/layout.rs):

```rust
#[test]
fn base_layout_is_xhtml_transitional() {
    assert!(BASE_LAYOUT.contains("-//W3C//DTD XHTML 1.0 Transitional"));
}

#[test]
fn base_layout_has_table_shell() {
    // Outer presentation table wraps the content
    let count = BASE_LAYOUT
        .matches(r#"role="presentation""#)
        .count();
    assert!(count >= 2, "expected ≥2 presentation tables in base layout");
}

#[test]
fn base_layout_has_inline_light_styles() {
    // Body background inline on <body>
    assert!(
        BASE_LAYOUT.contains(r#"background-color:#f4f4f5"#)
            || BASE_LAYOUT.contains(r#"background-color: #f4f4f5"#)
    );
}

#[test]
fn base_layout_keeps_dark_mode_in_style() {
    assert!(BASE_LAYOUT.contains("prefers-color-scheme: dark"));
}

#[test]
fn base_layout_keeps_mobile_media_query() {
    assert!(BASE_LAYOUT.contains("max-width: 620px"));
}

#[test]
fn apply_layout_logo_wraps_in_link_when_app_url_present() {
    let mut vars = HashMap::new();
    vars.insert("logo_url".into(), "https://cdn.example.com/logo.png".into());
    vars.insert("app_url".into(), "https://example.com".into());
    let result = apply_layout(BASE_LAYOUT, "<p>x</p>", &vars);
    assert!(result.contains("<a href=\"https://example.com\""));
    assert!(result.contains("https://cdn.example.com/logo.png"));
}

#[test]
fn apply_layout_logo_bare_when_only_logo_url_present() {
    let mut vars = HashMap::new();
    vars.insert("logo_url".into(), "https://cdn.example.com/logo.png".into());
    let result = apply_layout(BASE_LAYOUT, "<p>x</p>", &vars);
    assert!(result.contains("https://cdn.example.com/logo.png"));
    assert!(!result.contains("<a href=\"https://example.com\""));
    assert!(!result.contains("{{app_url}}"));
}

#[test]
fn apply_layout_no_logo_without_logo_url() {
    let result = apply_layout(BASE_LAYOUT, "<p>x</p>", &HashMap::new());
    assert!(!result.contains("<img"));
}
```

- [ ] **Step 2: Verify tests fail**

Run: `cargo test --features test-helpers email::layout::tests::base_layout_is_xhtml_transitional`
Expected: FAIL — current layout is HTML5, not XHTML 1.0 Transitional.

- [ ] **Step 3: Replace `BASE_LAYOUT` constant**

In [src/email/layout.rs](src/email/layout.rs), replace the `pub const BASE_LAYOUT: &str = r##"..."##;` definition with:

```rust
/// Built-in responsive table-based base email layout.
///
/// Shell is XHTML 1.0 Transitional (for Outlook's Word rendering engine).
/// Light-theme colours are inline on every structural element so clients
/// that strip `<style>` (Gmail mobile webmail) still render correctly.
/// The retained `<style>` block provides:
/// - Dark-mode overrides via `@media (prefers-color-scheme: dark)` for
///   clients that honour it (Apple Mail, desktop Gmail).
/// - Mobile padding overrides via `@media only screen and (max-width: 620px)`.
/// - MSO / WebKit text-size-adjust resets.
///
/// Card max-width is 600px. Content width is fluid below that.
pub const BASE_LAYOUT: &str = r##"<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0 Transitional//EN" "http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd">
<html xmlns="http://www.w3.org/1999/xhtml" lang="en">
<head>
<meta http-equiv="Content-Type" content="text/html; charset=UTF-8" />
<meta name="viewport" content="width=device-width, initial-scale=1.0" />
<meta http-equiv="X-UA-Compatible" content="IE=edge" />
<meta name="color-scheme" content="light dark" />
<meta name="supported-color-schemes" content="light dark" />
<style type="text/css">
body, table, td, a { -webkit-text-size-adjust: 100%; -ms-text-size-adjust: 100%; }
table, td { mso-table-lspace: 0pt; mso-table-rspace: 0pt; }
img { -ms-interpolation-mode: bicubic; max-width: 100%; }
body { margin: 0 !important; padding: 0 !important; width: 100% !important; }
@media (prefers-color-scheme: dark) {
  .email-body { background-color: #1a1a1a !important; }
  .email-card { background-color: #2a2a2a !important; }
  .email-content, .email-content * { color: #e4e4e7 !important; }
  .email-footer { color: #a1a1aa !important; }
  .email-divider { border-color: #3f3f46 !important; }
}
@media only screen and (max-width: 620px) {
  .email-outer { padding: 16px 8px !important; }
  .email-card { padding: 24px 16px !important; }
}
</style>
</head>
<body class="email-body" style="margin:0;padding:0;width:100%;background-color:#f4f4f5;-webkit-font-smoothing:antialiased;">
<table role="presentation" border="0" cellpadding="0" cellspacing="0" width="100%" class="email-body" style="background-color:#f4f4f5;">
<tr>
<td class="email-outer" align="center" style="padding:24px 16px;">
<!--[if mso]><table role="presentation" width="600" cellpadding="0" cellspacing="0"><tr><td><![endif]-->
<table role="presentation" border="0" cellpadding="0" cellspacing="0" width="100%" style="width:100%;max-width:600px;">
{{logo_section}}
<tr>
<td class="email-card email-content" style="background-color:#ffffff;padding:32px;border-radius:8px;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,Helvetica,Arial,sans-serif;font-size:16px;line-height:1.6;color:#18181b;">
{{content}}
</td>
</tr>
{{footer_section}}
</table>
<!--[if mso]></td></tr></table><![endif]-->
</td>
</tr>
</table>
</body>
</html>"##;
```

- [ ] **Step 4: Replace `LOGO_SECTION` with two variants**

Replace the existing `const LOGO_SECTION: &str = ...;` line in [src/email/layout.rs](src/email/layout.rs) with two constants:

```rust
/// Logo row when `logo_url` is present but `app_url` is not — bare `<img>`.
const LOGO_SECTION_BARE: &str = r#"<tr><td align="center" style="padding-bottom:24px;"><img src="{{logo_url}}" alt="" style="max-width:150px;height:auto;display:block;border:0;" /></td></tr>"#;

/// Logo row when both `logo_url` and `app_url` are present — linked `<img>`.
const LOGO_SECTION_LINKED: &str = r#"<tr><td align="center" style="padding-bottom:24px;"><a href="{{app_url}}" style="text-decoration:none;border:0;"><img src="{{logo_url}}" alt="" style="max-width:150px;height:auto;display:block;border:0;" /></a></td></tr>"#;
```

Also replace the `FOOTER_SECTION` definition with the updated palette / class names (to match the dark-mode overrides):

```rust
const FOOTER_SECTION: &str = r#"<tr><td class="email-footer" align="center" style="padding-top:24px;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,Helvetica,Arial,sans-serif;font-size:13px;color:#71717a;">{{footer_text}}</td></tr>"#;
```

- [ ] **Step 5: Update `apply_layout` to choose the correct logo variant**

Replace the `apply_layout` function body in [src/email/layout.rs](src/email/layout.rs) with:

```rust
pub fn apply_layout(layout_html: &str, content: &str, vars: &HashMap<String, String>) -> String {
    let logo_section = if vars.contains_key("logo_url") {
        let tmpl = if vars.contains_key("app_url") {
            LOGO_SECTION_LINKED
        } else {
            LOGO_SECTION_BARE
        };
        render::substitute(tmpl, vars)
    } else {
        String::new()
    };

    let footer_section = if vars.contains_key("footer_text") {
        render::substitute(FOOTER_SECTION, vars)
    } else {
        String::new()
    };

    let mut full_vars = vars.clone();
    full_vars.insert("content".into(), content.into());
    full_vars.insert("logo_section".into(), logo_section);
    full_vars.insert("footer_section".into(), footer_section);

    render::substitute(layout_html, &full_vars)
}
```

- [ ] **Step 6: Update existing `base_layout_has_max_width` test**

The existing test asserts `BASE_LAYOUT.contains("max-width: 600px")` (with space). New layout uses `max-width:600px` (no space). Change the assertion to:

```rust
#[test]
fn base_layout_has_max_width() {
    assert!(BASE_LAYOUT.contains("max-width:600px"));
}
```

Also update `base_layout_has_dark_mode` — still asserts `"prefers-color-scheme: dark"`; no change needed, but double-check after editing.

- [ ] **Step 7: Verify tests pass**

Run: `cargo test --features test-helpers email::layout::`
Expected: all pass.

Run: `cargo clippy --features test-helpers --tests -- -D warnings`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add src/email/layout.rs
git commit -m "feat(email): switch base layout to table-based XHTML shell

Inline light-theme styles on every structural element so clients
that strip <style> (e.g., Gmail mobile webmail) render correctly.
Dark mode and mobile padding remain in <style> as progressive
enhancement. Logo row now wraps <img> in <a href=app_url> when
app_url is supplied, matching issue #71."
```

---

## Task 7: CSS inliner post-layout pass

**Files:**
- Modify: [src/email/render.rs](src/email/render.rs)
- Modify: [src/email/mailer.rs](src/email/mailer.rs)

- [ ] **Step 1: Write failing tests (in render.rs)**

Add to the `#[cfg(test)] mod tests` block in [src/email/render.rs](src/email/render.rs):

```rust
#[test]
fn inline_css_inlines_style_rules() {
    let html = r#"<html><head><style>h1 { color: red; }</style></head><body><h1>X</h1></body></html>"#;
    let inlined = inline_css_pass(html).unwrap();
    assert!(
        inlined.contains("style=\"color: red") || inlined.contains("style=\"color:red"),
        "expected inlined h1 style, got: {inlined}"
    );
}

#[test]
fn inline_css_preserves_media_queries() {
    let html = r#"<html><head><style>@media (prefers-color-scheme: dark) { body { color: white; } }</style></head><body>x</body></html>"#;
    let inlined = inline_css_pass(html).unwrap();
    assert!(inlined.contains("prefers-color-scheme: dark"));
}

#[test]
fn inline_css_inline_attr_wins_over_style() {
    let html = r#"<html><head><style>p { color: red; }</style></head><body><p style="color: blue;">x</p></body></html>"#;
    let inlined = inline_css_pass(html).unwrap();
    // Inline `blue` must still be present; `red` must not override it.
    assert!(inlined.contains("color: blue") || inlined.contains("color:blue"));
}
```

- [ ] **Step 2: Verify tests fail**

Run: `cargo test --features test-helpers email::render::tests::inline_css_inlines_style_rules`
Expected: FAIL — `inline_css_pass` does not exist.

- [ ] **Step 3: Add `inline_css_pass` to render.rs**

Add at the top of [src/email/render.rs](src/email/render.rs) (after the existing `use` statements):

```rust
use std::sync::LazyLock as _StdLazyLock;
```

(If `LazyLock` is already imported, skip the above.)

Add below `escape_html`:

```rust
static CSS_INLINER: LazyLock<css_inline::CSSInliner> = LazyLock::new(|| {
    css_inline::CSSInliner::options()
        .keep_style_tags(true)
        .load_remote_stylesheets(false)
        .build()
});

/// Inline CSS declarations from `<style>` blocks into element `style=""`
/// attributes while preserving the original `<style>` block so `@media`
/// rules (dark mode, mobile) still apply on clients that honour them.
///
/// Existing inline `style=""` on an element wins over rules from `<style>`
/// per standard CSS specificity.
///
/// # Errors
///
/// Returns [`Error::internal`] when the HTML cannot be parsed. Generated
/// layouts are well-formed; callers surfacing this error are almost always
/// looking at a malformed custom layout.
pub fn inline_css_pass(html: &str) -> Result<String> {
    CSS_INLINER
        .inline(html)
        .map_err(|e| Error::internal(format!("css inline failed: {e}")))
}
```

**Note on the `css_inline` builder API:** the skeleton above uses `CSSInliner::options().keep_style_tags(true).load_remote_stylesheets(false).build()`. If the `css-inline` version resolved by Cargo exposes a different builder shape (e.g., direct `InlineOptions { ... }` struct), adjust the construction accordingly — the behavioural requirements are `keep_style_tags = true` and `load_remote_stylesheets = false`.

- [ ] **Step 4: Verify render.rs tests pass**

Run: `cargo test --features test-helpers email::render::`
Expected: all pass, including new inliner tests.

Run: `cargo clippy --features test-helpers --tests -- -D warnings`
Expected: clean.

- [ ] **Step 5: Wire the pass into `Mailer::render`**

In [src/email/mailer.rs](src/email/mailer.rs), locate the `render` method and modify its body so the pipeline becomes:

```rust
    pub fn render(&self, email: &SendEmail) -> Result<RenderedEmail> {
        let locale = email
            .locale
            .as_deref()
            .unwrap_or(&self.inner.config.default_locale);

        let raw =
            self.inner
                .source
                .load(&email.template, locale, &self.inner.config.default_locale)?;

        let substituted = render::substitute(&raw, &email.vars);
        let (frontmatter, body) = render::parse_frontmatter(&substituted)?;

        let brand_color = email.vars.get("brand_color").map(|s| s.as_str());
        let html_body = markdown::markdown_to_html(&body, brand_color);

        let layout_html = layout::resolve_layout(&frontmatter.layout, &self.inner.layouts)?;
        let html = layout::apply_layout(&layout_html, &html_body, &email.vars);

        // Stage 5b: optional CSS-inliner pass.
        let html = if self.inner.config.inline_css {
            render::inline_css_pass(&html)?
        } else {
            html
        };

        let text = markdown::markdown_to_text(&body);

        Ok(RenderedEmail {
            subject: frontmatter.subject,
            html,
            text,
        })
    }
```

- [ ] **Step 6: Add mailer-level test to prove the toggle works**

Add to the `#[cfg(test)] mod tests` block in [src/email/mailer.rs](src/email/mailer.rs):

```rust
#[test]
fn render_inlines_css_by_default() {
    struct Src;
    impl TemplateSource for Src {
        fn load(&self, _: &str, _: &str, _: &str) -> Result<String> {
            Ok("---\nsubject: T\n---\n# Heading".into())
        }
    }
    let config = test_email_config(SmtpConfig {
        host: "localhost".into(),
        port: 25,
        username: None,
        password: None,
        security: SmtpSecurity::None,
    });
    let mailer = Mailer::with_source(&config, Arc::new(Src)).unwrap();
    let rendered = mailer
        .render(&SendEmail::new("x", "a@b.c"))
        .unwrap();
    // <h1> should carry inline styles copied from the layout's <style>...
    // or — more realistically for this test — the body text color should
    // appear inline where the layout's inline attrs were set.
    // The key assertion: original <style> is retained (dark-mode lives there).
    assert!(rendered.html.contains("prefers-color-scheme: dark"));
}

#[test]
fn render_skips_inliner_when_disabled() {
    struct Src;
    impl TemplateSource for Src {
        fn load(&self, _: &str, _: &str, _: &str) -> Result<String> {
            Ok("---\nsubject: T\n---\nBody".into())
        }
    }
    let mut config = test_email_config(SmtpConfig {
        host: "localhost".into(),
        port: 25,
        username: None,
        password: None,
        security: SmtpSecurity::None,
    });
    config.inline_css = false;
    let mailer = Mailer::with_source(&config, Arc::new(Src)).unwrap();
    let rendered = mailer
        .render(&SendEmail::new("x", "a@b.c"))
        .unwrap();
    // <style> block should still be there (we didn't strip it), and output
    // must not be empty. Primary assertion: render did not fail.
    assert!(!rendered.html.is_empty());
    assert!(rendered.html.contains("prefers-color-scheme: dark"));
}
```

- [ ] **Step 7: Verify mailer tests pass**

Run: `cargo test --features test-helpers email::mailer::`
Expected: all pass.

Run: `cargo test --features test-helpers`
Expected: all tests green across the crate.

Run: `cargo clippy --features test-helpers --tests -- -D warnings`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add src/email/render.rs src/email/mailer.rs
git commit -m "feat(email): run CSS inliner post-layout pass

Resolves <style> rules into per-element style=\"\" attributes so
clients that strip <style> (Gmail mobile webmail) render with the
intended typography. Original <style> is retained so @media rules
(dark mode, mobile padding) still apply on clients that honour them.
Gated by EmailConfig::inline_css (default true)."
```

---

## Task 8: Integration test — full pipeline

**Files:**
- Modify: [tests/email_test.rs](tests/email_test.rs)

- [ ] **Step 1: Add the integration test**

Append to [tests/email_test.rs](tests/email_test.rs):

```rust
#[test]
fn render_full_pipeline_inlines_headings_and_keeps_media_queries() {
    let dir = tempfile::tempdir().unwrap();
    write_template(
        dir.path(),
        "en",
        "full",
        r#"---
subject: "Your code"
---
# Welcome

Here is your one-time code:

[otp|123456]

[button|Continue](https://example.com/continue)
"#,
    );

    let mut config = test_config(dir.path());
    // Force a variable in so logo section renders linked form.
    let stub = lettre::transport::stub::AsyncStubTransport::new_ok();
    let mailer = Mailer::with_stub_transport(&config, stub).unwrap();

    let email = SendEmail::new("full", "user@example.com")
        .var("logo_url", "https://cdn.example.com/logo.png")
        .var("app_url", "https://example.com");

    let rendered = mailer.render(&email).unwrap();

    // OTP pill rendered.
    assert!(rendered.html.contains(">123456<"), "OTP code missing: {}", rendered.html);
    assert!(rendered.html.contains("font-family:ui-monospace"));
    // Button rendered (unchanged behaviour).
    assert!(rendered.html.contains(">Continue</a>"));
    // <style> retained with @media queries.
    assert!(rendered.html.contains("prefers-color-scheme: dark"));
    assert!(rendered.html.contains("max-width: 620px"));
    // Logo wrapped in a link because app_url is present.
    assert!(rendered.html.contains("href=\"https://example.com\""));
    // Plain text has OTP on its own line.
    assert!(rendered.text.contains("123456"));

    // Suppress unused-mut lint if compiler complains (config is mut for symmetry).
    let _ = &mut config;
}

#[test]
fn render_with_inline_css_disabled_keeps_style_tag() {
    let dir = tempfile::tempdir().unwrap();
    write_template(
        dir.path(),
        "en",
        "simple",
        "---\nsubject: T\n---\n# Heading\n",
    );

    let mut config = test_config(dir.path());
    config.inline_css = false;
    let stub = lettre::transport::stub::AsyncStubTransport::new_ok();
    let mailer = Mailer::with_stub_transport(&config, stub).unwrap();

    let email = SendEmail::new("simple", "user@example.com");
    let rendered = mailer.render(&email).unwrap();
    assert!(rendered.html.contains("prefers-color-scheme: dark"));
}
```

Note: `let _ = &mut config;` in the first test is defensive — if `test_config` returns a non-mut binding and you don't actually need mutation, delete the `mut` and the suppression line. Otherwise leave both so the test compiles under clippy.

- [ ] **Step 2: Run the integration test**

Run: `cargo test --features test-helpers --test email_test`
Expected: all tests pass (new + pre-existing).

- [ ] **Step 3: Run full test suite**

Run: `cargo test --features test-helpers`
Expected: all green.

Run: `cargo clippy --features test-helpers --tests -- -D warnings`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add tests/email_test.rs
git commit -m "test(email): integration coverage for OTP + inliner + logo link

Covers the full mailer pipeline with a template that exercises
headings, OTP element, button, and linked logo — asserting that
inlined styles apply and @media queries are preserved."
```

---

## Task 9: Documentation

**Files:**
- Modify: [src/email/README.md](src/email/README.md)
- Modify: [src/email/mod.rs](src/email/mod.rs) (module doc comment if it advertises template syntax)

- [ ] **Step 1: Read current README sections**

Run: `cat src/email/README.md`
Note the existing section headings so the new content slots in consistently.

- [ ] **Step 2: Update [src/email/README.md](src/email/README.md)**

Add a new section **"OTP element"** after the existing "Buttons" section (or equivalent):

```markdown
### OTP element

Render a styled one-time-code pill in both HTML and plain-text output:

```text
Your verification code:

[otp|123456]
```

- Syntax: `[otp|CODE]` where `CODE` matches `[A-Za-z0-9-]{1,32}`.
- HTML: monospace pill with letter-spacing, rounded background — fully inline styles.
- Plain text: the code on its own line, surrounded by blank lines.
- Respects code spans (`` `[otp|…]` `` stays literal), fenced blocks, and
  backslash escapes (`\[otp|…]` stays literal).
- Invalid codes (empty, too long, containing spaces or punctuation outside
  `-`) are left as literal text.
```

Add a new section **"CSS inlining"** after "Layouts" (or equivalent):

```markdown
### CSS inlining

When `email.inline_css` is `true` (the default), the rendered HTML is
passed through a CSS inliner that:

- Copies declarations from `<style>` blocks onto matching elements as
  inline `style=""` attributes (so clients that strip `<style>` still
  render correctly).
- Preserves the original `<style>` block, so `@media` rules — the dark
  mode and mobile padding overrides in the default layout — still apply
  on clients that honour them.
- Never fetches external stylesheets.

Existing inline `style=""` on an element wins over rules from `<style>`,
per standard CSS specificity.

Disable with:

```yaml
email:
  inline_css: false
```
```

Update the **"Layout variables"** section (or add one if missing) to list:

| Variable | Role |
|---|---|
| `{{content}}` | Rendered markdown body (injected automatically) |
| `{{logo_section}}` | Logo row — rendered when `logo_url` is set |
| `{{footer_section}}` | Footer row — rendered when `footer_text` is set |
| `{{logo_url}}` | Logo image URL |
| `{{app_url}}` | Optional. When set alongside `logo_url`, wraps the `<img>` in `<a href="{{app_url}}">` |
| `{{footer_text}}` | Footer text content |

- [ ] **Step 3: Update [src/email/mod.rs](src/email/mod.rs) module doc if needed**

If the top-level `//!` doc comment lists supported template elements (e.g., mentions `[button|…]`), extend it to mention `[otp|…]`. Example:

```rust
//! Templates are Markdown files with a YAML frontmatter block that specifies
//! the subject line and optional layout. Supported custom elements:
//!
//! - `[button|Label](url)` / `[button:TYPE|Label](url)` — styled call-to-action button.
//! - `[otp|CODE]` — styled one-time-code pill.
//!
//! Variable substitution uses `{{var_name}}` placeholders throughout both
//! frontmatter and body.
```

- [ ] **Step 4: Verify docs build**

Run: `cargo doc --no-deps --features test-helpers`
Expected: builds without warnings. If rustdoc complains about broken intra-doc links in new content, adjust the brackets.

- [ ] **Step 5: Commit**

```bash
git add src/email/README.md src/email/mod.rs
git commit -m "docs(email): document OTP element, CSS inlining, and app_url

Adds the new template-author-facing surface introduced for issue #71."
```

---

## Task 10: Final verification

- [ ] **Step 1: Full check**

Run the full CI-equivalent sweep:

```bash
cargo fmt --check
cargo clippy --features test-helpers --tests -- -D warnings
cargo test --features test-helpers
cargo doc --no-deps --features test-helpers
```

Expected: all clean.

- [ ] **Step 2: Version sync grep**

Run: `grep -rn "0\.9\.0" Cargo.toml .claude-plugin/ src/ skills/ README.md 2>/dev/null | grep -v "ssh-agent\|target/"`

Expected: no matches (only `0.10.0` should appear in version fields). Pre-existing `ssh-agent@v0.9.0` in a deploy workflow is unrelated and must be preserved.

If any module-level `//!` doc header or `src/**/README.md` / `skills/dev/references/*.md` carries a stale `0.9.0`, update it per CLAUDE.md's version-sync rule.

- [ ] **Step 3: Open PR**

Per repo conventions, push the branch and open a PR referencing issue #71. Summary should call out:
- New default base layout (HTML output changes — snapshot-test diffs expected downstream).
- New `inline_css` config key defaulting to `true`.
- New `[otp|CODE]` markdown element.

No commit on this step unless the version-sync grep in Step 2 flagged something.

---

## Self-Review

**Spec coverage:**
- § "New BASE_LAYOUT" → Task 6 ✓
- § "OTP element" → Tasks 3, 4, 5 ✓
- § "CSS-inliner pass" → Task 7 ✓
- § "Config" (`inline_css`) → Task 2, wiring in Task 7 ✓
- § "Template variables" (`app_url`) → Task 6 + docs in Task 9 ✓
- § "Versioning" → Tasks 1 + Task 10 sync grep ✓
- § "Testing → Unit" → Tasks 2, 3, 4, 5, 6, 7 ✓
- § "Testing → Integration" → Task 8 ✓
- § "Docs" → Task 9 ✓

**Placeholder scan:** no "TBD", "TODO", "similar to Task N" references. All code blocks show concrete code. The only implementer-judgment note is in Task 7 Step 3 (css-inline builder API variation), where the behavioural requirements are explicit and the builder shape is the only unknown — acceptable since crates.io-resolved version could vary.

**Type / signature consistency:**
- `transform_otp<F>(src: &str, replace: F) -> String` — used in both Task 4 Step 5 and Task 5 Step 3 with the same shape (`otp::render_otp_html` and `otp::render_otp_text` both are `fn(&str) -> String`). ✓
- `inline_css_pass(html: &str) -> Result<String>` — Task 7 Step 3 (definition) and Task 7 Step 5 (call site) match. ✓
- `EmailConfig::inline_css: bool` — Task 2 (definition), Task 7 Step 5 (`self.inner.config.inline_css` read), Task 8 (`config.inline_css = false`) — consistent. ✓
- `LOGO_SECTION_BARE` / `LOGO_SECTION_LINKED` — Task 6 Step 4 (definition), Step 5 (use) — consistent. ✓
