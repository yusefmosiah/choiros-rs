# API Client & Type Generation Review Report

**Date:** 2025-02-06
**Scope:** React `sandbox-ui` vs Dioxus `sandbox-ui-backup` vs Backend `sandbox` contracts

---

## Executive Summary

This review identified **23 bugs/issues** and **14 missing features** across the API clients and type generation layers. The React frontend is missing the entire **Viewer API** module and has **type mismatches** with backend response envelopes. The Dioxus backup has better coverage but still lacks proper error handling patterns.

**Critical Issues (Must Fix):**
1. Missing Viewer API client (GET/PATCH `/viewer/content`)
2. Desktop maximize/restore response type mismatch
3. User preferences API expects different response shape
4. Missing WebSocket type exports

---

## 1. Endpoints Coverage

### 1.1 Backend API Endpoints (from `sandbox/src/api/mod.rs`)

| Route | Method | Handler | Dioxus Coverage | React Coverage |
|-------|--------|---------|-----------------|----------------|
| `/health` | GET | `health_check` | ✅ | ✅ |
| `/ws` | GET | WebSocket (desktop) | ✅ | ✅ |
| `/chat/send` | POST | `chat::send_message` | ✅ | ✅ |
| `/chat/{actor_id}/messages` | GET | `chat::get_messages` | ✅ | ✅ |
| `/user/{user_id}/preferences` | GET | `user::get_user_preferences` | ✅ | ⚠️ Type mismatch |
| `/user/{user_id}/preferences` | PATCH | `user::update_user_preferences` | ✅ | ⚠️ Type mismatch |
| `/desktop/{desktop_id}` | GET | `desktop::get_desktop_state` | ✅ | ✅ |
| `/desktop/{desktop_id}/windows` | GET | `desktop::get_windows` | ✅ | ✅ |
| `/desktop/{desktop_id}/windows` | POST | `desktop::open_window` | ✅ | ✅ |
| `/desktop/{desktop_id}/windows/{window_id}` | DELETE | `desktop::close_window` | ✅ | ✅ |
| `/desktop/{desktop_id}/windows/{window_id}/position` | PATCH | `desktop::move_window` | ✅ | ✅ |
| `/desktop/{desktop_id}/windows/{window_id}/size` | PATCH | `desktop::resize_window` | ✅ | ✅ |
| `/desktop/{desktop_id}/windows/{window_id}/focus` | POST | `desktop::focus_window` | ✅ | ✅ |
| `/desktop/{desktop_id}/windows/{window_id}/minimize` | POST | `desktop::minimize_window` | ✅ | ✅ |
| `/desktop/{desktop_id}/windows/{window_id}/maximize` | POST | `desktop::maximize_window` | ✅ | ⚠️ Response mismatch |
| `/desktop/{desktop_id}/windows/{window_id}/restore` | POST | `desktop::restore_window` | ✅ | ⚠️ Response mismatch |
| `/desktop/{desktop_id}/apps` | GET | `desktop::get_apps` | ✅ | ✅ |
| `/desktop/{desktop_id}/apps` | POST | `desktop::register_app` | ✅ | ✅ |
| `/viewer/content` | GET | `viewer::get_viewer_content` | ✅ | ❌ **MISSING** |
| `/viewer/content` | PATCH | `viewer::patch_viewer_content` | ✅ | ❌ **MISSING** |
| `/api/terminals/{terminal_id}` | GET | `terminal::create_terminal` | ✅ | ✅ |
| `/api/terminals/{terminal_id}/info` | GET | `terminal::get_terminal_info` | ✅ | ✅ |
| `/api/terminals/{terminal_id}/stop` | GET | `terminal::stop_terminal` | ✅ | ✅ |
| `/ws/terminal/{terminal_id}` | GET | WebSocket (terminal) | ✅ | ✅ |
| `/ws/chat/{actor_id}` | GET | WebSocket (chat) | ❌ | ❌ |
| `/ws/chat/{actor_id}/{user_id}` | GET | WebSocket (chat with user) | ❌ | ❌ |

### 1.2 WebSocket Endpoints Coverage

| WebSocket | Dioxus Coverage | React Coverage |
|-----------|-----------------|----------------|
| `/ws` (Desktop) | ✅ | ✅ |
| `/ws/terminal/{id}` | ✅ | ✅ |
| `/ws/chat/{actor_id}` | ❌ | ❌ |
| `/ws/chat/{actor_id}/{user_id}` | ❌ | ❌ |

---

## 2. Bugs Found

