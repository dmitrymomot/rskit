---
name: sync-skill
description: Recompile the modo skill and references by verifying them against current crate source code. Use when crate APIs have changed and skill docs need updating.
argument-hint: "[module-name ...]"
disable-model-invocation: true
---

# Sync Modo Skill References

You are updating the modo Claude plugin skill (`claude-plugin/skills/modo/`) to match the
current state of the codebase. This is a verification and correction task — not creative writing.

## Hard Rules

1. **Source code is the only truth.** Every type, method, field, variant, and trait bound you
   write in a reference doc must exist in the source. If you cannot point to a `pub` item in
   a `.rs` file, it does not belong in the reference.

2. **Never generate from memory or training data.** You will be tempted to fill in methods that
   "should" exist based on patterns you've seen. Do not. Read the actual file, find the actual
   `pub fn`, and copy the actual signature. If a method doesn't appear in the source, it doesn't
   exist — even if it "makes sense" for it to.

3. **Inventory before editing.** You must produce a complete list of every public item from source
   files *before* you touch a reference doc. This catches missing items that a casual read misses.

4. **Two-direction comparison.** Check source→reference (find undocumented APIs) AND
   reference→source (find stale/hallucinated items). Both directions matter equally.

5. **Preserve reference style.** Match the format of the existing reference doc exactly —
   section structure, heading levels, code block style, separator usage. Do not "improve" the
   formatting or reorganize sections.

6. **Remove line number references.** They rot immediately. Use type and function names only.

7. **Re-export lists must be exhaustive.** Verify every item against `src/lib.rs` and the
   module's `mod.rs`. Missing re-exports cause confusion downstream.

8. **Check feature gates.** Modules behind `#[cfg(feature = "X")]` must document which feature
   flag enables them. Always-available modules must NOT claim a feature gate.

## Module → Reference Mapping

$ARGUMENTS

If no specific module is given, sync all modules:

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

## Process

### Phase 1: Inventory the Public API

For each module being synced, read every `.rs` file in the source directory and its
subdirectories. Produce a structured inventory of every public item:

**What counts as "public API" — enumerate all of these:**

- `pub struct` — name, generic params, derive macros, every `pub` field with type
- `pub enum` — name, every variant with fields
- `pub trait` — name, supertraits, every method with full signature (async/sync, params, return type, generic bounds)
- `pub fn` (free functions) — full signature
- `impl` blocks on public types — every `pub fn` / `pub async fn` with full signature
- `pub type` aliases
- `pub const` / `pub static`
- `pub use` re-exports in `mod.rs`
- Trait implementations for public types that affect API surface (`Default`, `From`, `FromRequestParts`, `IntoResponse`, `Display`, etc.)

**What to skip:** `pub(crate)` items, private items, test-only items (`#[cfg(test)]`).

**Format the inventory as a flat list per source file:**

```
src/dns/config.rs:
  - pub struct DnsConfig { pub nameserver: String, pub txt_prefix: String, pub timeout_ms: u64 }
  - impl DnsConfig: pub fn new(nameserver: impl Into<String>) -> Self
  - impl DnsConfig: pub fn parse_nameserver(&self) -> Result<SocketAddr>
  - impl Default for DnsConfig
  - #[non_exhaustive], #[derive(Debug, Clone, Deserialize)]

src/dns/verifier.rs:
  - pub struct DomainVerifier (Arc<Inner> pattern)
  - impl DomainVerifier: pub fn from_config(config: &DnsConfig) -> Result<Self>
  - impl DomainVerifier: pub async fn check_txt(&self, domain: &str, expected_token: &str) -> Result<bool>
  ...
```

Also read and inventory:
- `src/lib.rs` — all `pub use` re-exports for the module's feature flag
- `Cargo.toml` — the feature flag definition and its dependencies

**For full syncs:** Spawn parallel agents to inventory different module groups simultaneously.
Group by reference file — all modules that map to the same reference file should be inventoried
together. Use this agent prompt template:

> Inventory every public API item in [source directories] of /Users/dmitrymomot/Dev/modo.
> Read every .rs file completely. For each file, list every `pub` item: structs (with all pub
> fields and derives), enums (with all variants), traits (with all method signatures), impl
> blocks (with all pub methods and their full signatures), free functions, type aliases, and
> constants. Skip `pub(crate)` and private items. Format as a flat list per source file.
> Also read the relevant sections of `src/lib.rs` for re-exports.

