# modo::sanitize

Input sanitization utilities for normalizing string fields before validation
or storage.

## Key types

| Item | Kind | Purpose |
|------|------|---------|
| `Sanitize` | trait | Implemented on input structs to normalize fields in place |
| `trim` | fn | Trim leading and trailing whitespace |
| `trim_lowercase` | fn | Trim whitespace and convert to lowercase |
| `collapse_whitespace` | fn | Collapse consecutive whitespace into a single space |
| `strip_html` | fn | Remove HTML tags and decode entities |
| `truncate` | fn | Limit string to a maximum character count |
| `normalize_email` | fn | Trim, lowercase, and strip `+tag` suffixes |

## Usage

### Implementing `Sanitize` on a request struct

```rust,ignore
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

The `JsonRequest`, `FormRequest`, `Query`, and `MultipartRequest` extractors
call `sanitize()` automatically after deserialization:

```rust,ignore
use modo::extractor::{JsonRequest, Query};

async fn create_user(JsonRequest(input): JsonRequest<CreateUserInput>) {
    // `input` has already been sanitized
}

async fn search(Query(params): Query<SearchParams>) {
    // `params` has already been sanitized
}
```

### Stripping HTML

```rust,ignore
use modo::sanitize::strip_html;

let mut field = String::from("<p>Hello <b>world</b></p><script>alert(1)</script>");
strip_html(&mut field);
assert_eq!(field, "Hello world");
```

### Normalizing email addresses

```rust,ignore
use modo::sanitize::normalize_email;

let mut email = String::from("  User+Tag@Example.COM  ");
normalize_email(&mut email);
assert_eq!(email, "user@example.com");
```

## Integration with modo

`Sanitize` and the helper functions all live under `modo::sanitize`:

```rust,ignore
use modo::sanitize::{Sanitize, trim, normalize_email, strip_html};
```
