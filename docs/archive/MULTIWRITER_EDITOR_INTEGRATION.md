# Multiwriter Collaborative Editor Integration Decision Document

**Date:** 2026-02-14  
**Project:** ChoirOS Sandbox  
**Target:** Google-Docs-style collaborative editing with inline suggestions  
**Status:** Research Complete - Ready for PoC

---

## Executive Summary

**Primary Recommendation:** TipTap (ProseMirror) + Yjs (via yrs) + Custom Track Changes Layer

1. **TipTap** provides the best balance of MIT licensing, active maintenance, and Yjs-native collaboration
2. **yrs** (Rust Yjs port) enables server-side CRDT operations matching ChoirOS actor architecture
3. Track changes requires custom implementation via ProseMirror decorations - no open-source solution is production-ready
4. **y-sweet** (Rust) or **Hocuspocus** (TypeScript) as WebSocket sync server
5. Integration: Embed TipTap in Dioxus webview via custom protocol, sync deltas to Rust backend via IPC
6. Comments/suggestions anchored using Yjs relative positions for conflict-free persistence
7. Revision snapshots via Yjs `encodeStateAsUpdate()` stored in SQLite/sqlx
8. Estimated PoC time: 2-3 weeks for basic collab + 2 weeks for track changes
9. **Fallback:** Lexical + Yjs if TipTap Pro licensing becomes problematic
10. **No-go criteria:** If track changes UX requires >40 hours custom work, evaluate CKEditor commercial license

---

## Comparison Matrix: 8 Candidate Stacks

| Stack | License | Track Changes | Comments | Collab Native | Rust Backend | Integration (1-5) | PoC Time | Last Release |
|-------|---------|---------------|----------|---------------|--------------|-------------------|----------|--------------|
| **TipTap + Yjs** | MIT | Pro (paid) / Custom | Pro (paid) / Custom | ✅ Yjs | yrs | 2 | 3-4 wks | Feb 2026 |
| **Lexical + Yjs** | MIT | Custom | Community | ✅ Yjs | yrs | 3 | 4-5 wks | Feb 2026 |
| **ProseMirror + Yjs** | MIT | Custom | Custom | ✅ Yjs | yrs | 4 | 5-6 wks | Active |
| **Milkdown + Yjs** | MIT | Custom | Custom | ✅ Yjs | yrs | 3 | 4-5 wks | Jan 2026 |
| **CKEditor 5 OSS** | GPL-2.0 | Premium | Premium | ✅ Native | None | 3 | 2-3 wks | Feb 2026 |
| **Slate + Yjs** | MIT | Custom | Custom | Community | yrs | 5 | 6+ wks | Dec 2025 |
| **BlockNote + Yjs** | MPL-2.0 | Custom | Custom | ✅ Yjs | yrs | 3 | 4 wks | Active |
| **Automerge Native** | MIT | Custom | Custom | ✅ Built-in | automerge-rs | 5 | 8+ wks | Feb 2026 |

### Scoring Rubric (1-5 each)

| Stack | Feature Complete | Suggestion UX | Rust Integration | Ops Complexity | Maintainability | License Fit | Performance | **Total** |
|-------|------------------|---------------|------------------|----------------|-----------------|-------------|-------------|-----------|
| TipTap + Yjs | 4 | 3 (custom) | 5 | 3 | 5 | 5 | 5 | **30** |
| Lexical + Yjs | 3 | 3 (custom) | 5 | 3 | 5 | 5 | 5 | **29** |
| ProseMirror + Yjs | 3 | 3 (custom) | 5 | 3 | 4 | 5 | 5 | **28** |
| CKEditor 5 | 5 | 5 (premium) | 1 | 2 | 4 | 2 (GPL) | 4 | **23** |
| Milkdown + Yjs | 3 | 3 (custom) | 5 | 3 | 4 | 5 | 5 | **28** |
| Slate + Yjs | 2 | 2 (custom) | 5 | 4 | 3 | 5 | 3 | **24** |
| BlockNote + Yjs | 3 | 2 (custom) | 5 | 3 | 3 | 4 (MPL) | 4 | **24** |
| Automerge Native | 1 | 1 (custom) | 5 | 4 | 4 | 5 | 3 | **23** |

