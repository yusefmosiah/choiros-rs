-- Document revision tracking for Writer API
-- Provides optimistic concurrency control

CREATE TABLE IF NOT EXISTS document_revisions (
    path TEXT PRIMARY KEY,
    revision INTEGER NOT NULL DEFAULT 1,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Index for timestamp-based queries
CREATE INDEX IF NOT EXISTS idx_document_revisions_updated
    ON document_revisions(updated_at);
