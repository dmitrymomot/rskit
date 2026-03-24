# modo::flash

Cookie-based flash messages for one-time cross-request notifications.

Flash messages survive exactly one redirect: the sending request queues a message
and the receiving request reads and consumes it. Once consumed the signed cookie is
cleared from the response automatically. No session dependency is required.

## Key Types

| Type         | Role                                                           |
| ------------ | -------------------------------------------------------------- |
| `FlashLayer` | Tower `Layer` — add to the router to enable flash support      |
| `Flash`      | Axum extractor — write and read flash messages in handlers     |
| `FlashEntry` | Data type — a single message with `level` and `message` fields |

## Setup

Add `FlashLayer` to the router. It requires a `CookieConfig` and a signing `Key`
from `modo::cookie`:

```rust,ignore
use modo::cookie::{CookieConfig, key_from_config};
use modo::flash::FlashLayer;
use axum::Router;

let config: CookieConfig = app_config.cookie.clone();
let key = key_from_config(&config).unwrap();

let app = Router::new()
    .route("/form", axum::routing::post(submit_handler))
    .route("/result", axum::routing::get(result_handler))
    .layer(FlashLayer::new(&config, &key));
```

## Writing Messages

Use the `Flash` extractor in a handler. Queued messages are written to a signed
cookie on the response and are available on the next request:

```rust,ignore
use modo::Flash;
use axum::response::Redirect;

async fn submit_handler(flash: Flash) -> Redirect {
    flash.success("Record saved.");
    Redirect::to("/result")
}
```

Available convenience methods on `Flash`:

- `flash.success(message)` — level `"success"`
- `flash.error(message)` — level `"error"`
- `flash.warning(message)` — level `"warning"`
- `flash.info(message)` — level `"info"`
- `flash.set(level, message)` — arbitrary level string

## Reading Messages

Call `flash.messages()` to retrieve and consume incoming messages. The cookie is
removed from the response after this call:

```rust,ignore
use modo::{Flash, FlashEntry};
use axum::response::Html;

async fn result_handler(flash: Flash) -> Html<String> {
    let msgs: Vec<FlashEntry> = flash.messages();
    let body = msgs
        .iter()
        .map(|m| format!("<p class=\"{}\">{}</p>", m.level, m.message))
        .collect::<Vec<_>>()
        .join("\n");
    Html(body)
}
```

Calling `messages()` multiple times within the same request returns the same data.

## Templates Integration

When the `templates` feature is enabled, `TemplateContextLayer` automatically
registers a `flash_messages()` function in the MiniJinja template context. Call
it from any template without touching the extractor directly:

```jinja
{% for msg in flash_messages() %}
  <div class="alert alert-{{ msg.keys() | first }}">
    {{ msg.values() | first }}
  </div>
{% endfor %}
```

## Cookie Details

| Attribute                    | Value                      |
| ---------------------------- | -------------------------- |
| Name                         | `flash`                    |
| Signing                      | HMAC via application `Key` |
| Max-Age                      | 300 seconds                |
| Path                         | `/`                        |
| Secure / HttpOnly / SameSite | from `CookieConfig`        |

Tampered or expired cookies are silently ignored and result in an empty message list.
