# ChoirOS Deployment Review: Critical Issues & Remediation Plan

**Date:** 2026-01-31  
**Reviewer:** Code Review Agent  
**Status:** 🟠 **Deployed & Working, but Misconfigured / Not Hardened**  
**Server:** <PUBLIC_IP> (AWS c5.large)

---

## Executive Summary

The ChoirOS deployment is **working**, but still **not production-ready**. Core functionality comes up, yet configuration choices (API origin, DB path, CORS, routing) make it fragile and easy to break across environments. Security hardening is also incomplete.

**Impact:** This previously caused "Error loading desktop: Failed to fetch". The app now loads, but the underlying risks remain if the API origin or routing drifts.

---

## Critical Issues (Fix Required Before Production)

### 1. 🔴 **CRITICAL: Frontend Hardcoded API Origin**

**Location:** `dioxus-desktop/src/api.rs:6`  
**Issue:** 
```rust
const API_BASE: &str = "http://localhost:8080";
```

**Impact:** Browser attempts to connect to the user's local machine instead of the server

**Fix:**
```rust
const API_BASE: &str = "";  // Use relative URLs (same origin)
```

**Rebuild Required:** Yes - requires `dx build --release` (NOT reinstalling dioxus-cli, just rebuilding the app)

---

### 2. 🔴 **HIGH: Backend Database Path Hardcoded for macOS**

**Location:** `sandbox/src/main.rs:19`  
**Issue:** 
```rust
let db_path = "/Users/wiz/choiros-rs/data/events.db";
```

**Impact:** 
- Path doesn't exist on Linux server
- Backend may fail to start or write to unexpected location
- Data loss risk if path is not writable

**Fix (applied):** Make it configurable via environment variable:
```rust
let db_path = std::env::var("DATABASE_URL")
    .unwrap_or_else(|_| "/opt/choiros/data/events.db".to_string());
```

---

### 3. 🟡 **MEDIUM: Production Using Dev Server (dx serve)**

**Current State:** 
- Frontend is being served via `dx serve` (development server on port 5173)
- Systemd service runs: `dx serve` instead of serving static files
- Caddy proxies to port 5173

**Problem:** 
- `dx serve` is for development, not production
- Slower, less stable, unnecessary overhead
- Built static files in `dist/` are unused

**Two Options:**

**Option A - Keep dx serve (Quick Fix):**
- Update API_BASE to "" (relative URLs)
- Ensure same-origin requests via Caddy (or access the dev server directly)
- Document that this is temporary

**Option B - Proper Production Setup (Recommended):**
1. Build static files: `dx build --release`
2. Update Caddy to serve static files directly:
   ```
   root * /opt/choiros/dioxus-desktop/dist
   file_server
   ```
3. Remove frontend systemd service (no longer needed)
4. Update deploy.sh to build and copy static files

---

### 4. 🟡 **MEDIUM: CORS Configuration Mismatch**

**Docs Say:** `docs/handoffs/2026-01-31-desktop-complete.md:413`
- "CORS limited to localhost:5173"

**Code Does:** `sandbox/src/main.rs:57`
- Uses `allow_any_origin/allow_any_method/allow_any_header` (very permissive)

**Issue:** 
- Security/documentation drift
- Makes debugging harder
- Weakens security assumptions

**Fix (applied):** 
```rust
// Restrict to specific origins in production
let cors = Cors::default()
    .allowed_origin("http://<PUBLIC_IP>")
    .allowed_origin("http://choir-ip.com")
    .allowed_origin("https://choir-ip.com")
    .allowed_origin("http://localhost:5173")
    .allowed_origin("http://127.0.0.1:5173")
    .allowed_methods(vec!["GET", "POST", "DELETE", "PATCH", "OPTIONS"])
    .allowed_headers(vec![header::CONTENT_TYPE, header::ACCEPT, header::AUTHORIZATION]);
```
**Note:** Actix disallows wildcard origins when credentials are enabled; keep origins explicit if/when cookies or auth headers are added. citeturn0search2

---

### 5. 🟡 **MEDIUM: API Route Documentation Mismatch**

**Docs Show:** `/api/*` routes  
**Backend Has:** `/chat/*`, `/desktop/*` (no /api prefix)

**Caddy Config Currently:**
```
handle /api/* {
    reverse_proxy localhost:8080
}
```

**Issue:** 
- Caddy uses `handle`, not `handle_path`, so **no prefix is stripped**
- Backend routes don't have `/api/` prefix
- Results in 404s if following docs literally

**Fix Options:**

**Option A - Remove /api from Caddy:**
```
handle /chat/* {
    reverse_proxy localhost:8080
}
handle /desktop/* {
    reverse_proxy localhost:8080
}
```

**Option B - Add /api prefix to backend:**
- Update backend routes to be `/api/chat/*`, `/api/desktop/*`
- Keep Caddy config as-is
- Requires backend code changes

---

## Missing Tests

### CORS Configuration Not Tested

**Location:** `docs/TESTING_STRATEGY.md:731`  
**Issue:** No automated tests verify CORS headers in production configuration  
**Risk:** CORS issues only discovered in production  

**Recommendation:** Add E2E test that verifies:
```bash
curl -H "Origin: http://example.com" http://server/health
# Should return appropriate Access-Control-Allow-Origin header
```

---

