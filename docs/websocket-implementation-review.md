# WebSocket Implementation Review Report

## Overview

This report provides a thorough analysis of the WebSocket implementation across the React TypeScript frontend (`dioxus-desktop/src/lib/ws/`) compared to the Dioxus Rust backup (`dioxus-desktop-backup/src/desktop/ws.rs`) and backend WebSocket handler (`sandbox/src/api/websocket.rs`).

## Bugs Found

### 1. Critical: Missing z_index in Dioxus WindowFocused Event
**Severity:** HIGH
**File:** `dioxus-desktop-backup/src/desktop/ws.rs:26`
**Issue:** The Dioxus `WsEvent::WindowFocused` variant only takes a `String` (window_id) but the backend sends `z_index` field.
```rust
// Dioxus backup - INCORRECT
WindowFocused(String),

// Backend expects - websocket.rs:56
WindowFocused { window_id: String, z_index: u32 },

// React types - types.ts:16 - CORRECT
{ type: 'window_focused'; window_id: string; z_index: number }
```
**Impact:** Dioxus backup will fail to receive/parse window focus events correctly. Any frontend switching from React to Dioxus will break window focus functionality.

### 2. Critical: Missing AppRegistered Event in Dioxus
**Severity:** HIGH
**File:** `dioxus-desktop-backup/src/desktop/ws.rs`
**Issue:** The Dioxus `WsEvent` enum is missing `AppRegistered` variant entirely.
```rust
// React has it - types.ts:28
| { type: 'app_registered'; app: AppDefinition }

// Backend sends it - websocket.rs:80-81
#[serde(rename = "app_registered")]
AppRegistered { app: shared_types::AppDefinition },

// Dioxus backup - MISSING
```
**Impact:** Dynamic app registration won't work in Dioxus backend. New apps registered via WebSocket won't be displayed.

### 3. Medium: Race Condition in useWebSocket Hook Status State
**Severity:** MEDIUM
**File:** `dioxus-desktop/src/hooks/useWebSocket.ts:148`
**Issue:** The hook returns a derived status that can be stale. Client status is set immediately, but React state is set in effect.
```typescript
return {
  status: wsConnected ? 'connected' : status,  // status may be stale!
  sendPing: () => client.ping(),
  disconnect: () => client.disconnect(),
};
```
**Problem:**
1. Client internally transitions to 'connected' (client.ts:118)
2. Status listener calls `setStatus('connected')` (useWebSocket.ts:120)
3. Client listener is set up in effect (useWebSocket.ts:114)
4. But the effect might not have run yet when `connect()` is called in second effect (useWebSocket.ts:140)

**Impact:** UI may show incorrect connection status during initial connection. Client might be 'connected' but hook returns 'disconnected' or 'connecting' temporarily.

### 4. Medium: Multiple useWebSocket Hooks with Single Client Instance
**Severity:** MEDIUM
**File:** `dioxus-desktop/src/hooks/useWebSocket.ts:8-14`
**Issue:** The client is a singleton shared across all hook instances, but each hook manages its own subscription/unsubscription.
```typescript
let wsClientInstance: DesktopWebSocketClient | null = null;

function getWsClient(): DesktopWebSocketClient {
  if (!wsClientInstance) {
    wsClientInstance = new DesktopWebSocketClient();
  }
  return wsClientInstance;
}
```
**Problem:** If two `useWebSocket` hooks are mounted simultaneously:
- Both try to connect/disconnect the same client
- Race conditions on `desktopId` changes
- Cleanup happens in unmount order, potentially leaving client in bad state

**Impact:** Unpredictable behavior in complex UIs with multiple components using WebSocket. One component disconnecting could affect others.

### 5. Medium: Status Override Bug in useWebSocket Return Value
**Severity:** MEDIUM
**File:** `dioxus-desktop/src/hooks/useWebSocket.ts:148`
**Issue:** The returned status overrides client status with desktop store value.
```typescript
return {
  status: wsConnected ? 'connected' : status,
  // ...
};
```
**Problem:**
- If client transitions to 'reconnecting', `status` will be 'reconnecting'
- But `wsConnected` is false (store not yet updated)
- So returned status will be 'reconnecting'
- But once client connects and calls `setStatus('connected')`, `wsConnected` becomes true
- So returned status is 'connected'

