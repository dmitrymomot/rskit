# Plan 16: Flash Messages

Cookie-based, signed, read-once-and-clear flash messages. Independent from session.

## Module Structure

```
src/flash/
├── mod.rs          — mod imports + re-exports (FlashLayer, Flash, FlashEntry)
├── state.rs        — FlashState, FlashEntry
├── extractor.rs    — Flash extractor
└── middleware.rs    — FlashLayer + FlashMiddleware

src/template/middleware.rs  — add flash_messages() registration (behind "templates" feature)
src/lib.rs                  — add `pub mod flash;`
```

Always available (no feature gate). The flash module has zero compile-time dependency on the template module. The `TemplateContextMiddleware` (behind `templates` feature) conditionally checks for `Arc<FlashState>` at runtime and registers the `flash_messages` function if present.

## Types

### FlashEntry

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlashEntry {
    pub level: String,
    pub message: String,
}
```

### FlashState

Shared state inserted into request extensions by middleware.

```rust
pub(crate) struct FlashState {
    /// Messages from the incoming cookie (previous request)
    incoming: Vec<FlashEntry>,
    /// Messages added during this request
    outgoing: Mutex<Vec<FlashEntry>>,
    /// Set to true when flash_messages() is called in a template
    read: AtomicBool,
}
```

- `incoming`: immutable, populated from cookie on request
- `outgoing`: collects messages set by handler during current request (`std::sync::Mutex`, never held across `.await`)
- `read`: flag set by `flash_messages()` template function

## Flash Extractor

Lightweight extractor for setting flash messages in handlers.

```rust
pub struct Flash {
    state: Arc<FlashState>,
}

impl Flash {
    /// Generic setter — any custom level
    pub fn set(&self, level: &str, message: &str);

    /// Predefined convenience methods
    pub fn success(&self, message: &str);
    pub fn error(&self, message: &str);
    pub fn warning(&self, message: &str);
    pub fn info(&self, message: &str);

    /// Read incoming flash messages and mark as read.
    /// Sets the read flag so middleware clears the cookie on response.
    /// Returns the same data on repeated calls. Works without templates.
    pub fn messages(&self) -> Vec<FlashEntry>;
}
```

- `FromRequestParts` impl pulls `Arc<FlashState>` from extensions
- Returns `Error::internal("flash middleware not applied")` if middleware missing
- No `OptionalFromRequestParts` impl — middleware is required when using `Flash`
- All methods push to `outgoing` via mutex
- No cookie handling — purely writes to shared state

### Handler usage

```rust
async fn create_item(flash: Flash) -> Redirect {
    // ... create item ...
    flash.success("Item created");
    Redirect::to("/items")
}

async fn delete_item(flash: Flash) -> Redirect {
    flash.error("Failed to delete");
    flash.warning("Item has dependencies");
    Redirect::to("/items")
}
```

## Middleware

### FlashLayer

```rust
pub struct FlashLayer {
    cookie_name: &'static str,  // hardcoded: "flash"
    key: Key,
    config: CookieConfig,
}