---

## Top 3 Integration Architectures

### Architecture 1: TipTap + yrs + Custom Track Changes (Recommended)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         ChoirOS Desktop (Dioxus)                            │
├─────────────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  WebView (wry)                                                      │    │
│  │  ┌───────────────────────────────────────────────────────────────┐  │    │
│  │  │  TipTap Editor (ProseMirror)                                  │  │    │
│  │  │  ┌─────────────────────────────────────────────────────────┐  │  │    │
│  │  │  │  Extensions:                                            │  │  │    │
│  │  │  │  - Collaboration (y-prosemirror)                        │  │  │    │
│  │  │  │  - CollaborationCursor (presence)                       │  │  │    │
│  │  │  │  - TrackChanges (custom plugin)                         │  │  │    │
│  │  │  │  - Comments (custom plugin)                             │  │  │    │
│  │  │  └─────────────────────────────────────────────────────────┘  │  │    │
│  │  └───────────────────────────────────────────────────────────────┘  │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                              │ IPC (postMessage)                            │
│                              ▼                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  Rust Backend (sandbox crate)                                       │    │
│  │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────────┐  │    │
│  │  │  yrs::Doc       │  │  WriterActor    │  │  EventBus           │  │    │
│  │  │  (CRDT state)   │  │  (coordination) │  │  (telemetry)        │  │    │
│  │  └────────┬────────┘  └────────┬────────┘  └─────────────────────┘  │    │
│  │           │                    │                                    │    │
│  │           ▼                    ▼                                    │    │
│  │  ┌─────────────────────────────────────────────────────────────┐    │    │
│  │  │  SQLite/sqlx (persistence)                                │    │    │
│  │  │  - Document snapshots (Yjs updates)                         │    │    │
│  │  │  - Suggestion metadata                                      │    │    │
│  │  │  - Revision history                                         │    │    │
│  │  └─────────────────────────────────────────────────────────────┘    │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                              │ WebSocket                                    │
│                              ▼                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  y-sweet Server (Rust) or Hocuspocus (TypeScript)                   │    │
│  │  - Sync protocol (y-sync)                                           │    │
│  │  - Awareness (presence)                                             │    │
│  │  - Persistence (S3 / filesystem)                                    │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Key Components:**
- **Editor:** TipTap v3 (MIT) with headless architecture
- **Collaboration:** y-prosemirror binding, yrs for Rust CRDT operations
- **Sync Server:** y-sweet (Rust, MIT) - https://github.com/jamsocket/y-sweet
- **Track Changes:** Custom ProseMirror plugin using decorations API
- **Comments:** Custom plugin using Yjs relative positions

**Data Flow:**
1. User edit → TipTap transaction → y-prosemirror → Y.Doc update
2. Y.Doc update → WebSocket → y-sweet server → broadcast to peers
3. Backend subscribes to updates via yrs, persists snapshots
4. Suggestions stored as Y.Map with relative positions

---

### Architecture 2: Lexical + Yjs + Custom Track Changes

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         ChoirOS Desktop (Dioxus)                            │
├─────────────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  WebView (wry)                                                      │    │
│  │  ┌───────────────────────────────────────────────────────────────┐  │    │
│  │  │  Lexical Editor (Meta)                                        │  │    │
│  │  │  ┌─────────────────────────────────────────────────────────┐  │  │    │
│  │  │  │  Plugins:                                               │  │  │    │
│  │  │  │  - LexicalCollaborationPlugin (@lexical/yjs)            │  │  │    │
│  │  │  │  - MarkNode (comment anchoring)                         │  │  │    │
│  │  │  │  - TrackChangesPlugin (custom)                          │  │  │    │
│  │  │  └─────────────────────────────────────────────────────────┘  │  │    │
│  │  └───────────────────────────────────────────────────────────────┘  │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                              │ IPC / JSON                                   │
│                              ▼                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  Rust Backend                                                       │    │
│  │  - yrs::Doc for server-side CRDT operations                         │    │
│  │  - Actor integration via EventBus                                   │    │
│  │  - SQLite persistence                                               │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                              │ WebSocket                                    │
│                              ▼                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  y-sweet or Hocuspocus                                              │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Key Components:**
- **Editor:** Lexical (Meta, MIT) - https://github.com/facebook/lexical
- **Collaboration:** `@lexical/yjs` - native Yjs integration
- **Comments:** `@lexical/mark` for anchored annotations
- **Track Changes:** Custom via Lexical node transforms