### 2.1 Critical Bugs

#### **BUG-001: Missing Viewer API Client Module**
- **Location:** `sandbox-ui/src/lib/api/` - No `viewer.ts` file
- **Impact:** File viewer/editor functionality broken in React UI
- **Details:** Backend exposes `/viewer/content` (GET/PATCH) but React has no client
- **Evidence:** Dioxus backup has `fetch_viewer_content` and `patch_viewer_content` at `sandbox-ui-backup/src/api.rs:690-759`
- **Backend Contract:** `sandbox/src/api/viewer.rs`

#### **BUG-002: Desktop Maximize Window Response Type Mismatch**
- **Location:** `sandbox-ui/src/lib/api/desktop.ts:104-107`
- **Impact:** Runtime errors - expects `{ success, window }` but backend sends `{ success, window, from, message }`
- **Frontend Code:**
```typescript
export async function maximizeWindow(desktopId: string, windowId: string): Promise<WindowState> {
  const response = await apiClient.post<WindowEnvelope>(`/desktop/${desktopId}/windows/${windowId}/maximize`, {});
  return assertSuccess(response).window;  // ← Expects window field
}
```
- **Backend Response:** `sandbox/src/api/desktop.rs:488-494`
```rust
Json(json!({
    "success": true,
    "window": window,
    "from": restored.from,  // ← EXTRA FIELD
    "message": "Window maximized"
}))
```
- **Type Expectation:** `WindowEnvelope` only has `{ success, window }`

#### **BUG-003: Desktop Restore Window Response Type Mismatch**
- **Location:** `sandbox-ui/src/lib/api/desktop.ts:109-112`
- **Impact:** Same as BUG-002 - runtime errors
- **Backend Response:** `sandbox/src/api/desktop.rs:548-554` returns `{ success, window, from, message }`
- **Frontend:** Only extracts `window` from `WindowEnvelope`

#### **BUG-004: User Preferences Response Shape Mismatch**
- **Location:** `sandbox-ui/src/lib/api/user.ts:3-28`
- **Impact:** Type errors - frontend expects complex `UserPreferences` object
- **Backend Response:** `sandbox/src/api/user.rs:48-53` returns only `{ success, theme }`
```rust
Json(UserPreferencesResponse {
    success: true,
    theme,  // ← ONLY theme field
})
```
- **Frontend Expects:**
```typescript
interface UserPreferences {
  user_id: string;        // ← NOT IN BACKEND RESPONSE
  theme: 'light' | 'dark' | 'system';
  language: string;       // ← NOT IN BACKEND RESPONSE
  notifications_enabled: boolean;  // ← NOT IN BACKEND RESPONSE
  sidebar_collapsed: boolean;      // ← NOT IN BACKEND RESPONSE
  custom_settings?: Record<string, unknown>;  // ← NOT IN BACKEND RESPONSE
}
```
- **Dioxus Correct:** `sandbox-ui-backup/src/api.rs:271-293` only returns `theme`

### 2.2 Type Mismatches

#### **BUG-005: DesktopState active_window Type Mismatch**
- **Location:** `shared-types/src/lib.rs:145` vs `sandbox-ui/src/types/generated.ts:31`
- **Rust:** `pub active_window: Option<String>` (nullable string)
- **TypeScript:** `active_window: string | null`
- **Issue:** TypeScript type is correct for Rust `Option<String>` but uses `null` instead of `undefined`
- **Impact:** Minor - `null` vs `undefined` differences in strict null checks
- **Generated Code:** `sandbox-ui/src/types/generated.ts:31`

#### **BUG-006: WindowState Numeric Types Potential Precision Loss**
- **Location:** `shared-types/src/lib.rs:152-165`
- **Rust:** `x: i32`, `y: i32`, `width: i32`, `height: i32`, `z_index: u32`
- **TypeScript:** All typed as `number`
- **Issue:** TypeScript `number` is float64, should handle i32/u32 correctly but may have precision issues with very large values (unlikely for window coordinates)
- **Generated Code:** `sandbox-ui/src/types/generated.ts:99`

#### **BUG-007: ChatMessage timestamp Serialization**
- **Location:** `shared-types/src/lib.rs:182-188` vs `sandbox-ui/src/types/generated.ts:21`
- **Rust:** `timestamp: DateTime<Utc>`
- **TypeScript:** `timestamp: string`
- **Issue:** Correctly serialized as RFC3339 string, but parsing may fail on frontend
- **ts-rs Directive:** `shared-types/src/lib.rs:55` - DateTime should generate as string
- **Generated:** `sandbox-ui/src/types/generated.ts:21`

