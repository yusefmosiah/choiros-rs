# R5 - Theme Style-Profile Architecture (Low Priority)

**Date:** 2026-02-05  
**Status:** Finalized

## Scope

Define a style-profile architecture that prioritizes user-prompted visual direction (for example: "make this feel glassmorphic") over a binary dark/light toggle, while preserving backend-canonical persistence and backward compatibility with current `theme`-only clients.

## Inputs

- `/Users/wiz/choiros-rs/docs/theme-system-research.md`
- `/Users/wiz/choiros-rs/docs/design/2026-02-05-ui-master-execution-plan.md`
- `/Users/wiz/choiros-rs/docs/design/2026-02-05-ui-storage-reconciliation.md`
- `/Users/wiz/choiros-rs/sandbox/src/api/user.rs`
- `/Users/wiz/choiros-rs/sandbox/src/api/mod.rs`
- `/Users/wiz/choiros-rs/shared-types/src/lib.rs`
- `/Users/wiz/choiros-rs/sandbox-ui/src/api.rs`
- `/Users/wiz/choiros-rs/sandbox-ui/src/desktop.rs`
- `/Users/wiz/choiros-rs/sandbox/tests/desktop_api_test.rs`

## Current-State Evidence

1. API and frontend are currently `theme: "light" | "dark"` only.
   - Request/response shape is string-only in `/Users/wiz/choiros-rs/sandbox/src/api/user.rs:18` and `/Users/wiz/choiros-rs/sandbox/src/api/user.rs:23`.
   - Frontend API expects `theme: String` in `/Users/wiz/choiros-rs/sandbox-ui/src/api.rs:171`.
2. Backend persists theme preference as an EventStore event (`user.theme_preference`).
   - Event type constant: `/Users/wiz/choiros-rs/shared-types/src/lib.rs:247`.
   - Append/read path: `/Users/wiz/choiros-rs/sandbox/src/api/user.rs:41` and `/Users/wiz/choiros-rs/sandbox/src/api/user.rs:95`.
3. Backend is authoritative and cache is non-authoritative by policy and implementation.
   - Policy: `/Users/wiz/choiros-rs/docs/design/2026-02-05-ui-storage-reconciliation.md:15`.
   - UI bootstrap reads cache first, then fetches backend and overwrites local state: `/Users/wiz/choiros-rs/sandbox-ui/src/desktop.rs:42`.
4. UI rendering is still toggle-centric.
   - Prompt bar toggle button: `/Users/wiz/choiros-rs/sandbox-ui/src/desktop.rs:515`.
   - Theme application hard-accepts only `light|dark`: `/Users/wiz/choiros-rs/sandbox-ui/src/desktop.rs:787`.
5. Existing tests cover default/get/set and invalid theme rejection only.
   - `/Users/wiz/choiros-rs/sandbox/tests/desktop_api_test.rs:603`
   - `/Users/wiz/choiros-rs/sandbox/tests/desktop_api_test.rs:620`
   - `/Users/wiz/choiros-rs/sandbox/tests/desktop_api_test.rs:653`

## Architecture Decision

Adopt a **profile-first preference model**:

1. `style_profile` is the primary stylistic selector (user-promptable and extensible).
2. `base_theme` remains a foundational contrast/background mode (`light|dark`) used by profile packs.
3. Legacy `theme` remains accepted and returned for old clients.
4. Backend EventStore remains canonical; browser cache remains hint-only.

This aligns with R5 objective in `/Users/wiz/choiros-rs/docs/design/2026-02-05-ui-master-execution-plan.md:160` and backend-canonical requirements in `/Users/wiz/choiros-rs/docs/theme-system-research.md:11`.

## Canonical Preference Schema (v2)

### Persisted payload (canonical)

```json
{
  "schema_version": 2,
  "base_theme": "dark",
  "style_profile": "glassmorphic",
  "custom_overrides": {
    "surface_window": "rgba(255,255,255,0.22)",
    "border_radius_lg": "18px"
  },
  "legacy_theme": "dark",
  "updated_at": "2026-02-05T00:00:00Z"
}
```

### Field contract

- `schema_version`: integer, currently `2`.
- `base_theme`: enum `light | dark`.
- `style_profile`: string slug; defaults to `default`.
- `custom_overrides`: map<string, string>, allowlisted token keys only.
- `legacy_theme`: derived mirror (`base_theme`) for compatibility reads.
- `updated_at`: RFC3339 server timestamp.

## API Compatibility Contract

### GET `/user/{user_id}/preferences`

Return a superset response:

```json
{
  "success": true,
  "theme": "dark",
  "theme_profile": {
    "schema_version": 2,
    "base_theme": "dark",
    "style_profile": "glassmorphic",
    "custom_overrides": {
      "surface_window": "rgba(255,255,255,0.22)"
    }
  }
}
```

Rules:

1. `theme` must always be present for old clients.
2. If only legacy event data exists, server synthesizes `theme_profile` from `theme`.
3. If `theme_profile` exists, `theme` is set to `theme_profile.base_theme`.

### PATCH `/user/{user_id}/preferences`

Accept both request shapes during migration:

