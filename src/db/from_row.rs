use std::collections::HashMap;

use crate::error::{Error, Result};

/// Trait for converting a `libsql::Row` into a Rust struct.
///
/// Implement this per struct, choosing positional (`row.get(idx)`) or
/// name-based ([`ColumnMap`]) access for each column.
pub trait FromRow: Sized {
    /// Convert a row into `Self`.
    ///
    /// # Errors
    ///
    /// Returns an error if a column is missing or has an incompatible type.
    fn from_row(row: &libsql::Row) -> Result<Self>;
}

/// Column name to index lookup built from a single row's column metadata.
///
/// Construct one inside your [`FromRow`] implementation to access columns by
/// name instead of positional index.
pub struct ColumnMap {
    map: HashMap<String, i32>,
}

impl ColumnMap {
    /// Build lookup from a row's column metadata.
    pub fn from_row(row: &libsql::Row) -> Self {
        let count = row.column_count();
        let mut map = HashMap::with_capacity(count as usize);
        for i in 0..count {
            if let Some(name) = row.column_name(i) {
                map.insert(name.to_string(), i);
            }
        }
        Self { map }
    }

    /// Look up the column index by name.
    ///
    /// Returns the zero-based column index, or an error if the column is not found.
    ///
    /// # Errors
    ///
    /// Returns an error if the column name does not exist in the row.
    pub fn index(&self, name: &str) -> Result<i32> {
        self.map
            .get(name)
            .copied()
            .ok_or_else(|| Error::internal(format!("column not found: {name}")))
    }

    /// Get a typed value by column name.
    ///
    /// Looks up the column index by name and extracts the raw `libsql::Value`,
    /// then converts it via the [`FromValue`] trait.
    /// Supported types: `String`, `i32`, `i64`, `u32`, `u64`, `f64`, `bool`,
    /// `Vec<u8>`, `Option<T>`, and `libsql::Value`.
    ///
    /// # Errors
    ///
    /// Returns an error if the column is not found or the value cannot be
    /// converted to `T`.
    pub fn get<T: FromValue>(&self, row: &libsql::Row, name: &str) -> Result<T> {
        let idx = self.index(name)?;
        let val = row.get_value(idx).map_err(Error::from)?;
        T::from_value(val)
    }
}

/// Converts a `libsql::Value` into a concrete Rust type.
///
/// This trait mirrors the sealed `FromValue` inside libsql, providing the same
/// conversions for use with [`ColumnMap::get`].
///
/// Implemented for: `String`, `i32`, `i64`, `u32`, `u64`, `f64`, `bool`,
/// `Vec<u8>`, `Option<T>` (where `T: FromValue`), and `libsql::Value`.
pub trait FromValue: Sized {
    /// Convert a value into `Self`.
    ///
    /// # Errors
    ///
    /// Returns an error on type mismatch or unexpected null.
    fn from_value(val: libsql::Value) -> Result<Self>;
}

impl FromValue for libsql::Value {
    fn from_value(val: libsql::Value) -> Result<Self> {
        Ok(val)
    }
}

impl FromValue for String {
    fn from_value(val: libsql::Value) -> Result<Self> {
        match val {
            libsql::Value::Text(s) => Ok(s),
            libsql::Value::Null => Err(Error::internal("unexpected null value")),
            _ => Err(Error::internal("invalid column type: expected text")),
        }
    }
}

impl FromValue for i32 {
    fn from_value(val: libsql::Value) -> Result<Self> {
        match val {
            libsql::Value::Integer(i) => {
                i32::try_from(i).map_err(|_| Error::internal("integer out of i32 range"))
            }
            libsql::Value::Null => Err(Error::internal("unexpected null value")),
            _ => Err(Error::internal("invalid column type: expected integer")),
        }
    }
}

impl FromValue for u32 {
    fn from_value(val: libsql::Value) -> Result<Self> {
        match val {
            libsql::Value::Integer(i) => {
                u32::try_from(i).map_err(|_| Error::internal("integer out of u32 range"))
            }
            libsql::Value::Null => Err(Error::internal("unexpected null value")),
            _ => Err(Error::internal("invalid column type: expected integer")),
        }
    }
}

impl FromValue for i64 {
    fn from_value(val: libsql::Value) -> Result<Self> {
        match val {
            libsql::Value::Integer(i) => Ok(i),
            libsql::Value::Null => Err(Error::internal("unexpected null value")),
            _ => Err(Error::internal("invalid column type: expected integer")),
        }
    }
}

impl FromValue for u64 {
    fn from_value(val: libsql::Value) -> Result<Self> {
        match val {
            libsql::Value::Integer(i) => {
                u64::try_from(i).map_err(|_| Error::internal("integer out of u64 range"))
            }
            libsql::Value::Null => Err(Error::internal("unexpected null value")),
            _ => Err(Error::internal("invalid column type: expected integer")),
        }
    }
}

impl FromValue for f64 {
    fn from_value(val: libsql::Value) -> Result<Self> {
        match val {
            libsql::Value::Real(f) => Ok(f),
            libsql::Value::Integer(i) => Ok(i as f64),
            libsql::Value::Null => Err(Error::internal("unexpected null value")),
            _ => Err(Error::internal("invalid column type: expected real")),
        }
    }
}

impl FromValue for bool {
    fn from_value(val: libsql::Value) -> Result<Self> {
        match val {
            libsql::Value::Integer(0) => Ok(false),
            libsql::Value::Integer(_) => Ok(true),
            libsql::Value::Null => Err(Error::internal("unexpected null value")),
            _ => Err(Error::internal("invalid column type: expected integer")),
        }
    }
}

impl FromValue for Vec<u8> {
    fn from_value(val: libsql::Value) -> Result<Self> {
        match val {
            libsql::Value::Blob(b) => Ok(b),
            libsql::Value::Null => Err(Error::internal("unexpected null value")),
            _ => Err(Error::internal("invalid column type: expected blob")),
        }
    }
}

impl<T: FromValue> FromValue for Option<T> {
    fn from_value(val: libsql::Value) -> Result<Self> {
        match val {
            libsql::Value::Null => Ok(None),
            other => T::from_value(other).map(Some),
        }
    }
}
