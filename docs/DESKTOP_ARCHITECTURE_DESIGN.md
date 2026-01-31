# ChoirOS Desktop Architecture Design

## Core Philosophy

**"The Automatic Computer"** - A web desktop where:
- Every app is an actor (state in SQLite, not React state)
- Apps can be created from prompts (AI generates components)
- Code updates while running (hot reload via WASM module replacement)
- Mobile-first responsive design (works on phone, tablet, desktop)
- Minimal bureaucracy to create new apps

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        BROWSER (Mobile/Desktop)                  │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ Desktop Actor (manages window state in SQLite)              │ │
│  │  - Window positions, sizes, z-index                        │ │
│  │  - Which apps are open                                     │ │
│  │  - App registry (dynamic - can add new apps at runtime)    │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ Dioxus UI (renders windows as divs with CSS transforms)    │ │
│  │                                                              │ │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐                  │ │
│  │  │ Chat Win │  │Writer Win│  │ New App  │  ...             │ │
│  │  │ (drag)   │  │ (drag)   │  │ (drag)   │                  │ │
│  │  └──────────┘  └──────────┘  └──────────┘                  │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ Taskbar (mobile: bottom sheet, desktop: bottom bar)        │ │
│  │  - App launcher icons                                      │ │
│  │  - Active app indicators                                   │ │
│  │  - "Create App" button (prompt-based app generation)       │ │
│  └────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
                              │
                    ┌─────────┴─────────┐
                    │   HTTP/WebSocket   │
                    └─────────┬─────────┘
                              │
┌─────────────────────────────────────────────────────────────────┐
│                      BACKEND (Actix Server)                      │
│                                                                  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
│  │DesktopActor  │  │ ChatActor    │  │WriterActor   │          │
│  │- Window state│  │- Chat state  │  │- File state  │          │
│  │- App registry│  │- Messages    │  │- Documents   │          │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘          │
│         │                  │                  │                 │
│         └──────────────────┼──────────────────┘                 │
│                            │                                    │
│                   ┌────────┴────────┐                          │
│                   │  EventStore     │                          │
│                   │  (SQLite/libsql)│                          │
│                   └─────────────────┘                          │
└─────────────────────────────────────────────────────────────────┘
```

## Key Design Decisions

### 1. Desktop Actor (State Management)

The Desktop Actor owns ALL window state. The UI just renders projections.

```rust
// Desktop actor state (in SQLite)
pub struct DesktopState {
    pub windows: Vec<WindowState>,
    pub active_window: Option<String>,
    pub apps: Vec<AppDefinition>, // Dynamic registry
}

pub struct WindowState {
    pub id: String,           // UUID
    pub app_id: String,       // "chat", "writer", "user-generated-123"
    pub title: String,
    pub x: i32,               // Position
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub z_index: u32,
    pub minimized: bool,
    pub maximized: bool,
    pub props: serde_json::Value, // App-specific data (file path, chat id, etc.)
}

pub struct AppDefinition {
    pub id: String,
    pub name: String,
    pub icon: String,         // SVG or emoji
    pub component_code: String, // WASM component bytes or source
    pub default_width: u32,
    pub default_height: u32,
}
```

**Why**: Window state survives page refresh. Multiple users can share desktop state (collaboration).

### 2. Responsive Window System

Windows adapt to screen size:

```rust
// Responsive sizing
fn get_window_constraints(viewport_width: u32, viewport_height: u32) -> (u32, u32) {
    if viewport_width < 600 {        // Mobile
        (viewport_width, viewport_height * 9 / 10)
    } else if viewport_width < 1024 { // Tablet
        (viewport_width * 8 / 10, viewport_height * 8 / 10)
    } else {                          // Desktop
        (800, 600) // Default, but can resize
    }
}
```

**Mobile behavior**:
- Single window visible at a time (full screen)
- Swipe between windows
- Taskbar becomes app switcher (like iOS/Android)
- Windows don't float - they stack

**Desktop behavior**:
- Floating, draggable windows
- Multiple windows visible
- Traditional window chrome (title bar, resize handles)

### 3. Simplified App Creation

**Goal**: Create a new app with minimal code.

**Current pattern** (too much bureaucracy):
1. Create new Rust file
2. Add to mod.rs
3. Add to registry
4. Recompile
5. Redeploy

**New pattern** (instant app creation):

```rust
// User prompts: "Create a calculator app"

// 1. AI generates component code
let app_code = r#"
#[component]
fn Calculator(props: WindowProps) -> Element {
    let display = use_signal(|| "0".to_string());
    
    rsx! {
        div { class: "calculator",
            input { value: "{display}", readonly: true }
            div { class: "buttons",
                button { onclick: move || display.set("1"), "1" }
                button { onclick: move || display.set("2"), "2" }
                // ... etc
            }
        }
    }
}
"#;

// 2. Compile to WASM (or interpret)
let wasm_bytes = compile_to_wasm(app_code);

// 3. Register with Desktop Actor
DesktopActor::register_app(AppDefinition {
    id: "calculator-123".to_string(),
    name: "Calculator".to_string(),
    icon: "🧮".to_string(),
    component_code: wasm_bytes,
    default_width: 300,
    default_height: 400,
});

// 4. UI immediately shows new app in taskbar
// 5. User can open windows of the new app
```

**Implementation options**:

**Option A: WASM Module Loading** (preferred)
- Compile each app to separate WASM module
- Desktop dynamically loads modules
- Uses `wasm-bindgen` + `js-sys` for interop
- Apps are truly isolated

**Option B: Hot Code Push** (simpler)
- Recompile entire UI with new app included
- Push update via WebSocket
- Browser reloads automatically
- Faster to implement, less isolation

### 4. Hot Reload Architecture

**For development**:
```bash
# Terminal 1: Watch and rebuild
cd sandbox-ui
dx serve --hot-reload

