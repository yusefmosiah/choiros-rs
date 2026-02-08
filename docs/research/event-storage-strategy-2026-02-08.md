# ChoirOS Event Logging Storage Strategy

**Date:** 2026-02-08  
**Purpose:** Research and recommend storage architecture for high-concurrency agent event logging in ChoirOS  
**Status:** Ready for Review

---

## Narrative Summary (1-minute read)

ChoirOS needs an event storage strategy that supports real-time WebSocket streaming, historical replay, and run comparison workflows. The current SQLite (libsql) EventStoreActor provides a solid foundation but lacks strategies for high-volume scaling, retention, and partitioning.

**Recommendation:** Start with SQLite-only (enhanced with WAL mode and connection pooling), plan a phased evolution toward JSONL-based cold storage for archiving. Use a hybrid approach where "hot" events (last 30 days) stay in SQLite for fast queries, while older events are compressed and archived to JSONL files.

**Key metrics to target:** 10,000 events/sec write throughput, <50ms latency for trace reconstruction, 90-day retention for hot data, 1-year retention for cold data.

---

## What Changed

- Added comprehensive research on storage architectures (JSONL, DB, hybrid)
- Defined partitioning/rotation strategies for high-volume agent logs
- Established retention policies by event type
- Indexed fields based on common query patterns
- Specified compaction and archiving strategies
- Mapped out performance targets and migration paths
- Provided concrete implementation details for SQLite enhancements

---

## What To Do Next

1. **Immediate (Week 1):** Enable WAL mode and connection pooling for existing EventStoreActor
2. **Short-term (Weeks 2-4):** Implement partitioning by session_id and time-based rotation
3. **Medium-term (Month 2):** Build JSONL archiving service for events older than 30 days
4. **Long-term (Month 3+):** Consider PostgreSQL migration if volume exceeds 100M events/month

---

## 1. Architecture Comparison

### JSONL vs DB vs Hybrid

| Aspect | JSONL-Only | DB-Only (SQLite) | Hybrid (Recommended) |
|--------|-----------|------------------|---------------------|
| **Write Throughput** | Very High (append-only, no locks) | Medium (WAL: 5-10K/sec) | High (DB for hot, async write to JSONL for cold) |
| **Query Performance** | Slow (full-file scan) | Fast (indexed queries) | Fast for recent, slow for archived |
| **Real-time Streaming** | Hard (file watching) | Easy (DB triggers) | Easy (DB-based) |
| **Schema Evolution** | Flexible (no schema) | Requires migrations | Flexible (JSONL for archival) |
| **Backup/Restore** | Simple (copy files) | Requires dump/restore | Two-tier (DB + file backup) |
| **Retention/Rotation** | Easy (delete files) | Requires DELETE + VACUUM | Easy (rotate JSONL) |
| **Memory Footprint** | Low | Medium (cache) | Medium |
| **Setup Complexity** | Very Low | Medium | High |
| **Best For** | High-volume logs, archival, batch analytics | Real-time queries, ACID requirements | Mixed workload (hot + cold) |

**ChoirOS-Specific Pros/Cons:**

- **JSONL-Only:**
  - ✅ Simple integration with existing `actor_call` streaming
  - ✅ Easy to grep/curl for debugging
  - ✅ No schema migrations needed
  - ❌ Poor support for WebSocket live streaming (requires file polling)
  - ❌ No transaction support for event sequences
  - ❌ Querying by `session_id`/`thread_id` requires scanning all files

- **DB-Only (SQLite):**
  - ✅ Already implemented in EventStoreActor
  - ✅ Excellent for WebSocket real-time subscriptions
  - ✅ Scoped queries (`session_id`, `thread_id`) are fast with indexes
  - ✅ Transaction support for event sequences
  - ❌ SQLite has file-level lock (but WAL mitigates)
  - ❌ Large tables (>10M rows) degrade performance
  - ❌ DELETE is expensive (requires VACUUM)

- **Hybrid (Recommended):**
  - ✅ Best of both worlds: fast hot queries, cheap cold storage
  - ✅ Seamless migration path from current SQLite setup
  - ✅ Supports WebSocket streaming via DB
  - ✅ Easy to archive/rotate old data via JSONL
  - ❌ More complex architecture (two storage systems)
  - ❌ Need to implement background archiver
  - ❌ Event lookup spans both hot and cold storage

