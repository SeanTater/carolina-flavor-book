use anyhow::Result;
// use google_cloud_storage::client::{Client, ClientConfig};

#[derive(Clone)]
pub struct Database {
    pub pool: r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>,
    // pub gcs_client: Client,
}

impl Database {
    pub async fn connect_default() -> Result<Self> {
        let manager = r2d2_sqlite::SqliteConnectionManager::file("data/recipes.db");
        let pool = r2d2::Pool::new(manager)?;
        // let config = ClientConfig::default().with_auth().await.unwrap();
        // let gcs_client = Client::new(config);
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