Actually this might be okay, BUT:
- If client is 'connecting', status should reflect that
- If client is 'reconnecting', that's important UI feedback
- The override hides these intermediate states

**Impact:** UI shows less granular connection states ('connecting', 'reconnecting') which are important for UX.

### 6. Medium: Silent Message Send Failures
**Severity:** MEDIUM
**File:** `dioxus-desktop/src/lib/ws/client.ts:71-77`
**Issue:** `send()` method silently drops messages if socket is not open, with no feedback to caller.
```typescript
send(message: WsClientMessage): void {
  if (!this.socket || this.socket.readyState !== WebSocket.OPEN) {
    return;  // Silent failure!
  }
  this.socket.send(JSON.stringify(message));
}
```
**Impact:** Messages sent during temporary disconnections are lost with no indication to the caller. No queuing, no retry, no error thrown. Critical messages (like subscribe) are handled in `connect()`, but user messages could be lost.

### 7. Low: Missing Error Handling for Unknown Message Types
**Severity:** LOW
**File:** `sandbox/src/api/websocket.rs:154-156`
**Issue:** Backend doesn't respond to unknown/invalid client messages.
```rust
_ => {
    tracing::warn!("Unknown or invalid WebSocket message: {}", text);
    // No error response sent to client!
}
```
**Impact:** Client has no way to know it sent an invalid message. Could lead to confusion debugging.

### 8. Low: Unnecessary Initial Pong on Connection
**Severity:** LOW
**File:** `sandbox/src/api/websocket.rs:107`
**Issue:** Backend sends an unsolicited `Pong` message immediately upon connection, before client sends anything.
```rust
let _ = send_json(&tx, &WsMessage::Pong);
```
**Impact:** The `Pong` type is a response to `Ping`. This unsolicited pong might confuse clients that expect strict request-response pattern.

### 9. Low: Weak Type Validation in parseWsServerMessage
**Severity:** LOW
**File:** `dioxus-desktop/src/lib/ws/types.ts:31-47`
**Issue:** Parser only validates `type` field, not the actual message structure.
```typescript
export function parseWsServerMessage(raw: string): WsServerMessage | null {
  const parsed: unknown = JSON.parse(raw);
  const msg = parsed as { type?: unknown };
  if (typeof msg.type !== 'string') {
    return null;
  }
  return parsed as WsServerMessage;  // UNSAFE CAST!
}
```
**Problem:** This uses an unsafe type assertion. If backend sends:
```json
{ "type": "window_focused", "window_id": "123" }  // Missing z_index!
```
This will pass validation but TypeScript won't catch missing `z_index` at runtime. Accessing `message.z_index` will be `undefined`, not a number.

**Impact:** Runtime type errors, silent failures, hard-to-debug issues when backend contracts change.

### 10. Info: WebSocket URL Construction Inconsistency
**Severity:** INFO
**Files:** `dioxus-desktop/src/lib/ws/client.ts:193-209` vs `sandbox/src/api/websocket.rs`
**Issue:** Client expects URL without `/ws` suffix, appends it internally.
```typescript
// client.ts:201
return httpToWsUrl(apiUrl) + '/ws';
```
Backend uses base URL from config and appends `/ws`:
```rust
// websocket.rs:180-182 (in connect_websocket in Dioxus, but backend handler is at /ws)
// The handler is registered at /ws route
```
This is actually correct behavior, but worth documenting to ensure config consistency.

### 11. Info: No Connection Timeout
**Severity:** INFO
**File:** `dioxus-desktop/src/lib/ws/client.ts`
**Issue:** No timeout for WebSocket connection. If `CONNECTING` state persists indefinitely, no fallback occurs.
```typescript
this.socket = new WebSocket(this.wsUrl);
// No timeout here
this.socket.onopen = () => { /* ... */ };
```
**Impact:** If network is slow or unresponsive, UI shows 'connecting' forever. No user feedback or automatic fallback.

