use std::sync::Arc;

/// Clone-able, single-connection database handle.
///
/// Wraps a `libsql::Database` and `libsql::Connection` behind an `Arc`.
/// Cloning is cheap (reference count increment). Use [`conn()`](Self::conn)
/// to access the underlying `libsql::Connection` for queries.
///
/// Created by [`connect`](super::connect).
#[derive(Clone)]
pub struct Database {
    inner: Arc<Inner>,
}

struct Inner {
    #[allow(dead_code)]
    db: libsql::Database,
    conn: libsql::Connection,
}

impl Database {
    pub(crate) fn new(db: libsql::Database, conn: libsql::Connection) -> Self {
        Self {
            inner: Arc::new(Inner { db, conn }),
        }
    }

    /// Returns a reference to the underlying libsql connection.
    pub fn conn(&self) -> &libsql::Connection {
        &self.inner.conn
    }
}
