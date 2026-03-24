---
name: sync-skill
description: Recompile the modo skill and references by verifying them against current crate source code. Use when crate APIs have changed and skill docs need updating.
argument-hint: "[module-name ...]"
disable-model-invocation: true
---

# Sync Modo Skill References

You are updating the modo Claude plugin skill (`claude-plugin/skills/modo/`) to match the
current state of the codebase. This is a verification and correction task — not creative writing.

## Principles

1. **The source code is the contract.** Read `src/<module>/` to understand the public API.
   modo v2 has zero proc macros — all APIs are plain structs, traits, and functions.

2. **Never trust examples as source of truth.** Always verify against the actual crate source
   (`src/`, `Cargo.toml`).

3. **Document the public API surface.** Users need to know types, methods, trait bounds, and
   constructor patterns. Show exact signatures from the source.

4. **Remove line number references.** They rot immediately. Use type names and function names
   that can be grepped instead.

5. **Verify every claim.** Every struct field, default value, method signature, enum variant,
   and trait bound in the reference must match the source code exactly.

6. **Re-export lists must be exhaustive or explicitly partial.** When a reference says "public
   re-exports," verify every item against `src/lib.rs`. Missing items cause confusion.

7. **Check feature gates.** Modules behind `#[cfg(feature = "X")]` must document which feature
   flag enables them. Always-available modules must NOT claim a feature gate.

## Process

### Step 1: Identify what changed

$ARGUMENTS

If no specific module is given, check all modules:

```
src/error/          → conventions.md (error handling section)
src/extractor/      → conventions.md (extractors section)
src/service/        → conventions.md (registry section)
src/sanitize/       → conventions.md
src/validate/       → conventions.md
src/id/             → conventions.md
src/encoding/       → conventions.md
src/cache/          → conventions.md
src/config/         → config.md
src/db/             → database.md
src/server/         → handlers.md
src/middleware/      → handlers.md
src/ip/             → handlers.md
src/session/        → sessions.md
src/flash/          → sessions.md
src/cookie/         → sessions.md
src/auth/           → auth.md
src/rbac/           → auth.md
src/job/            → jobs.md
src/cron/           → jobs.md
src/tenant/         → tenant.md
src/template/       → templates.md
src/sse/            → sse.md
src/email/          → email.md
src/storage/        → storage.md
src/webhook/        → webhooks.md
src/dns/            → dns.md
src/geolocation/    → geolocation.md
src/testing/        → testing.md
```

### Step 2: Read the source

For each module being verified, read these files **completely**:

- Every `src/<module>/*.rs` file — all public types, traits, functions, method signatures
- `src/lib.rs` — all public exports
- `Cargo.toml` — features, dependencies
- `tests/*.rs` — usage patterns (secondary source)

Use parallel Explore agents for different modules when verifying multiple at once.

### Step 3: Read the current reference doc

Read the corresponding reference file from `claude-plugin/skills/modo/references/`.

### Step 4: Cross-reference and find inconsistencies

For each claim in the reference doc, verify against source:

- [ ] Struct fields and their types match
- [ ] Default values verified against the `Default` impl or `#[serde(default)]`
- [ ] Method signatures match (parameter types, return types, async/sync)
- [ ] Parameter ownership is correct (`T` vs `&T` vs `&mut T`)
- [ ] Enum variants are complete and correctly named
- [ ] Import paths are correct
- [ ] Feature gates are correctly documented
- [ ] Code examples use current API
- [ ] `lib.rs` re-export lists in references are complete
- [ ] Trait bounds documented correctly (object-safe vs RPITIT)
- [ ] Arc<Inner> pattern types documented correctly (Inner is private)

### Step 5: Fix inconsistencies

Apply fixes. Common issues to watch for:

- **Missing new APIs** — new methods, traits, or types added but not documented
- **Wrong method signatures** — verify async/sync, parameter types, return types
- **Stale gotchas** — gotchas that no longer apply or new gotchas needed
- **Feature gate drift** — module moved from always-available to feature-gated or vice versa
- **Re-export list incomplete** — new types added to `lib.rs` but not in reference

### Step 6: Update SKILL.md if needed

Check the topic index table in `SKILL.md` — verify every row matches the actual
reference files that exist.

### Step 7: Verify CLAUDE.md consistency

Ensure `CLAUDE.md` conventions and gotchas still match what the references say. If you
changed an API pattern in a reference, check if CLAUDE.md needs a corresponding update.

### Step 8: Verify with cargo check

After all fixes are applied, run cargo check to confirm nothing is broken:

```bash
cargo check
cargo clippy --all-features --tests -- -D warnings
```

## Output

After completing the sync, list:

1. Files modified
2. Inconsistencies found and fixed (one line each)
3. New sections or examples added
4. `cargo check` result (pass or issues found)
5. Any items that need user clarification