**Pros over TipTap:**
- Meta backing, larger scale production use
- Cleaner immutability model
- Better TypeScript support

**Cons:**
- Less mature ecosystem
- React-centric docs (headless mode exists but less documented)
- Smaller plugin community

---

### Architecture 3: Pure Rust + yrs (Experimental)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         ChoirOS Desktop (Dioxus)                            │
├─────────────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  Dioxus Native UI (no webview JS)                                   │    │
│  │  ┌───────────────────────────────────────────────────────────────┐  │    │
│  │  │  Custom Editor Component (Rust)                               │  │    │
│  │  │  - yrs::Text backing                                          │  │    │
│  │  │  - Dioxus textarea with rich text rendering                   │  │    │
│  │  │  - Suggestion overlay layer                                   │  │    │
│  │  └───────────────────────────────────────────────────────────────┘  │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                              │ Direct Rust                                  │
│                              ▼                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  Rust Backend (same process)                                        │    │
│  │  - yrs::Doc shared with UI                                          │    │
│  │  - WriterActor coordination                                         │    │
│  │  - SQLite persistence                                               │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                              │ WebSocket (y-sync)                           │
│                              ▼                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  yrs-warp Server (Rust WebSocket)                                   │    │
│  │  - Native Rust sync server                                          │    │
│  │  - No JavaScript required                                           │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Key Components:**
- **Editor:** Custom Dioxus component backed by `yrs::Text`
- **Collaboration:** Native yrs (no JS bridge)
- **Sync Server:** `yrs-warp` - https://github.com/y-crdt/y-crdt/tree/main/yrs-warp

**Pros:**
- Pure Rust stack
- No JS interop complexity
- Direct memory access to CRDT state

**Cons:**
- **No rich text rendering in Dioxus** - would need custom markdown-to-widgets
- **Huge implementation effort** - building a WYSIWYG editor from scratch
- **Not recommended** unless rich text is simple markdown

---

## Deep Dive: Top 3 Candidates

### 1. TipTap + Yjs + yrs (Primary Recommendation)

#### Sources
- Repo: https://github.com/ueberdosis/tiptap (35.1k stars)
- Docs: https://tiptap.dev/docs/editor/introduction
- License: MIT (core), Commercial (Pro extensions)
- Last Release: v3.19.0 (Feb 3, 2026)
- Activity: 949 releases, very active

#### Architecture Pattern
- **Headless framework** built on ProseMirror
- Extension-based plugin system
- Framework-agnostic core with React/Vue/vanilla bindings
- JSON state model for serialization

#### Feature Support

| Feature | Native | Custom Required | Notes |
|---------|--------|-----------------|-------|
| Rich text editing | ✅ | - | Full WYSIWYG |
| Real-time collaboration | ✅ | - | y-prosemirror |
| Multi-cursor presence | ✅ | - | CollaborationCursor extension |
| Inline suggestions | ❌ | ✅ | Pro extension paid, custom via decorations |
| Comments anchored | ❌ | ✅ | Pro extension paid, custom via Y.Map |
| Revision history | ✅ | - | Snapshot extension (Pro) or Yjs snapshots |
| Offline editing | ✅ | - | Yjs IndexedDB provider |
| Markdown support | ✅ | - | Native extension |

#### Integration Complexity: 2/5
- TipTap designed for embedding
- Clear JSON API for state sync
- Well-documented IPC patterns from Tauri ecosystem