## Open Questions / Architectural Decisions

### 1. **Production Architecture**

**Question:** Should we serve static files via Caddy or keep dx serve?

**Recommendation:** Depends on live hot‑patching design:
- If hot‑patching requires live compilation, `dx serve` (or a patch service) may be necessary.
- If patches can be delivered as versioned assets/modules, serve **static** files and layer a secure patch loader.
- Either way, document how patches are authenticated and how rollbacks work.

### 2. **API Route Structure**

**Question:** Should routes be `/chat/*` or `/api/chat/*`?

**Recommendation:** Use `/api/*` prefix:
- Clear separation of API vs static assets
- Industry standard practice
- Easier to add API versioning later

### 3. **Database Location**

**Question:** Should DB path be configurable per environment?

**Recommendation:** Yes:
- Dev: Default local path
- Prod: `/opt/choiros/data/events.db` via env var
- Docker: Volume-mounted path

---

## Remediation Plan

### Phase 1: Critical Fixes (Required for Production)

1. **Fix API_BASE** (5 min)
   - Edit `dioxus-desktop/src/api.rs` line 6
   - Change to `const API_BASE: &str = "";`

2. **Fix Database Path** (5 min)
   - Edit `sandbox/src/main.rs` line 19
   - Use environment variable with fallback

3. **Rebuild & Redeploy** (10 min)
   ```bash
   # On server
   cd /opt/choiros/dioxus-desktop
   dx build --release
   sudo systemctl restart choiros-backend choiros-frontend
   ```

4. **Verify Fix** (5 min)
   ```bash
   curl http://<PUBLIC_IP>/health
   # Open browser to http://<PUBLIC_IP>
   # Desktop should load without errors
   ```

### Phase 2: Production Hardening (Recommended)

1. **Switch to Static File Serving**
   - Update Caddy to serve `dist/` folder directly
   - Remove frontend systemd service
   - Update deploy.sh

2. **Lock Down CORS**
   - Replace `Cors::permissive()` with specific origins
   - Add to deploy runbook

3. **Standardize API Routes**
   - Either add `/api/` prefix to backend OR remove from Caddy/docs
   - Document the chosen convention

---

## Hardening Changes Applied (2026-01-31)

- Enabled fail2ban at boot and verified SSH jail.
- Added Caddy security headers and log rotation.
- Added app log rotation via `/etc/logrotate.d/choiros`.
- Tightened permissions on `/opt/choiros/data` and `events.db`.
- Added systemd hardening drop-ins for backend/frontend services.

---

## Security Hardening Checklist (Next Pass)

### OS / Host
- Keep Ubuntu updated and enable unattended security updates. citeturn1search0
- Enforce least‑privilege (non‑root services; minimal sudo). citeturn1search0
- Firewall only required ports (SSH, HTTP/HTTPS). citeturn1search0

### Caddy / Edge
- Turn on Automatic HTTPS when a domain is attached (avoid `auto_https off`). citeturn1search2turn1search4
- Add explicit security headers via Caddy `header` directive (HSTS once HTTPS is enabled, X‑Content‑Type‑Options, etc.). citeturn1search5
- Enable/verify access logs and log rotation via the `log` directive; credentials are redacted by default. citeturn1search1

### API / CORS
- Explicit allow‑list origins and methods; avoid wildcard if credentials are added. citeturn0search2

4. **Add CORS Tests**
   - Add E2E test for CORS headers
   - Add test for cross-origin API access

### Phase 3: Documentation Updates

1. Update deployment runbook with:
   - Production architecture diagram
   - CORS configuration section
   - Troubleshooting guide for "Failed to fetch" errors

2. Update API documentation:
   - Correct route paths
   - Add CORS requirements

3. Update handoff docs:
   - Remove outdated CORS references
   - Clarify production vs dev setup

---

## Immediate Action Required

**DO NOT configure GitHub Secrets or enable auto-deployment until these are fixed:**

1. ✅ Fix `API_BASE` to use relative URLs
2. ✅ Fix database path to use env var
3. ✅ Rebuild frontend with corrected config
4. ✅ Verify deployment works from external network
5. ⬜ (Optional) Switch to static file serving

**Estimated Time to Fix:** 30 minutes  
**Estimated Time to Production-Ready:** 2-3 hours (with Phase 2 hardening)

---

## Appendix: Current vs Target State

| Component | Current (Broken) | Target (Working) |
|-----------|------------------|------------------|
| API_BASE | "http://localhost:8080" | "" (relative) |
| DB Path | Hardcoded macOS path | Env var + default |
| Frontend | dx serve (dev mode) | Static files via Caddy |
| CORS | Permissive (any origin) | Restricted to server domain |
| API Routes | /chat/*, /desktop/* | /api/chat/*, /api/desktop/* |
| Documentation | Outdated/mismatched | Accurate and tested |

---

**Next Steps:**
1. Decide on architectural approach (Option A or B for each issue)
2. Implement Phase 1 fixes immediately
3. Schedule Phase 2 for next iteration
4. Update documentation

**Sign-off Required:**  
- [ ] Technical Lead approves architectural decisions  
- [ ] DevOps validates deployment changes  
- [ ] QA confirms E2E tests pass

---

*Document Version: 1.0*  
*Last Updated: 2026-01-31*  
*Reviewed By: Code Review Agent*
