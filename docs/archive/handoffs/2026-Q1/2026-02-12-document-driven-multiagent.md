# Document-Driven Multiagent Collaboration

## Narrative Summary (1-minute read)

We're replacing event-based coordination with a simpler model: **the document is the coordination layer**.

Workers write directly to sections in a shared markdown file. They read the whole doc for context, enabling cross-pollination between agents. Conductor merges proposed text into canon text. Writer frontend renders the file and refreshes on changes.

This eliminates the complex event routing that's currently broken and creates a live, observable collaboration surface.

## What Changed

- **Removed**: EventStore as coordination mechanism for document updates
- **Removed**: Separate Run app (Writer is the only UI)
- **Removed**: Polling loop in prompt bar
- **Added**: Proposed/Canon text model for work-in-progress vs accepted content
- **Added**: Workers write directly to shared document sections
- **Added**: FileOwner actor for safe concurrent writes
- **Added**: File-watch based live updates for Writer

## What To Do Next

Implement checkpoints 0-5 sequentially, with E2E browser testing after each.

---

## Design Principles

### 1. Document as Coordination Layer

The shared markdown file replaces events as the coordination mechanism:

```
runs/{run_id}/draft.md

# Compare rari and dioxus

## Conductor
Rari and Dioxus differ in three key ways...

## Researcher
<!-- proposal -->
Found 3 sources...
Reading Terminal's benchmarks below - interesting that...
Based on the compile time data...

## Terminal
<!-- proposal -->
Running benchmarks...
Result: Dioxus 2x faster on SSR
Note to Researcher: the macro overhead is real

## User
<!-- proposal -->
/also check wasm support
```

