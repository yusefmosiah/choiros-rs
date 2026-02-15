# Handoff: Chat App Thread Management

## Previous Work Completed
- Fixed duplicate window creation bug (17 windows issue)
- Fixed WebSocket race conditions
- Fixed React StrictMode double-render issues
- All E2E tests passing, apps opening correctly

## Next Task: Chat Thread Management

### Current Bug
The Chat app windows replicate content instead of keeping individual threads. Opening multiple Chat windows shows the same conversation content across all windows.

### Requirements

1. **Individual Threads Per Window**
   - Each Chat window should have its own independent thread/conversation
   - Windows should not share message state
   - Each thread has unique ID and isolated message history

2. **Thread List UI**
   - Add a thread list sidebar/panel in Chat app
   - List shows all available threads for the user
   - Clicking a thread switches to that conversation
   - Visual indicator for active thread

3. **Thread Switching Logic**
   - Click thread in list → load that thread's messages
   - Thread state persisted via backend API
   - Clear visual separation between threads

4. **Already-Open Window Handling**
   - If user tries to switch to a thread already open as a window:
     - Grey out the thread in the list (disabled state)
     - Show toast notification: "Thread already open in window [X]"
     - Toast appears above/below the greyed-out thread item
     - Clicking toast or thread focuses the existing window

### Technical Notes

**Current Architecture:**
- Chat app uses `actorId` (window ID) to identify chat sessions
- WebSocket connection per Chat window
- Messages stored via backend ChatActor
- State managed in `chat.ts` store

**Files to Modify:**
- `src/components/apps/Chat/Chat.tsx` - Add thread list UI
- `src/components/apps/Chat/Chat.css` - Thread list styling
- `src/stores/chat.ts` - Thread management state
- `src/lib/api/chat.ts` - Thread API calls (may need new endpoints)
- Backend: `sandbox/src/actors/chat.rs` - Thread isolation

**Key Considerations:**
- Thread = conversation context with unique ID
- Window can host one thread at a time
- Need to track which threads are open in which windows
- Toast positioning and dismissal
- Thread persistence across reloads

### API Requirements (To Verify/Add)

Need to check if backend supports:
- `GET /api/chat/threads` - List all threads for user
- `POST /api/chat/threads` - Create new thread
- `GET /api/chat/threads/:id/messages` - Get thread messages
- `PUT /api/chat/threads/:id/switch` - Switch to thread (returns existing window ID if open)

### Open Questions

1. Should threads be per-desktop or global per-user?
2. Thread naming - auto-generated or user-defined?
3. Thread deletion/archiving?
4. Maximum threads per user?

### Acceptance Criteria

- [ ] Opening two Chat windows creates two separate threads
- [ ] Messages in one window don't appear in another
- [ ] Thread list visible in Chat app
- [ ] Can switch between threads
- [ ] Already-open threads are greyed out
- [ ] Toast notification appears for already-open threads
- [ ] Clicking toast focuses the existing window

---

**Status**: Ready to start implementation
**Priority**: High (core Chat functionality)
**Estimated Effort**: Medium (1-2 days)
