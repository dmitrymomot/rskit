use std::sync::Arc;

/// Single-connection database handle. Clone-able (Arc internally).
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