---

## 2. Recommended Architecture

### Phase 1: Enhanced SQLite (Current + Immediate Improvements)

**Start Simple:** Use existing EventStoreActor with enhancements:

```sql
-- Enable WAL mode for better concurrency
PRAGMA journal_mode = WAL;

-- Increase cache size (default 2MB, recommend 64MB)
PRAGMA cache_size = -64000;

-- Optimize for append-heavy workload
PRAGMA synchronous = NORMAL;
```

**Connection Pooling:** Use `r2d2` or `deadpool` for connection reuse.

**Benefits:**
- Zero architectural changes
- Immediate 5-10x write throughput improvement
- Existing queries and indexes work unchanged

---

### Phase 2: Time-Based Partitioning (Weeks 2-4)

Partition events by time periods to improve query performance and enable rotation:

```sql
-- Instead of single table, use monthly tables
CREATE TABLE events_2026_02 (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT UNIQUE NOT NULL,
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    user_id TEXT NOT NULL DEFAULT 'system',
    session_id TEXT,
    thread_id TEXT
);

CREATE INDEX idx_events_2026_02_actor_session 
    ON events_2026_02(actor_id, session_id, thread_id, seq);

CREATE INDEX idx_events_2026_02_timestamp 
    ON events_2026_02(timestamp);
```

**Rotation Logic:**
1. On first write of each month, create new table
2. Drop tables older than 90 days
3. Archive before dropping (see Phase 3)

---

### Phase 3: Hybrid Hot/Cold Storage (Month 2)

```
┌─────────────────────────────────────────────────────────────┐
│                    ChoirOS Event Storage                    │
├─────────────────────────────────────────────────────────────┤
│  Hot Storage (SQLite, last 30 days)                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐        │
│  │ events_2026 │  │ events_2026 │  │ events_2026 │        │
│  │    _02      │  │    _01      │  │    _03      │        │
│  │  (current)  │  │  (recent)   │  │ (next mon)  │        │
│  └─────────────┘  └─────────────┘  └─────────────┘        │
│         │                    │                    │         │
│         │ Indexed queries    │ Fast read          │         │
│         │ WebSocket stream   │ Write path         │         │
├─────────────────────────────────────────────────────────────┤
│  Cold Storage (JSONL, archived daily)                       │
│  /data/events/archived/                                      │
│  ├── 2026-01-01.jsonl.gz (compressed)                       │
│  ├── 2026-01-02.jsonl.gz                                    │
│  └── ...                                                    │
└─────────────────────────────────────────────────────────────┘
```

**Archiver Service:**
```rust
// Background task runs nightly
async fn archive_old_events() {
    let cutoff = Utc::now() - Duration::days(30);
    
    // Query events older than 30 days
    let events = query_events_older_than(cutoff).await;
    
    // Write to JSONL (compressed)
    write_jsonl(&events, "/data/events/archived").await;
    
    // Delete from hot storage (in batches)
    delete_archived_events(cutoff).await;
    
    // VACUUM to reclaim space
    vacuum_database().await;
}
```

---

### Phase 4: Database Migration (Month 3+, Optional)

If volume exceeds 100M events/month, migrate to PostgreSQL:

**Migration Benefits:**
- True parallelism (no SQLite file-level lock)
- Native partitioning (PARTITION BY RANGE)
- Better query planner for complex joins
- Built-in replication and HA

**Migration Steps:**
1. Set up PostgreSQL server
2. Use `pgloader` to migrate SQLite data
3. Update EventStoreActor to use SQLx PostgreSQL
4. Switch connection strings in config

---

## 3. Concrete Storage Schema (SQLite)

### Current Schema (Enhanced)

