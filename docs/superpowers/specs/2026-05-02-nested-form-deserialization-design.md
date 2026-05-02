# Nested form deserialization for `FormRequest`, `Query`, `MultipartRequest`

**Status:** Approved (brainstorming complete, awaiting implementation plan)
**Date:** 2026-05-02
**Related issue:** Follow-up to [#78](https://github.com/dmitrymomot/modo/issues/78)

## Context

Issue #78 added `Vec<scalar>` support for repeated form keys (multi-select
checkboxes, `tag=a&tag=b&tag=c`) by switching `FormRequest<T>`, `Query<T>`,
and the text-field path of `MultipartRequest<T>` from `serde_urlencoded` to
`serde_html_form`. That covers two of the three real-world cases in the
issue; it does **not** cover the third one — per-row dynamic forms with
multiple fields per row.

Concrete need: a "new client" form with `client_name: String` plus a
dynamic contacts list where each row has `type`, `value`, and `comment`.
Today the only viable shape is three parallel `Vec<String>` fields zipped in
the handler. That works but:

- Misaligned rows (e.g. browser drops one field) silently corrupt the
  result — the handler can't tell which row lost which field.
- Per-row validation (`Validate` on each `Contact`) requires the handler
  to do the zip first, then validate, instead of the framework doing both.
- Adding/removing a per-row field touches both the HTML and the handler in
  three places.

The fix: support `contacts[0][type]=email&contacts[0][value]=…&contacts[1][type]=phone&…`
form bodies and deserialize them into `Vec<Contact>` directly.

## Approach

Replace `serde_html_form` with `serde_qs` 1.1 in the three sanitizing
extractors (`FormRequest`, `Query`, `MultipartRequest` text-field path).

`serde_qs` is a strict superset of `serde_html_form` for our use cases:

- Flat fields work identically.
- Repeated keys → `Vec<scalar>` works ("If the deserializer expects a
  sequence, we'll deserialize all values into the sequence" — serde_qs
  docs). This preserves issue #78's behavior.
- Adds nested-struct deserialization (`address[city]=…`).
- Adds `Vec<Struct>` deserialization via indexed brackets
  (`contacts[0][type]=…`).
- Indexed and unindexed array syntax both accepted on input.

Cost: ~50% deserialize overhead vs `serde_urlencoded` (per crate docs,
still single-digit microseconds in absolute terms), and serde_qs error
messages differ in wording. Both acceptable.

### Why a single deserializer rather than a sibling extractor

The user's framework rule (`feedback_breaking_changes_ok` memory; pre-1.0
project) and the explicit decision in issue #78 ("framework should just do
the right thing") both push toward upgrading the existing extractors
in-place rather than introducing `NestedFormRequest` /
`MultiFormRequest`. There is no scenario where a handler would prefer the
narrower `serde_html_form` deserializer over the broader `serde_qs` one
for the same input shape.

### Safety re: db filtering and pagination (explicit user constraint)

Both subsystems are untouched and independent of the deserializer choice:

- **`src/db/filter.rs`** — `Filter::from_request_parts` walks the raw
  query string with a hand-rolled `split('&')` / `urlencoding::decode`
  loop, building a `HashMap<String, Vec<String>>`. No serde involved. Its
  syntax (`field.gt=value`, `sort=-name`) uses dots, not brackets, so
  there's no syntactic collision with bracketed forms either.
- **`src/db/page.rs`** — `PageRequest` and `CursorRequest` call
  `axum::extract::Query` (i.e. `serde_urlencoded`) directly, not our
  `Query<T>`. They have only flat scalar fields, so the deserializer
  choice would not change behavior even if we did upgrade them. We leave
  them as-is.

## Architecture

### Per-extractor configuration

Two `serde_qs::Config` instances, picked per surface:

| Extractor | Mode | Reason |
|---|---|---|
| `FormRequest<T>` | `use_form_encoding(true)` | Browsers percent-encode `[`/`]` as `%5B`/`%5D` in `application/x-www-form-urlencoded` bodies; this mode decodes both bare and percent-encoded brackets and treats `+` as space. |
| `MultipartRequest<T>` text fields | `use_form_encoding(true)` | Multipart text fields are equivalent to form fields after the boundary parse. |
| `Query<T>` | default (`use_form_encoding(false)`) | URL query strings conventionally allow bare `[`/`]`; default mode keeps URL templates readable while still accepting percent-encoded brackets. |