#### Track Changes Implementation
```javascript
// Custom TipTap extension for track changes
import { Extension } from '@tiptap/core'
import { Plugin, PluginKey } from 'prosemirror-state'
import { Decoration, DecorationSet } from 'prosemirror-view'

const TrackChangesExtension = Extension.create({
  name: 'trackChanges',
  
  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: new PluginKey('trackChanges'),
        state: {
          init: () => DecorationSet.empty,
          apply: (tr, oldSet) => {
            // Map decorations through transactions
            return oldSet.map(tr.mapping, tr.doc)
          }
        },
        props: {
          decorations: (state) => {
            // Get suggestions from Y.Map
            const suggestions = ydoc.getMap('suggestions')
            const decorations = []
            
            suggestions.forEach((suggestion, id) => {
              const from = resolvePosition(suggestion.from)
              const to = resolvePosition(suggestion.to)
              
              if (suggestion.type === 'insert') {
                decorations.push(Decoration.inline(from, to, {
                  class: 'suggestion-insert',
                  style: 'background-color: #e6ffec; border-bottom: 2px solid #35b266;'
                }))
              } else if (suggestion.type === 'delete') {
                decorations.push(Decoration.inline(from, to, {
                  class: 'suggestion-delete',
                  style: 'text-decoration: line-through; color: #ff6b6b;'
                }))
              }
            })
            
            return DecorationSet.create(state.doc, decorations)
          }
        }
      })
    ]
  }
})
```

#### Comments Implementation
```javascript
// Comments stored in Y.Map with relative positions
interface Comment {
  id: string
  threadId: string
  from: Y.RelativePosition  // Persists across edits
  to: Y.RelativePosition
  author: string
  content: string
  createdAt: number
  resolved: boolean
}

// Add comment
function addComment(ydoc, from, to, author, content) {
  const comments = ydoc.getMap('comments')
  const id = generateId()
  
  comments.set(id, {
    id,
    threadId: generateId(),
    from: Y.createRelativePositionFromTypeIndex(ydoc.getText('content'), from),
    to: Y.createRelativePositionFromTypeIndex(ydoc.getText('content'), to),
    author,
    content,
    createdAt: Date.now(),
    resolved: false
  })
}
```

#### Major Risks
1. **Track changes not open-source** - must build custom or pay for Pro
2. **Vendor lock-in** - commercial entity (ueberdosis) behind TipTap
3. **Pro extension beta** - comments extension still marked beta

#### Migration Strategy from Current Writer
1. **Phase 1:** Parallel run - existing writer generates Yjs doc
2. **Phase 2:** Import tool converts existing revisions to Yjs updates
3. **Phase 3:** Dual-write period for validation
4. **Phase 4:** Cutover to TipTap as primary editor

---

### 2. Lexical + Yjs + yrs (Fallback)

#### Sources
- Repo: https://github.com/facebook/lexical (22.9k stars)
- Docs: https://lexical.dev/docs/intro
- License: MIT
- Last Release: v0.40.0 (Feb 2, 2026)
- Activity: 84 releases, Meta-backed

#### Architecture Pattern
- **Immutable state model** (like Redux for editors)
- React-first but headless core available
- Node-based document structure
- Serialized state to JSON

#### Feature Support

| Feature | Native | Custom Required | Notes |
|---------|--------|-----------------|-------|
| Rich text editing | ✅ | - | Full WYSIWYG |
| Real-time collaboration | ✅ | - | @lexical/yjs |
| Multi-cursor presence | ✅ | - | Native support |
| Inline suggestions | ❌ | ✅ | Custom via node transforms |
| Comments anchored | ⚠️ | ✅ | @lexical/mark exists |
| Revision history | ✅ | - | State snapshots |
| Offline editing | ✅ | - | Yjs IndexedDB |

#### Integration Complexity: 3/5
- More React-centric than TipTap
- Headless mode exists but less documented
- State model is cleaner but less community examples

