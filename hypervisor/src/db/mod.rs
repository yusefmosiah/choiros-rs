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