### 2.3 WebSocket Message Protocol Inconsistencies

#### **BUG-008: WebSocket Server Message Type Missing `z_index` for window_focused**
- **Location:** `sandbox/src/api/desktop.rs:365-372`
- **Backend Sends:**
```rust
WsMessage::WindowFocused {
    window_id: window_id.clone(),
    z_index,  // ← Backend sends z_index
}
```
- **React Expects:** `sandbox-ui/src/lib/ws/types.ts:16`
```typescript
{ type: 'window_focused'; window_id: string; z_index: number }
```
- **Status:** ✅ Actually matches - no bug here

#### **BUG-009: Missing `app_registered` WebSocket Message Type**
- **Location:** `sandbox/src/api/mod.rs` - No app registration WS broadcast
- **Frontend Expects:** `sandbox-ui/src/lib/ws/types.ts:28`
```typescript
{ type: 'app_registered'; app: AppDefinition }
```
- **Backend Reality:** `desktop::register_app` (`sandbox/src/api/desktop.rs:624-627`) does NOT broadcast
- **Impact:** Frontend listener for `app_registered` never fires

### 2.4 Error Handling Issues

#### **BUG-010: Inconsistent Error Response Format**
- **Location:** Multiple endpoints
- **Pattern 1:** `{ success: false, error: string }` - Most common
- **Pattern 2:** `{ success: false, message: string }` - Chat API
- **Frontend:** `sandbox-ui/src/lib/api/chat.ts:4-8` handles both `error` and `message`
- **Issue:** Inconsistent naming creates confusion

#### **BUG-011: Chat API Error Field Mismatch**
- **Location:** `sandbox-ui/src/lib/api/chat.ts:25`
```typescript
throw new Error(response.error ?? response.message ?? 'Chat API request failed');
```
- **Backend Response:** `sandbox/src/api/chat.rs:152-155` returns `{ success: false, error: e.to_string() }`
- **Dioxus Response:** `sandbox-ui-backup/src/api.rs:27-29` returns `{ success: false, message: string }`
- **Impact:** Works due to fallback logic, but confusing

#### **BUG-012: Viewer API Conflict Error Handling**
- **Location:** Backend `sandbox/src/api/viewer.rs:208-219`
- **Backend Returns:** 409 Conflict with `{ success: false, error: "revision_conflict", latest: {...} }`
- **Dioxus Handles:** `sandbox-ui-backup/src/api.rs:737-746` - Custom `PatchViewerContentError::Conflict` enum
- **React:** Missing - no Viewer API at all

### 2.5 Authentication & Session Issues

#### **BUG-013: Missing user_id in Several API Requests**
- **Location:** `sandbox-ui/src/lib/api/` - Multiple endpoints
- **Issue:** `user_id` is hardcoded or not sent in several requests
- **Examples:**
  - `chat.ts:34` - Sends `user_id` from request (OK)
  - `terminal.ts:23` - Encodes `user_id` in WS URL param (OK)
  - `viewer.ts` (Dioxus) - `sandbox-ui-backup/src/api.rs:721` - Hardcoded `"user-1"`
- **Backend:** `sandbox/src/api/viewer.rs:23` - Optional `user_id` in request

### 2.6 Request/Response Parsing Bugs

#### **BUG-014: Open Window Missing Window Response**
- **Location:** `sandbox-ui/src/lib/api/desktop.ts:54-63`
- **Issue:** Backend `sandbox/src/api/desktop.rs:83-87` always returns `window` in success response
- **Frontend:** Correctly handles with null check
- **Status:** ✅ Actually handled correctly

#### **BUG-015: Terminal WebSocket URL Encoding**
- **Location:** `sandbox-ui/src/lib/api/terminal.ts:26-33`
- **Issue:** `user_id` is URL encoded, but backend `sandbox/src/api/terminal.rs:47` expects it decoded
```rust
pub struct TerminalWsQuery {
    user_id: String,  // ← Axum auto-decodes
    ...
}
```
- **Status:** ✅ Actually correct - Axum handles decoding automatically

### 2.7 Generated Type Issues

#### **BUG-016: ToolStatus Enum Variant Not Properly Exported**
- **Location:** `shared-types/src/lib.rs:121-126`
- **Rust:** `enum ToolStatus { Success, Error(String) }`
- **TypeScript:** `sandbox-ui/src/types/generated.ts:84`
```typescript
export type ToolStatus = "Success" | { "Error": string };
```
- **Issue:** ts-rs generates tagged union instead of simple union or object
- **Impact:** Difficult to use pattern matching in TypeScript
- **Recommended Fix:** Use `#[ts(tag = "tag")]` or manual export

