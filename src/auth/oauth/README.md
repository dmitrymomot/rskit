# modo::auth::oauth

OAuth 2.0 Authorization Code flow with PKCE for modo applications.

Provides built-in provider implementations for Google and GitHub, a shared `OAuthProvider`
trait for custom providers, and the axum types needed to wire login and callback routes.

## Key Types

| Type                   | Role                                                                              |
| ---------------------- | --------------------------------------------------------------------------------- |
| `OAuthProvider`        | Trait implemented by every provider (not object-safe — use concrete types)        |
| `Google`               | Google OAuth 2.0 provider                                                         |
| `GitHub`               | GitHub OAuth 2.0 provider                                                         |
| `OAuthConfig`          | Top-level config struct, loaded from YAML                                         |
| `OAuthProviderConfig`  | Per-provider credentials (`client_id`, `client_secret`, `redirect_uri`, `scopes`) |
| `AuthorizationRequest` | `IntoResponse` redirect that also sets the `_oauth_state` cookie                  |
| `OAuthState`           | axum extractor that reads and verifies the `_oauth_state` cookie                  |
| `CallbackParams`       | Deserialized `?code=…&state=…` query params from the provider callback            |
| `UserProfile`          | Normalized user profile returned after a successful exchange                      |

## Configuration

```yaml
# config/production.yaml
oauth:
    google:
        client_id: "${GOOGLE_CLIENT_ID}"
        client_secret: "${GOOGLE_CLIENT_SECRET}"
        redirect_uri: "https://example.com/auth/google/callback"
        # scopes is optional — defaults to ["openid", "email", "profile"]
    github:
        client_id: "${GITHUB_CLIENT_ID}"
        client_secret: "${GITHUB_CLIENT_SECRET}"
        redirect_uri: "https://example.com/auth/github/callback"
        # scopes is optional — defaults to ["user:email", "read:user"]
cookie:
    secret: "${COOKIE_SECRET}" # min 64 characters
    secure: true
    http_only: true
    same_site: "lax"
```

## Usage

### Wiring providers in main

```rust
use axum::Router;
use axum_extra::extract::cookie::Key;
use modo::auth::oauth::{GitHub, Google, OAuthConfig, OAuthProviderConfig};
use modo::cookie::{CookieConfig, key_from_config};
use modo::service::Registry;

fn build_router(oauth_cfg: &OAuthConfig, cookie_cfg: &CookieConfig, http_client: reqwest::Client) -> Router {
    let key = key_from_config(cookie_cfg).expect("cookie secret must be at least 64 chars");

    let mut registry = Registry::new();
    registry.add(key.clone());

    if let Some(cfg) = &oauth_cfg.google {
        let google = Google::new(cfg, cookie_cfg, &key, http_client.clone());
        registry.add(google);
    }
    if let Some(cfg) = &oauth_cfg.github {
        let github = GitHub::new(cfg, cookie_cfg, &key, http_client.clone());
        registry.add(github);
    }

    let state = registry.into_state();
    Router::new()
        .route("/auth/google", axum::routing::get(google_login))
        .route("/auth/google/callback", axum::routing::get(google_callback))
        .with_state(state)
}
```

### Login handler

```rust
use axum::response::{IntoResponse, Response};
use modo::auth::oauth::Google;
use modo::extractor::Service;

async fn google_login(Service(google): Service<Google>) -> modo::Result<Response> {
    Ok(google.authorize_url()?.into_response())
}
```

### Callback handler

```rust
use axum::extract::Query;
use axum::response::Redirect;
use modo::auth::oauth::{CallbackParams, Google, OAuthState, UserProfile};
use modo::extractor::Service;

async fn google_callback(
    oauth_state: OAuthState,
    Query(params): Query<CallbackParams>,
    Service(google): Service<Google>,
) -> modo::Result<Redirect> {
    let profile: UserProfile = google.exchange(&params, &oauth_state).await?;
    // persist the profile, create a session, etc.
    Ok(Redirect::to("/dashboard"))
}
```

### Custom provider

```rust
use modo::auth::oauth::{AuthorizationRequest, CallbackParams, OAuthProvider, OAuthState, UserProfile};

struct MyProvider { /* ... */ }

impl OAuthProvider for MyProvider {
    fn name(&self) -> &str { "myprovider" }

    fn authorize_url(&self) -> modo::Result<AuthorizationRequest> {
        // build redirect + cookie
        todo!()
    }

    async fn exchange(
        &self,
        params: &CallbackParams,
        state: &OAuthState,
    ) -> modo::Result<UserProfile> {
        // verify state, exchange code, return profile
        todo!()
    }
}
```

## Flow Overview

1. **Login route** — call `provider.authorize_url()` and return the `AuthorizationRequest`
   directly from the handler. It sends `303 See Other` to the provider and sets a signed
   `_oauth_state` cookie (5-minute TTL) containing the PKCE verifier and state nonce.

2. **Callback route** — axum extracts `OAuthState` from the signed cookie and
   `CallbackParams` from the query string. Pass both to `provider.exchange()` which
   verifies the state nonce, performs the PKCE token exchange, and returns a `UserProfile`.

## Security Notes

- PKCE (S256) is used on all providers — the code verifier never leaves the server.
- The `_oauth_state` cookie is HMAC-signed with the application's cookie `Key`.
- State nonce is verified inside `exchange()` — a mismatch returns `Error::bad_request`.
- Provider mismatch (cookie says "google", handler uses `GitHub`) is also rejected.
