-- Runtime registry foundation for per-user VM/branch lifecycle orchestration.
-- These tables are additive and do not alter existing auth/session behavior.

CREATE TABLE IF NOT EXISTS user_vms (
    id            TEXT PRIMARY KEY,   -- opaque VM/runtime id
    user_id       TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    backend       TEXT NOT NULL,      -- process | vfkit | cloud-hypervisor | ...
    state         TEXT NOT NULL,      -- running | stopped | failed
    host          TEXT,
    metadata_json TEXT,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_user_vms_user_id
    ON user_vms (user_id);

CREATE INDEX IF NOT EXISTS idx_user_vms_state
    ON user_vms (state);

CREATE TABLE IF NOT EXISTS branch_runtimes (
    id             TEXT PRIMARY KEY,   -- opaque runtime id
    user_id        TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    vm_id          TEXT REFERENCES user_vms(id) ON DELETE SET NULL,
    branch_name    TEXT NOT NULL,
    role           TEXT,               -- optional compatibility role: live | dev
    port           INTEGER NOT NULL,
    state          TEXT NOT NULL,      -- running | stopped | failed
    workspace_path TEXT,
    db_path        TEXT,
    metadata_json  TEXT,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL,
    UNIQUE(user_id, branch_name)
);

CREATE INDEX IF NOT EXISTS idx_branch_runtimes_user_id
    ON branch_runtimes (user_id);

CREATE INDEX IF NOT EXISTS idx_branch_runtimes_vm_id
    ON branch_runtimes (vm_id);

CREATE INDEX IF NOT EXISTS idx_branch_runtimes_state
    ON branch_runtimes (state);

CREATE TABLE IF NOT EXISTS route_pointers (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    pointer_name TEXT NOT NULL,       -- main | dev | exp-*
    target_kind  TEXT NOT NULL,       -- role | branch
    target_value TEXT NOT NULL,       -- live | dev | <branch-name>
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    UNIQUE(user_id, pointer_name)
);

CREATE INDEX IF NOT EXISTS idx_route_pointers_user_id
    ON route_pointers (user_id);

CREATE TABLE IF NOT EXISTS runtime_events (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id       TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    runtime_id    TEXT,
    event_type    TEXT NOT NULL,      -- runtime.start | runtime.stop | pointer.swap | ...
    detail_json   TEXT,
    correlation_id TEXT,
    created_at    INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_runtime_events_user_created
    ON runtime_events (user_id, created_at);
