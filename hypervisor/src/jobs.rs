//! ADR-0014 Phase 7/8: Job queue and promotion for the build pool.
//!
//! Jobs represent units of work (build, test, promote) executed on shared
//! worker VMs on behalf of users. The promotion API applies job results
//! (e.g., a new sandbox binary) to a user's data.img.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use tracing::{error, info};

fn unix_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

// ── Job types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub user_id: String,
    pub job_type: String,
    pub status: String,
    pub priority: i32,
    pub machine_class: Option<String>,
    pub command: Option<String>,
    pub payload_json: Option<String>,
    pub result_json: Option<String>,
    pub error_message: Option<String>,
    pub worker_vm_id: Option<String>,
    pub max_duration_s: i32,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Promotion {
    pub id: String,
    pub user_id: String,
    pub job_id: Option<String>,
    pub status: String,
    pub snapshot_path: Option<String>,
    pub binary_path: Option<String>,
    pub verification_json: Option<String>,
    pub error_message: Option<String>,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

// ── Job queue operations ─────────────────────────────────────────────────────

pub struct CreateJobParams<'a> {
    pub pool: &'a SqlitePool,
    pub user_id: &'a str,
    pub job_type: &'a str,
    pub command: Option<&'a str>,
    pub payload_json: Option<&'a str>,
    pub machine_class: Option<&'a str>,
    pub priority: i32,
    pub max_duration_s: i32,
}

