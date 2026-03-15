---
name: sync-skill
description: Recompile the modo skill and references by verifying them against current crate source code. Use when crate APIs have changed and skill docs need updating.
argument-hint: "[crate-name ...]"
disable-model-invocation: true
---

# Sync Modo Skill References

You are updating the modo Claude plugin skill (`claude-plugin/skills/modo/`) to match the
current state of the codebase. This is a verification and correction task — not creative writing.

## Principles

1. **The proc macro output is the contract.** Read `modo-macros/src/`, `modo-db-macros/src/`,
   `modo-jobs-macros/src/`, `modo-upload-macros/src/` to understand what code is generated.
   The generated method signatures, trait impls, and struct shapes are what users interact with.

2. **Never trust examples as source of truth.** Examples in `examples/` may lag behind crate
   APIs. Always verify against the actual crate source (`modo/src/`, `modo-db/src/`, etc.).

3. **Document the generated API, not just macro attributes.** Users need to know what methods
   appear on their structs after macro expansion — show exact signatures.

4. **Remove line number references.** They rot immediately. Use type names and function names
   that can be grepped instead.

5. **Verify every claim.** Every struct field, default value, method signature, enum variant,
   and trait bound in the reference must match the source code exactly.

6. **Check macro attribute parsers, not just doc comments.** The parser source (e.g.
   `parse_name_attr` in utils.rs) defines what syntax is accepted. Doc comments may show
   wrong syntax.

7. **Re-export lists must be exhaustive or explicitly partial.** When a reference says "current
   public re-exports," verify every item against `lib.rs`. Missing items cause unnecessary
   manual imports downstream.

## Process

### Step 1: Identify what changed

$ARGUMENTS

If no specific crate is given, check all crates:

```
modo/           → conventions.md, handlers.md, config.md, templates-htmx.md, testing.md
modo-db/        → database.md
modo-db-macros/ → database.md (generated code section)
modo-jobs/      → jobs.md
modo-jobs-macros/ → jobs.md
modo-email/     → email.md
modo-auth/      → auth-sessions.md
modo-session/   → auth-sessions.md
modo-tenant/    → tenant.md
modo-upload/    → upload.md
modo-upload-macros/ → upload.md
modo-macros/    → SKILL.md (macro cheat sheet), handlers.md, conventions.md
```

### Step 2: Build crate docs

Before reading source, build the rustdoc for the target crates. This gives you the
compiler-verified public API surface — every public type, trait, method, and re-export:

```bash
# All workspace crates
cargo doc --workspace --no-deps

# Specific crate
cargo doc -p modo-db --no-deps

# Include private items (useful for understanding internals)
cargo doc -p modo-db --document-private-items --no-deps
```

Generated docs land in `target/doc/<crate_name>/index.html`. Read the generated doc pages
to get the authoritative public API. If `cargo doc` fails, that itself is a signal — fix
the build first.

### Step 3: Read the crate source

For each crate being verified, read these files **completely**:

- `src/lib.rs` — all public exports
- Every `src/*.rs` file — all public types, traits, functions, method signatures
- `Cargo.toml` — features, dependencies
- `tests/*.rs` — usage patterns (secondary source)
- The crate's README if it exists

Use parallel Explore agents for different crates when verifying multiple at once.

### Step 4: Read the current reference doc

Read the corresponding reference file from `claude-plugin/skills/modo/references/`.

### Step 5: Cross-reference and find inconsistencies

For each claim in the reference doc, verify against source:

- [ ] Struct fields and their types match
- [ ] Default values match
- [ ] Method signatures match (parameter types, return types)
- [ ] Enum variants are complete and correctly named
- [ ] Import paths are correct
- [ ] Feature gates are correctly documented
- [ ] Code examples use current API (not deprecated patterns)
- [ ] Generated code descriptions match what macros actually produce
- [ ] Gotchas are still accurate
- [ ] Macro attribute syntax matches the parser, not just doc comments
- [ ] `lib.rs` re-export lists in references are complete
- [ ] Macro-generated code compiles for each parameter kind (payload, `Service<T>`, `Db`, no-args) — trace the types through the generated setup statements
- [ ] Prose that enumerates enum variants or states is exhaustive — prefer "not in X state" over listing every other variant

### Step 6: Fix inconsistencies

Apply fixes. Common issues to watch for:

- **Raw SeaORM patterns where Record trait methods exist** — replace `Entity::find()`,
  `ActiveModel { .. }.insert()` with domain-struct methods like `Todo::find_by_id()`,
  `todo.insert()`.
- **Missing new APIs** — new methods, traits, or types added to crates but not documented.
- **Wrong macro attribute syntax** — verify against the macro's parser code, not doc comments.
  E.g. `#[template_function]` only accepts `name = "alias"`, not `("alias")`.
- **Stale line number references** — remove them entirely.
- **Incomplete type/export lists** — verify against `lib.rs` re-exports.
- **Abstraction layer drift** — the crate may have added a higher-level API (like `Record`
  trait) on top of a lower-level one (raw SeaORM). Always document the idiomatic higher-level
  API as the primary pattern, with the lower-level as an escape hatch.
- **Non-Rust terminology** — replace borrowed terms from other ecosystems (e.g. "goroutine",
  "channel" when meaning tokio channel) with standard Rust/Tokio vocabulary.

### Step 7: Update SKILL.md if needed

Check the macro cheat sheet table in `SKILL.md` — verify every row against the actual
proc macro source. Pay attention to:

- Parameter names and types
- Generated struct/function names
- Feature gates
- Brief descriptions accuracy

### Step 8: Verify CLAUDE.md consistency

Ensure `CLAUDE.md` conventions and gotchas still match what the references say. If you
changed an API pattern in a reference, check if CLAUDE.md needs a corresponding update.

### Step 9: Verify with cargo doc

After all fixes are applied, rebuild docs to confirm nothing is broken:

```bash
# Rebuild docs — catches broken links, missing types, wrong paths
cargo doc --workspace --no-deps 2>&1

# If doc warnings appear, fix them before finishing
```

## Output

After completing the sync, list:

1. Files modified
2. Inconsistencies found and fixed (one line each)
3. New sections or examples added
4. `cargo doc` result (pass or issues found)
5. Any items that need user clarification
6. Code bugs discovered (not doc issues) — type mismatches, dead code paths, latent compile errors in macro codegen
