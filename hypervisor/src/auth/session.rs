use tower_sessions::Session;

pub const SESSION_USER_ID_KEY: &str = "user_id";
pub const SESSION_USERNAME_KEY: &str = "username";

/// Extract the authenticated user ID from the session, if present.
pub async fn get_user_id(session: &Session) -> Option<String> {
    session
        .get::<String>(SESSION_USER_ID_KEY)
        .await
        .ok()
        .flatten()
}

/// Write user identity into the session after successful authentication.
pub async fn set_user(session: &Session, user_id: &str, username: &str) -> anyhow::Result<()> {
    session
        .insert(SESSION_USER_ID_KEY, user_id.to_string())
        .await?;
    session
        .insert(SESSION_USERNAME_KEY, username.to_string())
        .await?;
    Ok(())
}

/// Clear the session on logout.
pub async fn clear(session: &Session) -> anyhow::Result<()> {
    session.flush().await?;
    Ok(())
}
