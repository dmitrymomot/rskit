# modo::error

HTTP-aware error type for the modo web framework.

## Key Types

| Type        | Description                                                                                                              |
| ----------- | ------------------------------------------------------------------------------------------------------------------------ |
| `Error`     | The primary framework error: carries a status code, message, optional source, optional details, and optional error code. |
| `Result<T>` | Alias for `std::result::Result<T, Error>`.                                                                               |
| `HttpError` | Copy enum of common HTTP status categories; converts into `Error` via `From`.                                            |

## Usage

### Basic handler errors

```rust
use modo::error::{Error, Result};

async fn get_user(id: u64) -> Result<String> {
    if id == 0 {
        return Err(Error::not_found("user not found"));
    }
    Ok("Alice".to_string())
}
```

### Builder pattern

```rust
use modo::error::Error;
use serde_json::json;

let err = Error::unprocessable_entity("validation failed")
    .with_details(json!({ "field": "email", "reason": "invalid format" }))
    .with_code("validation:email");
```

### Causal chaining

```rust
use modo::error::Error;
use std::io;

let io_err = io::Error::other("disk full");
let err = Error::internal("could not write file").chain(io_err);

// Before the error becomes a response, downcast the source:
let source = err.source_as::<io::Error>();
assert!(source.is_some());
```

### Error identity across response boundaries

`source` is dropped when `Error` is cloned or serialised into a response. Attach a static
error code to preserve identity for downstream middleware:

```rust
use modo::error::Error;
use axum::response::IntoResponse;

let err = Error::unauthorized("token expired").with_code("jwt:expired");
let response = err.into_response();

// Middleware reads the code back from response extensions:
let ext = response.extensions().get::<Error>().unwrap();
assert_eq!(ext.error_code(), Some("jwt:expired"));
```

### Using HttpError

```rust
use modo::error::{Error, HttpError};

let err: Error = HttpError::NotFound.into();
assert_eq!(err.message(), "Not Found");
```

## Response Shape

`Error::into_response` produces a JSON body with the HTTP status code:

```json
{ "error": { "status": 404, "message": "user not found" } }
```

When `with_details` is called, a `"details"` key is added under `"error"`. A copy of the
error (without `source`) is also inserted into response extensions under the type `Error`.

## Status-Code Constructors

| Method                             | Status |
| ---------------------------------- | ------ |
| `Error::bad_request(msg)`          | 400    |
| `Error::unauthorized(msg)`         | 401    |
| `Error::forbidden(msg)`            | 403    |
| `Error::not_found(msg)`            | 404    |
| `Error::conflict(msg)`             | 409    |
| `Error::unprocessable_entity(msg)` | 422    |
| `Error::payload_too_large(msg)`    | 413    |
| `Error::too_many_requests(msg)`    | 429    |
| `Error::internal(msg)`             | 500    |
| `Error::bad_gateway(msg)`          | 502    |
| `Error::gateway_timeout(msg)`      | 504    |

## Automatic From Conversions

| Source type            | Resulting status |
| ---------------------- | ---------------- |
| `std::io::Error`       | 500              |
| `serde_json::Error`    | 400              |
| `serde_yaml_ng::Error` | 500              |