1. Legacy: `{ "theme": "light" }`
2. Profile: `{ "theme_profile": { ...v2... } }`
3. Mixed payload: accepted; server normalizes to canonical v2 and enforces `theme == base_theme` in response.

## Style Profile Catalog

Built-in `style_profile` identifiers:

- `default`
- `neuebrutalist`
- `glassmorphic`
- `frutiger_aero`
- `liquid_metal`

Extensibility:

- Unknown profile IDs do not fail hard; backend stores requested slug, renderer falls back to `default` profile pack at runtime if pack is unavailable.
- This allows user-prompted requests to be captured canonically before full visual implementation exists.

## Token Layering Model

Final application order (lowest to highest precedence):

1. **Semantic base tokens** from `base_theme` (`light|dark`)  
   Source: existing token groups in `/Users/wiz/choiros-rs/sandbox-ui/src/desktop.rs:630` and selectors in `/Users/wiz/choiros-rs/sandbox-ui/src/desktop.rs:672`.
2. **Profile pack overrides** for `style_profile`  
   Applied as curated token deltas against semantic tokens.
3. **User `custom_overrides`**  
   Applied last, restricted by token allowlist.
4. **Hard fallback**  
   On invalid profile or token value, retain previous valid token and fall back to semantic base token.

Determinism rule:

- The same input tuple (`base_theme`, `style_profile`, `custom_overrides`) must produce stable token output independent of client.

## Safety Constraints

1. **Token-key allowlist only**
   - Reject non-allowlisted keys.
   - Start allowlist from currently used semantic token set in `/Users/wiz/choiros-rs/sandbox-ui/src/desktop.rs:633`.
2. **Value validation by token type**
   - Color tokens: hex/rgb/rgba/hsl only.
   - Length/radius tokens: `px|rem|em|%` constrained ranges.
   - Shadow tokens: strict parser; no semicolons/braces/url/function escapes.
3. **No arbitrary CSS injection path**
   - Payload cannot set raw CSS blocks, selectors, or property names.
4. **Contrast floor for critical pairs**
   - Enforce WCAG AA minimum for core text/surface pairs as required by research `/Users/wiz/choiros-rs/docs/theme-system-research.md:166`.
5. **Graceful degradation**
   - Any invalid override is dropped; request may still succeed with warnings and sanitized result.

## Migration Plan

1. **Phase 1: Read compatibility (no client breakage)**
   - Keep current route in `/Users/wiz/choiros-rs/sandbox/src/api/mod.rs:40`.
   - Expand response with optional `theme_profile`, keep `theme` required.
2. **Phase 2: Write compatibility**
   - PATCH accepts both legacy and v2 shapes.
   - Legacy writes are translated to canonical v2 (`style_profile: default`, empty overrides).
3. **Phase 3: UI bridge**
   - Replace binary toggle UX with style picker + prompt-driven style action.
   - Keep toggle as shorthand for `base_theme` only.
4. **Phase 4: Event payload normalization**
   - Continue using `user.theme_preference` event type (`/Users/wiz/choiros-rs/shared-types/src/lib.rs:247`) with versioned payload.
   - No event-type rename required for migration.
5. **Phase 5: Deprecation window**
   - After clients adopt `theme_profile`, mark `theme` write path deprecated but still readable.

## Test Implications

### Existing coverage to preserve

- Default theme retrieval, update roundtrip, invalid theme rejection in `/Users/wiz/choiros-rs/sandbox/tests/desktop_api_test.rs:603`.

### New backend tests required

1. `GET` returns both `theme` and `theme_profile` when v2 payload exists.
2. Legacy-only event payload is upgraded on read to synthesized `theme_profile`.
3. `PATCH` profile payload validates allowlisted keys and value grammar.
4. Unknown `style_profile` stored but response/render fallback marked as `default_applied: true` (or equivalent metadata).
5. Mixed payload (`theme` + `theme_profile`) resolves deterministically with `theme == base_theme` in response.

### New frontend tests required

1. Bootstrap conflict: backend profile overrides cached legacy theme (policy gate from `/Users/wiz/choiros-rs/docs/design/2026-02-05-r4-storage-conformance-audit.md:37`).
2. Theme application accepts profile-resolved token maps (not only `light|dark` guard currently in `/Users/wiz/choiros-rs/sandbox-ui/src/desktop.rs:788`).
3. Prompt-bar style actions update profile state without requiring binary toggle.

## Open Implementation Notes

1. Keep profile resolution logic in a dedicated `ThemeBridge` module (aligned with `/Users/wiz/choiros-rs/docs/design/2026-02-05-r1-dioxus-architecture-decomposition.md:35`).
2. Introduce explicit server-side sanitizer/validator for `custom_overrides`; do not trust client filtering.
3. Track profile provenance (`preset`, `prompt-generated`, `hybrid`) as optional metadata in future schema without changing core fields.

## Acceptance Checklist

- [x] Backward compatibility strategy complete (`theme` read/write compatibility).
- [x] Token layering order finalized (base -> profile -> overrides -> fallback).
- [x] Safety rules finalized (allowlist, validators, contrast floor, sanitization).
- [x] Profile catalog + extensibility model documented.
- [x] Migration path staged with no event-type break.
- [x] Test implications listed for backend and frontend.
