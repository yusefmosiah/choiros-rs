use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use sqlx::{Row, SqlitePool};

use crate::sandbox::SandboxRole;

const POINTER_MAIN: &str = "main";
const POINTER_DEV: &str = "dev";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PointerTarget {
    Role(SandboxRole),
    Branch(String),
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RoutePointerRecord {
    pub pointer_name: String,
    pub target_kind: String,
    pub target_value: String,
    pub created_at: i64,
    pub updated_at: i64,
}

pub fn is_valid_pointer_name(pointer_name: &str) -> bool {
    !pointer_name.trim().is_empty()
        && pointer_name.len() <= 64
        && pointer_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

pub fn is_valid_branch_name(branch: &str) -> bool {
    !branch.trim().is_empty()
        && branch.len() <= 64
        && branch
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

pub async fn ensure_default_pointers(pool: &SqlitePool, user_id: &str) -> anyhow::Result<()> {
    let now = unix_ts();

    sqlx::query(
        r#"
        INSERT OR IGNORE INTO route_pointers (
            user_id, pointer_name, target_kind, target_value, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(user_id)
    .bind(POINTER_MAIN)
    .bind("role")
    .bind("live")
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .with_context(|| format!("insert default pointer '{POINTER_MAIN}' for user {user_id}"))?;

    sqlx::query(
        r#"
        INSERT OR IGNORE INTO route_pointers (
            user_id, pointer_name, target_kind, target_value, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(user_id)
    .bind(POINTER_DEV)
    .bind("role")
    .bind("dev")
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .with_context(|| format!("insert default pointer '{POINTER_DEV}' for user {user_id}"))?;

    Ok(())
}

pub async fn resolve_pointer_target(
    pool: &SqlitePool,
    user_id: &str,
    pointer_name: &str,
) -> anyhow::Result<Option<PointerTarget>> {
    if !is_valid_pointer_name(pointer_name) {
        anyhow::bail!("invalid pointer name '{pointer_name}' (allowed: [A-Za-z0-9._-], max 64)");
    }

    let row_opt = sqlx::query(
        r#"
        SELECT target_kind, target_value
        FROM route_pointers
        WHERE user_id = ? AND pointer_name = ?
        LIMIT 1
        "#,
    )
    .bind(user_id)
    .bind(pointer_name)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row_opt else {
        return Ok(None);
    };

    let target_kind: String = row.get("target_kind");
    let target_value: String = row.get("target_value");

    let target = match target_kind.as_str() {
        "role" => match target_value.as_str() {
            "live" => PointerTarget::Role(SandboxRole::Live),
            "dev" => PointerTarget::Role(SandboxRole::Dev),
            _ => anyhow::bail!("unsupported role pointer target: {target_value}"),
        },
        "branch" => {
            if !is_valid_branch_name(&target_value) {
                anyhow::bail!("invalid branch pointer target: {target_value}");
            }
            PointerTarget::Branch(target_value)
        }
        _ => anyhow::bail!("unsupported pointer target kind: {target_kind}"),
    };

    Ok(Some(target))
}

pub async fn list_route_pointers(
    pool: &SqlitePool,
    user_id: &str,
) -> anyhow::Result<Vec<RoutePointerRecord>> {
    ensure_default_pointers(pool, user_id).await?;

    let rows = sqlx::query(
        r#"
        SELECT pointer_name, target_kind, target_value, created_at, updated_at
        FROM route_pointers
        WHERE user_id = ?
        ORDER BY pointer_name ASC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| RoutePointerRecord {
            pointer_name: row.get("pointer_name"),
            target_kind: row.get("target_kind"),
            target_value: row.get("target_value"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
        .collect())
}

pub async fn upsert_route_pointer(
    pool: &SqlitePool,
    user_id: &str,
    pointer_name: &str,
    target: &PointerTarget,
) -> anyhow::Result<()> {
    if !is_valid_pointer_name(pointer_name) {
        anyhow::bail!("invalid pointer name '{pointer_name}' (allowed: [A-Za-z0-9._-], max 64)");
    }

    let (target_kind, target_value) = match target {
        PointerTarget::Role(role) => ("role".to_string(), role.to_string()),
        PointerTarget::Branch(branch) => {
            if !is_valid_branch_name(branch) {
                anyhow::bail!("invalid branch name '{branch}'");
            }
            ("branch".to_string(), branch.to_string())
        }
    };

    let now = unix_ts();
    sqlx::query(
        r#"
        INSERT INTO route_pointers (
            user_id, pointer_name, target_kind, target_value, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(user_id, pointer_name)
        DO UPDATE SET
            target_kind = excluded.target_kind,
            target_value = excluded.target_value,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(user_id)
    .bind(pointer_name)
    .bind(&target_kind)
    .bind(&target_value)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;

    let detail_json = serde_json::json!({
        "pointer_name": pointer_name,
        "target_kind": target_kind,
        "target_value": target_value
    })
    .to_string();

    sqlx::query(
        r#"
        INSERT INTO runtime_events (user_id, runtime_id, event_type, detail_json, correlation_id, created_at)
        VALUES (?, NULL, 'pointer.swap', ?, NULL, ?)
        "#,
    )
    .bind(user_id)
    .bind(detail_json)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(())
}

fn unix_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_default_pointers, list_route_pointers, resolve_pointer_target, upsert_route_pointer,
        PointerTarget,
    };
    use crate::{db, sandbox::SandboxRole};

    async fn with_test_db() -> sqlx::SqlitePool {
        let path = std::env::temp_dir().join(format!(
            "hypervisor-runtime-registry-{}.db",
            uuid::Uuid::new_v4()
        ));
        let db_url = format!("sqlite:{}", path.display());
        db::connect(&db_url).await.expect("db connect")
    }

    async fn insert_user(pool: &sqlx::SqlitePool, user_id: &str) {
        sqlx::query(
            r#"
            INSERT INTO users (id, username, display_name, created_at)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(user_id)
        .bind(format!("{user_id}@example.com"))
        .bind(user_id)
        .bind(0i64)
        .execute(pool)
        .await
        .expect("insert user");
    }

    #[tokio::test]
    async fn defaults_resolve_to_live_and_dev_roles() {
        let pool = with_test_db().await;
        let user = "user-defaults";
        insert_user(&pool, user).await;

        ensure_default_pointers(&pool, user)
            .await
            .expect("defaults");

        let main = resolve_pointer_target(&pool, user, "main")
            .await
            .expect("resolve")
            .expect("main exists");
        let dev = resolve_pointer_target(&pool, user, "dev")
            .await
            .expect("resolve")
            .expect("dev exists");

        assert_eq!(main, PointerTarget::Role(SandboxRole::Live));
        assert_eq!(dev, PointerTarget::Role(SandboxRole::Dev));
    }

    #[tokio::test]
    async fn upsert_pointer_to_branch_is_reflected_in_resolution_and_listing() {
        let pool = with_test_db().await;
        let user = "user-pointer";
        insert_user(&pool, user).await;

        upsert_route_pointer(
            &pool,
            user,
            "main",
            &PointerTarget::Branch("feature_login".to_string()),
        )
        .await
        .expect("upsert pointer");

        let main = resolve_pointer_target(&pool, user, "main")
            .await
            .expect("resolve main")
            .expect("main exists");
        assert_eq!(main, PointerTarget::Branch("feature_login".to_string()));

        let pointers = list_route_pointers(&pool, user)
            .await
            .expect("list pointers");
        assert!(pointers.iter().any(|p| {
            p.pointer_name == "main"
                && p.target_kind == "branch"
                && p.target_value == "feature_login"
        }));
    }

    #[tokio::test]
    async fn invalid_pointer_or_branch_rejected() {
        let pool = with_test_db().await;
        let user = "user-invalid";
        insert_user(&pool, user).await;

        let bad_pointer = upsert_route_pointer(
            &pool,
            user,
            "main/invalid",
            &PointerTarget::Role(SandboxRole::Live),
        )
        .await;
        assert!(bad_pointer.is_err());

        let bad_branch = upsert_route_pointer(
            &pool,
            user,
            "main",
            &PointerTarget::Branch("bad/branch".to_string()),
        )
        .await;
        assert!(bad_branch.is_err());
    }
}