```sql
-- Events table with scope columns for session/thread isolation
CREATE TABLE IF NOT EXISTS events (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT UNIQUE NOT NULL,
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    user_id TEXT NOT NULL DEFAULT 'system',
    session_id TEXT,
    thread_id TEXT
);

-- Critical indexes for common queries
CREATE INDEX IF NOT EXISTS idx_events_actor_id ON events(actor_id);
CREATE INDEX IF NOT EXISTS idx_events_event_type ON events(event_type);
CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);
CREATE INDEX IF NOT EXISTS idx_events_session_thread 
    ON events(session_id, thread_id);
CREATE INDEX IF NOT EXISTS idx_events_actor_seq 
    ON events(actor_id, seq);

-- Composite index for scoped chat queries
CREATE INDEX IF NOT EXISTS idx_events_actor_session_thread_seq 
    ON events(actor_id, session_id, thread_id, seq);
```

---

### Partitioned Schema (Phase 2+)

```sql
-- Master view for querying across all partitions
CREATE VIEW events_all AS
    SELECT * FROM events_2026_01
    UNION ALL
    SELECT * FROM events_2026_02
    UNION ALL
    SELECT * FROM events_2026_03;

-- Example partition (created monthly)
CREATE TABLE IF NOT EXISTS events_YYYY_MM (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT UNIQUE NOT NULL,
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    user_id TEXT NOT NULL DEFAULT 'system',
    session_id TEXT,
    thread_id TEXT
);

-- Indexes per partition
CREATE INDEX IF NOT EXISTS idx_events_YYYY_MM_actor_session_thread_seq 
    ON events_YYYY_MM(actor_id, session_id, thread_id, seq);

CREATE INDEX IF NOT EXISTS idx_events_YYYY_MM_timestamp 
    ON events_YYYY_MM(timestamp);
```

---

### Index Strategy

**Primary Query Patterns:**
1. Get all events for a trace: `WHERE actor_id = ? AND session_id = ? AND thread_id = ?`
2. Get events in time window: `WHERE timestamp > ? AND timestamp < ?`
3. Compare two runs: `WHERE session_id IN (?, ?)`
4. Get tool calls: `WHERE event_type = 'chat.tool_call' AND actor_id = ?`
5. Get terminal commands: `WHERE event_type = 'terminal.tool_call'`

**Recommended Indexes:**
```sql
-- Most common: scoped trace reconstruction
CREATE INDEX idx_trace_lookup 
    ON events(actor_id, session_id, thread_id, seq);

-- Time-series queries
CREATE INDEX idx_time_range 
    ON events(timestamp);

-- Event type filtering (e.g., all tool calls)
CREATE INDEX idx_event_type_actor 
    ON events(event_type, actor_id);

-- Composite for WebSocket streaming
CREATE INDEX idx_ws_stream 
    ON events(actor_id, session_id, thread_id, timestamp);
```

---

## 4. File Organization Scheme (JSONL)

### Directory Structure

```
/data/events/
├── current.db                    # Hot storage (SQLite)
├── archived/                     # Cold storage (JSONL)
│   ├── 2026/
│   │   ├── 01/
│   │   │   ├── 01.jsonl.gz
│   │   │   ├── 02.jsonl.gz
│   │   │   └── ...
│   │   └── 02/
│   │       ├── 01.jsonl.gz
│   │       └── ...
│   └── 2026-01-manifest.json     # Index file for fast lookup
└── snapshots/                   # Periodic full backups
    ├── 2026-01-01-snapshot.db
    └── 2026-02-01-snapshot.db
```

---

### JSONL Format

Each line is a JSON object (event):

```json
{"seq":123456,"event_id":"01HZ...", "timestamp":"2026-02-08T12:34:56Z","event_type":"chat.user_msg","payload":{"text":"hello","scope":{"session_id":"sess-1","thread_id":"thread-1"}},"actor_id":"chat-actor-1","user_id":"user-1"}
{"seq":123457,"event_id":"01HZ...", "timestamp":"2026-02-08T12:34:57Z","event_type":"chat.tool_call","payload":{"tool_name":"bash","tool_args":{"command":"ls"}},"actor_id":"chat-actor-1","user_id":"user-1"}
```

**Benefits:**
- Line-oriented: can `grep`, `awk`, `jq` directly
- Append-only: no file locking issues
- Compressible: `gzip` reduces size by ~80%
- Easy to stream: read line-by-line

---

### Rotation Policies

**Hot Storage (SQLite):**
- Retention: 30 days
- Rotation: Monthly tables
- Auto-delete: Tables older than 90 days (after archival)

