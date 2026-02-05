# R5 - Theme Style-Profile Architecture (Low Priority)

**Date:** 2026-02-05
**Status:** In progress

## Scope

Define a profile-based theme architecture for user-prompted visual styles, while preserving backend-canonical preference persistence.

## Inputs

- `docs/theme-system-research.md`
- `sandbox-ui/src/desktop.rs`
- `sandbox/src/api/user.rs`
- `shared-types/src/lib.rs`

## Direction

Move from a binary light/dark emphasis to style profiles:
- `neuebrutalist`
- `glassmorphic`
- `frutiger_aero`
- `liquid_metal`
- custom profile combinations

## Non-Goals

- No full prompt-to-theme generator implementation in this lane.
- No heavy visual redesign before core architecture work (R1-R4) stabilizes.

## Proposed Preference Model (Draft)

```json
{
  "base_theme": "dark",
  "style_profile": "glassmorphic",
  "custom_overrides": {
    "surface_window": "rgba(255,255,255,0.25)",
    "border_radius_lg": "18px"
  }
}
```

## Token Architecture (Draft)

1. Keep semantic tokens as base contract.
2. Layer profile packs as token override sets.
3. Layer user overrides last, with allowlisted token keys.
4. Provide hard fallback to base theme tokens.

## Safety and Quality Constraints (Draft)

- Contrast floor (WCAG AA target for core text/surfaces).
- Allowlist token keys to avoid arbitrary CSS injection patterns.
- Fallback when invalid token values are supplied.

## Migration Path (Draft)

1. Continue supporting current `theme` preference shape.
2. Add profile metadata fields without breaking old clients.
3. On missing profile fields, map to legacy light/dark behavior.

## Acceptance Checklist

- [ ] Backward compatibility strategy complete.
- [ ] Token layering order finalized.
- [ ] Safety rules finalized.
- [ ] Profile catalog + extensibility model documented.
