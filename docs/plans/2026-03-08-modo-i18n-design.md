# modo-i18n Design

Internationalization module for modo. File-based YAML translations with locale resolution middleware, an `I18n` extractor, and a `t!()` proc macro.

## File Structure

```
locales/
├── en/
│   ├── common.yml
│   ├── auth.yml
│   └── dashboard.yml
├── es/
│   ├── common.yml
│   └── auth.yml
└── de/
    └── common.yml
```

- Locale directories must be pure lowercase language codes: `en`, `es`, `de`, `fr`
- No region suffixes (`en-US`, `pt-BR`) — region stripping only happens on user input
- Available locales are auto-discovered from subdirectory names
- Partial translations allowed — missing keys fall back to default locale

## YAML Format

```yaml
# en/common.yml
greeting: "Hello, {name}!"
items_count:
  zero: "No items"
  one: "One item"
  other: "{count} items"

# en/auth.yml
page:
  title: "Sign In"
  subtitle: "Welcome back, {name}"
  errors:
    invalid_email: "Please enter a valid email"
    password_too_short: "Password must be at least {min} characters"
```

- Namespace = filename stem (`auth.yml` → `auth`)
- Nested YAML keys are flattened with `.` separator
- Full key = `{namespace}.{flattened.path}` → `auth.page.errors.invalid_email`
- Variables use `{name}` syntax — simple string replacement, no expressions

## Pluralization

Simple scheme: `zero`, `one`, `other`. The `other` key is required.

```yaml
items_count:
  zero: "No items"
  one: "One item"
  other: "{count} items"
```

Plural detection: a YAML map whose keys are a subset of `{zero, one, other}` and contains `other` is treated as a plural entry. Otherwise it's a regular nested namespace.

Plural category resolution:
- `count == 0` → `zero` (fallback to `other`)
- `count == 1` → `one` (fallback to `other`)
- `count >= 2` → `other`

## Locale Resolution Chain

Per-request, tried in order. First valid match against loaded locales wins.

| Priority | Source | Details |
|----------|--------|---------|
| 1 | Custom source | Optional user-provided `Fn(&Request) -> Option<String>` |
| 2 | Cookie | Cookie name configurable, default: `lang` |
| 3 | Query parameter | `?lang=es`. When present and valid, also sets the cookie |
| 4 | Accept-Language header | Parsed with quality weights, normalized, matched |
| 5 | Default locale | Config value, default: `en` |

### Language Normalization

All input values are normalized before matching:
- `en-US` → `en`, `es-MX` → `es`, `pt_BR` → `pt`
- Split on `-` or `_`, take first part, lowercase

### Accept-Language Parsing

- Parse full header with quality weights (default `q=1.0`)
- Sort descending by weight
- Normalize each tag to bare language code
- Deduplicate, filter `*`
- First match against available locales wins

## Crate Structure

```
modo-i18n/
├── Cargo.toml
└── src/
    ├── lib.rs          # re-exports
    ├── config.rs       # I18nConfig
    ├── store.rs        # TranslationStore
    ├── entry.rs        # Entry enum (Plain/Plural)
    ├── extractor.rs    # I18n extractor
    ├── middleware.rs    # locale resolution middleware
    ├── locale.rs       # normalization, Accept-Language parsing
    └── error.rs        # I18nError

modo-i18n-macros/
├── Cargo.toml
└── src/
    └── lib.rs          # t!() proc macro
```

## Config

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct I18nConfig {
    pub path: String,           // default: "locales"
    pub default_lang: String,   // default: "en"
    pub cookie_name: String,    // default: "lang"
    pub query_param: String,    // default: "lang"
}
```

Loaded via standard modo config pattern:

```rust
#[derive(Default, Deserialize)]
struct AppConfig {
    #[serde(flatten)]
    server: modo::config::ServerConfig,
    database: DatabaseConfig,
    #[serde(default)]
    i18n: I18nConfig,
}
```

```yaml
# config/development.yaml
i18n:
  path: locales
  default_lang: en
```

## App Setup

```rust
#[modo::main]
async fn main(app: modo::app::AppBuilder) -> Result<(), Box<dyn std::error::Error>> {
    let config: AppConfig = modo::config::load_or_default()?;
    let i18n = modo_i18n::load(&config.i18n)?;

    app.server_config(config.server)
       .service(i18n.clone())
       .layer(modo_i18n::layer(i18n))
       .run()
       .await
}

// With custom locale source:
app.layer(modo_i18n::layer_with_source(i18n, |req| {
    req.extensions().get::<User>().map(|u| u.language.clone())
}))
```

## TranslationStore

```rust
pub struct TranslationStore {
    config: I18nConfig,
    translations: HashMap<String, HashMap<String, Entry>>,
    available_langs: Vec<String>,
    custom_source: Option<Arc<dyn Fn(&Request<Body>) -> Option<String> + Send + Sync>>,
}

enum Entry {
    Plain(String),
    Plural {
        zero: Option<String>,
        one: Option<String>,
        other: String,  // required
    },
}
```

Loading: scan `locales/` directory, parse all `.yml` files, recursively flatten nested YAML maps into dotted keys, detect plural entries, store in `Arc<TranslationStore>`.

## I18n Extractor

```rust
pub struct I18n {
    store: Arc<TranslationStore>,
    lang: String,
    default_lang: String,
}

impl I18n {
    pub fn t(&self, key: &str, vars: &[(&str, &str)]) -> String;
    pub fn t_plural(&self, key: &str, count: u64, vars: &[(&str, &str)]) -> String;
    pub fn lang(&self) -> &str;
    pub fn available_langs(&self) -> Vec<&str>;
}
```

Lookup chain: user's lang → default lang → return key as-is.

Implements `FromRequestParts<AppState>`. Reads `TranslationStore` from services and `ResolvedLang` from request extensions (set by middleware).

## `t!()` Macro

Standalone `modo-i18n-macros` crate, re-exported from `modo-i18n`.

```rust
use modo_i18n::t;

t!(i18n, "auth.page.title")
// → i18n.t("auth.page.title", &[])

t!(i18n, "common.greeting", name = "Alice")
// → i18n.t("common.greeting", &[("name", &("Alice").to_string())])

t!(i18n, "common.items_count", count = 5)
// → i18n.t_plural("common.items_count", 5, &[("count", &(5).to_string())])
```

When `count` is among the named arguments, the macro emits `t_plural()` and passes the count value both as the `u64` argument and as a string variable for `{count}` interpolation.

## Middleware

Axum async middleware function (not Tower Layer+Service):

1. Get `TranslationStore` from app state services
2. Run resolution chain: custom source → cookie → query param → Accept-Language → default
3. Insert `ResolvedLang` into request extensions
4. Call next
5. If query param was used, set `lang` cookie on response

## Error Types

- `I18nError::DirectoryNotFound` — `locales/` directory missing
- `I18nError::DefaultLangMissing` — default lang directory not found
- `I18nError::ParseError { lang, file, source }` — invalid YAML
- `I18nError::PluralMissingOther { lang, key }` — plural entry without `other` key

## Dependencies

- `serde`, `serde_yaml` — YAML parsing
- `axum`, `http`, `tower` — middleware/extractor
- `tracing` — logging
- `modo` — core framework types (AppState, ServiceRegistry)
- `syn`, `quote`, `proc-macro2` — for `modo-i18n-macros`