### 12. Info: No Keep-Alive/Ping Interval
**Severity:** INFO
**Files:** `dioxus-desktop/src/lib/ws/client.ts`, `dioxus-desktop/src/hooks/useWebSocket.ts`
**Issue:** There's a `ping()` method but no automatic keep-alive mechanism.
```typescript
// client.ts:79-81
ping(): void {
  this.send({ type: 'ping' });
}

// useWebSocket.ts:149-151
sendPing: () => {
  client.ping();
},
```
**Impact:** Connections might be dropped by proxies/firewalls due to inactivity. Dioxus backup also lacks keep-alive. No automatic reconnection due to idle timeout.

## Refactoring Opportunities

### 1. Use Shared Types from shared-types
**File:** `dioxus-desktop/src/lib/ws/types.ts`
**Current:** Defines `WsServerMessage` separately from backend `WsMessage`
**Recommendation:** Use TypeScript types generated by `ts_rs` from `shared_types::WsMessage`

```rust
// shared-types/src/lib.rs:251-277
pub enum WsMsg {
  Subscribe { actor_id: ActorId },
  Send { actor_id: ActorId, payload: serde_json::Value },
  Event { actor_id: ActorId, event: Event },
  State { actor_id: ActorId, state: serde_json::Value },
  Error { message: String },
}
```

Wait - these are different protocols! Backend has TWO message protocols:
1. Desktop WebSocket protocol (WsMessage in websocket.rs)
2. Generic Actor Event protocol (WsMsg in shared-types)

The React client uses protocol 1, which is correct. However, there's no TypeScript type for `WsMessage` from shared-types.

**Recommendation:**
- Add `WsMessage` to shared-types with `ts_rs` export
- Generate TypeScript types to avoid duplication
- Ensure single source of truth

### 2. Improve Message Validation
**File:** `dioxus-desktop/src/lib/ws/types.ts:31-47`
**Current:** Minimal validation
**Recommendation:** Use Zod or io-ts for runtime type validation
```typescript
import { z } from 'zod';

const WsServerMessageSchema = z.discriminatedUnion('type', [
  z.object({ type: z.literal('pong') }),
  z.object({ type: z.literal('desktop_state'), desktop: DesktopStateSchema }),
  z.object({ type: z.literal('window_focused'), window_id: z.string(), z_index: z.number() }),
  // ... all other variants
]);

export function parseWsServerMessage(raw: string): WsServerMessage | null {
  return WsServerMessageSchema.safeParse(JSON.parse(raw)).success
    ? WsServerMessageSchema.parse(JSON.parse(raw))
    : null;
}
```

### 3. Add Message Queue with Retry
**File:** `dioxus-desktop/src/lib/ws/client.ts`
**Current:** Messages dropped when not connected
**Recommendation:** Queue messages when disconnected, send on reconnect with TTL
```typescript
class DesktopWebSocketClient {
  private messageQueue: Array<{ message: WsClientMessage; timestamp: number }> = [];
  private readonly queueTtlMs = 30000; // 30 seconds

  send(message: WsClientMessage): void {
    if (this.socket?.readyState === WebSocket.OPEN) {
      this.socket.send(JSON.stringify(message));
    } else {
      this.messageQueue.push({ message, timestamp: Date.now() });
    }
  }

  private flushMessageQueue(): void {
    const now = Date.now();
    this.messageQueue = this.messageQueue.filter(({ message, timestamp }) => {
      if (now - timestamp > this.queueTtlMs) {
        return false; // Expired
      }
      this.send(message); // Try to send
      return true; // Keep in queue if still not sent
    });
  }

  // Call flushMessageQueue() in onopen handler
}
```

### 4. Fix Client State Return Value
**File:** `dioxus-desktop/src/hooks/useWebSocket.ts:148`
**Current:** Overrides client status
**Recommendation:** Return client status directly, add separate `isConnected` prop
```typescript
return {
  status,  // Direct from client, includes 'connecting', 'reconnecting'
  isConnected: wsConnected,  // From store, boolean
  sendPing: () => client.ping(),
  disconnect: () => client.disconnect(),
};
```

