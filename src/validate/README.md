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

Rules are applied through a `FieldValidator` obtained inside the `Validator::field` closure.
String rules require `T: AsRef<str>`; numeric rules require `T: PartialOrd + Display`.

## Usage

### Implementing `Validate` on a request struct

```rust
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

```rust
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

```rust
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

## Integration with modo

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

`Validate`, `ValidationError`, and `Validator` are also re-exported at the crate root
as `modo::Validate`, `modo::ValidationError`, and `modo::Validator`.