Workers read the whole doc (see each other's work), write to their section.

### 2. Proposed/Canon Text Model

| Style | Meaning | Who Writes |
|-------|---------|------------|
| Canon (default) | Accepted synthesis | Conductor only |
| Proposed (`<!-- proposal -->`) | Work in progress | Workers, User |

Conductor is the single writer of canon text. Proposed sections are collaborative scratch space.

**Dark/Light Mode Safe**: Use semantic CSS classes, not literal colors:
- `.text-canon` = default text color (varies by theme)
- `.text-proposed` = muted/secondary color (varies by theme)

### 3. Concurrency: FileOwner Actor

Single actor owns all file writes. Workers send messages to FileOwner, which serializes writes atomically.

```
Researcher ──┐
Terminal ────┼──→ FileOwner Actor → atomic write to draft.md
Conductor ───┘
```

No file locks, no race conditions, fits ractor supervision tree.

### 4. Section Parsing: pulldown-cmark

Already in codebase. Use `Parser::into_offset_iter()` for byte-range-based section find/replace.

### 5. Live Updates via File Watch

Backend watches `runs/` directory. On change, emit WebSocket event. Writer subscribes and refreshes.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                         User                                 │
│                          │                                   │
│                          ▼                                   │
│  ┌─────────────────────────────────────────────────────┐   │
│  │                   Prompt Bar                         │   │
│  │   - Submits objective                                │   │
│  │   - Opens Writer immediately (Checkpoint 0)          │   │
│  └─────────────────────────────────────────────────────┘   │
│                          │                                   │
│                          ▼                                   │
│  ┌─────────────────────────────────────────────────────┐   │
│  │                   Conductor                          │   │
│  │   - Creates draft.md with sections (Checkpoint 1)    │   │
│  │   - Spawns workers with doc path                     │   │
│  │   - Merges proposed→canon (Checkpoint 5)             │   │
│  └─────────────────────────────────────────────────────┘   │
│                          │                                   │
│          ┌───────────────┼───────────────┐                  │
│          ▼               ▼               ▼                  │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐           │
│  │ Researcher │  │  Terminal  │  │   Future   │           │
│  │            │  │            │  │   Workers  │           │
│  │ - Reads    │  │ - Reads    │  │            │           │
│  │   whole    │  │   whole    │  │            │           │
│  │   doc      │  │   doc      │  │            │           │
│  │ - Writes   │  │ - Writes   │  │            │           │
│  │   to own   │  │   to own   │  │            │           │
│  │   section  │  │   section  │  │            │           │
│  └────────────┘  └────────────┘  └────────────┘           │
│          │               │               │                  │
│          └───────────────┼───────────────┘                  │
│                          ▼                                   │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              FileOwner Actor                         │   │
│  │   - Single writer for all files                      │   │
│  │   - Atomic writes (temp + rename)                    │   │
│  │   - Section-aware operations                         │   │
│  └─────────────────────────────────────────────────────┘   │
│                          │                                   │
│                          ▼                                   │
│  ┌─────────────────────────────────────────────────────┐   │
│  │                  draft.md                            │   │
│  │   Shared document - coordination layer               │   │
│  └─────────────────────────────────────────────────────┘   │
│                          │                                   │
│                          ▼                                   │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              File Watcher                            │   │
│  │   - Watches runs/ directory                          │   │
│  │   - Emits WebSocket events on change                 │   │
│  └─────────────────────────────────────────────────────┘   │
│                          │                                   │
│                          ▼                                   │
│  ┌─────────────────────────────────────────────────────┐   │
│  │               Writer Frontend                        │   │
│  │   - Renders proposed/canon text (Checkpoint 4)       │   │
│  │   - Subscribes to file change events (Checkpoint 3)  │   │
│  │   - Auto-refreshes on write                          │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

---

## File Tools API (Best Practices)

Following LLM agent file tool best practices:

### Core Principles

1. **Read before edit** - Caller must have fresh content
2. **Exact match required** - Search string must match exactly once
3. **Helpful errors** - Report no-match or multiple-match with context
4. **Atomic writes** - Write to temp file, then rename
5. **Section-aware** - Operate on markdown sections, not raw text

### FileOwner Actor API

```rust
pub enum FileCommand {
    /// Read entire file content
    Read { path: String },
    
    /// Write entire file (creates if not exists)
    Write { path: String, content: String },
    
    /// Find section by title, return content
    ReadSection { path: String, section: String },
    
    /// Replace entire section content
    WriteSection { 
        path: String, 
        section: String, 
        content: String,
    },
    
    /// Append to section (for streaming progress)
    AppendSection { 
        path: String, 
        section: String, 
        content: String,
    },
    
    /// Search and replace within a section
    EditSection {
        path: String,
        section: String,
        old: String,  // Must match exactly once
        new: String,
    },
    
    /// Create document with standard section structure
    CreateRunDocument {
        run_id: String,
        objective: String,
    },
}

pub enum FileResult {
    Content { content: String },
    Written { bytes: usize },
    SectionContent { section: String, content: String },
    Created { path: String },
    Error { message: String, kind: FileErrorKind },
}

pub enum FileErrorKind {
    NotFound,
    SectionNotFound,
    NoMatch,
    MultipleMatches { count: usize },
    PathTraversal,
    IoError,
}
```

### Section Parser (pulldown-cmark)

```rust
pub struct SectionParser;

impl SectionParser {
    /// Find all sections in markdown
    pub fn find_sections(markdown: &str) -> Vec<Section>;
    
    /// Find specific section by title (case-insensitive)
    pub fn find_section(markdown: &str, title: &str) -> Option<Section>;
    
    /// Replace section content, return new markdown
    pub fn replace_section(markdown: &str, title: &str, new_content: &str) -> String;
    
    /// Append content to section
    pub fn append_to_section(markdown: &str, title: &str, content: &str) -> Option<String>;
    
    /// Check if section has <!-- proposal --> marker
    pub fn is_proposal(section: &Section) -> bool;
    
    /// Add/remove proposal marker
    pub fn set_proposal(markdown: &str, title: &str, is_proposal: bool) -> String;
}

pub struct Section {
    pub title: String,
    pub level: u8,
    pub start: usize,  // byte offset
    pub end: usize,    // byte offset
    pub content: String,
}
```

---

## Checkpoints (Iterative E2E Testing)

### Checkpoint 0: Writer Opens Immediately

**Goal**: Submit prompt → Writer opens with document path

**Changes**:
- `prompt_bar.rs`: After `execute_conductor()` returns task_id, construct path `runs/{task_id}/draft.md`
- Open Writer immediately (remove polling loop)
- Writer shows path in toolbar, "Running..." status badge

**Files**:
- `dioxus-desktop/src/desktop/components/prompt_bar.rs`
- `dioxus-desktop/src/api.rs` (if needed for window open)

**E2E Test**:
1. Open desktop in browser
2. Type "hello world" in prompt bar
3. Press Enter
4. Verify: Writer window opens within 2 seconds
5. Verify: Document path shown as `runs/{task_id}/draft.md`
6. Verify: Status shows "Running..."

---

### Checkpoint 1: Section Structure

**Goal**: Conductor creates doc with section headers

**Changes**:
- `bootstrap.rs`: Create `runs/{run_id}/draft.md` with structure
- Document structure:
  ```markdown
  # {objective}
  
  ## Conductor
  
  ## Researcher
  <!-- proposal -->
  
  ## Terminal
  <!-- proposal -->
  
  ## User
  <!-- proposal -->
  ```

**Files**:
- `sandbox/src/actors/conductor/runtime/bootstrap.rs`
- `sandbox/src/actors/conductor/file_tools.rs` (or new file_owner.rs)

**E2E Test**:
1. Submit "compare rari and dioxus"
2. Writer opens
3. Verify: Document shows:
   ```
   # compare rari and dioxus
   
   ## Conductor
   
   ## Researcher
   <!-- proposal -->
   
   ## Terminal
   <!-- proposal -->
   
   ## User
   <!-- proposal -->
   ```

---

### Checkpoint 2: Worker Writes to Section

**Goal**: Researcher appends findings to its section

**Changes**:
- Add `SectionParser` using pulldown-cmark
- Create `FileOwnerActor` for atomic writes
- Pass `document_path` to worker spawn
- Worker calls `AppendSection` after tool execution
- Researcher adapter writes progress

**Files**:
- `sandbox/src/actors/file_owner.rs` (new)
- `sandbox/src/actors/conductor/workers.rs` (pass doc path)
- `sandbox/src/actors/researcher/adapter.rs` (write to doc)
- `sandbox/src/actors/conductor/section_parser.rs` (new)

**E2E Test**:
1. Submit "research rust async runtimes"
2. Writer opens
3. Wait 5-10 seconds
4. Verify: `## Researcher` section contains findings (not empty)
5. Verify: Content appears progressively (multiple writes)

---

### Checkpoint 3: Writer Live Refresh

**Goal**: Writer refreshes when file changes

**Changes**:
- Add file watch on `runs/` directory using `notify` crate
- Emit WebSocket `document.changed` event on file modification
- Writer subscribes to events for its document path
- On event, reload document via `writer_open()` API

**Files**:
- `sandbox/src/api/websocket.rs` (emit events)
- `sandbox/src/watcher.rs` (file watch logic)
- `dioxus-desktop/src/components/writer.rs` (subscribe, reload)

**E2E Test**:
1. Submit prompt
2. Writer opens
3. Watch document update in real-time without manual refresh
4. Verify: New content appears within 1 second of backend write

---

### Checkpoint 4: Proposed/Canon Rendering

**Goal**: Proposed sections styled differently from canon

**Changes**:
- Parse `<!-- proposal -->` markers in Writer
- Apply CSS classes:
  ```css
  .text-canon { color: var(--text-primary); }
  .text-proposed { color: var(--text-secondary); opacity: 0.7; }
  ```
- Works in both Edit and Preview modes
- Strikethrough support: `~~deleted text~~` in proposed sections

**Files**:
- `dioxus-desktop/src/components/writer.rs`
- `dioxus-desktop/src/style.css` (or inline styles)

**E2E Test**:
1. Submit prompt
2. Writer opens
3. Verify: `## Researcher` content is gray/muted
4. Verify: After conductor merge, `## Conductor` content is normal color
5. Toggle dark/light mode
6. Verify: Colors adapt to theme

---

### Checkpoint 5: Conductor Merge

**Goal**: Conductor synthesizes proposed sections into canon

**Changes**:
- On worker completion, conductor reads document
- Calls synthesis (LLM or simple aggregation)
- Writes to `## Conductor` section (canon, no `<!-- proposal -->`)
- Optionally clears or marks worker sections as complete

**Files**:
- `sandbox/src/actors/conductor/runtime/decision.rs`
- `sandbox/src/actors/conductor/runtime/call_result.rs`

**BAML Changes**:
- Add `MergeResults` action
- Conductor receives document content in decision input

**E2E Test**:
1. Submit prompt requiring research
2. Watch researchers write to `## Researcher` (gray)
3. Wait for completion
4. Verify: `## Conductor` contains synthesis (not gray)
5. Verify: Synthesis summarizes researcher findings

---

## Dependency Graph

```
Checkpoint 0 ──→ Checkpoint 1 ──→ Checkpoint 2 ──→ Checkpoint 4 ──→ Checkpoint 5
                                    │
                                    └──→ Checkpoint 3 (parallel)
```

- Checkpoint 0-2 are sequential (foundation)
- Checkpoint 3 (live refresh) can be done in parallel with 2
- Checkpoint 4-5 need 2 complete

---

## File Structure

```
runs/
├── {run_id_1}/
│   └── draft.md           # Main collaboration document
├── {run_id_2}/
│   └── draft.md
└── ...

draft.md format:
# {objective}

## Conductor
{canon - synthesis, no marker}

## Researcher
<!-- proposal -->
{findings in progress}

## Terminal
<!-- proposal -->
{commands/output in progress}

## User
<!-- proposal -->
{inline directives}
```

---

## CSS Styling (Theme-Safe)

```css
/* Semantic classes, not literal colors */
.text-canon {
  color: var(--text-primary, inherit);
}

.text-proposed {
  color: var(--text-secondary, #6b7280);
  opacity: 0.8;
}

.text-proposed-deleted {
  color: var(--text-secondary, #6b7280);
  text-decoration: line-through;
  opacity: 0.6;
}

/* Dark mode example */
@media (prefers-color-scheme: dark) {
  :root {
    --text-primary: #f9fafb;
    --text-secondary: #9ca3af;
  }
}

/* Light mode example */
@media (prefers-color-scheme: light) {
  :root {
    --text-primary: #111827;
    --text-secondary: #6b7280;
  }
}
```

---

## Implementation Order (Subagent Tasks)

### Task A: Checkpoint 0 - Writer Opens Immediately
1. Modify `prompt_bar.rs` to open Writer after execute_conductor
2. Remove `poll_conductor_task_until_complete()` polling loop
3. Pass document path to Writer
4. Add "Running..." status indicator

### Task B: Checkpoint 1 - Section Structure
1. Create `section_parser.rs` with pulldown-cmark
2. Modify `bootstrap.rs` to create structured document
3. Ensure document is created before returning from bootstrap

### Task C: Checkpoint 2 - FileOwner Actor + Worker Writes
1. Create `file_owner.rs` actor with atomic writes
2. Wire FileOwner into supervisor tree
3. Pass document_path to worker spawn
4. Researcher adapter calls FileOwner to append

### Task D: Checkpoint 3 - Live Refresh
1. Add `notify` crate for file watching
2. Create file watcher that emits WebSocket events
3. Writer subscribes to document.changed events
4. Writer reloads on event

### Task E: Checkpoint 4 - Proposed/Canon Rendering
1. Parse `<!-- proposal -->` markers in Writer
2. Apply semantic CSS classes
3. Test in dark and light modes

### Task F: Checkpoint 5 - Conductor Merge
1. Add `MergeResults` action to BAML
2. Conductor reads document on worker completion
3. Synthesize proposed sections
4. Write to `## Conductor` section

---

## Cherry-Pick from Uncommitted Changes

### Keep
- `file_tools.rs` - validation logic (adapt for FileOwner)
- `document_path` in `ConductorRunState`
- BAML simplification (4 actions)
- `emit_document_update` function (repurpose for file watch)

### Discard
- Run app (`dioxus-desktop/src/components/run.rs`)
- Polling loop in prompt_bar
- EventStore-based document coordination
- `UpdateDraft` action (replaced by worker direct writes)

---

## Open Questions / Future Work

### Deferred to Phase 2
1. **CRDT for true collaboration** - yrs (y-crdt) when we need multi-user
2. **Writer Actor** - per-document actor for human interaction loop
3. **Multi-app spawning** - Conductor spawns Mail, Calendar, etc.
4. **Conflict resolution** - When user edits during active run

### MVP Assumptions
1. **Single user** - No multi-user collaboration yet
2. **No concurrent runs** - One active run at a time (for now)
3. **Simple merge** - Conductor synthesizes, no complex reconciliation
4. **File is source of truth** - No separate state sync needed

---

## Success Criteria

1. ✅ Submit prompt → Writer opens immediately (< 2s)
2. ✅ Document has correct section structure
3. ✅ Workers write to sections → Writer shows live progress
4. ✅ Proposed text is visually distinct from canon
5. ✅ Conductor merges → Canon synthesis appears
6. ✅ Works in both dark and light modes
7. ✅ No polling timeout errors
8. ✅ Multi-agent cooperation visible in doc structure

---

## Testing Commands

```bash
# Run backend
just dev-sandbox

# Run frontend
just dev-ui

# Run specific checkpoint tests
cargo test -p sandbox --test checkpoint_0_writer_opens
cargo test -p sandbox --test checkpoint_1_section_structure
cargo test -p sandbox --test checkpoint_2_worker_writes

# E2E with agent-browser
agent-browser open http://localhost:3000
agent-browser screenshot tests/checkpoint_0_initial.png
# ... interact ...
agent-browser screenshot tests/checkpoint_0_result.png
```
