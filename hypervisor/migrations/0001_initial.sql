-- Users registered in the hypervisor (passkey auth)
CREATE TABLE IF NOT EXISTS users (
    id           TEXT PRIMARY KEY,         -- UUID v4
    username     TEXT UNIQUE NOT NULL,
    display_name TEXT NOT NULL,
    created_at   INTEGER NOT NULL          -- Unix timestamp
);

-- Passkeys registered per user
CREATE TABLE IF NOT EXISTS passkeys (
    credential_id TEXT PRIMARY KEY,        -- base64url of credential id
    user_id       TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    passkey_json  TEXT NOT NULL,           -- serde_json of webauthn_rs::Passkey
    name          TEXT,                    -- user-assigned label ("MacBook Touch ID")
    created_at    INTEGER NOT NULL,
    last_used_at  INTEGER
);

-- Single-use recovery codes per user (hashed with argon2id)
CREATE TABLE IF NOT EXISTS recovery_codes (
    id         TEXT PRIMARY KEY,           -- UUID v4
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash  TEXT NOT NULL,              -- argon2id hash
    used_at    INTEGER,                    -- NULL = not yet consumed
    created_at INTEGER NOT NULL
);

-- Active server-side sessions (tower-sessions-sqlx-store creates its own table;
-- this table tracks application-level session metadata for audit)
CREATE TABLE IF NOT EXISTS audit_log (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id    TEXT,
    event      TEXT NOT NULL,              -- "login" | "logout" | "register" | "recovery_code_used" | "passkey_added" | "passkey_removed"
    detail     TEXT,
    ip         TEXT,
    created_at INTEGER NOT NULL
);
