/// Generates a ULID-based newtype ID with standard trait implementations.
///
/// # Usage
///
/// ```rust,ignore
/// modo::ulid_id!(SessionId);
/// modo::ulid_id!(JobId);
/// ```
///
/// Generates:
/// - A newtype struct wrapping `String`
/// - `new()` → generates a new ULID
/// - `from_raw(impl Into<String>)` → wraps existing string without validation
/// - `as_str()` → borrows inner string
/// - `into_string()` → consumes and returns inner string
/// - `Default` (delegates to `new()`)
/// - `Display`, `FromStr` (infallible)
/// - `Serialize`, `Deserialize`
/// - `From<String>`, `From<&str>`, `AsRef<str>`
/// - `Debug`, `Clone`, `PartialEq`, `Eq`, `Hash`
#[macro_export]
macro_rules! ulid_id {
    ($name:ident) => {
        /// Unique identifier backed by a ULID string.
        #[derive(
            Debug, Clone, PartialEq, Eq, Hash, $crate::serde::Serialize, $crate::serde::Deserialize,
        )]
        pub struct $name(String);

        impl $name {
            /// Generate a new, globally unique ID.
            pub fn new() -> Self {
                Self($crate::ulid::Ulid::new().to_string())
            }

            /// Wrap an existing string as an ID without validation.
            pub fn from_raw(s: impl Into<String>) -> Self {
                Self(s.into())
            }

            /// Borrow the underlying ULID string.
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Consume the ID, returning the inner `String`.
            pub fn into_string(self) -> String {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl std::str::FromStr for $name {
            type Err = std::convert::Infallible;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(s.to_string()))
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}