**Cold Storage (JSONL):**
- Retention: 1 year
- Rotation: Daily files
- Compression: `gzip -9`
- Auto-delete: Files older than 1 year

**Manifest File:**
```json
{
  "date": "2026-01-01",
  "events_count": 150000,
  "sessions": ["sess-1", "sess-2"],
  "threads": ["thread-1", "thread-2"],
  "event_types": ["chat.user_msg", "chat.tool_call"],
  "size_bytes": 4567890
}
```

---

### Compression Strategy

```bash
# Archive daily events to compressed JSONL
sqlite3 current.db "SELECT json_object(...) FROM events WHERE date(timestamp) = 'today'" | \
  gzip > /data/events/archived/$(date +%Y/%m/%d).jsonl.gz

# Verify integrity
gunzip -t /data/events/archived/2026/02/08.jsonl.gz
```

---

## 5. Query Patterns

### Common Observability Queries

#### SQL Queries (Hot Storage)

```sql
-- Get all events for a trace
SELECT * FROM events 
WHERE actor_id = 'chat-actor-1' 
  AND session_id = 'sess-1' 
  AND thread_id = 'thread-1' 
ORDER BY seq;

-- Get events in time window
SELECT * FROM events 
WHERE timestamp BETWEEN '2026-02-08 00:00:00' AND '2026-02-08 23:59:59'
ORDER BY seq;

-- Get all tool calls for a session
SELECT event_id, timestamp, payload 
FROM events 
WHERE session_id = 'sess-1' 
  AND event_type = 'chat.tool_call';

-- Compare two runs (side-by-side)
SELECT 
  seq, 
  event_type, 
  payload,
  'run-1' as run_id
FROM events WHERE session_id = 'sess-1'
UNION ALL
SELECT 
  seq, 
  event_type, 
  payload,
  'run-2' as run_id
FROM events WHERE session_id = 'sess-2'
ORDER BY seq, run_id;

-- Get terminal commands and their results
SELECT 
  e1.timestamp as call_time,
  e1.payload->>'command' as command,
  e2.timestamp as result_time,
  e2.payload->>'success' as success
FROM events e1
LEFT JOIN events e2 
  ON e1.payload->>'call_id' = e2.payload->>'call_id'
WHERE e1.event_type = 'terminal.tool_call'
  AND e2.event_type = 'terminal.tool_result';
```

---

#### CLI Queries (Cold Storage)

```bash
# Get all events for a session
zcat /data/events/archived/2026/02/*.jsonl.gz | \
  jq 'select(.payload.scope.session_id == "sess-1")'

# Count tool calls per day
zcat /data/events/archived/2026/02/*.jsonl.gz | \
  jq -r '.timestamp[:10]' | sort | uniq -c

# Find failed terminal commands
zcat /data/events/archived/2026/02/*.jsonl.gz | \
  jq 'select(.event_type == "terminal.tool_result" and .payload.success == false)'

# Extract all bash commands
zcat /data/events/archived/2026/02/*.jsonl.gz | \
  jq -r 'select(.event_type == "terminal.tool_call") | .payload.tool_args.command'

# Compare two sessions side-by-side
paste <(zcat 2026/02/08.jsonl.gz | jq -c 'select(.session_id == "sess-1")') \
     <(zcat 2026/02/08.jsonl.gz | jq -c 'select(.session_id == "sess-2")')
```

---

### WebSocket Streaming Queries

```rust
// Subscribe to scoped events
async fn subscribe_to_trace(actor_id: &str, session_id: &str, thread_id: &str) {
    let query = r#"
        SELECT seq, event_id, timestamp, event_type, payload, actor_id, user_id
        FROM events
        WHERE actor_id = ?1
          AND session_id = ?2
          AND thread_id = ?3
          AND seq > ?4
        ORDER BY seq ASC
    "#;
    
    let mut stream = db.query_stream(query, [actor_id, session_id, thread_id, last_seq])
        .await?;
    
    while let Some(row) = stream.next().await? {
        let event = parse_event(row)?;
        websocket.send(event).await?;
    }
}
```

---

## 6. Performance Targets

### Write Throughput