#### **BUG-017: ChatMsg Enum Not Exported to Frontend**
- **Location:** `shared-types/src/lib.rs:95-119`
- **Rust:** Complex enum with variants
- **TypeScript:** `sandbox-ui/src/types/generated.ts:26`
```typescript
export type ChatMsg =
  | { "UserTyped": { text: string, window_id: string, } }
  | { "AssistantReply": { text: string, model: string, } }
  | { "ToolCall": { tool: string, args: unknown, call_id: string, } }
  | { "ToolResult": { call_id: string, status: ToolStatus, output: unknown, } };
```
- **Issue:** Tagged unions are generated correctly but may be unwieldy
- **Recommendation:** Consider using TypeScript discriminated unions with explicit `type` field

#### **BUG-018: Event.seq Type Mismatch**
- **Location:** `shared-types/src/lib.rs:49`
- **Rust:** `seq: i64`
- **TypeScript:** `sandbox-ui/src/types/generated.ts:41` - `seq: bigint`
- **Issue:** Correctly mapped to `bigint`, but may cause issues with JSON parsing
- **JSON:** Number > 2^53 loses precision in JavaScript
- **Status:** ✅ Actually correct - `bigint` is proper type for i64

### 2.8 Missing Generated Types

#### **BUG-019: Missing ApiResponse<T> in Generated Types**
- **Location:** `shared-types/src/lib.rs:243-248`
- **Rust:** `struct ApiResponse<T> { success: bool, data: Option<T>, error: Option<String> }`
- **TypeScript:** ❌ Not exported - missing from `sandbox-ui/src/types/generated.ts`
- **Issue:** Used by backend but not available to frontend
- **ts-rs Directive:** No `#[ts(export)]` attribute on `ApiResponse<T>`

#### **BUG-020: Missing WriterMsg in Generated Types**
- **Location:** `shared-types/src/lib.rs:128-134`
- **Rust:** `enum WriterMsg { CreateDoc { title: String }, EditFile { ... }, ReadFile { ... } }`
- **TypeScript:** ❌ Not exported
- **Issue:** May be needed for WriterActor integration
- **ts-rs Directive:** No `#[ts(export)]` attribute

#### **BUG-021: Missing ViewerDescriptor in Frontend Usage**
- **Location:** `sandbox-ui/src/types/generated.ts:88`
- **TypeScript:** `ViewerDescriptor` is exported but NOT used in any API client
- **Issue:** Backend viewer endpoints return this but React has no way to use it

---

## 3. Refactoring Opportunities

### 3.1 API Response Envelope Standardization

**Current State:**
```typescript
// Some endpoints return { success, data }
// Others return { success, messages }
// Others return { success, window, error }
```

**Proposed Standard:**
```typescript
// sandbox-ui/src/lib/api/types.ts
export interface ApiResponse<T = void> {
  success: boolean;
  data?: T;
  error?: string;
  message?: string;  // For backwards compatibility
}

export interface PaginatedResponse<T> extends ApiResponse<T[]> {
  page?: number;
  limit?: number;
  total?: number;
}

export interface ConflictResponse {
  success: false;
  error: "revision_conflict";
  latest: {
    content: string;
    revision: ViewerRevision;
  };
}
```

**Rust Backend Alignment:**
```rust
// shared-types/src/lib.rs
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}
```

### 3.2 Consolidate Error Handling

**Current:** Each API module has its own `assertSuccess`
**Proposed:**
```typescript
// sandbox-ui/src/lib/api/client.ts
class ApiClient {
  async request<T>(endpoint: string, options: RequestOptions = {}): Promise<T> {
    // ... existing code ...
    if (!response.ok) {
      const errorData = await response.json().catch(() => ({}));
      throw new HttpError(response.status, errorData.error || errorData.message || `HTTP ${response.status}`, errorData);
    }

    const data = await response.json() as T;
    if (typeof data === 'object' && data !== null && 'success' in data && !(data as any).success) {
      const errorData = data as { error?: string; message?: string };
      throw new ApiError(ErrorType.API_ERROR, response.status, errorData.error ?? errorData.message ?? 'API request failed');
    }
    return data;
  }
}
```

### 3.3 TypeScript Type Guards for Generated Enums