### 5. Extract WebSocket Client Management
**File:** `dioxus-desktop/src/hooks/useWebSocket.ts`
**Current:** Singleton client in hook file
**Recommendation:** Move to separate module with proper lifecycle
```typescript
// src/lib/ws/clientManager.ts
export class WebSocketClientManager {
  private static instance: WebSocketClientManager;
  private clients: Map<string, DesktopWebSocketClient> = new Map();

  static getInstance(): WebSocketClientManager {
    if (!this.instance) {
      this.instance = new WebSocketClientManager();
    }
    return this.instance;
  }

  getOrCreateClient(desktopId: string): DesktopWebSocketClient {
    let client = this.clients.get(desktopId);
    if (!client) {
      client = new DesktopWebSocketClient();
      this.clients.set(desktopId, client);
    }
    return client;
  }

  disconnectAll(): void {
    this.clients.forEach(client => client.disconnect());
    this.clients.clear();
  }
}
```

### 6. Add Connection Timeout
**File:** `dioxus-desktop/src/lib/ws/client.ts`
**Recommendation:**
```typescript
export class DesktopWebSocketClient {
  private connectTimer: ReturnType<typeof setTimeout> | null = null;
  private readonly connectionTimeoutMs = 10000; // 10 seconds

  private openSocket(status: WsConnectionStatus): void {
    this.setStatus(status);

    this.connectTimer = setTimeout(() => {
      if (this.socket?.readyState === WebSocket.CONNECTING) {
        this.socket.close();
        this.scheduleReconnect();
      }
    }, this.connectionTimeoutMs);

    try {
      this.socket = new WebSocket(this.wsUrl);
    } catch {
      this.scheduleReconnect();
      return;
    }

    // Clear timer on open
    this.socket.onopen = () => {
      this.clearConnectTimer();
      // ... rest of open handler
    };
  }

  private clearConnectTimer(): void {
    if (this.connectTimer) {
      clearTimeout(this.connectTimer);
      this.connectTimer = null;
    }
  }
}
```

### 7. Add Keep-Alive Mechanism
**File:** `dioxus-desktop/src/hooks/useWebSocket.ts`
**Recommendation:**
```typescript
export function useWebSocket(desktopId: string | null): UseWebSocketResult {
  // ... existing code

  useEffect(() => {
    if (!desktopId) return;

    // Start keep-alive
    const interval = setInterval(() => {
      if (client.getStatus() === 'connected') {
        client.ping();
      }
    }, 30000); // Every 30 seconds

    return () => {
      clearInterval(interval);
    };
  }, [desktopId, client]);

  // ... rest of hook
}
```

### 8. Better Error Handling in Backend
**File:** `sandbox/src/api/websocket.rs:154-156`
**Recommendation:**
```rust
_ => {
    tracing::warn!("Unknown or invalid WebSocket message: {}", text);
    let _ = send_json(
        &tx,
        &WsMessage::Error {
            message: format!("Unknown message type: {}", text),
        },
    );
}
```

### 9. Remove Unsolicited Pong
**File:** `sandbox/src/api/websocket.rs:107`
**Recommendation:** Remove line 107, or add a different greeting message type.

## Missing Features from Dioxus

### 1. Reconnection with Exponential Backoff
**Dioxus backup:** None - simple one-shot connection
**React client:** Full implementation (client.ts:159-176)

### 2. Connection State Management
**Dioxus backup:** Only `Connected`, `Disconnected`, `Error` events
**React client:** `'disconnected' | 'connecting' | 'connected' | 'reconnecting'`

### 3. Intentional Disconnect Handling
**Dioxus backup:** No distinction between intentional and unexpected disconnect
**React client:** `intentionalClose` flag prevents reconnection (client.ts:21, 59-68)

### 4. Listener Management
**Dioxus backup:** Single callback passed to `connect_websocket`
**React client:** Multiple listeners with unsubscribe (client.ts:83-104)

### 5. DesktopId Management
**Dioxus backup:** Desktop ID passed once at connection
**React client:** Can change desktop ID mid-connection (client.ts:42-56)

