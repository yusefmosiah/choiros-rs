use async_trait::async_trait;
use sqlx::SqlitePool;
use time::OffsetDateTime;
use tower_sessions::{
    session::{Id, Record},
    session_store, SessionStore,
};
use tracing::error;

/// SQLite-backed session store using the hypervisor's existing SqlitePool.
///
/// Schema (created on [`SqliteSessionStore::migrate`]):
/// ```sql
/// CREATE TABLE IF NOT EXISTS sessions (
///     id          TEXT    PRIMARY KEY,
///     data        TEXT    NOT NULL,
///     expiry_date INTEGER NOT NULL   -- Unix timestamp (seconds)
/// );
/// ```
#[derive(Debug, Clone)]
pub struct SqliteSessionStore {
    pool: SqlitePool,
}

impl SqliteSessionStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Create the sessions table if it does not exist.
    pub async fn migrate(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS sessions (
                id          TEXT    PRIMARY KEY,
                data        TEXT    NOT NULL,
                expiry_date INTEGER NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS sessions_expiry ON sessions (expiry_date)",
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Delete all rows whose expiry_date is in the past.
    pub async fn delete_expired(&self) -> Result<(), sqlx::Error> {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        sqlx::query("DELETE FROM sessions WHERE expiry_date <= ?")
            .bind(now)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl SessionStore for SqliteSessionStore {
    async fn create(&self, record: &mut Record) -> session_store::Result<()> {
        let id = record.id.to_string();
        let data = serde_json::to_string(&record.data)
            .map_err(|e| session_store::Error::Encode(e.to_string()))?;
        let expiry = record.expiry_date.unix_timestamp();

        // Retry on ID collision (INSERT OR IGNORE + re-check).
        loop {
            let rows = sqlx::query(
                "INSERT OR IGNORE INTO sessions (id, data, expiry_date) VALUES (?, ?, ?)",
            )
            .bind(&id)
            .bind(&data)
            .bind(expiry)
            .execute(&self.pool)
            .await
            .map_err(|e| session_store::Error::Backend(e.to_string()))?
            .rows_affected();

            if rows > 0 {
                return Ok(());
            }

            // ID collision — generate a new one and retry.
            record.id = Id::default();
        }
    }

    async fn save(&self, record: &Record) -> session_store::Result<()> {
        let id = record.id.to_string();
        let data = serde_json::to_string(&record.data)
            .map_err(|e| session_store::Error::Encode(e.to_string()))?;
        let expiry = record.expiry_date.unix_timestamp();

        sqlx::query(
            "INSERT INTO sessions (id, data, expiry_date) VALUES (?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET data = excluded.data, expiry_date = excluded.expiry_date",
        )
        .bind(&id)
        .bind(&data)
        .bind(expiry)
        .execute(&self.pool)
        .await
        .map_err(|e| session_store::Error::Backend(e.to_string()))?;

        Ok(())
    }

    async fn load(&self, session_id: &Id) -> session_store::Result<Option<Record>> {
        let id = session_id.to_string();
        let now = OffsetDateTime::now_utc().unix_timestamp();

        let row: Option<(String, i64)> =
            sqlx::query_as("SELECT data, expiry_date FROM sessions WHERE id = ? AND expiry_date > ?")
                .bind(&id)
                .bind(now)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| session_store::Error::Backend(e.to_string()))?;

        match row {
            None => Ok(None),
            Some((data_json, expiry_ts)) => {
                let data = serde_json::from_str(&data_json)
                    .map_err(|e| session_store::Error::Decode(e.to_string()))?;
                let expiry_date = OffsetDateTime::from_unix_timestamp(expiry_ts)
                    .map_err(|e| session_store::Error::Decode(e.to_string()))?;
                Ok(Some(Record {
                    id: *session_id,
                    data,
                    expiry_date,
                }))
            }
        }
    }

    async fn delete(&self, session_id: &Id) -> session_store::Result<()> {
        let id = session_id.to_string();
        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(&id)
            .execute(&self.pool)
            .await
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;
        Ok(())
    }
}

/// Background task: delete expired sessions every `period`.
pub async fn run_expired_session_cleanup(
    store: SqliteSessionStore,
    period: std::time::Duration,
) {
    let mut interval = tokio::time::interval(period);
    interval.tick().await; // first tick is immediate; skip it
    loop {
        interval.tick().await;
        if let Err(e) = store.delete_expired().await {
            error!("session cleanup failed: {e}");
        }
    }
}
