use sqlx::SqlitePool;

pub async fn connect(database_url: &str) -> anyhow::Result<SqlitePool> {
    // Resolve the file path and ensure the parent directory exists.
    // Handles both "sqlite:./foo.db" and "sqlite:../foo.db" forms.
    let file_path = database_url.strip_prefix("sqlite:").unwrap_or(database_url);

    let abs_path = std::env::current_dir()?.join(file_path);
    if let Some(parent) = abs_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let pool = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&abs_path)
            .create_if_missing(true),
    )
    .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::connect;
    use sqlx::Row;

    #[tokio::test]
    async fn connect_applies_runtime_registry_migration() {
        let tmp_db =
            std::env::temp_dir().join(format!("hypervisor-migration-{}.db", uuid::Uuid::new_v4()));
        let database_url = format!("sqlite:{}", tmp_db.display());

        let pool = connect(&database_url).await.expect("db should connect");

        let expected_tables = [
            "users",
            "passkeys",
            "recovery_codes",
            "audit_log",
            "user_vms",
            "branch_runtimes",
            "route_pointers",
            "runtime_events",
        ];

        for table in expected_tables {
            let row = sqlx::query(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ? LIMIT 1",
            )
            .bind(table)
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|_| panic!("expected table to exist: {table}"));

            let actual: String = row.get("name");
            assert_eq!(actual, table);
        }

        pool.close().await;
        let _ = tokio::fs::remove_file(&tmp_db).await;
    }
}