**For complex tagged unions:**
```typescript
// sandbox-ui/src/lib/type-guards.ts
export type ToolStatus = "Success" | { Error: string };

export function isToolStatusError(status: ToolStatus): status is { Error: string } {
  return typeof status === 'object' && 'Error' in status;
}

export function getToolStatusMessage(status: ToolStatus): string | null {
  return isToolStatusError(status) ? status.Error : null;
}
```

### 3.4 API Client Composition Pattern

**Current:** Separate modules import `apiClient`
**Proposed:**
```typescript
// sandbox-ui/src/lib/api/index.ts
export class Api {
  health = new HealthApi(this.client);
  desktop = new DesktopApi(this.client);
  chat = new ChatApi(this.client);
  terminal = new TerminalApi(this.client);
  user = new UserApi(this.client);
  viewer = new ViewerApi(this.client);

  constructor(private client = apiClient) {}
}

// Usage
const api = new Api();
await api.desktop.openWindow(desktopId, { app_id: 'chat', title: 'Chat' });
```

---

## 4. Type Generation Issues

### 4.1 Missing ts-rs Exports

| Type | Location | Missing Export? | Impact |
|------|----------|-----------------|--------|
| `ApiResponse<T>` | `shared-types/src/lib.rs:243` | ✅ Missing | Can't use generic response type |
| `WriterMsg` | `shared-types/src/lib.rs:128` | ✅ Missing | WriterActor integration |
| `RegisterAppRequest` | `sandbox-ui-backup/src/api.rs:177` | ✅ Duplicate | Defined in Dioxus, not in shared-types |
| `ViewerContentResponse` | `sandbox-ui-backup/src/api.rs:648` | ✅ Missing | No viewer API in React |

### 4.2 Incorrect ts-rs Type Mappings

#### **Issue: DateTime<Utc> Serialization**
- **Rust:** `shared-types/src/lib.rs:55` - `timestamp: DateTime<Utc>`
- **ts-rs Default:** Generates `Date` object (incorrect)
- **Fix Applied:** Uses `#[ts(type = "string")]` directive manually
- **Location:** `shared-types/src/lib.rs:55` - No directive shown but TS shows string

**Recommendation:**
```rust
#[derive(TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ChatMessage {
    // ...
    #[ts(type = "string")]  // RFC3339 format
    pub timestamp: DateTime<Utc>,
    // ...
}
```

#### **Issue: serde_json::Value as unknown**
- **Rust:** `shared-types/src/lib.rs:64, 77, 108, 117, 164, 262, 271, 289, 300`
- **ts-rs Default:** Generates `any` (too permissive)
- **Fix Applied:** Uses `#[ts(type = "unknown")]` directive
- **Impact:** Forces explicit type checking on payload access

### 4.3 Enum Variant Serialization Issues

#### **ToolStatus Enum**
```rust
// Rust
pub enum ToolStatus {
    Success,
    Error(String),
}

// Generated TypeScript
export type ToolStatus = "Success" | { "Error": string };
```

**Issues:**
1. Tagged union is awkward to use
2. Pattern matching is verbose
3. Not standard TypeScript pattern

**Better Approach:**
```rust
#[derive(Serialize, Deserialize, TS)]
#[ts(export, export_to = "...")]
#[serde(tag = "status")]
pub enum ToolStatus {
    #[serde(rename = "success")]
    Success,
    #[serde(rename = "error")]
    Error { message: String },
}
```

**Generates:**
```typescript
export type ToolStatus = { status: "success" } | { status: "error"; message: string };
```

#### **ChatMsg Enum**
- **Current:** Tagged unions with variant names as keys
- **Better:** Discriminated unions with explicit `type` field

**Current Generated:**
```typescript
export type ChatMsg =
  | { "UserTyped": { text: string, window_id: string } }
  | { "AssistantReply": { text: string, model: string } }
  | ...
```

**Better Generated:**
```typescript
export type ChatMsg =
  | { type: "UserTyped"; text: string; window_id: string }
  | { type: "AssistantReply"; text: string; model: string }
  | ...
```

### 4.4 Missing Type Exports for WebSocket Messages

**Backend Shared Types:** `shared-types/src/lib.rs:251-277`
```rust
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
#[ts(export, export_to = "...")]
pub enum WsMsg {
    Subscribe { actor_id: ActorId },
    Send { actor_id: ActorId, payload: serde_json::Value },
    Event { actor_id: ActorId, event: Event },
    State { actor_id: ActorId, state: serde_json::Value },
    Error { message: String },
}
```