### Array semantics

serde_qs's deserializer accepts **both** indexed and unindexed array
syntax on input (`array_format` only controls *serialization*):

- `tag=a&tag=b` → `Vec<String>` ✅
- `tags[0]=a&tags[1]=b` → `Vec<String>` ✅
- `contacts[0][type]=email&contacts[0][value]=…` → `Vec<Contact>` ✅

For `Vec<Struct>` the **indexed form is required** — without indices the
deserializer cannot disambiguate which field belongs to which row. This
must be called out in the README.

### Top-level shape constraint

serde_qs requires the top-level type to be a struct or map. All existing
handlers already pass structs, and the multi-field form-body model
inherently fits a struct shape. No behavior change.

## Error handling

Same as today: any deserialize failure becomes
`crate::Error::bad_request(format!("invalid form data: {e}"))` (or the
analogous query/multipart message). serde_qs returns structured errors
for malformed bracketed keys (unclosed `[`, mismatched depth, etc.); the
formatted message is forwarded verbatim. No new error variants needed.

## Testing

Add to `tests/extractor_test.rs`:

1. `test_form_request_nested_struct` — `client[name]=Acme&client[id]=42`
   deserializes into a `Client { name, id }` field.
2. `test_form_request_vec_of_structs` — full real-world body
   `name=ACME&contacts[0][type]=email&contacts[0][value]=a@b.com&contacts[0][comment]=primary&contacts[1][type]=phone&contacts[1][value]=555-0100&contacts[1][comment]=`
   deserializes into `Vec<Contact>` of length 2 with the right field
   values, including the empty-comment case.
3. `test_form_request_vec_of_structs_percent_encoded_brackets` — same
   body but with `%5B`/`%5D`, locking in form-encoding mode behavior.
4. `test_query_extractor_nested_struct` —
   `?filter[status]=active&filter[role]=admin` via `Query<T>` →
   `FilterParams { status, role }`.
5. `test_multipart_request_vec_of_structs` — multipart body with
   `contacts[0][type]` etc. → `Vec<Contact>`.

All existing tests must continue to pass unchanged, including the four
issue-#78 tests for repeated keys and parallel-arrays-via-zip.

Verification commands (per `CLAUDE.md`):

```
cargo fmt --check
cargo clippy --features test-helpers --tests -- -D warnings
cargo test --features test-helpers
cargo test --doc --features test-helpers
```

## Files to modify

| File | Change |
|---|---|
| `Cargo.toml` | `+ serde_qs = "1.1"`; `- serde_html_form`. |
| `src/extractor/form.rs` | Replace `serde_html_form::from_bytes` with `serde_qs::Config::new().use_form_encoding(true).deserialize_bytes(&bytes)`. Keep the existing content-type guard, body-bytes read, and sanitize step. |
| `src/extractor/query.rs` | Replace `serde_html_form::from_str` with `serde_qs::from_str` (default config). |
| `src/extractor/multipart.rs` | Replace `serde_html_form::from_str(&encoded)` with `serde_qs::Config::new().use_form_encoding(true).deserialize_str(&encoded)`. Re-encode step stays the same. |
| `src/extractor/mod.rs` | Update module-level doc comment to mention nested-struct + `Vec<Struct>` support. |
| `src/extractor/README.md` | Add "Nested structures" and "Vec of structs (dynamic rows)" subsections. The existing "Per-row parallel arrays" subsection from issue #78 stays as a documented alternative for the case where the HTML cannot emit indexed names. |
| `tests/extractor_test.rs` | Five new tests listed above. |

## Out of scope

- Touching `Filter` (`src/db/filter.rs`) or pagination extractors
  (`src/db/page.rs`) — explicit user constraint. They already work and
  use parsers independent of the deserializer choice.
- Bumping `Cargo.toml` version or running the project's version-sync
  sweep across READMEs / skill references — release-time chore, not part
  of this change.
- Adding a sibling `NestedFormRequest` / `MultiFormRequest` extractor —
  single-extractor approach approved.
- Migrating `src/testing/request.rs` (form-body builder) or `db::page`
  test calls away from `serde_urlencoded` — they only need flat encoding
  and serde_urlencoded stays a dependency.
