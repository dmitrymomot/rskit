use sqlx::Arguments;
use sqlx::sqlite::SqliteArguments;

/// Owned, cloneable representation of a single SQLite bind parameter.
///
/// Used as the internal storage type for deferred query parameters.
/// You should not need to construct this directly; use [`IntoSqliteValue`] impls.
#[doc(hidden)]
#[derive(Clone, Debug)]
pub enum SqliteValue {
    Null,
    Bool(bool),
    Int(i32),
    Int64(i64),
    Double(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl SqliteValue {
    /// Push this value into a [`SqliteArguments`] buffer.
    pub(crate) fn add_to(self, args: &mut SqliteArguments<'_>) {
        match self {
            Self::Null => args.add(Option::<String>::None).unwrap(),
            Self::Bool(v) => args.add(v).unwrap(),
            Self::Int(v) => args.add(v).unwrap(),
            Self::Int64(v) => args.add(v).unwrap(),
            Self::Double(v) => args.add(v).unwrap(),
            Self::Text(v) => args.add(v).unwrap(),
            Self::Blob(v) => args.add(v).unwrap(),
        }
    }
}

/// Convert a Rust value into a [`SqliteValue`] for deferred binding.
#[doc(hidden)]
pub trait IntoSqliteValue {
    fn into_sqlite_value(self) -> SqliteValue;
}

impl IntoSqliteValue for bool {
    fn into_sqlite_value(self) -> SqliteValue {
        SqliteValue::Bool(self)
    }
}

impl IntoSqliteValue for i32 {
    fn into_sqlite_value(self) -> SqliteValue {
        SqliteValue::Int(self)
    }
}

impl IntoSqliteValue for i64 {
    fn into_sqlite_value(self) -> SqliteValue {
        SqliteValue::Int64(self)
    }
}

impl IntoSqliteValue for f64 {
    fn into_sqlite_value(self) -> SqliteValue {
        SqliteValue::Double(self)
    }
}

impl IntoSqliteValue for String {
    fn into_sqlite_value(self) -> SqliteValue {
        SqliteValue::Text(self)
    }
}

impl IntoSqliteValue for &str {
    fn into_sqlite_value(self) -> SqliteValue {
        SqliteValue::Text(self.to_owned())
    }
}

impl IntoSqliteValue for Vec<u8> {
    fn into_sqlite_value(self) -> SqliteValue {
        SqliteValue::Blob(self)
    }
}

impl IntoSqliteValue for &[u8] {
    fn into_sqlite_value(self) -> SqliteValue {
        SqliteValue::Blob(self.to_vec())
    }
}

impl<T: IntoSqliteValue> IntoSqliteValue for Option<T> {
    fn into_sqlite_value(self) -> SqliteValue {
        match self {
            Some(v) => v.into_sqlite_value(),
            None => SqliteValue::Null,
        }
    }
}

/// Build a [`SqliteArguments`] from a slice of [`SqliteValue`]s.
pub(crate) fn build_args(values: &[SqliteValue]) -> SqliteArguments<'static> {
    let mut args = SqliteArguments::default();
    for v in values {
        v.clone().add_to(&mut args);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_converts_to_text() {
        let val = "hello".into_sqlite_value();
        assert!(matches!(val, SqliteValue::Text(ref s) if s == "hello"));
    }

    #[test]
    fn owned_string_converts_to_text() {
        let val = String::from("world").into_sqlite_value();
        assert!(matches!(val, SqliteValue::Text(ref s) if s == "world"));
    }

    #[test]
    fn i32_converts_to_int() {
        let val = 42i32.into_sqlite_value();
        assert!(matches!(val, SqliteValue::Int(42)));
    }

    #[test]
    fn i64_converts_to_int64() {
        let val = 100i64.into_sqlite_value();
        assert!(matches!(val, SqliteValue::Int64(100)));
    }

    #[test]
    fn f64_converts_to_double() {
        let val = 1.5f64.into_sqlite_value();
        assert!(matches!(val, SqliteValue::Double(v) if (v - 1.5).abs() < f64::EPSILON));
    }

    #[test]
    fn bool_converts() {
        let val = true.into_sqlite_value();
        assert!(matches!(val, SqliteValue::Bool(true)));
    }

    #[test]
    fn none_string_converts_to_null() {
        let val: Option<String> = None;
        let sv = val.into_sqlite_value();
        assert!(matches!(sv, SqliteValue::Null));
    }

    #[test]
    fn some_string_converts_to_text() {
        let val: Option<String> = Some("hi".into());
        let sv = val.into_sqlite_value();
        assert!(matches!(sv, SqliteValue::Text(ref s) if s == "hi"));
    }

    #[test]
    fn clone_preserves_value() {
        let val = "cloned".into_sqlite_value();
        let val2 = val.clone();
        assert!(matches!(val2, SqliteValue::Text(ref s) if s == "cloned"));
    }
}
