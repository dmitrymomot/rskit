# modo::error

HTTP-aware error type for the modo web framework.

`Error` carries an HTTP status code, a human-readable message, and optional
source/details/error_code/locale_key fields. It implements
`axum::response::IntoResponse`, so handlers can return `Result<T>` and use `?`
everywhere.

## Key Types

| Type        | Description                                                                                                                         |
| ----------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| `Error`     | The primary framework error: status code, message, optional source, details, error_code, locale_key, and an SSE `lagged` flag.       |
| `Result<T>` | Alias for `std::result::Result<T, Error>`.                                                                                          |
| `HttpError` | `Copy` enum of common HTTP status categories; converts into `Error` via `From`.                                                     |

## Status-Code Constructors

| Method                                    | Status |
| ----------------------------------------- | ------ |
| `Error::bad_request(msg)`                 | 400    |
| `Error::unauthorized(msg)`                | 401    |
| `Error::forbidden(msg)`                   | 403    |
| `Error::not_found(msg)`                   | 404    |
| `Error::conflict(msg)`                    | 409    |
| `Error::payload_too_large(msg)`           | 413    |
| `Error::unprocessable_entity(msg)`        | 422    |
| `Error::too_many_requests(msg)`           | 429    |
| `Error::internal(msg)`                    | 500    |
| `Error::bad_gateway(msg)`                 | 502    |
| `Error::gateway_timeout(msg)`             | 504    |
| `Error::lagged(skipped)`                  | 500    |
| `Error::new(status, msg)`                 | any    |
| `Error::with_source(status, msg, source)` | any    |
| `Error::localized(status, key)`           | any    |

`Error::lagged` sets `is_lagged()` to `true` and is used by the SSE broadcaster
when a subscriber drops messages. `Error::new`, `Error::with_source`, and
`Error::localized` accept any `http::StatusCode` for cases without a dedicated
constructor. `Error::localized` stores an i18n translation key that the
`default_error_handler` middleware resolves against a `Translator` at
response-build time.

## Builder Methods

| Method                      | Purpose                                                                                     |
| --------------------------- | ------------------------------------------------------------------------------------------- |
| `.chain(src)`               | Attach a source error (stored as `Box<dyn Error + Send + Sync>`).                           |
| `.with_code(code)`          | Attach a static `&'static str` error code that survives clone and response.                 |
| `.with_details(json)`       | Attach a structured JSON payload, rendered under `"error.details"` in the response.          |
| `.with_locale_key(key)`     | Tag an existing error with a translation key without replacing the fallback message.        |

## Accessors

| Method               | Returns                                                                    |
| -------------------- | -------------------------------------------------------------------------- |
| `.status()`          | `StatusCode`                                                               |
| `.message()`         | `&str`                                                                     |
| `.details()`         | `Option<&serde_json::Value>`                                               |
| `.error_code()`      | `Option<&'static str>`                                                     |
| `.locale_key()`      | `Option<&'static str>`                                                     |
| `.source_as::<T>()`  | `Option<&T>` — downcast the source to `T` (`None` after clone or response) |
| `.is_lagged()`       | `bool` — `true` for `Error::lagged(..)` errors                             |

## Usage

### Basic handler errors

```rust,no_run
use modo::error::{Error, Result};

async fn get_user(id: u64) -> Result<String> {
    if id == 0 {
        return Err(Error::not_found("user not found"));
    }
    Ok("Alice".to_string())
}
```

### Builder pattern

```rust,no_run
use modo::error::Error;
use serde_json::json;

let err = Error::unprocessable_entity("validation failed")
    .with_details(json!({ "field": "email", "reason": "invalid format" }))
    .with_code("validation:email");
```

### Causal chaining

```rust,no_run
use modo::error::Error;
use std::io;

let io_err = io::Error::other("disk full");
let err = Error::internal("could not write file").chain(io_err);

// Pre-response: downcast the source while you still own the Error.
let source = err.source_as::<io::Error>();
assert!(source.is_some());
```

### Pre-response vs post-response identity