impl FlashLayer {
    pub fn new(config: &CookieConfig, key: &Key) -> Self;
}
```

Constructor takes `&CookieConfig` and `&Key` (same `Key` instance reused from session setup via `key_from_config()`). Cookie name is hardcoded to `"flash"`.

### FlashMiddleware

```rust
pub struct FlashMiddleware<S> {
    inner: S,
    cookie_name: &'static str,
    key: Key,
    config: CookieConfig,
}
```

Follows tower `Layer`/`Service` pattern with manual `Clone` impl and `std::mem::swap` in `call()`.

### Request path

1. Read signed cookie from request headers
2. Deserialize JSON into `Vec<FlashEntry>` (empty vec if no cookie, invalid signature, or malformed JSON)
3. Create `Arc<FlashState>` with incoming data, empty outgoing, read=false
4. Insert into request extensions

### Response path

| outgoing? | read flag? | Response action |
|-----------|-----------|-----------------|
| yes | no | Write outgoing to signed cookie |
| yes | yes | Write only outgoing to signed cookie (incoming discarded) |
| no | yes | Remove cookie (set max_age=0) |
| no | no | Pass through untouched |

On the response path, the middleware locks the outgoing mutex once, drains the vec, and checks emptiness + read flag to decide the action. Outgoing messages replace the cookie entirely — incoming messages are never merged back. Read flag controls whether stale incoming data is cleared when there's nothing new to write.

JSON serialization failures on the response path are logged via `tracing::error!` and the cookie is not written (response continues without flash cookie).

Use `headers_mut().append(SET_COOKIE, ...)` (not `insert`) to avoid clobbering session cookies.

## Cookie

- **Name:** `flash` (hardcoded)
- **Format:** JSON-serialized `Vec<FlashEntry>`
- **Signing:** Uses `cookie::Key` (same key as session, derived from `CookieConfig` secret via `key_from_config()`)
- **Attributes:** `path=/`, `secure`, `http_only`, `same_site` — all from `CookieConfig`
- **Max age:** 300 seconds (5 minutes), hardcoded — flash messages are ephemeral
- **Size:** No enforcement — flash messages are short by convention; browsers silently drop cookies exceeding ~4KB

## Template Integration

### flash_messages() function

Registered in `TemplateContextMiddleware` when `Arc<FlashState>` is present in extensions.

```rust
// In TemplateContextMiddleware::call(), after csrf_token check:
if let Some(flash_state) = parts.extensions.get::<Arc<FlashState>>() {
    let state = flash_state.clone();
    ctx.set(
        "flash_messages",
        Value::from_function(move |_args: &[Value]| -> Result<Value, minijinja::Error> {
            state.read.store(true, Ordering::Release);
            // return incoming as list of {level: message} objects
            Ok(/* ... */)
        }),
    );
}
```

The closure signature uses `&[Value] -> Result<Value, minijinja::Error>` to satisfy MiniJinja's `Function` trait bounds.

Middleware reads the `read` flag with `Ordering::Acquire` to pair with the `Release` store.

**Return format:**

```json
[{"error": "some error"}, {"error": "another error"}, {"info": "some info"}]
```

Each entry is a single-key object where the key is the level and the value is the message. This format was chosen deliberately to support multiple messages at the same level while preserving insertion order.

**Calling multiple times** in the same template is safe — returns the same data, flag already set.

### Template usage

```jinja
{% for msg in flash_messages() %}
  {% for level, text in msg|items %}
    <div class="flash-{{ level }}">{{ text }}</div>
  {% endfor %}
{% endfor %}
```

### Layer ordering

`FlashLayer` must be applied as an outer layer (before `TemplateContextLayer`) so that `FlashState` is in extensions when `TemplateContextMiddleware` builds the template context.

```rust
let key = key_from_config(&cookie_config)?;

Router::new()
    .route("/items", get(list_items))
    .layer(TemplateContextLayer::new(engine))     // inner — runs second
    .layer(FlashLayer::new(&cookie_config, &key))  // outer — runs first
```

## Lifecycle

Standard Post/Redirect/Get flow:

1. **POST** handler sets flash via `Flash` extractor → responds with redirect
2. Middleware writes outgoing messages to signed cookie on response
3. **GET** request arrives with flash cookie → middleware reads into `FlashState::incoming`
4. Template calls `flash_messages()` → returns incoming, sets read flag
5. Middleware sees read flag → removes cookie from response

Flash messages set during a request are NOT visible to `flash_messages()` in the same request — they appear on the next request only.

## Re-exports

`src/flash/mod.rs` re-exports:
- `FlashLayer` — for router wiring
- `Flash` — handler extractor
- `FlashEntry` — for testing/inspection

`src/lib.rs` adds:
- `pub mod flash;`

## Dependencies

- `cookie` crate — already used by session module
- `serde` / `serde_json` — already in dependencies
- No new crate dependencies required

## CLAUDE.md Update

Replace the Plan 16 entry with:
```
- **Plan 16 (Flash Messages):** Cookie-based (signed), read-once-and-clear. `Flash` extractor with `flash.success()` / `flash.set()`. Template function `flash_messages()`. No session dependency
```

## Testing Strategy

### Unit tests (state.rs)

- `FlashState` push to outgoing, read incoming
- Read flag toggle
- Multiple messages with same level preserved in order

### Unit tests (extractor.rs)

- `Flash::set()` pushes to outgoing
- Convenience methods (`success`, `error`, `warning`, `info`) use correct levels
- Missing middleware returns internal error

### Unit tests (middleware.rs)

- No cookie → empty FlashState in extensions
- Valid signed cookie → populated incoming
- Invalid/tampered cookie → empty incoming (no error)
- Outgoing messages → signed cookie in response
- Read flag set → cookie removed from response
- Outgoing + read flag → only outgoing written (incoming discarded)
- No activity → response untouched
- Cookie attributes (secure, http_only, same_site) applied correctly
- JSON serialization failure → logged, no cookie written

### Integration tests (template function)

- `flash_messages()` returns incoming data in correct format
- `flash_messages()` sets read flag
- Full redirect flow: set flash → redirect → render → cookie cleared
- Multiple calls to `flash_messages()` in same template return same data

### Integration tests (tests/flash.rs)

- Full Post/Redirect/Get cycle with Router + oneshot
- Flash survives redirect, cleared after render
- Multiple flash messages preserved in order
- Custom levels via `set()`