### 6. Error Events to UI
**Dioxus backup:** Logs error, doesn't emit event (ws.rs:241-245)
**React client:** Emits error events to listeners (client.ts:141-145)

## Test Coverage Gaps

### 1. Missing: Connection Failure Tests
**File:** `dioxus-desktop/src/lib/ws/client.test.ts`
**Issue:** Mock WebSocket always succeeds. No tests for:
- Connection timeout
- Immediate connection failure
- Network unreachable

**Recommendation:**
```typescript
it('handles connection timeout', async () => {
  class FailingMockWebSocket extends MockWebSocket {
    constructor(url: string) {
      super(url);
      // Never call onopen
    }
  }
  // Mock to use FailingMockWebSocket
  // Test that client schedules reconnect after timeout
});
```

### 2. Missing: Message Validation Tests
**File:** `dioxus-desktop/src/lib/ws/types.ts`
**Issue:** No tests for `parseWsServerMessage` edge cases:
- Valid JSON with missing required fields
- Wrong field types
- Unknown message types

**Recommendation:**
```typescript
describe('parseWsServerMessage', () => {
  it('rejects messages with missing required fields', () => {
    const result = parseWsServerMessage('{"type":"window_focused","window_id":"123"}');
    // Missing z_index - should be null or handled
  });

  it('rejects messages with wrong field types', () => {
    const result = parseWsServerMessage('{"type":"window_moved","window_id":"123","x":"not_a_number","y":0}');
    expect(result).toBeNull();
  });
});
```

### 3. Missing: Subscription Error Tests
**File:** `sandbox/src/api/websocket.rs`
**Issue:** No tests for invalid desktop IDs or subscription failures.

**Recommendation:** Add integration tests in `sandbox/tests/` that:
- Attempt to subscribe to non-existent desktop
- Test error message is sent back
- Verify client handles error messages

### 4. Missing: Multiple Clients Tests
**File:** `dioxus-desktop/src/lib/ws/client.test.ts`
**Issue:** No tests for:
- Multiple `useWebSocket` hooks with same desktop
- Different desktop IDs
- Race conditions on simultaneous connect/disconnect

### 5. Missing: Reconnection Edge Cases
**File:** `dioxus-desktop/src/lib/ws/client.test.ts`
**Current:** Tests exist but limited
**Missing:**
- Reconnect during already reconnecting
- Rapid connect/disconnect/connect cycles
- Max backoff saturation behavior
- DesktopId change during reconnection

### 6. Missing: Integration Tests
**Issue:** No E2E tests with actual backend

**Recommendation:** Add in `sandbox/tests/`:
```rust
#[tokio::test]
async fn test_websocket_subscription_flow() {
    // Start server
    // Connect WebSocket
    // Subscribe to desktop
    // Verify desktop_state received
    // Send ping, verify pong
    // Trigger window event, verify received
}
```

### 7. Missing: Message Queue Tests
**File:** `dioxus-desktop/src/lib/ws/client.ts`
**Issue:** No tests for proposed message queue feature (if implemented).

## Integration Issues

### 1. Protocol Mismatch: Desktop vs Generic Actor
**Files:** `shared-types/src/lib.rs`, `sandbox/src/api/websocket.rs`
**Issue:** There are TWO WebSocket protocols in the codebase:
1. Desktop-specific protocol (WsMessage in sandbox/src/api/websocket.rs)
2. Generic actor protocol (WsMsg in shared-types/src/lib.rs)

**Impact:** Confusion about which protocol to use. The desktop WebSocket endpoint uses protocol 1, but shared-types exports protocol 2. React client uses protocol 1.

**Recommendation:** Clearly document which protocol is used where. Consider consolidating or clearly separating.

### 2. Type Duplication
**Files:** `dioxus-desktop/src/lib/ws/types.ts`, `shared-types/src/lib.rs`
**Issue:** `WsServerMessage` in frontend duplicates fields from backend `WsMessage`. If backend changes, frontend must be manually updated.

**Impact:** Maintenance burden. Type drift possible.

**Recommendation:** Use `ts_rs` to generate frontend types from Rust:
```rust
// shared-types/src/lib.rs - Add this export
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
#[ts(export, export_to = "../../dioxus-desktop/src/types/generated.ts")]
pub enum DesktopWsMessage {
    // ... all WsMessage variants from websocket.rs
}
```

