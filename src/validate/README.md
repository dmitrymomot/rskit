# modo::validate

Input validation for request data in the `modo` web framework.

This module provides a fluent `Validator` builder that collects per-field
errors across all fields before returning, and a `Validate` trait for types
that validate themselves. `ValidationError` converts automatically into
`modo::Error` (HTTP 422) with the field map in the response `details`.

## Key Types

| Type              | Description                                                                          |
| ----------------- | ------------------------------------------------------------------------------------ |
| `Validator`       | Fluent builder that collects errors for multiple fields and returns them all at once |
| `ValidationError` | Per-field error collection; converts into `Error` (HTTP 422)                         |
| `Validate`        | Trait for types that know how to validate themselves                                 |

`FieldValidator` is the per-field rule chain — it is handed to you inside the
`Validator::field` closure and is never constructed directly.

Rules are applied through a `FieldValidator` obtained inside the `Validator::field` closure.
String rules require `T: AsRef<str>`; numeric rules require `T: PartialOrd + Display`.

## Usage

### Implementing `Validate` on a request struct

```rust,ignore
use modo::validate::{Validate, ValidationError, Validator};

struct CreateUser {
    name: String,
    email: String,
    age: u32,
}

impl Validate for CreateUser {
    fn validate(&self) -> Result<(), ValidationError> {
        Validator::new()
            .field("name", &self.name, |f| f.required().min_length(2).max_length(100))
            .field("email", &self.email, |f| f.required().email())
            .field("age", &self.age, |f| f.range(18..=120))
            .check()
    }
}
```

### Using `Validator` inline in a handler

```rust,ignore
use modo::validate::Validator;

async fn handler(name: String, email: String) -> modo::Result<()> {
    Validator::new()
        .field("name", &name, |f| f.required().min_length(2))
        .field("email", &email, |f| f.required().email())
        .check()?;  // converts ValidationError -> modo::Error (HTTP 422)

    Ok(())
}
```

### Inspecting errors after validation

```rust,ignore
use modo::validate::Validator;

let result = Validator::new()
    .field("email", &"bad-input", |f| f.email())
    .check();

if let Err(e) = result {
    // iterate all field errors
    for (field, messages) in e.fields() {
        println!("{field}: {}", messages.join(", "));
    }

    // query a specific field
    let errs = e.field_errors("email");
    assert!(!errs.is_empty());
}
```

## Available Rules

### String rules (`T: AsRef<str>`)

| Method                       | Description                                                    |
| ---------------------------- | -------------------------------------------------------------- |
| `required()`                 | Value must not be empty after trimming                         |
| `min_length(n)`              | At least `n` Unicode characters                                |
| `max_length(n)`              | At most `n` Unicode characters                                 |
| `email()`                    | Simple structural email check                                  |
| `url()`                      | Must start with `http://` or `https://` and contain no spaces  |
| `one_of(options)`            | Value must equal one of the provided string slices             |
| `matches_regex(pattern)`     | Value must match the given regex; records error on bad pattern |
| `custom(predicate, message)` | User-supplied predicate; records `message` on failure          |

### Numeric rules (`T: PartialOrd + Display`)

| Method               | Description                              |
| -------------------- | ---------------------------------------- |
| `range(start..=end)` | Value must be within the inclusive range |

### Cross-field validation with `Validator`

`Validator` gathers errors from multiple fields — including rules that depend
on more than one field — before returning. Use `custom()` for per-field
predicates, and chain an extra `field(...)` with a synthetic name for
genuinely cross-field checks:

```rust,ignore
use modo::validate::{Validate, ValidationError, Validator};

struct ChangePassword {
    new_password: String,
    confirm_password: String,
}

impl Validate for ChangePassword {
    fn validate(&self) -> Result<(), ValidationError> {
        Validator::new()
            .field("new_password", &self.new_password, |f| {
                f.required().min_length(8)
            })
            .field("confirm_password", &self.confirm_password, |f| {
                f.required().custom(
                    |v| v == self.new_password,
                    "must match new_password",
                )
            })
            .check()
    }
}
```

## Integration with `JsonRequest` / `FormRequest`

The `JsonRequest<T>` and `FormRequest<T>` extractors deserialize and sanitize
the body but do not automatically call `validate()`. Invoke it explicitly in
the handler — `?` converts `ValidationError` into an HTTP 422 response:

```rust,ignore
use modo::extractor::JsonRequest;
use modo::prelude::*;
use serde::Deserialize;

#[derive(Deserialize)]
struct CreateUser {
    name: String,
    email: String,
}

impl modo::sanitize::Sanitize for CreateUser {
    fn sanitize(&mut self) {
        self.name = self.name.trim().to_string();
        self.email = self.email.trim().to_lowercase();
    }
}

impl Validate for CreateUser {
    fn validate(&self) -> Result<(), ValidationError> {
        Validator::new()
            .field("name", &self.name, |f| f.required().min_length(2))
            .field("email", &self.email, |f| f.required().email())
            .check()
    }
}

async fn create(JsonRequest(body): JsonRequest<CreateUser>) -> Result<()> {
    body.validate()?; // HTTP 422 on failure
    // ... persist body ...
    Ok(())
}
```

The same pattern works with `FormRequest<T>` for `application/x-www-form-urlencoded`
bodies.

## Error response shape

`ValidationError` implements `From<ValidationError> for modo::Error`, so `?`
propagates it as an HTTP 422 response with a JSON body:

```json
{
    "error": {
        "status": 422,
        "message": "validation failed",
        "details": {
            "email": ["must be a valid email address"],
            "name": ["is required"]
        }
    }
}
```

`Validate`, `ValidationError`, and `Validator` are in `modo::prelude`; they
are also reachable via `modo::validate::{Validate, ValidationError, Validator}`.
