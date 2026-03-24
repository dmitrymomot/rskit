# modo::sanitize

Input sanitization utilities for the modo web framework.

The module provides the `Sanitize` trait and a collection of standalone helper
functions for normalizing `String` fields before validation or storage. The
`JsonRequest`, `FormRequest`, and `Query` extractors all call `sanitize()`
automatically after deserialization, so implementing the trait on a struct is
the only wiring required.

## Key Types

| Item                  | Kind  | Purpose                                                   |
| --------------------- | ----- | --------------------------------------------------------- |
| `Sanitize`            | trait | Implemented on input structs to normalize fields in place |
| `trim`                | fn    | Trim leading and trailing whitespace                      |
| `trim_lowercase`      | fn    | Trim whitespace and convert to lowercase                  |
| `collapse_whitespace` | fn    | Collapse consecutive whitespace into a single space       |
| `strip_html`          | fn    | Remove HTML tags and decode entities                      |
| `truncate`            | fn    | Limit string to a maximum character count                 |
| `normalize_email`     | fn    | Trim, lowercase, and strip `+tag` suffixes                |

## Usage

### Implementing `Sanitize` on a request struct

```rust
use modo::sanitize::{Sanitize, trim, trim_lowercase, normalize_email, truncate};

#[derive(serde::Deserialize)]
struct CreateUserInput {
    username: String,
    email: String,
    bio: String,
}

impl Sanitize for CreateUserInput {
    fn sanitize(&mut self) {
        trim_lowercase(&mut self.username);
        normalize_email(&mut self.email);
        trim(&mut self.bio);
        truncate(&mut self.bio, 500);
    }
}
```

### Using the extractors

Once `Sanitize` is implemented, the extractors handle sanitization automatically:

```rust
use axum::routing::post;
use axum::Router;
use modo::extractor::{JsonRequest, FormRequest, Query};

async fn create_user(JsonRequest(input): JsonRequest<CreateUserInput>) {
    // `input` has already been sanitized
}

async fn search(Query(params): Query<SearchParams>) {
    // `params` has already been sanitized
}
```

### Stripping HTML

```rust
use modo::sanitize::strip_html;

let mut field = String::from("<p>Hello <b>world</b></p><script>alert(1)</script>");
strip_html(&mut field);
assert_eq!(field, "Hello world");
```

### Normalizing email addresses

```rust
use modo::sanitize::normalize_email;

let mut email = String::from("  User+Tag@Example.COM  ");
normalize_email(&mut email);
assert_eq!(email, "user@example.com");
```

## Integration with modo

`Sanitize` is re-exported at the crate root as `modo::Sanitize`.

```rust
use modo::Sanitize;
```

The functions are available under `modo::sanitize::*`:

```rust
use modo::sanitize::{trim, normalize_email, strip_html, truncate, collapse_whitespace, trim_lowercase};
```