Then in frontend:
```typescript
import type { DesktopWsMessage } from '@/types/generated';
export type WsServerMessage = DesktopWsMessage;
```

### 3. z_index Type Inconsistency
**Files:** `dioxus-desktop/src/lib/ws/types.ts`, `shared-types/src/lib.rs`
**Issue:** Frontend uses `number`, backend uses `u32`. This is fine at runtime (both are numbers) but worth noting.

**Recommendation:** Document this in type comments.

### 4. Window State Schema
**Files:** `shared-types/src/lib.rs:149-165`
**Issue:** `WindowState` uses `i32` for coordinates and dimensions. Frontend expects these to be numbers (compatible).

**Potential Issue:** What happens if backend sends negative values? Frontend might not validate.

### 5. Error Message Handling
**Files:** `dioxus-desktop/src/lib/ws/types.ts:29`, `sandbox/src/api/websocket.rs:83-84`
**Issue:** Both have `{ type: 'error', message: string }`, but error handling is inconsistent.
- Frontend: Logs to store (useWebSocket.ts:124-125)
- Backend: Sends but doesn't guarantee error state persists

**Recommendation:** Ensure error messages are displayed to users appropriately.

### 6. Desktop State Sync
**Files:** `dioxus-desktop/src/hooks/useWebSocket.ts:25-28`, `sandbox/src/api/websocket.rs:136-150`
**Issue:** When subscribing, backend sends full desktop state. Frontend applies it to both desktop and windows stores.

**Potential Issue:** If desktop state is large, this could be slow. No incremental sync or diffing.

**Recommendation:** Consider sending deltas for state changes, not full state on every event. But full state on initial subscribe is good.

### 7. App Registration Flow
**Files:** `dioxus-desktop/src/hooks/useWebSocket.ts:80-82`, `shared-types/src/lib.rs:167-177`
**Issue:** `AppDefinition` includes `component_code` (source code or WASM path). This is potentially large.

**Recommendation:** Ensure app registration doesn't send huge payloads over WebSocket. Consider sending just metadata, with code loaded separately.

### 8. Browser Compatibility
**File:** `dioxus-desktop/src/lib/ws/client.ts`
**Issue:** Uses modern WebSocket API. No polyfills or fallbacks.

**Recommendation:** Document browser requirements. WebSocket is well-supported (IE 10+), so this is likely fine.

### 9. Production Deployment
**Issue:** WebSocket URL resolution uses `window.location` (client.ts:204-206). This works in browser, but what about:
- Production behind reverse proxy?
- Different API domain than UI domain?

**Recommendation:** Ensure `VITE_WS_URL` or `VITE_API_URL` are properly configured in production.

### 10. Server-Side Rendering
**File:** `dioxus-desktop/src/lib/ws/client.ts`
**Issue:** Client checks `typeof window !== 'undefined'` (client.ts:204). But what if this code runs in SSR context?

**Current Behavior:** Falls back to `ws://localhost:8080/ws` (line 209).

**Recommendation:** Ensure hooks don't try to connect on server. `useWebSocket` hook should check for browser context.

## Summary

### Critical Issues (Fix Immediately)
1. Missing `z_index` in Dioxus `WindowFocused` event
2. Missing `AppRegistered` event in Dioxus
3. Type safety issues in message parsing

### High Priority
1. Race condition in useWebSocket status state
2. Multiple hooks with singleton client
3. Silent message send failures

### Medium Priority
1. Message queue with retry
2. Connection timeout
3. Keep-alive mechanism
4. Better error handling

### Low Priority
1. Remove unsolicited pong
2. Integration test coverage
3. Type consolidation with shared-types

### Future Improvements
1. Protocol consolidation
2. Incremental state sync
3. Performance optimization for large payloads

---

**Report generated:** 2025-02-06
**Files analyzed:** 7 files across 3 projects
**Total issues found:** 12 bugs, 9 refactoring opportunities, 6 missing features, 7 test gaps, 10 integration issues