The source field is a `Box<dyn std::error::Error + Send + Sync>`, which is not
`Clone`. **Both `Clone` and `Error::into_response` drop the source.** This
means:

- **Pre-response** (inside a handler / middleware that still owns the `Error`):
  use `source_as::<T>()` to downcast the source to a concrete type.
- **Post-response** (middleware reading the error copy from response
  extensions): the source is gone; use `error_code()` instead. Attach it up
  front with `.with_code(..)`.

A typical pattern is `.chain(e).with_code(e.code())` — keep both the causal
source (inspectable pre-response) and the static code (stable post-response).

```rust,no_run
use modo::error::Error;
use axum::response::IntoResponse;

let err = Error::unauthorized("token expired").with_code("jwt:expired");
let response = err.into_response();

// Downstream middleware reads the code back from response extensions:
let ext = response.extensions().get::<Error>().unwrap();
assert_eq!(ext.error_code(), Some("jwt:expired"));
// ext.source_as::<SomeError>() is always None here — source was dropped.
```

### Using `Error::into_response()` in middleware and guards

Middleware and route guards that short-circuit with an error should build an
`Error` and call `.into_response()` — never hand-roll a raw
`axum::response::Response`. This keeps the JSON body shape and the
response-extension copy consistent across the framework.

```rust,no_run
use modo::error::Error;
use axum::response::{IntoResponse, Response};

fn require_role(role: Option<&str>) -> Result<(), Response> {
    let Some(role) = role else {
        return Err(Error::unauthorized("authentication required").into_response());
    };
    if role != "admin" {
        return Err(Error::forbidden("insufficient role").into_response());
    }
    Ok(())
}
```

### Localized errors

`Error::localized(status, key)` stores a translation key that the
`default_error_handler` middleware resolves into a user-facing string when a
`Translator` is present in the request extensions. Without the middleware (or
without a `Translator`), the response falls back to the raw key.

```rust,no_run
use modo::error::Error;
use modo::axum::http::StatusCode;

let err = Error::localized(StatusCode::NOT_FOUND, "errors.user.not_found");
assert_eq!(err.locale_key(), Some("errors.user.not_found"));
```

Use `.with_locale_key(key)` instead when you already have a descriptive
fallback message and want to add a translation key alongside it.

### Using HttpError

```rust,no_run
use modo::error::{Error, HttpError};

let err: Error = HttpError::NotFound.into();
assert_eq!(err.message(), "Not Found");
```

## Response Shape

`Error::into_response` produces a JSON body with the HTTP status code:

```json
{ "error": { "status": 404, "message": "user not found" } }
```

When `with_details` is called, a `"details"` key is added under `"error"`. A
copy of the error (without `source`) is also inserted into response
extensions under the type `Error`, so downstream middleware can inspect it.

## HttpError Variants

`HttpError` is a `Copy` enum with `status_code()` and `message()` methods. It
converts into `Error` via `From<HttpError>` using the canonical HTTP reason
phrase as the message.

| Variant                          | Status |
| -------------------------------- | ------ |
| `HttpError::BadRequest`          | 400    |
| `HttpError::Unauthorized`        | 401    |
| `HttpError::Forbidden`           | 403    |
| `HttpError::NotFound`            | 404    |
| `HttpError::MethodNotAllowed`    | 405    |
| `HttpError::Conflict`            | 409    |
| `HttpError::Gone`                | 410    |
| `HttpError::PayloadTooLarge`     | 413    |
| `HttpError::UnprocessableEntity` | 422    |
| `HttpError::TooManyRequests`     | 429    |
| `HttpError::InternalServerError` | 500    |
| `HttpError::BadGateway`          | 502    |
| `HttpError::ServiceUnavailable`  | 503    |
| `HttpError::GatewayTimeout`      | 504    |

## Automatic From Conversions

| Source type            | Resulting status |
| ---------------------- | ---------------- |
| `std::io::Error`       | 500              |
| `serde_json::Error`    | 400              |
| `serde_yaml_ng::Error` | 500              |
