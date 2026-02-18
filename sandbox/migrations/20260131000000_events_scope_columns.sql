-- Add session_id and thread_id scope columns for safe session/thread isolation.
-- Previously added via manual PRAGMA table_info introspection in run_migrations().
-- Now tracked properly as a migration.

ALTER TABLE events ADD COLUMN session_id TEXT;
ALTER TABLE events ADD COLUMN thread_id TEXT;

CREATE INDEX IF NOT EXISTS idx_events_session_thread ON events(session_id, thread_id);

-- Backfill any existing scoped payload rows into explicit columns.
UPDATE events
SET
    session_id = COALESCE(session_id, json_extract(payload, '$.scope.session_id')),
    thread_id  = COALESCE(thread_id,  json_extract(payload, '$.scope.thread_id'))
WHERE session_id IS NULL OR thread_id IS NULL;