#### Track Changes Implementation
```typescript
// Lexical track changes via custom node transform
import { $createTextNode, $getNodeByKey, $getSelection } from 'lexical'
import { registerNodeTransform } from '@lexical/rich-text'

function registerTrackChanges(editor, ydoc) {
  // Store pending changes in Y.Map
  const changes = ydoc.getMap('pendingChanges')
  
  editor.registerNodeTransform(TextNode, (node) => {
    const selection = $getSelection()
    if (!selection) return
    
    // Check if in suggestion mode
    if (suggestionMode.active) {
      const change = {
        type: 'insert',
        nodeKey: node.getKey(),
        content: node.getTextContent(),
        author: currentUser.id,
        timestamp: Date.now()
      }
      changes.set(generateId(), change)
    }
  })
}
```

#### Major Risks
1. **Smaller ecosystem** - fewer plugins than ProseMirror/TipTap
2. **React-centric docs** - headless mode underdocumented
3. **Meta priority** - developed for Meta's needs first

---

### 3. CKEditor 5 Commercial (If Budget Available)

#### Sources
- Repo: https://github.com/ckeditor/ckeditor5 (10.4k stars)
- Docs: https://ckeditor.com/docs/ckeditor5/latest/
- License: GPL-2.0 OR Commercial
- Last Release: v47.5.0 (Feb 11, 2026)

#### Why Consider
- **Only production-ready track changes** in open-source ecosystem
- Comments, suggestions, revisions all built-in
- Full collaboration stack

#### License Reality
- **GPL-2.0** requires open-sourcing your application OR purchasing commercial license
- Commercial license cost: Contact CKSource for pricing
- For proprietary ChoirOS: Must purchase license

#### Integration Complexity: 3/5
- Full-featured, but heavy
- TypeScript-based, good API
- Self-hostable with commercial license

#### Estimated License Cost
- Unknown - requires quote
- Similar products typically $500-2000/month for teams

---

## Step-by-Step PoC Plans

### PoC 1: TipTap + yrs (Recommended)

**Duration:** 3-4 weeks  
**Goal:** Validate real-time collaboration + basic track changes

#### Week 1: Basic Editor Setup
```bash
# 1. Create TipTap test app
npm create vite@latest tiptap-poc -- --template vanilla-ts
cd tiptap-poc
npm install @tiptap/core @tiptap/starter-kit yjs y-prosemirror

# 2. Add to Dioxus
# Create custom protocol to serve editor HTML
```

**Tasks:**
- [ ] Create minimal TipTap editor with starter kit
- [ ] Integrate y-prosemirror for local collaboration
- [ ] Test basic real-time sync between two browser tabs
- [ ] Embed in Dioxus webview via custom protocol

**Validation:**
- Two tabs edit same document
- Changes appear in real-time
- Cursor positions visible

#### Week 2: yrs Backend Integration
```toml
# Cargo.toml
[dependencies]
yrs = "0.25"
y-sync = "0.3"
tokio = { version = "1", features = ["full"] }
axum = "0.8"
```

**Tasks:**
- [ ] Add yrs to sandbox crate
- [ ] Create WebSocket endpoint using y-sync protocol
- [ ] Connect TipTap to Rust backend
- [ ] Persist document updates to SQLite

**Validation:**
- Document state survives server restart
- Multiple clients sync through Rust backend
- No data loss on network interruption

#### Week 3: Track Changes Prototype
```rust
// shared-types/src/suggestion.rs
use serde::{Deserialize, Serialize};
use yrs::block::ID;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    pub id: ulid::Ulid,
    pub suggestion_type: SuggestionType,
    pub from_pos: Vec<u8>,  // Serialized Y.RelativePosition
    pub to_pos: Vec<u8>,
    pub author_id: String,
    pub content: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub status: SuggestionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SuggestionType {
    Insert,
    Delete,
    FormatChange,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SuggestionStatus {
    Pending,
    Accepted,
    Rejected,
}
```

**Tasks:**
- [ ] Design suggestion data model
- [ ] Implement ProseMirror decoration plugin
- [ ] Add accept/reject handlers
- [ ] Test suggestion persistence across sessions