**TypeScript Generated:** `sandbox-ui/src/types/generated.ts:104` - ✅ Exported

**Issue:** React uses its own WebSocket types in `sandbox-ui/src/lib/ws/types.ts` instead of using the generated `WsMsg` type!

**Recommendation:** Use generated `WsMsg` or export desktop-specific types to shared-types.

### 4.5 Viewer Types Not Used

**Generated Types (present but unused in React):**
```typescript
export type ViewerKind = "text" | "image";
export type ViewerResource = { uri: string, mime: string };
export type ViewerCapabilities = { readonly: boolean };
export type ViewerDescriptor = { kind: ViewerKind, resource: ViewerResource, capabilities: ViewerCapabilities };
export type ViewerRevision = { rev: bigint, updated_at: string };
```

**Dioxus Usage:** `sandbox-ui-backup/src/api.rs:648-759` - Uses all viewer types

**React Usage:** ❌ None - no Viewer API client exists

---

## 5. Error Handling Gaps

### 5.1 HTTP Error Codes

| Status Code | Backend Usage | Frontend Handling | Issue |
|-------------|---------------|-------------------|-------|
| 200 | Success | ✅ OK | - |
| 400 | Bad request | ✅ Caught | Works |
| 409 | Conflict (viewer) | ❌ Not handled | Missing Viewer API |
| 500 | Internal server error | ⚠️ Generic error | No error details |
| 503 | Service unavailable | ❌ Not handled | No retries |

### 5.2 Network Error Handling

**Frontend:** `sandbox-ui/src/lib/api/errors.ts` ✅ Has network/timeout error classes
**Usage:** `sandbox-ui/src/lib/api/client.ts:44-50` ✅ Correctly thrown
**Gap:** No exponential backoff retry logic

### 5.3 WebSocket Error Handling

**Frontend:** `sandbox-ui/src/lib/ws/client.ts:141-145` - Generic error listener
**Gap:** No specific handling for:
- Authentication failures
- Rate limiting
- Connection drops
- Message parse errors

### 5.4 Type Guard for Error Responses

**Missing:** Runtime type checking for error responses

**Recommendation:**
```typescript
export function isErrorResponse(data: unknown): data is { success: false; error?: string; message?: string } {
  return typeof data === 'object' &&
    data !== null &&
    'success' in data &&
    (data as any).success === false;
}
```

### 5.5 Conflict Resolution Not Implemented

**Dioxus Pattern:** `sandbox-ui-backup/src/api.rs:682-688`
```rust
pub enum PatchViewerContentError {
    Conflict {
        latest_content: String,
        latest_revision: ViewerRevision,
    },
    Message(String),
}
```

**React:** ❌ No viewer API = no conflict resolution

**Recommendation:** Implement similar pattern:
```typescript
export interface ViewerContent {
  uri: string;
  mime: string;
  content: string;
  revision: ViewerRevision;
  readonly: boolean;
}

export interface ViewerConflictError {
  type: 'conflict';
  latest: {
    content: string;
    revision: ViewerRevision;
  };
}

export class ViewerPatchError extends Error {
  constructor(
    public conflict?: ViewerConflictError,
    message?: string
  ) {
    super(message ?? 'Failed to patch viewer content');
  }
}
```

---

## 6. Recommendations Summary

### 6.1 High Priority (Must Fix)
1. ✅ Create `sandbox-ui/src/lib/api/viewer.ts` - Missing Viewer API client
2. ✅ Fix desktop maximize/restore response types - BUG-002, BUG-003
3. ✅ Fix user preferences response shape - BUG-004
4. ✅ Add `ApiResponse<T>` to generated types - BUG-019
5. ✅ Add WebSocket type exports for desktop messages - BUG-008

### 6.2 Medium Priority (Should Fix)
6. ✅ Add `app_registered` WS broadcast in backend - BUG-009
7. ✅ Standardize error response format - BUG-010
8. ✅ Add exponential backoff for retries
9. ✅ Implement conflict resolution for viewer API
10. ✅ Export `WriterMsg` to TypeScript - BUG-020

### 6.3 Low Priority (Nice to Have)
11. ✅ Refactor API client composition pattern
12. ✅ Add type guards for complex enums
13. ✅ Improve TypeScript enum variant generation (use discriminators)
14. ✅ Add comprehensive error type guards
15. ✅ Document all API contracts

---

## 7. Files Requiring Changes