### Phase 2: Compare in Both Directions

Read the current reference doc from `claude-plugin/skills/modo/references/`.

**Direction A — Source → Reference (find undocumented items):**

Go through every item in your Phase 1 inventory. For each one, check if it's documented in the
reference. Mark items as:
- DOCUMENTED — present and signature matches
- MISSING — not in reference at all
- WRONG — present but signature/type/field doesn't match source

**Direction B — Reference → Source (find stale/hallucinated items):**

Go through every type, method, field, and variant mentioned in the reference doc. For each one,
confirm it exists in your Phase 1 inventory (which was extracted directly from source). Mark items as:
- VERIFIED — exists in source with matching signature
- STALE — was in source before but has been removed or renamed
- HALLUCINATED — never existed in source (this is the critical one to catch)

**Produce a diff summary before editing:**

```
## dns.md Sync Report

### Missing from reference (source → reference):
- DnsConfig::parse_nameserver(&self) -> Result<SocketAddr>  [src/dns/config.rs]

### Stale in reference (reference → source):
- DomainVerifier::lookup() — does not exist in source  [REMOVE]

### Signature mismatches:
- DomainVerifier::check_txt: reference says `(&self, &str) -> bool`, source says `(&self, &str, &str) -> Result<bool>`

### Verified OK:
- DnsConfig struct and all fields
- DomainVerifier::from_config
- ...
```

### Phase 3: Apply Edits

Now apply the verified changes to the reference doc. Rules:

1. **Add MISSING items** — place them in the appropriate section, matching the existing style.
   Look at how neighboring items are documented and follow the same pattern.

2. **Remove STALE/HALLUCINATED items** — delete them completely. Do not comment them out.

3. **Fix WRONG signatures** — update to match source exactly. Copy the signature character by
   character if needed.

4. **Do not rewrite sections that are VERIFIED OK.** Leave them alone.

5. **Do not add commentary, opinions, or "improvements"** beyond what the source code warrants.

**Reference doc format to maintain:**

- Feature flag declaration at top (if applicable)
- `## Public API` section with re-export list from `src/lib.rs`
- `## TypeName` sections separated by `---`, each containing:
  - Brief description (one line)
  - Rust code block showing struct/enum definition with derives
  - `### method_name(params) -> ReturnType` subsections for methods
  - Prose explaining behavior, error cases, edge cases under each method
- `## Gotchas` section at the bottom for non-obvious behavior
- Code examples use realistic patterns, not toy examples

### Phase 4: Verify Completeness

After editing, run a mechanical check:

1. **Grep for public items** in the source files and count them:

   ```bash
   grep -rn "pub fn\|pub async fn\|pub struct\|pub enum\|pub trait\|pub type\|pub const" src/<module>/
   ```

2. **Compare the count** to what's in your reference doc. If the numbers don't match,
   you missed something — go back to Phase 2.

3. **Verify re-exports** — read `src/lib.rs` and confirm every re-exported item from this
   module appears in the reference's "Public API" section.

4. **Spot-check 3 method signatures** — pick 3 methods at random from the reference, re-read
   the source file, and confirm the signature matches exactly. This catches copy errors.

### Phase 5: Update SKILL.md and CLAUDE.md

1. Check the topic index table in `claude-plugin/skills/modo/SKILL.md` — verify every row
   matches the actual reference files that exist.

2. If you changed an API pattern in a reference, check if `CLAUDE.md` conventions or gotchas
   need a corresponding update.

### Phase 6: Verify with cargo

After all fixes are applied, confirm the crate still builds:

```bash
cargo check
cargo clippy --all-features --tests -- -D warnings
```

These commands verify the source code, not the references — but if you introduced
inconsistencies in CLAUDE.md that affect how code is written, this catches downstream issues.

## Output

After completing the sync, report:

1. Files modified
2. Items added to references (one line each, with source file)
3. Items removed from references (one line each, with reason)
4. Signature fixes applied
5. `cargo check` / `cargo clippy` result
6. Any items that need user clarification