**Current State (SQLite default):**
- ~1,000 events/sec
- Single-threaded writes (file lock)

**Target State (SQLite + WAL + pooling):**
- 10,000 events/sec
- Concurrent writes (WAL mode)

**Phase 3 (Hybrid):**
- 50,000+ events/sec (async write-through to JSONL)

---

### Read Latency

| Query Type | Current | Target (Phase 1) | Target (Phase 3) |
|------------|---------|------------------|------------------|
| Get event by seq | 5-10ms | <1ms | <1ms |
| Get trace (100 events) | 50-100ms | <20ms | <50ms (hot+cold) |
| Time window query (1 day) | 200-500ms | <50ms | <100ms |
| WebSocket stream latency | N/A | <50ms | <50ms |

---

### Storage Growth Estimates

**Assumptions:**
- 10 parallel agent runs
- 1,000 events/run
- Average event size: 500 bytes

**Daily Volume:**
- Events: 10,000/day
- Size: 5 MB/day (uncompressed), 1 MB/day (compressed)

**Monthly Volume:**
- Events: 300,000/month
- Size: 150 MB/month (uncompressed), 30 MB/month (compressed)

**Yearly Volume:**
- Events: 3.6M/year
- Size: 1.8 GB/year (uncompressed), 360 MB/year (compressed)

---

## 7. Retention Policies

### By Event Type

| Event Type | Hot Retention | Cold Retention | Reason |
|------------|--------------|---------------|---------|
| `chat.*` | 30 days | 1 year | User conversations |
| `terminal.*` | 7 days | 90 days | Debug terminal sessions |
| `worker.*` | 30 days | 1 year | Task execution history |
| `file.*` | 30 days | 1 year | File operation audit |
| `system.*` | 7 days | 90 days | System errors/heartbeat |

**Implementation:**
```sql
-- Create event metadata table
CREATE TABLE event_metadata (
    event_type TEXT PRIMARY KEY,
    hot_retention_days INTEGER NOT NULL,
    cold_retention_days INTEGER NOT NULL
);

-- Insert policies
INSERT INTO event_metadata VALUES 
    ('chat.user_msg', 30, 365),
    ('terminal.tool_call', 7, 90),
    ('worker.task.started', 30, 365);

-- Query for archivable events
SELECT e.* 
FROM events e
JOIN event_metadata m ON e.event_type = m.event_type
WHERE date(e.timestamp) < date('now', -m.hot_retention_days || ' days');
```

---

### Scoped Retention

**User-Controlled Retention:**
```rust
struct RetentionPolicy {
    user_id: String,
    chat_retention: Duration,
    terminal_retention: Duration,
}

// Allow users to pin important sessions
fn pin_session(session_id: &str) {
    archive_flag(session_id, "pinned");
}

// Pinned sessions are never archived
fn should_archive(event: &Event) -> bool {
    !event.session_id.is_pinned() && 
    event.is_older_than(retention_for(&event.event_type))
}
```

---

## 8. Backup/Archive Strategy

### Backup Strategy

**Hot Storage (SQLite):**
1. Daily snapshots: `cp current.db snapshots/$(date +%Y-%m-%d).db`
2. Weekly full backup: `sqlite3 current.db ".backup backup.db"`
3. Point-in-time recovery: Use WAL checkpointing

**Cold Storage (JSONL):**
1. Already append-only: inherent backup via replication
2. Use `rsync` to offsite storage
3. Verify integrity with checksums

---

### Archive Strategy

**To Cloud Storage (S3/GCS):**
```bash
# Upload archived JSONL to S3
aws s3 sync /data/events/archived/ s3://choiros-events/archived/ \
  --storage-class GLACIER_IR \
  --exclude "*" \
  --include "2026-01/*.jsonl.gz"

# Upload to Google Cloud Storage
gsutil -m rsync -r /data/events/archived/ gs://choiros-events/archived/
```