**Validation:**
- Insert suggestion shows green text
- Delete suggestion shows strikethrough
- Accept applies change, reject removes suggestion
- Suggestions persist after page reload

#### Week 4: Integration with WriterActor
```rust
// sandbox/src/actors/writer_actor.rs additions
impl WriterActor {
    async fn handle_suggestion(&mut self, suggestion: Suggestion) -> Result<()> {
        match suggestion.suggestion_type {
            SuggestionType::Insert => {
                // Content already in doc as decoration
                // Accept: remove decoration, keep content
                // Reject: remove decoration, delete content
            }
            SuggestionType::Delete => {
                // Content still in doc
                // Accept: delete content, remove decoration
                // Reject: just remove decoration
            }
        }
        self.event_bus.emit(Event::SuggestionHandled {
            suggestion_id: suggestion.id,
            status: suggestion.status,
        }).await;
        Ok(())
    }
}
```

**Tasks:**
- [ ] Wire suggestions to WriterActor
- [ ] Emit events for suggestion lifecycle
- [ ] Add API endpoints for suggestion operations
- [ ] Test programmatic insertion of worker suggestions

**Validation:**
- Worker can insert suggestions programmatically
- Human can accept/reject via UI
- Full audit trail in EventBus

---

### PoC 2: Lexical + Yjs (Fallback)

**Duration:** 4-5 weeks  
**Goal:** Validate as alternative if TipTap Pro licensing blocks

#### Week 1-2: Lexical Editor Setup
```bash
npm install lexical @lexical/react @lexical/rich-text @lexical/yjs yjs
```

**Tasks:**
- [ ] Create Lexical editor with collaboration plugin
- [ ] Test Yjs sync with vanilla WebSocket
- [ ] Embed in Dioxus webview
- [ ] Compare developer experience vs TipTap

#### Week 3-4: Track Changes via Node Transforms
**Tasks:**
- [ ] Implement suggestion nodes using Lexical node system
- [ ] Create decoration system for visual feedback
- [ ] Wire to yrs backend

#### Week 5: Evaluation
**Tasks:**
- [ ] Compare with TipTap PoC
- [ ] Document pros/cons
- [ ] Make final decision

---

## Final Recommendation

### Primary Choice: TipTap + yrs + Custom Track Changes

**Rationale:**
1. **Best ecosystem** - 35k+ stars, active development, largest plugin community
2. **MIT license** - no GPL contamination, no forced purchases
3. **yrs compatibility** - Rust CRDT operations match ChoirOS architecture
4. **Headless design** - fits Dioxus webview pattern naturally
5. **Track changes buildable** - ProseMirror decorations API is well-documented

**Estimated Total Effort:**
- PoC: 3-4 weeks
- Production: 8-12 weeks (including track changes, comments, revision history)

### Fallback Choice: Lexical + Yjs

**Trigger Conditions:**
- TipTap Pro licensing becomes blocker
- Custom track changes exceeds 40 hours
- Meta ecosystem preferred (team experience)

### Go/No-Go Criteria

| Criterion | Threshold | Decision |
|-----------|-----------|----------|
| Track changes PoC working | Week 3 | Go if functional |
| yrs sync latency | <100ms | Go if met |
| Bundle size | <500KB gzipped | Go if met |
| Worker suggestion insertion | Programmatic API exists | Go if met |
| License compatibility | MIT/Apache | No-go if GPL required |
| **Track changes effort** | <40 hours custom work | No-go if exceeds, evaluate CKEditor |

### Explicit No-Go Scenarios

1. **GPL contamination risk** - If any dependency forces GPL, reject
2. **Track changes >40 hours** - Custom implementation too complex, buy CKEditor
3. **Sync latency >200ms** - UX unacceptable for real-time collaboration
4. **Bundle size >1MB** - Desktop app performance impact
5. **No worker API** - Cannot insert suggestions programmatically

---

## Unknowns and Validation Experiments

### Must Validate Before Production

