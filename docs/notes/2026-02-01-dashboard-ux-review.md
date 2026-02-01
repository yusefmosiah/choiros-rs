# ChoirOS Dashboard UX Review

**Date:** 2026-02-01  
**Scope:** sandbox-ui/ - Dioxus-based frontend dashboard  
**Purpose:** Document UI architecture, components, user flows, and identify documentation gaps

---

## 1. UI Architecture Overview

### Technology Stack
- **Framework:** Dioxus (Rust-based WASM frontend)
- **Styling:** CSS-in-Rust with CSS custom properties (tokens)
- **HTTP Client:** gloo-net
- **WebSocket:** Native web-sys WebSocket
- **State Management:** Dioxus signals (`use_signal`)

### Module Structure

```
sandbox-ui/src/
├── main.rs          # Entry point - launches Desktop component
├── lib.rs           # Module exports
├── desktop.rs       # Core desktop shell + window management
├── components.rs    # ChatView, MessageBubble, LoadingIndicator
├── api.rs           # HTTP API client for backend
└── interop.rs       # WASM/JS interop (viewport, drag/resize)
```

---

## 2. Key Components

### Desktop Shell (`desktop.rs`)

**Purpose:** Main container managing the entire desktop environment

**Key Features:**
- **Responsive Layout:** Desktop (>1024px) vs Mobile layouts
- **Desktop Icons:** Grid-based app launcher (4 cols desktop, 2 cols mobile)
- **Window Canvas:** Floating window container with z-index management
- **Prompt Bar:** Bottom command input with running app indicators

**Core Apps (Hardcoded):**
| App ID | Name | Icon | Default Size |
|--------|------|------|--------------|
| chat | Chat | 💬 | 600x500 |
| writer | Writer | 📝 | 800x600 |
| terminal | Terminal | 🖥️ | 700x450 |
| files | Files | 📁 | 700x500 |

**State Management:**
- `desktop_state: Signal<Option<DesktopState>>` - All windows/apps
- `viewport: Signal<(u32, u32)>` - Browser viewport dimensions
- `ws_connected: Signal<bool>` - WebSocket connection status

### FloatingWindow Component

**Features:**
- Draggable titlebar (desktop only)
- Resizable via corner handle (desktop only)
- Active/inactive visual states
- Responsive sizing (mobile = near-fullscreen)
- Window content routing by `app_id`

**Window Operations:**
- `on_close` - Remove window from desktop
- `on_focus` - Bring to front (z-index++)
- `on_move` - Update position (x, y)
- `on_resize` - Update dimensions (w, h)

### ChatView Component (`components.rs`)

**Features:**
- Message list with scrollable area
- Optimistic message updates (pending state)
- Typing indicator during AI response
- Auto-resizing textarea input
- Keyboard shortcuts (Enter to send, Shift+Enter newline)

**Message Types:**
- User messages (blue bubble, right-aligned)
- Assistant messages (dark bubble, left-aligned)
- Pending messages ("sending..." badge)

### Prompt Bar

**Layout:**
```
[? Help] [Input: "Ask anything, paste URL, or type ? for commands..."] [App Icons] [Status]
```

**Features:**
- Global command input
- Running app indicators (click to focus)
- WebSocket connection status badge
- Submit triggers chat window open + message send

---

## 3. User Flow Documentation

### Primary Flow: Chat Interaction

```
1. User arrives at dashboard
   ↓
2. Desktop loads via GET /desktop/{id}
   ↓
3. WebSocket connects to /ws
   ↓
4. User types in Prompt Bar, presses Enter
   ↓
5. System checks for existing chat window
   ├─ Exists: Focus window + send message
   └─ Missing: Open new chat window + send message
   ↓
6. ChatView displays optimistic user message
   ↓
7. Backend ChatAgent processes message
   ↓
8. WebSocket broadcasts assistant response
   ↓
9. UI updates with confirmed messages
```

### Window Management Flow

```
Open Window:
  Double-click desktop icon → POST /desktop/{id}/windows
  
Close Window:
  Click × button → DELETE /desktop/{id}/windows/{window_id}
  
Focus Window:
  Click window chrome → POST /desktop/{id}/windows/{window_id}/focus
  
Move Window:
  Drag titlebar → PATCH /desktop/{id}/windows/{window_id}/position
  
Resize Window:
  Drag resize handle → PATCH /desktop/{id}/windows/{window_id}/size
```

### Mobile Adaptation

**Viewport Detection:**
- Desktop: `vw > 1024px` - Full floating windows
- Mobile: `vw <= 1024px` - Near-fullscreen windows with margins

**Mobile-Specific Behaviors:**
- Running app indicators hidden on small screens
- Desktop icons use smaller grid (2 columns)
- Windows are not draggable/resizable
- Prompt bar uses compact padding

