# modo::i18n

Internationalization primitives for the modo web framework.

Loads YAML translation files from disk, resolves the active locale from the
request through a pluggable chain (query param, cookie, session,
`Accept-Language`), exposes a `Translator` axum extractor for handlers, and
powers the MiniJinja `t()` function used by `modo::template`.

## Key types

| Type / Trait             | Purpose                                                                     |
| ------------------------ | --------------------------------------------------------------------------- |
| `I18n`                   | Factory that owns the store, resolver chain, and default locale             |
| `I18nConfig`             | Configuration for locales path, default locale, cookie/query-param names   |
| `I18nLayer`              | Tower middleware that resolves the locale and injects a `Translator`        |
| `Translator`             | Axum extractor with `t(key, kwargs)` / `t_plural(key, count, kwargs)`        |
| `TranslationStore`       | Cheap `Arc`-wrapped store; cloneable across threads                         |
| `LocaleResolver` (trait) | Pluggable interface for per-request locale detection                        |
| `QueryParamResolver`     | Resolves locale from a URL query parameter                                  |
| `CookieResolver`         | Resolves locale from a cookie                                               |
| `SessionResolver`        | Resolves locale from the current session                                    |
| `AcceptLanguageResolver` | Resolves locale from the `Accept-Language` header                           |
| `make_t_function`        | Builds a MiniJinja-compatible `t()` function from a `TranslationStore`       |

## Three ways to translate

### 1. Inside an axum handler — `Translator` extractor

```rust,no_run
use modo::i18n::Translator;

async fn greet(t: Translator) -> String {
    t.t("common.greeting", &[("name", "World")])
}
```

The `I18nLayer` must be installed on the router — otherwise extraction returns
`Error::internal("I18nLayer not installed")` and the handler gets a 500.

### 2. Outside a request — `I18n::translator`

```rust,no_run
use modo::i18n::{I18n, I18nConfig};

# fn example() -> modo::Result<()> {
let i18n = I18n::new(&I18nConfig::default())?;
let t = i18n.translator("uk");
let msg = t.t_plural("items.count", 5, &[]);
# let _ = msg;
# Ok(())
# }
```

Useful in background jobs, CLI commands, and tests, where there is no incoming
request to resolve the locale from.

### 3. Inside MiniJinja templates — `t()` function

`modo::template::Engine` wires up a `t()` function through
`make_t_function(store)`. Templates call it directly:

```jinja
{{ t("common.greeting", name="World") }}
{{ t("items.count", count=5) }}
```

The function reads the `locale` variable from the template context; it falls
back to the store's default locale when no locale is set.

## Wiring

```rust,no_run
use modo::i18n::{I18n, I18nConfig};

# fn example() -> modo::Result<()> {
let i18n = I18n::new(&I18nConfig::default())?;
let router: axum::Router = axum::Router::new()
    // ... routes ...
    .layer(i18n.layer());
# let _ = router;
# Ok(())
# }
```

`I18n::new` loads translations from `config.locales_path`. If the directory
does not exist the store is initialised empty (useful in scaffolds / tests);
only an unreadable directory or malformed YAML surfaces as an error.

## Locale resolution chain

By default the chain runs in order:

1. `QueryParamResolver` — `?lang=...`
2. `CookieResolver` — `Cookie: lang=...`
3. `SessionResolver` — `session.data["locale"]`
4. `AcceptLanguageResolver` — `Accept-Language` header

Each resolver is constrained to locales discovered on disk. The first
resolver that returns `Some` wins; if all return `None`, the request falls back
to `I18nConfig::default_locale`.

`SessionResolver` needs [`auth::session::SessionLayer`](../auth/session/)
installed earlier in the stack; without it the resolver returns `None` and the
chain continues.

## YAML config

```yaml
locales_path: "locales"       # directory of locale subdirectories
default_locale: "en"          # fallback when no resolver matches
locale_cookie: "lang"         # cookie name read by CookieResolver
locale_query_param: "lang"    # query param read by QueryParamResolver
```

All fields are optional and fall back to the defaults shown above.

## Translation files

```
locales/
├── en/
│   ├── common.yaml
│   └── auth.yaml
└── uk/
    ├── common.yaml
    └── auth.yaml
```

Each subdirectory is a locale. YAML/YML files inside become namespaces — the
file's basename is used as the key prefix. Nested keys are flattened with `.`
separators.

```yaml
# locales/en/common.yaml
greeting: "Hello, {name}!"
auth:
  login: "Log in"
  logout: "Log out"
```

Gives keys `common.greeting`, `common.auth.login`, `common.auth.logout`.

## Plural rules

A mapping with an `other` key (plus any subset of `zero`, `one`, `two`, `few`,
`many`) is treated as a plural entry:

```yaml
# locales/en/items.yaml
count:
    one: "{count} item"
    other: "{count} items"
```

Plural category selection uses [`intl_pluralrules`](https://docs.rs/intl-pluralrules)
and covers CLDR categories. Missing categories fall back to `other`. The
`count` argument is automatically available as the `{count}` placeholder.

```rust,no_run
# use modo::i18n::Translator;
# fn example(t: &Translator) {
t.t_plural("items.count", 1, &[]);   // "1 item"
t.t_plural("items.count", 5, &[]);   // "5 items"
# }
```

Ukrainian (and other Slavic languages) cover `one`, `few`, `many`, `other`:

```yaml
# locales/uk/items.yaml
count:
    one: "{count} елемент"
    few: "{count} елементи"
    many: "{count} елементів"
    other: "{count} елементів"
```

## Placeholder syntax

Placeholders use `{name}` syntax. Unmatched placeholders are left in place so
missing kwargs are easy to spot in output:

```rust,no_run
# use modo::i18n::Translator;
# fn example(t: &Translator) {
// "welcome: Hello, {name}!"  →  "Hello, World!"
t.t("welcome", &[("name", "World")]);

// "welcome: Hello, {name}!"  →  "Hello, {name}!"
t.t("welcome", &[]);
# }
```

Placeholders do not support type coercion — all values go in as `&str`. For
non-string values, format them before passing in.

## Fallback behaviour

1. Look up the key in the requested locale.
2. Fall back to the default locale.
3. Fall back to the key itself.

`Translator::t` and `Translator::t_plural` never panic — failures in the
underlying store return the key unchanged.
