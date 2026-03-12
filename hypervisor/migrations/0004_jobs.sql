-- ADR-0014 Phase 7: Job queue for build pool
-- Jobs represent units of work (build, test, promote) executed on
-- shared worker VMs on behalf of users.

CREATE TABLE IF NOT EXISTS jobs (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    job_type TEXT NOT NULL,         -- 'build', 'test', 'promote', 'custom'
    status TEXT NOT NULL DEFAULT 'queued',  -- queued, assigned, running, completed, failed, cancelled
    priority INTEGER NOT NULL DEFAULT 0,   -- higher = higher priority (tier-based)
    machine_class TEXT,            -- worker VM class (null = use default worker class)
    command TEXT,                   -- shell command to execute in worker VM
    payload_json TEXT,             -- arbitrary JSON payload for the job
    result_json TEXT,              -- job result (stdout, artifacts, etc.)
    error_message TEXT,            -- error message if failed
    worker_vm_id TEXT,             -- assigned worker VM instance ID
    max_duration_s INTEGER NOT NULL DEFAULT 1800,  -- 30 min default
    created_at INTEGER NOT NULL,
    started_at INTEGER,
    completed_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_jobs_user_id ON jobs(user_id);
CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs(status);
CREATE INDEX IF NOT EXISTS idx_jobs_created_at ON jobs(created_at);

-- Promotion history — tracks sandbox binary updates and rollbacks
CREATE TABLE IF NOT EXISTS promotions (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    job_id TEXT REFERENCES jobs(id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'pending',  -- pending, promoting, completed, failed, rolled_back
    snapshot_path TEXT,             -- pre-promotion btrfs snapshot path
    binary_path TEXT,              -- path to new sandbox binary (if applicable)
    verification_json TEXT,        -- verification gate results
    error_message TEXT,
    created_at INTEGER NOT NULL,
    completed_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_promotions_user_id ON promotions(user_id);