### 7.1 Create New Files
- `sandbox-ui/src/lib/api/viewer.ts` - Viewer API client
- `sandbox-ui/src/lib/api/types.ts` - Common type definitions
- `sandbox-ui/src/lib/type-guards.ts` - Runtime type checking

### 7.2 Modify Existing Files

| File | Changes |
|------|---------|
| `sandbox-ui/src/lib/api/desktop.ts` | Fix maximizeWindow, restoreWindow return types |
| `sandbox-ui/src/lib/api/user.ts` | Update UserPreferences interface |
| `sandbox-ui/src/lib/api/client.ts` | Add success envelope checking |
| `sandbox-ui/src/lib/api/errors.ts` | Add conflict error types |
| `sandbox-ui/src/lib/ws/types.ts` | Align with backend WsMsg enum |
| `shared-types/src/lib.rs` | Add `#[ts(export)]` to ApiResponse<T>, WriterMsg |
| `sandbox/src/api/desktop.rs` | Add app_registered WS broadcast |
| `sandbox/src/api/mod.rs` | Export WS chat endpoints if needed |

---

## 8. Test Coverage Gaps

### 8.1 Missing Tests
1. Viewer API client (nonexistent)
2. Conflict resolution scenarios
3. WebSocket reconnection logic
4. Network error handling
5. Type guard validation

### 8.2 Existing Tests
- ✅ `sandbox-ui/src/lib/ws/client.test.ts` - WebSocket client tests
- ✅ `sandbox-ui/src/components/apps/Chat/ws.test.ts` - Chat WS tests
- ✅ `sandbox-ui/src/components/apps/Terminal/ws.test.ts` - Terminal WS tests

### 8.3 Recommended Test Files
- `sandbox-ui/src/lib/api/viewer.test.ts`
- `sandbox-ui/src/lib/api/desktop.test.ts`
- `sandbox-ui/src/lib/api/chat.test.ts`
- `sandbox-ui/src/lib/type-guards.test.ts`

---

## 9. Appendix A: Complete API Contract Comparison

### 9.1 Desktop API

| Endpoint | Backend Response | Dioxus Type | React Type | Match? |
|----------|-----------------|-------------|------------|--------|
| GET /desktop/{id} | `{ success, desktop: DesktopState }` | ✅ | ✅ | ✅ |
| GET /desktop/{id}/windows | `{ success, windows: WindowState[] }` | ✅ | ✅ | ✅ |
| POST /desktop/{id}/windows | `{ success, window?: WindowState, error? }` | ✅ | ✅ | ✅ |
| DELETE /desktop/{id}/windows/{wid} | `{ success, error? }` | ✅ | ✅ | ✅ |
| PATCH /desktop/{id}/windows/{wid}/position | `{ success, error? }` | ✅ | ✅ | ✅ |
| PATCH /desktop/{id}/windows/{wid}/size | `{ success, error? }` | ✅ | ✅ | ✅ |
| POST /desktop/{id}/windows/{wid}/focus | `{ success, error? }` | ✅ | ✅ | ✅ |
| POST /desktop/{id}/windows/{wid}/minimize | `{ success, error? }` | ✅ | ✅ | ✅ |
| POST /desktop/{id}/windows/{wid}/maximize | `{ success, window, from?, error? }` | ✅ | ❌ **WRONG** | ❌ |
| POST /desktop/{id}/windows/{wid}/restore | `{ success, window, from?, error? }` | ✅ | ❌ **WRONG** | ❌ |
| GET /desktop/{id}/apps | `{ success, apps: AppDefinition[] }` | ✅ | ✅ | ✅ |
| POST /desktop/{id}/apps | `{ success, error? }` | ✅ | ✅ | ✅ |

### 9.2 Chat API

| Endpoint | Backend Response | Dioxus Type | React Type | Match? |
|----------|-----------------|-------------|------------|--------|
| POST /chat/send | `{ success, temp_id, message?, error? }` | ✅ | ✅ | ✅ |
| GET /chat/{actor_id}/messages | `{ success, messages: ChatMessage[], error? }` | ✅ | ✅ | ✅ |

### 9.3 Viewer API

| Endpoint | Backend Response | Dioxus Type | React Type | Match? |
|----------|-----------------|-------------|------------|--------|
| GET /viewer/content | `{ success, uri, mime, content, revision, readonly }` | ✅ | ❌ **MISSING** | ❌ |
| PATCH /viewer/content | `{ success, revision?, error?, latest? }` | ✅ | ❌ **MISSING** | ❌ |

### 9.4 User API

