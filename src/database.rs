use std::sync::Arc;

use anyhow::Result;
use rusqlite::params;

use crate::models::Recipe;

#[derive(Clone, Debug)]
pub struct Database {
    pub pool: r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>,
}

impl Database {
    pub async fn connect_default() -> Result<Self> {
        let manager = r2d2_sqlite::SqliteConnectionManager::file("recipes.db");
        let pool = r2d2::Pool::new(manager)?;
        let me = Self { pool };
        me.migrate().await?;
        Ok(me.into())
    }

    /// Migrate the database to the latest version.
    async fn migrate(&self) -> Result<()> {
        let migrations = [include_str!("migrations/01-initial.sql")];
        // Find the current migration version. If it fails, we need to run all the migrations.
        let conn = self.pool.get()?;
        let current_version = conn
            .query_row(
                "SELECT value FROM metadata WHERE key = 'schema_version'",
                rusqlite::params![],
                |row| row.get(0),
            )
            .unwrap_or(0);
        for migration in &migrations[current_version as usize..] {
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
}

pub trait FromRow {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self>
    where
        Self: Sized;
}