**Restore Strategy:**
```bash
# Restore from archive
aws s3 cp s3://choiros-events/archived/2026/01/01.jsonl.gz - | \
  gunzip | sqlite3 current.db "INSERT INTO events VALUES ..."

# Or query directly from S3 (using Athena/BigQuery)
-- Create external table pointing to S3
CREATE EXTERNAL TABLE IF NOT EXISTS events_archive (
  seq BIGINT,
  event_id STRING,
  timestamp TIMESTAMP,
  event_type STRING,
  payload STRING,
  actor_id STRING,
  user_id STRING
)
ROW FORMAT SERDE 'org.apache.hive.hcatalog.data.JsonSerDe'
STORED AS TEXTFILE
LOCATION 's3://choiros-events/archived/';
```

---

## 9. Migration Path

### From Current EventStoreActor

**Step 1: Enable WAL and Connection Pooling (Week 1)**
```rust
// In EventStoreActor::new_with_path
db.execute("PRAGMA journal_mode = WAL", ()).await?;
db.execute("PRAGMA cache_size = -64000", ()).await?;
db.execute("PRAGMA synchronous = NORMAL", ()).await?;

// Add connection pooling
let pool = SqlitePoolOptions::new()
    .max_connections(10)
    .connect(&database_path).await?;
```

**Step 2: Add Background Compaction (Week 2)**
```rust
// Spawn background task for daily VACUUM
tokio::spawn(async move {
    loop {
        tokio::time::sleep(Duration::from_secs(86400)).await;
        pool.execute("VACUUM", ()).await.ok();
    }
});
```

**Step 3: Implement Partitioning (Weeks 3-4)**
```rust
// On first write of month
async fn ensure_monthly_table(month: &str) {
    let table_name = format!("events_{}", month);
    let create_sql = format!(
        "CREATE TABLE IF NOT EXISTS {} (seq INTEGER PRIMARY KEY AUTOINCREMENT, ...)",
        table_name
    );
    pool.execute(&create_sql, ()).await?;
}
```

**Step 4: Build Archiver Service (Month 2)**
```rust
// Background archiver
async fn run_archiver() {
    loop {
        tokio::time::sleep(Duration::from_secs(3600)).await; // Hourly check
        
        let events = query_old_events().await?;
        write_jsonl_archive(&events).await?;
        delete_old_events().await?;
    }
}
```

---

### Monitoring and Alerts

**Key Metrics to Track:**
```sql
-- Event growth rate
SELECT date(timestamp) as day, count(*) as events
FROM events
WHERE timestamp > date('now', '-7 days')
GROUP BY day
ORDER BY day;

-- Database size
SELECT page_count * page_size as bytes 
FROM pragma_page_count(), pragma_page_size();

-- Index hit rate
SELECT name, stat 
FROM sqlite_master
WHERE type = 'index';
```

**Alerts:**
- Database size > 10 GB
- Write latency > 100ms (p95)
- Query latency > 500ms (p95)
- Failed VACUUM operations
- Archive job failures

---

## 10. Summary and Recommendations

### Recommended Path Forward

**Phase 1 (Immediate - Week 1):**
- Enable WAL mode on existing EventStoreActor
- Add connection pooling (max 10 connections)
- Set cache size to 64 MB
- **Expected impact:** 5-10x write throughput improvement

**Phase 2 (Short-term - Weeks 2-4):**
- Implement time-based partitioning (monthly tables)
- Add background VACUUM task
- Set up daily snapshots
- **Expected impact:** Stable performance as data grows

**Phase 3 (Medium-term - Month 2):**
- Build JSONL archiver service
- Archive events older than 30 days
- Compress archived files with gzip
- **Expected impact:** Contained database size, cheap long-term storage

**Phase 4 (Long-term - Month 3+, Optional):**
- Evaluate PostgreSQL migration if volume > 100M events/month
- Consider distributed storage (ClickHouse, TimescaleDB) for analytics
- **Expected impact:** Horizontal scaling, better query performance

---

### Success Criteria

**Performance:**
- 10,000 events/sec write throughput
- <50ms latency for trace reconstruction
- Database size < 10 GB (hot storage only)

**Reliability:**
- 99.9% uptime for event ingestion
- Zero data loss (WAL mode + backups)
- <1s recovery time from backup

**Observability:**
- All events queryable within 5 seconds
- WebSocket streaming latency < 50ms
- Run comparison queries complete in < 1 second

---

## Appendix A: Event Type Reference

