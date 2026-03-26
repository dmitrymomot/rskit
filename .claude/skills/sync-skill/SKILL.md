---
name: sync-skill
description: Recompile the modo skill and references by verifying them against current crate source code. Use when crate APIs have changed and skill docs need updating.
argument-hint: "[module-name ...]"
disable-model-invocation: true
---

# Sync Modo Skill References

You are updating the modo skill references to match the current state of the codebase.
This is a verification and correction task — not creative writing.

**Output directory:** `skills/dev/` at the project root (NOT `.claude/skills/dev/`).
Reference files go in `skills/dev/references/`, the topic index is `skills/dev/SKILL.md`.
All paths in this document are relative to the project root unless stated otherwise.

## Hard Rules

0. **Write to `skills/dev/`, never `.claude/skills/dev/`.** The skill definition lives in
   `.claude/skills/sync-skill/` but its OUTPUT goes to `skills/dev/` at the project root.
   These are different directories. Double-check every file path before writing.

1. **Source code is the only truth.** Every type, method, field, variant, and trait bound you
   write in a reference doc must exist in the source. If you cannot point to a `pub` item in
   a `.rs` file, it does not belong in the reference.

2. **Never generate from memory or training data.** You will be tempted to fill in methods that
   "should" exist based on patterns you've seen. Do not. Read the actual file, find the actual
   `pub fn`, and copy the actual signature. If a method doesn't appear in the source, it doesn't
   exist — even if it "makes sense" for it to.

3. **Inventory before editing.** You must produce a complete list of every public item from source
   files _before_ you touch a reference doc. This catches missing items that a casual read misses.

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
src/health/         → conventions.md
src/geolocation/    → geolocation.md
src/runtime/        → handlers.md
src/tracing/        → config.md
src/testing/        → testing.md
src/qrcode/         → qrcode.md
```

## Process

### Phase 0: Validate Mapping

Before inventorying, confirm the Module → Reference Mapping table is complete:

1. List every `src/*/` directory that contains a `mod.rs`.
2. Compare against the mapping table above.
3. If any source module is missing from the table, determine which reference file it
   belongs to (based on topic affinity with existing mappings) and add it before proceeding.

This catches modules added since the last sync-skill update.

### Phase 1: Inventory the Public API

For each module being synced, read every `.rs` file in the source directory and its
subdirectories. Produce a structured inventory of every public item:

**What counts as "public API" — enumerate all of these:**

- `pub struct` — name, generic params, derive macros, `#[non_exhaustive]` if present, every `pub` field with type
- `pub enum` — name, `#[non_exhaustive]` if present, every variant with fields (note `#[default]` variant)
- `pub trait` — name, supertraits, every method with full signature (async/sync, params, return type, generic bounds)
- `pub fn` (free functions) — full signature
- `impl` blocks on public types — every `pub fn` / `pub async fn` with full signature
- **Constructors deserve extra attention:** `new()`, `from_*()`, `with_*()`, `default_*()` builder
  methods are the most commonly missed items. After listing all `impl` methods, double-check that
  every constructor is included.
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

> Inventory every public API item in [source directories] (relative to the project root).
> Read every .rs file completely. For each file, list every `pub` item: structs (with all pub
> fields, types, and derives), enums (with all variants and their fields), traits (with all
> method signatures including `async`, `&self`, all param names with types, and return types),
> impl blocks (with all pub methods — each must show the COMPLETE signature: `pub fn name(&self,
> param: Type) -> ReturnType` or `pub async fn name(&self, param: Type) -> ReturnType`), free
> functions (complete signatures), type aliases, and constants. Skip `pub(crate)` and private
> items. CRITICAL: never abbreviate signatures — write every parameter with its type and the
> full return type. Format as a flat list per source file.
> Also read the relevant sections of `src/lib.rs` for re-exports.

### Phase 2: Compare in Both Directions

Read the current reference doc from `skills/dev/references/` (project root, NOT `.claude/`).

**First-time creation:** If no reference doc exists yet for a module, skip Direction B
(reference→source) since there's nothing to compare. All inventory items are MISSING by
definition. Proceed directly to Phase 3 to create the reference doc. Phase 4 verification
still applies — spot-check 3 signatures from the newly written doc against source.

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

**Path reminder:** All reference files go in `skills/dev/references/` at the project root.
When dispatching subagents to write files, use the absolute path
`<project_root>/skills/dev/references/<name>.md` — never `.claude/skills/dev/`.

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
    - `### method_name(&self, param: Type) -> ReturnType` subsections for methods
    - Include `async` in the heading for async methods: `### async method_name(...)`
    - Prose explaining behavior, error cases, edge cases under each method
- `## Gotchas` section at the bottom for non-obvious behavior
- Code examples use realistic patterns, not toy examples

**For full syncs:** Writing agents may be dispatched in parallel for different reference files.
Each agent prompt MUST include the full output path (`skills/dev/references/<name>.md` relative
to the project root). The orchestrator writes files — agents doing inventory are read-only.

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

1. Check the topic index table in `skills/dev/SKILL.md` — verify every row
   matches the actual reference files that exist.

2. If you changed an API pattern in a reference, check if `CLAUDE.md` conventions or gotchas
   need a corresponding update.

3. If the module defines a config struct that is a field on `ModoConfig` (in `src/config/modo.rs`),
   verify that `config.md` has a matching sub-config table with correct fields and defaults.

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
