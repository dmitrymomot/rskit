# modo::service

Type-map service registry and axum application state for the modo web framework.

## Overview

The module provides two complementary types:

- `Registry` — a mutable builder that holds services during startup.
- `AppState` — an immutable, `Clone`-cheap snapshot passed to axum as application state.

Services are keyed by their concrete Rust type. Each type can be registered once; a
second `add` call for the same type overwrites the previous entry.

## Key Types

| Type       | Description                                                          |
| ---------- | -------------------------------------------------------------------- |
| `Registry` | Mutable type-map; add services at startup, then call `into_state()`. |
| `AppState` | Frozen service map wrapped in `Arc`; used as axum router state.      |

## Usage

### Registering services and building a router

```rust
use modo::service::Registry;

let mut registry = Registry::new();
registry.add(my_db_pool);        // any Send + Sync + 'static value
registry.add(my_email_client);

let state = registry.into_state();

let app = axum::Router::new()
    .route("/", axum::routing::get(index))
    .with_state(state);
```

### Retrieving a service inside a handler

Use the `Service<T>` extractor (re-exported as `modo::Service`). It resolves from
`AppState` automatically when the router was built with `with_state(state)`.

```rust
use modo::Service;

async fn index(Service(pool): Service<MyPool>) -> String {
    // pool is Arc<MyPool>
    format!("connected")
}
```

The extractor returns a `500 Internal Server Error` if the requested type was not
registered, with a message that names the missing type.

### Accessing the state directly

`AppState::get<T>()` is available when you hold an `AppState` outside of a handler,
for example inside middleware or during startup checks:

```rust
use modo::service::{AppState, Registry};

let mut registry = Registry::new();
registry.add(42u32);
let state = registry.into_state();

let value: Option<std::sync::Arc<u32>> = state.get::<u32>();
assert_eq!(*value.unwrap(), 42);
```

## Integration with modo

`AppState` implements `axum::extract::FromRef<AppState>` via axum's blanket identity
impl, so it works directly with every extractor that is bound by `AppState: FromRef<S>`,
including `Service<T>`, `Renderer`, and the OAuth state extractor.
