# Handoff: Desktop Architecture Design Complete

**Date:** 2026-01-31  
**Status:** Design complete, ready for implementation  
**Commits:** 2 ahead of origin/main (77bfc81, f31f7c8)

---

## What Was Just Completed

### 1. ChoirOS Desktop Architecture Design
Created comprehensive design document at `docs/DESKTOP_ARCHITECTURE_DESIGN.md` covering:
- **Mobile-first responsive design** - Single window on mobile, floating windows on desktop
- **Actor-owned state** - Desktop Actor manages all window state in SQLite
- **Simplified CSS** - Inline styles, no build pipeline, minimal bureaucracy
- **Dynamic app creation** - Apps can be added at runtime without restart
- **Hot reload** - Code updates while running via WebSocket events

### 2. Current System State

**✅ Working:**
- Backend: libsql/SQLite with EventStoreActor, ChatActor
- REST API on localhost:8080 (health, chat/send, chat/messages)
- Dioxus Chat UI (compiles, sends messages to backend)
- All 11 unit tests passing
- CORS enabled for cross-origin UI access

**📋 Ready to Implement:**
- Desktop Actor (backend state management)
- Desktop UI component (window chrome, taskbar)
- Mobile-first window system
- Responsive design (phone → tablet → desktop)

---

## Key Design Decisions (From Architecture Doc)

### 1. Mobile-First Responsive Windows
```rust
// Screen < 600px: Single full-screen window (like iOS app)
// Screen > 1024px: Floating, draggable windows (like macOS)
// In-between: Tablet-optimized layout
```

### 2. State Lives in Desktop Actor (SQLite)
```rust
// Desktop Actor owns:
- Window positions, sizes, z-index
- Which apps are open
- App registry (dynamic - add at runtime)

// UI just renders projections
```

### 3. Simplified CSS (No Build Pipeline)
```rust
// Each component defines its own styles
const CHAT_STYLES: &str = r#"
.chat-container {
    display: flex;
    flex-direction: column;
    height: 100%;
}
"#;

#[component]
fn ChatApp(props: WindowProps) -> Element {
    use_styles(CHAT_STYLES);
    rsx! { div { class: "chat-container", ... } }
}
```

### 4. Dynamic App Creation
```rust
// Create app from prompt:
1. AI generates component code
2. Compile to WASM
3. DesktopActor::register_app()
4. UI immediately shows new app icon
5. User opens windows of new app
```

---

## Implementation Plan

### Phase 1: Desktop Actor + Single Window Mode (Mobile)
**Files to create:**
- `sandbox/src/actors/desktop.rs` - Desktop actor
- `sandbox/src/api/desktop.rs` - Desktop API endpoints
- `sandbox-ui/src/desktop.rs` - Desktop UI component
- `sandbox-ui/src/window.rs` - Window chrome
- `sandbox-ui/src/taskbar.rs` - Mobile app switcher

**Key features:**
- Full-screen window view (mobile)
- Taskbar with app icons
- Swipe between windows
- Wrap existing Chat UI

### Phase 2: Multi-Window + Desktop Mode
- Add drag to move
- Resize handles
- Z-index management
- Responsive switching

### Phase 3: Dynamic App Creation
- WASM compilation pipeline
- Hot reload mechanism
- AI integration (BAML prompts)

---

## Working Commands

```bash
# Start backend
cargo run -p sandbox

# Test backend
curl http://localhost:8080/health

# Test chat API
curl -X POST http://localhost:8080/chat/send \
  -H "Content-Type: application/json" \
  -d '{"actor_id":"test","user_id":"me","text":"hello"}'

# Build UI (compiles successfully)
cargo build -p sandbox-ui

# Run UI dev server (install dx first)
cargo install dioxus-cli
cd sandbox-ui && dx serve

# Run tests
cargo test -p sandbox
```

---

## Critical Context for Next Session

### What NOT to Do
- **Don't** use React/Redux patterns from original choirOS
- **Don't** create separate CSS build pipeline
- **Don't** make app creation require recompilation
- **Don't** start with desktop floating windows (start mobile-first)

### What TO Do
- **Do** put all state in Desktop Actor (SQLite)
- **Do** use inline CSS per component
- **Do** start with mobile single-window mode
- **Do** make window state survive page refresh
- **Do** enable dynamic app registration

### Key Patterns
```rust
// Pattern: Actor owns state, UI renders projection
DesktopActor {
    windows: Vec<WindowState>,  // SQLite
    apps: Vec<AppDefinition>,   // SQLite
}

// UI queries actor, never owns state
let windows = use_resource(|| async {
    fetch_windows().await  // GET /desktop/windows
});
```

### File Locations
```
sandbox/src/actors/desktop.rs      <- IMPLEMENT THIS FIRST
sandbox/src/api/desktop.rs         <- Add endpoints here  
sandbox-ui/src/desktop.rs          <- Mobile-first UI
sandbox-ui/src/window.rs           <- Window chrome
sandbox-ui/src/taskbar.rs          <- App switcher
docs/DESKTOP_ARCHITECTURE_DESIGN.md <- Full spec
```

---

## Open Questions (Answer in Next Session)

1. **WASM Compilation**: Do we compile apps to separate WASM modules, or use a simpler hot-reload approach?
   - Option A: Dynamic WASM loading (more complex, true isolation)
   - Option B: Full UI recompile with hot reload (simpler, faster to implement)

2. **CSS Framework**: Use inline styles only, or include Tailwind via CDN for rapid AI-generated styling?
   - Inline only: More control, no dependencies
   - Tailwind CDN: AI knows Tailwind classes, faster styling

3. **Window Persistence**: Save window positions across sessions, or reset on refresh?
   - Persist: More like real desktop
   - Reset: Simpler, cleaner

---

## Related Documentation

- `docs/ARCHITECTURE_SPECIFICATION.md` - Full system architecture
- `docs/DESKTOP_ARCHITECTURE_DESIGN.md` - Desktop-specific design
- `docs/archive/` - Old/outdated docs from imported/
- `progress.md` - Current status

---

## Next Steps

1. Implement DesktopActor (backend)
2. Create mobile-first Desktop UI
3. Port Chat UI into window
4. Test on phone + desktop
5. Then: dynamic app creation from prompts

---

**Ready to implement the Desktop!**

*Created after completing architecture design and reviewing choirOS patterns*