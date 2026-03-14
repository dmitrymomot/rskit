# modo-auth

[![docs.rs](https://img.shields.io/docsrs/modo-auth)](https://docs.rs/modo-auth)

Session-based authentication and Argon2id password hashing for modo applications.

## Features

| Feature     | What it enables                                                                           |
| ----------- | ----------------------------------------------------------------------------------------- |
| `templates` | `UserContextLayer` — injects the authenticated user into the minijinja template context |

## Key Types

| Type                     | Purpose                                                                    |
| ------------------------ | -------------------------------------------------------------------------- |
| `UserProvider`           | Trait — implement on your user repository to load users by session ID      |
| `UserProviderService<U>` | Type-erased wrapper around a `UserProvider`; register with `app.service()` |
| `Auth<U>`                | Extractor — requires an authenticated user; returns 401 if absent          |
| `OptionalAuth<U>`        | Extractor — resolves user if present, yields `None` if not authenticated   |
| `PasswordHasher`         | Argon2id hashing service with `hash_password` / `verify_password`          |
| `PasswordConfig`         | Argon2id tuning knobs (memory, iterations, parallelism)                    |
| `UserContextLayer`       | Tower layer (feature `templates`) — injects user into template context     |

## Usage

### 1. Implement `UserProvider`

```rust
use modo_auth::{UserProvider, UserProviderService};
use serde::Serialize;

#[derive(Clone, Serialize)]
struct MyUser {
    id: String,
    name: String,
}

struct UserRepo {
    // db pool, etc.
}

impl UserProvider for UserRepo {
    type User = MyUser;

    async fn find_by_id(&self, id: &str) -> Result<Option<MyUser>, modo::Error> {
        // load from your database
        todo!()
    }
}
```

Note: `serde::Serialize` is required on the user type only when using `UserContextLayer` (feature `templates`).

### 2. Register services in `main`

```rust
use modo_auth::{UserProviderService, PasswordHasher};

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = UserRepo { /* ... */ };
    let hasher = PasswordHasher::default();

    app.service(UserProviderService::new(repo))
       .service(hasher)
       .run()
       .await
}
```

### 3. Use extractors in handlers

```rust
use modo_auth::{Auth, OptionalAuth};

// Requires authentication — returns 401 if no session / user not found
async fn profile(Auth(user): Auth<MyUser>) -> String {
    format!("Hello, {}", user.name)
}

// Optional — never rejects, yields None when not authenticated
async fn home(OptionalAuth(user): OptionalAuth<MyUser>) -> String {
    match user {
        Some(u) => format!("Welcome back, {}", u.name),
        None => "Welcome, guest".to_string(),
    }
}
```

### 4. Hash and verify passwords

```rust
use modo_auth::PasswordHasher;
use modo::Service;

// Extract the hasher in a handler
async fn register(
    Service(hasher): Service<PasswordHasher>,
) -> Result<(), modo::Error> {
    let hash = hasher.hash_password("correct-horse-battery-staple").await?;
    let valid = hasher.verify_password("correct-horse-battery-staple", &hash).await?;
    // store hash in DB...
    Ok(())
}
```

### 5. Custom Argon2id parameters

```rust
use modo_auth::{PasswordConfig, PasswordHasher};

fn build_hasher() -> Result<PasswordHasher, modo::Error> {
    let config = PasswordConfig {
        memory_cost_kib: 32768, // 32 MiB
        time_cost: 3,
        parallelism: 1,
    };
    PasswordHasher::new(config)
}
```

`PasswordConfig` implements `serde::Deserialize` with `#[serde(default)]`, so you can load it from YAML with partial overrides:

```yaml
password:
    memory_cost_kib: 32768
    # time_cost and parallelism fall back to defaults (2 and 1)
```

### 6. Inject user into template context (feature `templates`)

The user type must implement `serde::Serialize` for this layer.

```rust
use modo_auth::{UserContextLayer, UserProviderService};

#[modo::main]
async fn main(
    app: modo::app::AppBuilder,
    config: Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = UserRepo { /* ... */ };
    let user_svc = UserProviderService::new(repo);

    app.service(user_svc.clone())
       .layer(UserContextLayer::new(user_svc))
       .run()
       .await
}
```

The layer inserts the authenticated user as `"user"` into the minijinja `TemplateContext`, available in every template without explicit handler code. If no session exists or the user is not found, nothing is injected.

## Error Behaviour

| Condition                               | `Auth<U>` | `OptionalAuth<U>` |
| --------------------------------------- | --------- | ----------------- |
| No session                              | 401       | `None`            |
| Session present, user not found         | 401       | `None`            |
| Provider returns `Err`                  | 500       | 500               |
| Session middleware not registered       | 500       | 500               |
| `UserProviderService<U>` not registered | 500       | 500               |