# Changes to src/ automatically reload browser
```

**For production (AI-generated updates)**:
```rust
// When AI generates new app version
#[derive(Message)]
#[rtype(result = "()")]
pub struct HotSwapApp {
    pub app_id: String,
    pub new_wasm_bytes: Vec<u8>,
}

impl Handler<HotSwapApp> for DesktopActor {
    fn handle(&mut self, msg: HotSwapApp, _ctx: &mut Context<Self>) {
        // 1. Update app registry
        if let Some(app) = self.apps.get_mut(&msg.app_id) {
            app.component_code = msg.new_wasm_bytes;
        }
        
        // 2. Notify all connected UIs
        self.broadcast_event(DesktopEvent::AppUpdated {
            app_id: msg.app_id,
        });
        
        // 3. UIs fetch new WASM and reload component
    }
}
```

### 5. CSS Strategy (Simplified)

**No CSS modules, no build steps, minimal bureaucracy**:

```rust
// Each component defines its own styles
const CHAT_STYLES: &str = r#"
.chat-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: #1a1a2e;
    color: white;
}

@media (max-width: 600px) {
    .chat-container {
        border-radius: 0;
    }
}
"#;

#[component]
fn ChatApp(props: WindowProps) -> Element {
    use_styles(CHAT_STYLES); // Injects CSS once
    
    rsx! {
        div { class: "chat-container",
            // ...
        }
    }
}
```

**Global styles** (minimal):
```rust
// Desktop provides window chrome styling only
const DESKTOP_STYLES: &str = r#"
.window-chrome {
    border: 1px solid #333;
    border-radius: 8px;
    box-shadow: 0 4px 20px rgba(0,0,0,0.5);
    background: #16213e;
}

@media (max-width: 600px) {
    .window-chrome {
        border-radius: 0;
        border: none;
    }
}
"#;
```

**Why**: 
- No CSS build pipeline
- Each app self-contained
- Easy for AI to generate
- Mobile responsive by default

## Data Flow

### Opening an App Window

```
User clicks app icon in taskbar
    ↓
Dioxus: POST /desktop/open-window {app_id}
    ↓
DesktopActor: 
  1. Create WindowState with UUID
  2. Position (cascade from existing windows)
  3. Store in SQLite via EventStore
  4. Broadcast WindowOpened event
    ↓
All connected UIs receive WebSocket event
    ↓
Dioxus: Re-render with new window
    ↓
Window appears on desktop
```

### Creating New App from Prompt

```
User: "Create a todo list app"
    ↓
ChatActor sends to LLM (BAML)
    ↓
LLM generates:
  - Component code (Rust/Dioxus)
  - App metadata (name, icon)
    ↓
Compile to WASM
    ↓
DesktopActor::register_app()
    ↓
EventStore persists app definition
    ↓
Broadcast AppRegistered event
    ↓
UI shows new icon in taskbar
    ↓
User can immediately open windows
```

## Implementation Plan

### Phase 1: Desktop Actor + Single Window Mode (Mobile-First)

1. **Create DesktopActor** (backend)
   - Window state management
   - App registry
   - CRUD operations for windows

2. **Create Desktop UI** (frontend)
   - Full-screen window view (mobile)
   - Taskbar/app switcher
   - Window content area

3. **Port existing Chat UI**
   - Wrap in window chrome
   - Test on mobile

### Phase 2: Multi-Window + Desktop Mode

1. **Add window positioning**
   - Drag to move
   - Resize handles
   - Z-index management

2. **Responsive switching**
   - Auto-detect screen size
   - Switch between mobile/desktop modes

### Phase 3: Dynamic App Creation

1. **WASM compilation pipeline**
   - Tiny compiler service
   - Or: Rust interpreter mode

2. **Hot reload mechanism**
   - WebSocket push
   - Dynamic module loading

3. **AI integration**
   - BAML prompt for app generation
   - Compile and register flow

## File Structure

```
sandbox/src/
  actors/
    mod.rs
    event_store.rs      # Existing
    chat.rs             # Existing
    desktop.rs          # NEW: Desktop actor
    writer.rs           # NEW: Writer actor (Phase 2)
  api/
    mod.rs
    chat.rs             # Existing
    desktop.rs          # NEW: Desktop API endpoints

sandbox-ui/src/
  main.rs
  desktop.rs            # NEW: Desktop component
  window.rs             # NEW: Window chrome
  taskbar.rs            # NEW: Taskbar/app switcher
  apps/
    mod.rs              # NEW: App registry
    chat.rs             # Existing, move here
    writer.rs           # NEW: Writer app
    calculator.rs       # EXAMPLE: AI-generated
```

## Simplifications from choirOS

| choirOS (React) | ChoirOS-RS (Dioxus) |
|----------------|---------------------|
| Redux/Zustand stores | Actor-owned state in SQLite |
| Separate CSS files | Inline styles per component |
| Complex build pipeline | `dx serve` or simple cargo |
| Static app registry | Dynamic runtime registration |
| File-based routing | Component-based |
| Window state in React | Window state in actor |

## Next Steps

1. ✅ Review this design
2. Implement DesktopActor (backend)
3. Create mobile-first Desktop UI
4. Port Chat UI into window
5. Test on phone + desktop
6. Then: dynamic app creation

**Ready to proceed?**