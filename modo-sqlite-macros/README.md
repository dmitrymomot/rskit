# modo-sqlite-macros

Proc-macro crate for `modo-sqlite`. Exports the `embed_migrations!` macro,
which scans a directory of `.sql` migration files at compile time and registers
them with `modo-sqlite`'s `inventory`-based migration system.

This crate is a proc-macro implementation detail. Use the macro through the
`modo-sqlite` re-export:

```toml
[dependencies]
modo-sqlite = { version = "0.3" }
```

## `embed_migrations!`

Scans `$CARGO_MANIFEST_DIR/<path>/*.sql` and registers each file as a
`MigrationRegistration` via `inventory`. Each `.sql` filename must follow the
pattern `{YYYYMMDDHHmmss}_{description}.sql`.

Files are embedded with `include_str!`, so the compiler tracks them as
dependencies and recompiles when they change.

If the migration directory does not exist the macro expands to nothing — no
registrations are emitted and no compile error is raised.

### Arguments

Both arguments are optional and use `key = "value"` syntax:

| Argument | Default | Description |
|----------|---------|-------------|
| `path` | `"migrations"` | Directory relative to `$CARGO_MANIFEST_DIR` |
| `group` | `"default"` | Logical group name for selective migration runs |

### Usage

```rust
// Scan `migrations/` relative to Cargo.toml, register under "default" group.
modo_sqlite::embed_migrations!();

// Scan a custom directory, register under a named group.
modo_sqlite::embed_migrations!(path = "db/migrations", group = "jobs");
```

Call the macro once in any module that is linked into the binary. The
registrations are global and collected at program startup via `inventory`.

### Running migrations at startup

After calling `embed_migrations!`, run the registered migrations at startup
using the functions provided by `modo-sqlite`:

```rust
// All groups:
modo_sqlite::run_migrations(&pool).await?;

// One group only:
modo_sqlite::run_migrations_group(&pool, "jobs").await?;

// All groups except the listed ones:
modo_sqlite::run_migrations_except(&pool, &["jobs"]).await?;
```

### Compile errors

The macro aborts compilation when:

- A filename does not contain an `_` separator after exactly 14 digits.
- The 14-character prefix contains non-numeric characters.
- Two `.sql` files in the same invocation share the same version number.

### File naming

```
migrations/
  20260101120000_create_users.sql
  20260102090000_add_email_index.sql
```

The 14-digit prefix encodes `YYYYMMDDHHmmss`. Files are processed in
lexicographic (filename) order, which matches timestamp order when names are
well-formed.
