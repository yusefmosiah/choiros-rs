-- SQLite event store schema
-- Append-only log for all state changes

CREATE TABLE IF NOT EXISTS events (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT UNIQUE NOT NULL,
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL, -- JSON
    actor_id TEXT NOT NULL,
    user_id TEXT NOT NULL DEFAULT 'system'
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_events_actor_id ON events(actor_id);
CREATE INDEX IF NOT EXISTS idx_events_actor_seq ON events(actor_id, seq);
CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);

-- For projection queries (e.g., get all chat messages)
CREATE INDEX IF NOT EXISTS idx_events_actor_type ON events(actor_id, event_type);