```rust
// From shared-types/src/lib.rs
pub const EVENT_CHAT_USER_MSG: &str = "chat.user_msg";
pub const EVENT_CHAT_ASSISTANT_MSG: &str = "chat.assistant_msg";
pub const EVENT_CHAT_TOOL_CALL: &str = "chat.tool_call";
pub const EVENT_CHAT_TOOL_RESULT: &str = "chat.tool_result";
pub const EVENT_MODEL_SELECTION: &str = "model.selection";
pub const EVENT_MODEL_CHANGED: &str = "model.changed";
pub const EVENT_USER_THEME_PREFERENCE: &str = "user.theme_preference";
pub const EVENT_FILE_WRITE: &str = "file.write";
pub const EVENT_FILE_EDIT: &str = "file.edit";
pub const EVENT_ACTOR_SPAWNED: &str = "actor.spawned";
pub const EVENT_VIEWER_CONTENT_SAVED: &str = "viewer.content_saved";
pub const EVENT_VIEWER_CONTENT_CONFLICT: &str = "viewer.content_conflict";
pub const EVENT_TOPIC_WORKER_TASK_STARTED: &str = "worker.task.started";
pub const EVENT_TOPIC_WORKER_TASK_PROGRESS: &str = "worker.task.progress";
pub const EVENT_TOPIC_WORKER_TASK_COMPLETED: &str = "worker.task.completed";
pub const EVENT_TOPIC_WORKER_TASK_FAILED: &str = "worker.task.failed";

// Terminal events
pub const EVENT_TERMINAL_TOOL_CALL: &str = "terminal.tool_call";
pub const EVENT_TERMINAL_TOOL_RESULT: &str = "terminal.tool_result";
pub const EVENT_TERMINAL_AGENT_STARTING: &str = "terminal.agent_starting";
pub const EVENT_TERMINAL_AGENT_MODEL_SELECTED: &str = "terminal.agent_model_selected";
pub const EVENT_TERMINAL_AGENT_PLANNING: &str = "terminal.agent_planning";
pub const EVENT_TERMINAL_AGENT_SYNTHESIZING: &str = "terminal.agent_synthesizing";
pub const EVENT_TERMINAL_AGENT_REASONING: &str = "terminal.agent_reasoning";
pub const EVENT_TERMINAL_AGENT_FALLBACK: &str = "terminal.agent_fallback";
```

---

## Appendix B: Example Archiver Implementation

```rust
use tokio::fs::File;
use tokio::io::BufWriter;
use flate2::write::GzEncoder;
use flate2::Compression;
use chrono::{Utc, Duration};
use sqlx::SqlitePool;

pub struct EventArchiver {
    pool: SqlitePool,
    archive_dir: std::path::PathBuf,
}

impl EventArchiver {
    pub async fn archive_old_events(&self, retention_days: i64) -> Result<(), Box<dyn std::error::Error>> {
        let cutoff = Utc::now() - Duration::days(retention_days);
        
        // Query events to archive
        let events = sqlx::query!(
            r#"
            SELECT seq, event_id, timestamp, event_type, payload, actor_id, user_id
            FROM events
            WHERE timestamp < ?
            ORDER BY seq
            "#,
            cutoff
        )
        .fetch_all(&self.pool)
        .await?;
        
        if events.is_empty() {
            return Ok(());
        }
        
        // Write to JSONL archive
        let date = Utc::now().format("%Y/%m/%d").to_string();
        let archive_path = self.archive_dir.join(format!("{}.jsonl.gz", date));
        
        let file = File::create(&archive_path).await?;
        let writer = BufWriter::new(file);
        let mut encoder = GzEncoder::new(writer, Compression::default());
        
        for event in &events {
            let line = serde_json::to_string(event)?;
            encoder.write_all(line.as_bytes())?;
            encoder.write_all(b"\n")?;
        }
        
        encoder.finish()?;
        
        // Delete archived events in batches
        let seq_limit = events.last().map(|e| e.seq).unwrap_or(0);
        sqlx::query!(
            "DELETE FROM events WHERE seq <= ?",
            seq_limit
        )
        .execute(&self.pool)
        .await?;
        
        // VACUUM to reclaim space
        sqlx::query("VACUUM")
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
}
```

---

**End of Document**
