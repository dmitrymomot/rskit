# modo::service

Type-map service registry and axum application state for the modo web framework.

## Overview

The module provides two complementary public types:

- `Registry` — a mutable builder that holds services during startup.
- `AppState` — an immutable, `Clone`-cheap snapshot passed to axum as application state.

Services are keyed by their concrete Rust type. Each type can be registered once; a
second `add` call for the same type overwrites the previous entry.

## Key Types

| Type          | Description                                                                          |
| ------------- | ------------------------------------------------------------------------------------ |
| `Registry`    | Mutable type-map; add services at startup, then call `into_state()`. Impl `Default`. |
| `AppState`    | Frozen service map wrapped in `Arc`; used as axum router state. Impl `Clone`.        |
| `Service<T>`  | axum extractor that retrieves `Arc<T>` from `AppState`; 500 if the type is absent.  |

## Usage

### Registering services and building a router

```rust,ignore
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

Use the `Service<T>` extractor. It resolves the service from `AppState`
automatically when the router was built with `with_state(state)`.

```rust,ignore
use modo::service::Service;

async fn index(Service(pool): Service<MyPool>) -> String {
    // pool is Arc<MyPool>
    format!("connected")
}
```

The extractor returns a `500 Internal Server Error` if the requested type was not
registered, with a message that names the missing type.

### Accessing the state directly

`AppState::get::<T>()` returns `Option<Arc<T>>` and is available wherever you hold
an `AppState` directly — for example inside middleware or during startup validation:

```rust,ignore
use modo::service::{AppState, Registry};

let mut registry = Registry::new();
registry.add(42u32);
let state = registry.into_state();

let value: Option<std::sync::Arc<u32>> = state.get::<u32>();
assert_eq!(*value.unwrap(), 42);
```

### Startup validation with `Registry::get`

`Registry::get::<T>()` lets you verify that a service was registered before calling
`into_state()`:

```rust,ignore
use modo::service::Registry;

let mut registry = Registry::new();
registry.add(my_db_pool);

// Confirm the pool is present before starting the server.
assert!(registry.get::<MyPool>().is_some());

let state = registry.into_state();
```

## Integration with modo

`AppState` satisfies axum's `FromRef<AppState>` bound via the blanket
`impl<S: Clone> FromRef<S> for S` in axum, so it composes with every extractor
bound by `AppState: FromRef<S>`:

- `Service<T>` — retrieves any registered service by type.
- `Renderer` (`modo::template`) — retrieves the template engine registered as a service.
- `OAuthState` (`modo::auth`) — reads and verifies the OAuth state cookie.