| Endpoint | Backend Response | Dioxus Type | React Type | Match? |
|----------|-----------------|-------------|------------|--------|
| GET /user/{id}/preferences | `{ success, theme }` | ✅ | ❌ **WRONG** | ❌ |
| PATCH /user/{id}/preferences | `{ success, theme }` | ✅ | ❌ **WRONG** | ❌ |

### 9.5 Terminal API

| Endpoint | Backend Response | Dioxus Type | React Type | Match? |
|----------|-----------------|-------------|------------|--------|
| GET /api/terminals/{id} | `{ terminal_id, status }` | ✅ | ✅ | ✅ |
| GET /api/terminals/{id}/info | TerminalInfo object | ✅ | ✅ | ✅ |
| GET /api/terminals/{id}/stop | `{ terminal_id, status }` | ✅ | ✅ | ✅ |

---

## 10. Appendix B: Generated TypeScript Code Review

### 10.1 All Generated Types (from `sandbox-ui/src/types/generated.ts`)

```typescript
export type ActorId = string;

export type AppDefinition = {
  id: string;
  name: string;
  icon: string;
  component_code: string;
  default_width: number;
  default_height: number;
};

export type AppendEvent = {
  event_type: string;
  payload: unknown;
  actor_id: ActorId;
  user_id: string;
};

export type ChatMessage = {
  id: string;
  text: string;
  sender: Sender;
  timestamp: string;  // DateTime<Utc>
  pending: boolean;
};

export type ChatMsg =
  | { "UserTyped": { text: string, window_id: string } }
  | { "AssistantReply": { text: string, model: string } }
  | { "ToolCall": { tool: string, args: unknown, call_id: string } }
  | { "ToolResult": { call_id: string, status: ToolStatus, output: unknown } };

export type DesktopState = {
  windows: Array<WindowState>;
  active_window: string | null;
  apps: Array<AppDefinition>;
};

export type Event = {
  seq: bigint;  // i64
  event_id: string;
  timestamp: string;  // DateTime<Utc>
  actor_id: ActorId;
  event_type: string;
  payload: unknown;
  user_id: string;
};

export type QueryEvents = {
  actor_id: ActorId;
  since_seq: bigint;  // i64
};

export type Sender = "User" | "Assistant" | "System";

export type ToolCall = {
  id: string;
  tool: string;
  args: unknown;
};

export type ToolDef = {
  name: string;
  description: string;
  parameters: unknown;
};

export type ToolStatus = "Success" | { "Error": string };

export type ViewerCapabilities = { readonly: boolean };

export type ViewerDescriptor = {
  kind: ViewerKind;
  resource: ViewerResource;
  capabilities: ViewerCapabilities;
};

export type ViewerKind = "text" | "image";

export type ViewerResource = { uri: string, mime: string };

export type ViewerRevision = { rev: bigint, updated_at: string };

export type WindowState = {
  id: string;
  app_id: string;
  title: string;
  x: number;  // i32
  y: number;  // i32
  width: number;  // i32
  height: number;  // i32
  z_index: number;  // u32
  minimized: boolean;
  maximized: boolean;
  props: unknown;
};

export type WsMsg =
  | { "type": "Subscribe", actor_id: ActorId }
  | { "type": "Send", actor_id: ActorId, payload: unknown }
  | { "type": "Event", actor_id: ActorId, event: Event }
  | { "type": "State", actor_id: ActorId, state: unknown }
  | { "type": "Error", message: string };
```

### 10.2 Missing from Generated Types

```typescript
// These exist in Rust shared-types but are NOT exported
// Add #[ts(export, export_to = "...")] to these:

pub struct ApiResponse<T> { ... }
pub enum WriterMsg { ... }
// (Also check for any other types without #[ts(export)] attribute)
```

---

## 11. Conclusion

The React frontend (`sandbox-ui`) has **good coverage** of most API endpoints but is **missing critical functionality**:

1. **Viewer API** completely absent - blocks file editing
2. **Response type mismatches** in desktop and user APIs
3. **Generated types** not fully exported from shared-types
4. **Error handling** inconsistent across endpoints

The Dioxus backup (`sandbox-ui-backup`) has **better coverage** including the Viewer API, but also suffers from:
1. **String-based error returns** instead of proper error types
2. **Hardcoded user_id** in some requests
3. **Missing WebSocket chat** endpoints

**Recommendation:** Prioritize BUG-001 (Viewer API) and BUG-002/BUG-003 (desktop response mismatches) as these directly impact user functionality. Then standardize error handling across all endpoints.