---

## 4. Backend Connection Architecture

### HTTP API (api.rs)

**Base URL Resolution:**
- Localhost: `http://localhost:8080`
- Production: Same origin (empty string)

**Endpoints Used:**

| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/desktop/{id}` | Load desktop state |
| GET | `/desktop/{id}/windows` | List windows |
| POST | `/desktop/{id}/windows` | Open new window |
| DELETE | `/desktop/{id}/windows/{wid}` | Close window |
| POST | `/desktop/{id}/windows/{wid}/focus` | Focus window |
| PATCH | `/desktop/{id}/windows/{wid}/position` | Move window |
| PATCH | `/desktop/{id}/windows/{wid}/size` | Resize window |
| GET | `/chat/{actor_id}/messages` | Fetch chat history |
| POST | `/chat/send` | Send message |

### WebSocket Protocol

**Connection:** `ws://{host}/ws`

**Client → Server Messages:**
```json
{"type": "subscribe", "desktop_id": "..."}
{"type": "ping"}
```

**Server → Client Messages:**
```json
{"type": "pong"}
{"type": "desktop_state", "desktop": {...}}
{"type": "window_opened", "window": {...}}
{"type": "window_closed", "window_id": "..."}
{"type": "window_moved", "window_id": "...", "x": 0, "y": 0}
{"type": "window_resized", "window_id": "...", "width": 0, "height": 0}
{"type": "window_focused", "window_id": "..."}
{"type": "error", "message": "..."}
```

---

## 5. Connection to Actor System

### Desktop Actor (Backend)

The UI connects to backend actors via HTTP/WebSocket:

```
UI Component → HTTP API → DesktopActor (Actix)
                ↓
         WindowState (in-memory)
                ↓
         WebSocket Broadcast
                ↓
         All Connected UIs
```

**Actor Messages:**
- `OpenWindow` → Creates new WindowState
- `CloseWindow` → Removes window
- `FocusWindow` → Updates z-index, active_window
- `MoveWindow` / `ResizeWindow` → Updates geometry
- `GetDesktopState` → Returns full state snapshot

### Chat Actor Integration

```
ChatView → POST /chat/send
              ↓
       ChatActor (per actor_id)
              ↓
       EventStore (persistence)
              ↓
       ChatAgent (AI processing)
              ↓
       WebSocket update
```

**Event Types:**
- `chat.user_msg` - User message logged
- `chat.assistant_msg` - AI response logged
- Events enable replay/rehydration of chat history

---

## 6. Missing UX Documentation Areas

### Critical Gaps

1. **Theme System Documentation**
   - CSS tokens exist but no theme creation guide
   - No dark/light mode toggle implementation
   - Missing token reference documentation

2. **App Registration Flow**
   - Hardcoded apps in desktop.rs
   - No documentation for adding custom apps
   - AppDefinition schema not documented for users

3. **Error Handling UX**
   - Generic error states in UI
   - No retry mechanisms documented
   - Network failure handling not specified

4. **Accessibility (a11y)**
   - No ARIA labels
   - Keyboard navigation not documented
   - Screen reader support missing

5. **Drag & Resize Implementation**
   - `start_drag()` and `start_resize()` are TODO stubs
   - No actual implementation in interop.rs
   - Windows currently not draggable

### Documentation Needed

1. **User Guide**
   - How to use the prompt bar
   - Window management shortcuts
   - Mobile vs desktop differences

2. **Developer Guide**
   - Adding new apps to the desktop
   - Creating custom window content
   - WebSocket event handling patterns

3. **API Integration Guide**
   - Authentication flow (if any)
   - Rate limiting behavior
   - Error response handling

4. **Styling Guide**
   - CSS custom properties reference
   - Creating custom themes
   - Component override patterns

---

## 7. Code Quality Observations

### Strengths
- Clean component separation
- Type-safe shared types between frontend/backend
- Responsive design considerations
- Optimistic UI updates for chat

### Areas for Improvement
- Drag/resize interop not implemented
- Hardcoded app list
- No error boundary components
- Limited loading states
- No offline mode support

---

## 8. Summary

The ChoirOS dashboard provides a desktop-like experience in the browser with floating windows, a global prompt bar, and real-time updates via WebSocket. The architecture cleanly separates concerns with Dioxus components, a dedicated API layer, and shared types for type safety.

**Key Integration Points:**
- Desktop state managed by backend DesktopActor
- Chat messages persisted via EventStore
- Real-time sync via WebSocket broadcast
- HTTP API for state mutations

**Next Documentation Priorities:**
1. Complete drag/resize implementation
2. Theme system documentation
3. App registration guide
4. User-facing feature documentation