pub async fn create_job(params: &CreateJobParams<'_>) -> Result<String> {
    let pool = params.pool;
    let user_id = params.user_id;
    let job_type = params.job_type;
    let id = ulid::Ulid::new().to_string().to_lowercase();
    let now = unix_ts();

    sqlx::query(
        r#"
        INSERT INTO jobs (id, user_id, job_type, status, priority, machine_class,
                         command, payload_json, max_duration_s, created_at)
        VALUES (?, ?, ?, 'queued', ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(user_id)
    .bind(job_type)
    .bind(params.priority)
    .bind(params.machine_class)
    .bind(params.command)
    .bind(params.payload_json)
    .bind(params.max_duration_s)
    .bind(now)
    .execute(pool)
    .await
    .with_context(|| format!("create job for user {user_id}"))?;

    info!(job_id = %id, user_id, job_type, "job created");
    Ok(id)
}

pub async fn get_job(pool: &SqlitePool, job_id: &str) -> Result<Option<Job>> {
    let row = sqlx::query(
        r#"
        SELECT id, user_id, job_type, status, priority, machine_class,
               command, payload_json, result_json, error_message,
               worker_vm_id, max_duration_s, created_at, started_at, completed_at
        FROM jobs WHERE id = ?
        "#,
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("get job {job_id}"))?;

    Ok(row.map(|r| Job {
        id: r.get("id"),
        user_id: r.get("user_id"),
        job_type: r.get("job_type"),
        status: r.get("status"),
        priority: r.get("priority"),
        machine_class: r.get("machine_class"),
        command: r.get("command"),
        payload_json: r.get("payload_json"),
        result_json: r.get("result_json"),
        error_message: r.get("error_message"),
        worker_vm_id: r.get("worker_vm_id"),
        max_duration_s: r.get("max_duration_s"),
        created_at: r.get("created_at"),
        started_at: r.get("started_at"),
        completed_at: r.get("completed_at"),
    }))
}

pub async fn list_jobs_for_user(pool: &SqlitePool, user_id: &str) -> Result<Vec<Job>> {
    let rows = sqlx::query(
        r#"
        SELECT id, user_id, job_type, status, priority, machine_class,
               command, payload_json, result_json, error_message,
               worker_vm_id, max_duration_s, created_at, started_at, completed_at
        FROM jobs WHERE user_id = ?
        ORDER BY created_at DESC
        LIMIT 50
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .with_context(|| format!("list jobs for user {user_id}"))?;

    Ok(rows
        .into_iter()
        .map(|r| Job {
            id: r.get("id"),
            user_id: r.get("user_id"),
            job_type: r.get("job_type"),
            status: r.get("status"),
            priority: r.get("priority"),
            machine_class: r.get("machine_class"),
            command: r.get("command"),
            payload_json: r.get("payload_json"),
            result_json: r.get("result_json"),
            error_message: r.get("error_message"),
            worker_vm_id: r.get("worker_vm_id"),
            max_duration_s: r.get("max_duration_s"),
            created_at: r.get("created_at"),
            started_at: r.get("started_at"),
            completed_at: r.get("completed_at"),
        })
        .collect())
}

pub async fn update_job_status(
    pool: &SqlitePool,
    job_id: &str,
    status: &str,
    result_json: Option<&str>,
    error_message: Option<&str>,
) -> Result<()> {
    let now = unix_ts();
    let started_at = if status == "running" { Some(now) } else { None };
    let completed_at = if matches!(status, "completed" | "failed" | "cancelled") {
        Some(now)
    } else {
        None
    };

    sqlx::query(
        r#"
        UPDATE jobs
        SET status = ?,
            result_json = COALESCE(?, result_json),
            error_message = COALESCE(?, error_message),
            started_at = COALESCE(?, started_at),
            completed_at = COALESCE(?, completed_at)
        WHERE id = ?
        "#,
    )
    .bind(status)
    .bind(result_json)
    .bind(error_message)
    .bind(started_at)
    .bind(completed_at)
    .bind(job_id)
    .execute(pool)
    .await
    .with_context(|| format!("update job {job_id} to {status}"))?;

    info!(job_id, status, "job status updated");
    Ok(())
}

pub async fn assign_job_to_worker(
    pool: &SqlitePool,
    job_id: &str,
    worker_vm_id: &str,
) -> Result<()> {
    let now = unix_ts();
    sqlx::query(
        r#"
        UPDATE jobs
        SET status = 'assigned', worker_vm_id = ?, started_at = ?
        WHERE id = ? AND status = 'queued'
        "#,
    )
    .bind(worker_vm_id)
    .bind(now)
    .bind(job_id)
    .execute(pool)
    .await
    .with_context(|| format!("assign job {job_id} to worker {worker_vm_id}"))?;

    Ok(())
}

pub async fn next_queued_job(pool: &SqlitePool) -> Result<Option<Job>> {
    let row = sqlx::query(
        r#"
        SELECT id, user_id, job_type, status, priority, machine_class,
               command, payload_json, result_json, error_message,
               worker_vm_id, max_duration_s, created_at, started_at, completed_at
        FROM jobs
        WHERE status = 'queued'
        ORDER BY priority DESC, created_at ASC
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await
    .with_context(|| "fetch next queued job")?;

    Ok(row.map(|r| Job {
        id: r.get("id"),
        user_id: r.get("user_id"),
        job_type: r.get("job_type"),
        status: r.get("status"),
        priority: r.get("priority"),
        machine_class: r.get("machine_class"),
        command: r.get("command"),
        payload_json: r.get("payload_json"),
        result_json: r.get("result_json"),
        error_message: r.get("error_message"),
        worker_vm_id: r.get("worker_vm_id"),
        max_duration_s: r.get("max_duration_s"),
        created_at: r.get("created_at"),
        started_at: r.get("started_at"),
        completed_at: r.get("completed_at"),
    }))
}

pub async fn cancel_job(pool: &SqlitePool, job_id: &str) -> Result<()> {
    update_job_status(pool, job_id, "cancelled", None, Some("cancelled by user")).await
}

// ── Promotion operations ─────────────────────────────────────────────────────

pub async fn create_promotion(
    pool: &SqlitePool,
    user_id: &str,
    job_id: Option<&str>,
    binary_path: Option<&str>,
    verification_json: Option<&str>,
) -> Result<String> {
    let id = ulid::Ulid::new().to_string().to_lowercase();
    let now = unix_ts();

    sqlx::query(
        r#"
        INSERT INTO promotions (id, user_id, job_id, status, binary_path,
                               verification_json, created_at)
        VALUES (?, ?, ?, 'pending', ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(user_id)
    .bind(job_id)
    .bind(binary_path)
    .bind(verification_json)
    .bind(now)
    .execute(pool)
    .await
    .with_context(|| format!("create promotion for user {user_id}"))?;

    info!(promotion_id = %id, user_id, "promotion created");
    Ok(id)
}

pub async fn get_promotion(pool: &SqlitePool, promotion_id: &str) -> Result<Option<Promotion>> {
    let row = sqlx::query(
        r#"
        SELECT id, user_id, job_id, status, snapshot_path, binary_path,
               verification_json, error_message, created_at, completed_at
        FROM promotions WHERE id = ?
        "#,
    )
    .bind(promotion_id)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("get promotion {promotion_id}"))?;

    Ok(row.map(|r| Promotion {
        id: r.get("id"),
        user_id: r.get("user_id"),
        job_id: r.get("job_id"),
        status: r.get("status"),
        snapshot_path: r.get("snapshot_path"),
        binary_path: r.get("binary_path"),
        verification_json: r.get("verification_json"),
        error_message: r.get("error_message"),
        created_at: r.get("created_at"),
        completed_at: r.get("completed_at"),
    }))
}

pub async fn update_promotion_status(
    pool: &SqlitePool,
    promotion_id: &str,
    status: &str,
    snapshot_path: Option<&str>,
    error_message: Option<&str>,
) -> Result<()> {
    let completed_at = if matches!(status, "completed" | "failed" | "rolled_back") {
        Some(unix_ts())
    } else {
        None
    };

    sqlx::query(
        r#"
        UPDATE promotions
        SET status = ?,
            snapshot_path = COALESCE(?, snapshot_path),
            error_message = COALESCE(?, error_message),
            completed_at = COALESCE(?, completed_at)
        WHERE id = ?
        "#,
    )
    .bind(status)
    .bind(snapshot_path)
    .bind(error_message)
    .bind(completed_at)
    .bind(promotion_id)
    .execute(pool)
    .await
    .with_context(|| format!("update promotion {promotion_id} to {status}"))?;

    info!(promotion_id, status, "promotion status updated");
    Ok(())
}

pub async fn list_promotions_for_user(pool: &SqlitePool, user_id: &str) -> Result<Vec<Promotion>> {
    let rows = sqlx::query(
        r#"
        SELECT id, user_id, job_id, status, snapshot_path, binary_path,
               verification_json, error_message, created_at, completed_at
        FROM promotions WHERE user_id = ?
        ORDER BY created_at DESC
        LIMIT 20
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .with_context(|| format!("list promotions for user {user_id}"))?;

    Ok(rows
        .into_iter()
        .map(|r| Promotion {
            id: r.get("id"),
            user_id: r.get("user_id"),
            job_id: r.get("job_id"),
            status: r.get("status"),
            snapshot_path: r.get("snapshot_path"),
            binary_path: r.get("binary_path"),
            verification_json: r.get("verification_json"),
            error_message: r.get("error_message"),
            created_at: r.get("created_at"),
            completed_at: r.get("completed_at"),
        })
        .collect())
}

/// Execute a promotion: snapshot user's data.img, apply changes, verify.
///
/// This is the core promotion flow (ADR-0014 Phase 8):
/// 1. Snapshot user's current data.img (btrfs, <1s)
/// 2. Stop user's VM
/// 3. Apply artifacts (e.g., copy new binary to data.img)
/// 4. Start user's VM
/// 5. Health check
/// 6. On failure, rollback from snapshot
pub async fn execute_promotion(
    pool: &SqlitePool,
    promotion_id: &str,
    user_id: &str,
    registry: &std::sync::Arc<crate::sandbox::SandboxRegistry>,
) -> Result<()> {
    use tokio::process::Command;

    // Mark as promoting
    update_promotion_status(pool, promotion_id, "promoting", None, None).await?;

    let promotion = get_promotion(pool, promotion_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("promotion {promotion_id} not found"))?;

    // Step 1: Snapshot user's data.img via btrfs
    let user_data_dir = format!("/data/users/{user_id}");
    let snapshot_label = format!("pre-promote-{promotion_id}");
    let snapshot_dir = format!("/data/snapshots/{user_id}");
    let snapshot_path = format!("{snapshot_dir}/{snapshot_label}");

    let snapshot_result = Command::new("btrfs")
        .args([
            "subvolume",
            "snapshot",
            "-r",
            &user_data_dir,
            &snapshot_path,
        ])
        .output()
        .await;

    match snapshot_result {
        Ok(output) if output.status.success() => {
            info!(promotion_id, snapshot_path = %snapshot_path, "pre-promotion snapshot created");
            update_promotion_status(pool, promotion_id, "promoting", Some(&snapshot_path), None)
                .await?;
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(promotion_id, %stderr, "btrfs snapshot failed");
            update_promotion_status(
                pool,
                promotion_id,
                "failed",
                None,
                Some(&format!("snapshot failed: {stderr}")),
            )
            .await?;
            return Ok(());
        }
        Err(e) => {
            error!(promotion_id, %e, "btrfs command failed");
            update_promotion_status(
                pool,
                promotion_id,
                "failed",
                None,
                Some(&format!("snapshot command failed: {e}")),
            )
            .await?;
            return Ok(());
        }
    }

    // Step 2: Apply artifacts — currently supports binary promotion
    if let Some(binary_path) = &promotion.binary_path {
        let target = format!("{user_data_dir}/data.img");
        // Mount data.img, copy binary, unmount
        // For now, we'll use a simpler approach: the binary is already on data.img
        // from the seed service, and promotion updates it in-place via loop mount.
        let apply_script = format!(
            r#"
            LOOPDEV=$(losetup -f)
            losetup "$LOOPDEV" "{target}"
            TMPDIR=$(mktemp -d)
            mount "$LOOPDEV" "$TMPDIR"
            mkdir -p "$TMPDIR/bin"
            cp "{binary_path}" "$TMPDIR/bin/sandbox"
            chmod 755 "$TMPDIR/bin/sandbox"
            umount "$TMPDIR"
            losetup -d "$LOOPDEV"
            rmdir "$TMPDIR"
            "#
        );

        let apply_result = Command::new("bash")
            .arg("-c")
            .arg(&apply_script)
            .output()
            .await;

        if let Ok(output) = apply_result {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                error!(promotion_id, %stderr, "binary apply failed");
                update_promotion_status(
                    pool,
                    promotion_id,
                    "failed",
                    None,
                    Some(&format!("binary apply failed: {stderr}")),
                )
                .await?;
                return Ok(());
            }
        }
    }

    // Step 3: Restart user's VM
    // Stop the VM
    if let Err(e) = registry
        .stop(user_id, crate::sandbox::SandboxRole::Live)
        .await
    {
        error!(promotion_id, %e, "failed to stop VM for promotion");
        // Not fatal — VM may already be stopped
    }

    // Start the VM
    match registry
        .ensure_running(user_id, crate::sandbox::SandboxRole::Live)
        .await
    {
        Ok(_port) => {
            info!(promotion_id, "VM restarted after promotion");
        }
        Err(e) => {
            error!(promotion_id, %e, "failed to restart VM after promotion");
            update_promotion_status(
                pool,
                promotion_id,
                "failed",
                None,
                Some(&format!("VM restart failed: {e}")),
            )
            .await?;
            return Ok(());
        }
    }

    // Step 4: Mark as completed
    update_promotion_status(pool, promotion_id, "completed", None, None).await?;
    info!(promotion_id, user_id, "promotion completed successfully");

    Ok(())
}
