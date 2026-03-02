use crate::config::DatabaseConfig;
use anyhow::Result;

#[derive(Clone)]
pub struct Database {
    pub pool: r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>,
}

impl Database {
    pub async fn connect(conf: &DatabaseConfig) -> Result<Self> {
        let manager = r2d2_sqlite::SqliteConnectionManager::file(conf.path.clone());
        let pool = r2d2::Pool::new(manager)?;
        let me = Self { pool };
        me.migrate().await?;
        Ok(me)
    }

    /// Connect to an in-memory SQLite database for testing.
    /// Uses shared cache mode so multiple connections share the same in-memory DB.
    pub async fn connect_memory() -> Result<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let uri = format!("file:memdb{}?mode=memory&cache=shared", id);
        let manager = r2d2_sqlite::SqliteConnectionManager::file(uri);
        let pool = r2d2::Pool::new(manager)?;
        let me = Self { pool };
        me.migrate().await?;
        Ok(me)
    }

    /// Migrate the database to the latest version.
    async fn migrate(&self) -> Result<()> {
        let migrations = [
            include_str!("migrations/01-initial.sql"),
            include_str!("migrations/02-embeddings.sql"),
            include_str!("migrations/03-repair-revisions.sql"),
            include_str!("migrations/04-tasks.sql"),
            include_str!("migrations/05-add-indexes.sql"),
            include_str!("migrations/06-names-are-revisions.sql"),
            include_str!("migrations/07-image-extra-index.sql"),
            include_str!("migrations/08-image-prompts.sql"),
        ];
        // Find the current migration version. If it fails, we need to run all the migrations.
        let conn = self.pool.get()?;
        let current_version: String = conn
            .query_row(
                "SELECT value FROM metadata WHERE key = 'schema_version'",
                rusqlite::params![],
                |row| row.get(0),
            )
            .unwrap_or("0".to_string());
        let current_version = current_version.parse::<u32>().unwrap_or(0);
        tracing::warn!("Current schema version: {}", current_version);
        for migration in &migrations[current_version as usize..] {
            tracing::warn!("Applying migration: {}", migration);
            conn.execute_batch(migration)?;
        }
        Ok(())
    }

    /// Convenience method to collect rows from a query into a Vec.
    pub fn collect_rows<T: FromRow, P: rusqlite::Params>(
        &self,
        sql: &str,
        parameters: P,
    ) -> Result<Vec<T>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query(parameters)?;
        rows.mapped(T::from_row)
            .map(|r| r.map_err(Into::into))
            .collect::<Result<_>>()
    }

    /// Pull a whole table into memory.
    pub fn collect_table<T: FromRow>(&self, table: &str) -> Result<Vec<T>> {
        self.collect_rows(&format!("SELECT * FROM {}", table), [])
    }
}

pub trait FromRow {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self>
    where
        Self: Sized;
}