1. **Yjs relative position stability**
   - Experiment: Insert 10k characters, verify comment anchors remain
   - Risk: Position drift in edge cases

2. **yrs ↔ JavaScript Yjs compatibility**
   - Experiment: Sync updates between yrs and y-prosemirror
   - Risk: Binary encoding differences

3. **Large document performance**
   - Experiment: 100k+ word document with 100+ suggestions
   - Risk: Decoration recalculation lag

4. **Offline merge conflicts**
   - Experiment: Edit offline for 1 hour, reconnect
   - Risk: Unexpected merge behavior

5. **Worker suggestion insertion**
   - Experiment: AI worker inserts suggestion via yrs API
   - Risk: Permission model, attribution

### Secondary Unknowns

1. **Dioxus webview eval() performance** - Measure IPC overhead
2. **Yjs snapshot restoration** - Test full document rollback
3. **Presence at scale** - Test with 10+ simultaneous users
4. **Mobile support** - Dioxus mobile + touch editor

---

## Reference Links

### Editor Frameworks
- TipTap: https://github.com/ueberdosis/tiptap | https://tiptap.dev
- Lexical: https://github.com/facebook/lexical | https://lexical.dev
- ProseMirror: https://github.com/ProseMirror/prosemirror | https://prosemirror.net
- Milkdown: https://github.com/Milkdown/milkdown | https://milkdown.dev

### Collaboration
- Yjs: https://github.com/yjs/yjs | https://docs.yjs.dev
- yrs: https://github.com/y-crdt/y-crdt | https://docs.rs/yrs/
- Hocuspocus: https://github.com/ueberdosis/hocuspocus
- y-sweet: https://github.com/jamsocket/y-sweet
- Automerge: https://github.com/automerge/automerge | https://automerge.org

### Track Changes References
- ProseMirror Decorations: https://prosemirror.net/docs/ref/#view.Decoration
- Yjs Relative Positions: https://docs.yjs.dev/api/relative-positions
- CKEditor Track Changes: https://ckeditor.com/docs/ckeditor5/latest/features/collaboration/track-changes/track-changes.html

### Dioxus Integration
- Dioxus Desktop API: https://docs.rs/dioxus-desktop/0.7.3/dioxus_desktop/
- Wry WebView: https://github.com/tauri-apps/wry

### Benchmarks
- CRDT Benchmarks: https://github.com/dmonad/crdt-benchmarks
- Josephg's CRDT analysis: https://josephg.com/blog/crdts-go-brrr/

---

## Appendix: Data Model Mappings

### ChoirOS Writer → Yjs Schema

```rust
// Existing Writer revision model
pub struct Revision {
    pub id: Ulid,
    pub document_id: Ulid,
    pub content: String,
    pub diff_from_previous: Option<Diff>,
    pub author: ActorId,
    pub created_at: DateTime<Utc>,
}

// Yjs document structure
Y.Doc {
    "content": Y.XmlFragment,  // Main document content
    "suggestions": Y.Map<Suggestion>,
    "comments": Y.Map<Comment>,
    "metadata": Y.Map {
        "title": String,
        "created_at": Number,
        "updated_at": Number,
    }
}
```

### Suggestion → Y.Map Entry

```javascript
// JavaScript (TipTap side)
ydoc.getMap('suggestions').set('suggestion_123', {
    id: 'suggestion_123',
    type: 'insert',  // or 'delete', 'format'
    from: Y.createRelativePositionFromTypeIndex(content, 42),
    to: Y.createRelativePositionFromTypeIndex(content, 56),
    author: 'user_456',
    content: 'suggested text',
    createdAt: Date.now(),
    status: 'pending'
})
```

```rust
// Rust (yrs side)
let suggestions = doc.get_or_insert_map("suggestions");
let mut txn = doc.transact_mut();
let suggestion = yrs::Map::new();
suggestion.insert(&mut txn, "id", "suggestion_123");
suggestion.insert(&mut txn, "type", "insert");
// ... etc
```

---

*Document generated: 2026-02-14*  
*Next review: After PoC completion*
