use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tower_sessions::Session;
use tracing::{error, info, warn};
use uuid::Uuid;
use webauthn_rs::prelude::*;

use crate::auth::{generate_recovery_codes, session as sess, verify_recovery_code};
use crate::AppState;

// ── Registration ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct RegisterStartBody {
    pub username: String,
    pub display_name: String,
}

/// POST /auth/register/start
pub async fn register_start(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(body): Json<RegisterStartBody>,
) -> Response {
    let username = body.username.trim().to_string();
    if username.is_empty() {
        return (StatusCode::BAD_REQUEST, "username required").into_response();
    }

    // Fetch any existing credential IDs for this user to exclude
    let existing_ids: Vec<CredentialID> = match fetch_credential_ids(&state.db, &username).await {
        Ok(ids) => ids,
        Err(e) => {
            error!("DB error fetching credential ids: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Look up or generate a stable user UUID
    let user_uuid = match fetch_or_create_user_uuid(&state.db, &username, &body.display_name).await
    {
        Ok(id) => id,
        Err(e) => {
            error!("DB error in fetch_or_create_user_uuid: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let exclude = if existing_ids.is_empty() {
        None
    } else {
        Some(existing_ids)
    };

    match state.webauthn.start_passkey_registration(
        user_uuid,
        &username,
        &body.display_name,
        exclude,
    ) {
        Ok((ccr, reg_state)) => {
            if let Err(e) = session
                .insert("reg_state", (&username, user_uuid, reg_state))
                .await
            {
                error!("session insert failed: {e}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            Json(ccr).into_response()
        }
        Err(e) => {
            error!("start_passkey_registration error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Serialize)]
pub struct RegisterFinishResponse {
    pub recovery_codes: Vec<String>,
    pub is_first_passkey: bool,
}

/// POST /auth/register/finish
pub async fn register_finish(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(reg): Json<RegisterPublicKeyCredential>,
) -> Response {
    let Some((username, user_uuid, reg_state)) = session
        .get::<(String, Uuid, PasskeyRegistration)>("reg_state")
        .await
        .ok()
        .flatten()
    else {
        return (StatusCode::BAD_REQUEST, "no pending registration").into_response();
    };
    let _ = session
        .remove::<(String, Uuid, PasskeyRegistration)>("reg_state")
        .await;

    let passkey = match state.webauthn.finish_passkey_registration(&reg, &reg_state) {
        Ok(p) => p,
        Err(e) => {
            warn!("finish_passkey_registration failed: {e}");
            return (StatusCode::BAD_REQUEST, format!("registration failed: {e}")).into_response();
        }
    };

    // Persist the user record (idempotent) and the new passkey
    let user_id = user_uuid.to_string();
    let now = Utc::now().timestamp();
    let cred_id = base64_url_encode(passkey.cred_id());
    let passkey_json = match serde_json::to_string(&passkey) {
        Ok(j) => j,
        Err(e) => {
            error!("serialize passkey: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if let Err(e) = sqlx::query!(
        "INSERT OR IGNORE INTO users (id, username, display_name, created_at) VALUES (?, ?, ?, ?)",
        user_id,
        username,
        username,
        now,
    )
    .execute(&state.db)
    .await
    {
        error!("insert user: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    if let Err(e) = sqlx::query!(
        "INSERT OR REPLACE INTO passkeys (credential_id, user_id, passkey_json, created_at) VALUES (?, ?, ?, ?)",
        cred_id,
        user_id,
        passkey_json,
        now,
    )
    .execute(&state.db)
    .await
    {
        error!("insert passkey: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    // Issue recovery codes on first passkey only
    let passkey_count: i64 =
        sqlx::query_scalar!("SELECT COUNT(*) FROM passkeys WHERE user_id = ?", user_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(1);

    let is_first = passkey_count <= 1;
    let recovery_codes = if is_first {
        let (plaintexts, hashes) = match generate_recovery_codes() {
            Ok(c) => c,
            Err(e) => {
                error!("generate recovery codes: {e}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };
        for hash in &hashes {
            let code_id = Uuid::new_v4().to_string();
            if let Err(e) = sqlx::query!(
                "INSERT INTO recovery_codes (id, user_id, code_hash, created_at) VALUES (?, ?, ?, ?)",
                code_id,
                user_id,
                hash,
                now,
            )
            .execute(&state.db)
            .await
            {
                error!("insert recovery code: {e}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
        info!(username, "first passkey registered, recovery codes issued");
        plaintexts
    } else {
        vec![]
    };

    audit(&state.db, Some(&user_id), "register", None, None).await;
    // Auto-login: set user session immediately after successful registration
    // so the caller doesn't need a separate login step.
    if let Err(e) = sess::set_user(&session, &user_id, &username).await {
        error!("session set_user after register: {e}");
        // Non-fatal — passkey is saved; user can log in manually.
    }
    Json(RegisterFinishResponse {
        recovery_codes,
        is_first_passkey: is_first,
    })
    .into_response()
}

// ── Authentication ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct LoginStartBody {
    pub username: String,
}

/// POST /auth/login/start
pub async fn login_start(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(body): Json<LoginStartBody>,
) -> Response {
    let passkeys = match fetch_passkeys(&state.db, &body.username).await {
        Ok(p) => p,
        Err(e) => {
            error!("fetch passkeys: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if passkeys.is_empty() {
        return (
            StatusCode::NOT_FOUND,
            "user not found or no passkeys registered",
        )
            .into_response();
    }

    match state.webauthn.start_passkey_authentication(&passkeys) {
        Ok((rcr, auth_state)) => {
            if let Err(e) = session
                .insert("auth_state", (&body.username, auth_state))
                .await
            {
                error!("session insert auth_state: {e}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            Json(rcr).into_response()
        }
        Err(e) => {
            error!("start_passkey_authentication: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// POST /auth/login/finish
pub async fn login_finish(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(auth): Json<PublicKeyCredential>,
) -> Response {
    let Some((username, auth_state)) = session
        .get::<(String, PasskeyAuthentication)>("auth_state")
        .await
        .ok()
        .flatten()
    else {
        return (StatusCode::BAD_REQUEST, "no pending authentication").into_response();
    };
    let _ = session
        .remove::<(String, PasskeyAuthentication)>("auth_state")
        .await;

    let auth_result = match state
        .webauthn
        .finish_passkey_authentication(&auth, &auth_state)
    {
        Ok(r) => r,
        Err(e) => {
            warn!(username, "finish_passkey_authentication failed: {e}");
            return (
                StatusCode::UNAUTHORIZED,
                format!("authentication failed: {e}"),
            )
                .into_response();
        }
    };

    // Update credential counter in DB
    if let Err(e) = update_passkey_counter(&state.db, &username, &auth_result).await {
        error!("update_passkey_counter: {e}");
        // Non-fatal — still let the login succeed
    }

    let user_id = match fetch_user_id(&state.db, &username).await {
        Ok(Some(id)) => id,
        _ => {
            error!(username, "user_id not found after successful auth");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if let Err(e) = sess::set_user(&session, &user_id, &username).await {
        error!("session set_user: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    info!(username, "login successful");
    audit(&state.db, Some(&user_id), "login", None, None).await;
    StatusCode::OK.into_response()
}

/// POST /auth/logout
pub async fn logout(session: Session) -> Response {
    let _ = sess::clear(&session).await;
    Redirect::to("/login").into_response()
}

// ── Session check ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct MeResponse {
    pub authenticated: bool,
    pub user_id: Option<String>,
    pub username: Option<String>,
}

/// GET /auth/me
///
/// Returns the current session state. Always 200; the Dioxus app checks
/// `authenticated` to decide whether to render the BIOS login flow or
/// route to the main app shell.
pub async fn me(session: Session) -> Json<MeResponse> {
    let user_id = sess::get_user_id(&session).await;
    let username = session
        .get::<String>(sess::SESSION_USERNAME_KEY)
        .await
        .ok()
        .flatten();
    let authenticated = user_id.is_some();
    Json(MeResponse {
        authenticated,
        user_id,
        username,
    })
}

// ── Recovery codes ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct RecoveryBody {
    pub username: String,
    pub code: String,
}

/// POST /auth/recovery
///
/// Consumes a valid recovery code and sets a short-lived "recovery session"
/// that allows the user to register a new passkey.
pub async fn recovery(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(body): Json<RecoveryBody>,
) -> Response {
    let user_id = match fetch_user_id(&state.db, &body.username).await {
        Ok(Some(id)) => id,
        _ => {
            // Timing-safe: still do argon2 work before returning
            {
                use argon2::{
                    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
                    Argon2,
                };
                let salt = SaltString::generate(&mut OsRng);
                let _ = Argon2::default().hash_password(b"dummy", &salt);
            }
            warn!(username = %body.username, "recovery attempt for unknown user");
            return (StatusCode::UNAUTHORIZED, "invalid username or code").into_response();
        }
    };

    // Fetch all unused recovery codes for this user
    let rows = sqlx::query!(
        "SELECT id, code_hash FROM recovery_codes WHERE user_id = ? AND used_at IS NULL",
        user_id
    )
    .fetch_all(&state.db)
    .await;

    let rows = match rows {
        Ok(r) => r,
        Err(e) => {
            error!("fetch recovery codes: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if rows.is_empty() {
        warn!(username = %body.username, "no unused recovery codes");
        return (StatusCode::UNAUTHORIZED, "invalid username or code").into_response();
    }

    let matched = rows
        .iter()
        .find(|r| verify_recovery_code(&body.code, &r.code_hash));

    match matched {
        None => {
            warn!(username = %body.username, "recovery code mismatch");
            (StatusCode::UNAUTHORIZED, "invalid username or code").into_response()
        }
        Some(row) => {
            let now = Utc::now().timestamp();
            if let Err(e) = sqlx::query!(
                "UPDATE recovery_codes SET used_at = ? WHERE id = ?",
                now,
                row.id
            )
            .execute(&state.db)
            .await
            {
                error!("mark recovery code used: {e}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }

            // Set a recovery session — allows passkey registration but not full access
            if let Err(e) = session.insert("recovery_user_id", &user_id).await {
                error!("session insert recovery_user_id: {e}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }

            info!(username = %body.username, "recovery code accepted");
            audit(&state.db, Some(&user_id), "recovery_code_used", None, None).await;
            StatusCode::OK.into_response()
        }
    }
}

// ── Auth pages — serve Dioxus index.html (WASM router handles routes) ─────────
//
// When the Dioxus dist is built, index.html is served for /login, /register,
// /recovery and the WASM router takes over client-side.
// BIOS_SHELL is the fallback when dist/index.html doesn't exist yet.

pub async fn login_page() -> impl IntoResponse {
    serve_index_html()
}

pub async fn register_page() -> impl IntoResponse {
    serve_index_html()
}

pub async fn recovery_page() -> impl IntoResponse {
    serve_index_html()
}

fn serve_index_html() -> axum::response::Response {
    let dist = crate::config::frontend_dist_from_env();
    let index_path = format!("{dist}/index.html");

    match std::fs::read_to_string(&index_path) {
        Ok(html) => axum::response::Html(html).into_response(),
        Err(_) => axum::response::Html(BIOS_SHELL).into_response(),
    }
}

/// BIOS shell — handles /login, /register, /recovery client-side.
/// Routes by pathname, calls navigator.credentials directly (no external deps).
/// Replace with Dioxus WASM build once the frontend BIOS components are ready.
static BIOS_SHELL: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>ChoirOS</title>
  <style>
    *, *::before, *::after { box-sizing: border-box; }
    body { margin: 0; background: #0f172a; color: #f8fafc;
           font-family: system-ui, sans-serif;
           display: flex; align-items: flex-start; justify-content: center;
           padding-top: 8rem; min-height: 100vh; }
    .card { background: #1e293b; border: 1px solid #334155; border-radius: 8px;
            padding: 2rem; width: 100%; max-width: 380px; }
    h1 { font-size: 1.1rem; font-weight: 600; margin: 0 0 1.5rem;
         color: #94a3b8; letter-spacing: .05em; text-transform: uppercase; }
    label { display: block; font-size: .8rem; color: #94a3b8; margin-bottom: .3rem; }
    input { display: block; width: 100%; padding: .55rem .75rem;
            background: #0f172a; color: #f8fafc; border: 1px solid #334155;
            border-radius: 4px; font-size: .95rem; margin-bottom: 1rem; }
    input:focus { outline: none; border-color: #6366f1; }
    button.primary { width: 100%; padding: .65rem; background: #6366f1;
                     color: #fff; border: none; border-radius: 4px;
                     font-size: .95rem; font-weight: 500; cursor: pointer; }
    button.primary:hover { background: #818cf8; }
    button.primary:disabled { opacity: .5; cursor: not-allowed; }
    .nav { margin-top: 1.25rem; font-size: .82rem; color: #64748b;
           display: flex; gap: 1rem; }
    .nav a { color: #94a3b8; text-decoration: none; }
    .nav a:hover { color: #f8fafc; }
    .status { margin-top: .75rem; font-size: .85rem; min-height: 1.2em; }
    .status.err { color: #f87171; }
    .status.ok  { color: #4ade80; }
    .codes { display: none; margin-top: 1.25rem; padding: 1rem;
             background: #0f172a; border: 1px solid #334155; border-radius: 4px; }
    .codes h2 { font-size: .85rem; color: #94a3b8; margin: 0 0 .5rem; }
    .codes p  { font-size: .8rem; color: #64748b; margin: 0 0 .75rem; }
    .codes ol { margin: 0; padding-left: 1.5rem;
                font-family: monospace; font-size: .9rem;
                color: #f8fafc; line-height: 1.9; }
    .codes .warn { color: #f87171; font-size: .78rem; margin-top: .5rem; }
  </style>
</head>
<body>
<div class="card" id="app"></div>
<script>
// ── base64url helpers ─────────────────────────────────────────────────────────
function b64uToBytes(b64u) {
  const pad = b64u.length % 4 === 0 ? '' : '='.repeat(4 - b64u.length % 4);
  const b64 = (b64u + pad).replace(/-/g, '+').replace(/_/g, '/');
  return Uint8Array.from(atob(b64), c => c.charCodeAt(0));
}
function bytesToB64u(buf) {
  const bytes = new Uint8Array(buf);
  let s = '';
  bytes.forEach(b => s += String.fromCharCode(b));
  return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=/g, '');
}

// Recursively decode base64url strings to ArrayBuffer for fields the
// WebAuthn browser API expects as BufferSource.
function decodePublicKeyOptions(opts) {
  const o = Object.assign({}, opts);
  if (o.challenge)    o.challenge    = b64uToBytes(o.challenge).buffer;
  if (o.user?.id)     o.user = Object.assign({}, o.user, { id: b64uToBytes(o.user.id).buffer });
  if (o.excludeCredentials) {
    o.excludeCredentials = o.excludeCredentials.map(c =>
      Object.assign({}, c, { id: b64uToBytes(c.id).buffer }));
  }
  if (o.allowCredentials) {
    o.allowCredentials = o.allowCredentials.map(c =>
      Object.assign({}, c, { id: b64uToBytes(c.id).buffer }));
  }
  return o;
}

// Encode a PublicKeyCredential (create or get) into plain JSON for the server.
function encodeCredential(cred) {
  const resp = cred.response;
  const out = {
    id: cred.id,
    rawId: bytesToB64u(cred.rawId),
    type: cred.type,
    response: {},
  };
  if (resp.attestationObject) {
    // registration
    out.response.attestationObject = bytesToB64u(resp.attestationObject);
    out.response.clientDataJSON     = bytesToB64u(resp.clientDataJSON);
    if (resp.getTransports) out.transports = resp.getTransports();
    const ext = cred.getClientExtensionResults ? cred.getClientExtensionResults() : {};
    if (ext.credProps) out.clientExtensionResults = { credProps: ext.credProps };
    else out.clientExtensionResults = {};
  } else {
    // authentication
    out.response.authenticatorData = bytesToB64u(resp.authenticatorData);
    out.response.clientDataJSON     = bytesToB64u(resp.clientDataJSON);
    out.response.signature          = bytesToB64u(resp.signature);
    if (resp.userHandle) out.response.userHandle = bytesToB64u(resp.userHandle);
    out.clientExtensionResults = cred.getClientExtensionResults
      ? cred.getClientExtensionResults() : {};
  }
  return out;
}

// ── status helper ─────────────────────────────────────────────────────────────
function setStatus(el, msg, isErr) {
  el.textContent = msg;
  el.className = 'status ' + (isErr ? 'err' : 'ok');
}

// ── router ────────────────────────────────────────────────────────────────────
const path = window.location.pathname;
const app  = document.getElementById('app');

if (path === '/register') {
  renderRegister();
} else if (path === '/recovery') {
  renderRecovery();
} else {
  renderLogin();
}

// ── register ──────────────────────────────────────────────────────────────────
function renderRegister() {
  app.innerHTML = `
    <h1>ChoirOS</h1>
    <form id="form">
      <label for="username">Username</label>
      <input type="text" id="username" autocomplete="username" required>
      <label for="display">Display name</label>
      <input type="text" id="display" autocomplete="name" placeholder="optional">
      <button class="primary" type="submit">Register passkey</button>
    </form>
    <div class="codes" id="codes">
      <h2>Save your recovery codes</h2>
      <p>These 10 codes are shown once. Store them somewhere safe.</p>
      <ol id="code-list"></ol>
      <p class="warn">You cannot retrieve these later.</p>
    </div>
    <button class="primary" id="done" style="display:none;margin-top:1rem"
            onclick="window.location.href='/'">Continue to ChoirOS</button>
    <div class="status err" id="status"></div>
    <div class="nav"><a href="/login">Already have an account?</a></div>`;

  document.getElementById('form').addEventListener('submit', async e => {
    e.preventDefault();
    const btn      = e.target.querySelector('button');
    const status   = document.getElementById('status');
    const username = document.getElementById('username').value.trim();
    const display  = document.getElementById('display').value.trim() || username;
    btn.disabled   = true;
    setStatus(status, '', false);

    try {
      const startRes = await fetch('/auth/register/start', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ username, display_name: display }),
      });
      if (!startRes.ok) { setStatus(status, await startRes.text(), true); btn.disabled = false; return; }

      const opts = decodePublicKeyOptions((await startRes.json()).publicKey);
      const cred = await navigator.credentials.create({ publicKey: opts });

      const finishRes = await fetch('/auth/register/finish', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(encodeCredential(cred)),
      });
      if (!finishRes.ok) { setStatus(status, await finishRes.text(), true); btn.disabled = false; return; }

      const result = await finishRes.json();
      document.getElementById('form').style.display = 'none';

      if (result.is_first_passkey && result.recovery_codes.length > 0) {
        const list = document.getElementById('code-list');
        result.recovery_codes.forEach(code => {
          const li = document.createElement('li');
          li.textContent = code;
          list.appendChild(li);
        });
        document.getElementById('codes').style.display = 'block';
      }
      setStatus(status, 'Passkey registered!', false);
      document.getElementById('done').style.display = 'block';
    } catch (err) {
      setStatus(status, err.name + ': ' + err.message, true);
      btn.disabled = false;
    }
  });
}

// ── login ─────────────────────────────────────────────────────────────────────
function renderLogin() {
  app.innerHTML = `
    <h1>ChoirOS</h1>
    <form id="form">
      <label for="username">Username</label>
      <input type="text" id="username" autocomplete="username webauthn" required>
      <button class="primary" type="submit">Sign in with passkey</button>
    </form>
    <div class="status" id="status"></div>
    <div class="nav">
      <a href="/register">Register</a>
      <a href="/recovery">Recover account</a>
    </div>`;

  document.getElementById('form').addEventListener('submit', async e => {
    e.preventDefault();
    const btn      = e.target.querySelector('button');
    const status   = document.getElementById('status');
    const username = document.getElementById('username').value.trim();
    btn.disabled   = true;
    setStatus(status, '', false);

    try {
      const startRes = await fetch('/auth/login/start', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ username }),
      });
      if (!startRes.ok) { setStatus(status, await startRes.text(), true); btn.disabled = false; return; }

      const opts = decodePublicKeyOptions((await startRes.json()).publicKey);
      const cred = await navigator.credentials.get({ publicKey: opts });

      const finishRes = await fetch('/auth/login/finish', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(encodeCredential(cred)),
      });
      if (finishRes.ok) {
        window.location.href = '/';
      } else {
        setStatus(status, await finishRes.text(), true);
        btn.disabled = false;
      }
    } catch (err) {
      setStatus(status, err.name + ': ' + err.message, true);
      btn.disabled = false;
    }
  });
}

// ── recovery ──────────────────────────────────────────────────────────────────
function renderRecovery() {
  app.innerHTML = `
    <h1>ChoirOS</h1>
    <p style="font-size:.85rem;color:#64748b;margin:0 0 1.25rem">
      Enter your username and a recovery code to unlock passkey registration.</p>
    <form id="form">
      <label for="username">Username</label>
      <input type="text" id="username" autocomplete="username" required>
      <label for="code">Recovery code</label>
      <input type="text" id="code" placeholder="xxxxx-xxxxx-xxxxx-xxxxx" required>
      <button class="primary" type="submit">Verify recovery code</button>
    </form>
    <div id="next" style="display:none;margin-top:1rem;font-size:.85rem;color:#4ade80">
      Code accepted. <a href="/register" style="color:#818cf8">Register a new passkey</a>.
    </div>
    <div class="status" id="status"></div>
    <div class="nav"><a href="/login">Back to sign in</a></div>`;

  document.getElementById('form').addEventListener('submit', async e => {
    e.preventDefault();
    const btn      = e.target.querySelector('button');
    const status   = document.getElementById('status');
    const username = document.getElementById('username').value.trim();
    const code     = document.getElementById('code').value.trim();
    btn.disabled   = true;

    try {
      const res = await fetch('/auth/recovery', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ username, code }),
      });
      if (res.ok) {
        document.getElementById('form').style.display = 'none';
        document.getElementById('next').style.display = 'block';
      } else {
        setStatus(status, await res.text(), true);
        btn.disabled = false;
      }
    } catch (err) {
      setStatus(status, err.name + ': ' + err.message, true);
      btn.disabled = false;
    }
  });
}
</script>
</body>
</html>
"#;

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn fetch_user_id(pool: &SqlitePool, username: &str) -> anyhow::Result<Option<String>> {
    let row: Option<Option<String>> =
        sqlx::query_scalar!("SELECT id FROM users WHERE username = ?", username)
            .fetch_optional(pool)
            .await?;
    // query_scalar! on a non-null TEXT primary key returns Option<Option<String>>;
    // the outer Option is "row present?", inner is "value null?" — flatten them.
    Ok(row.flatten())
}

async fn fetch_or_create_user_uuid(
    pool: &SqlitePool,
    username: &str,
    display_name: &str,
) -> anyhow::Result<Uuid> {
    if let Some(id) = fetch_user_id(pool, username).await? {
        return Ok(Uuid::parse_str(&id)?);
    }
    let new_id = Uuid::new_v4();
    let id_str = new_id.to_string();
    let now = Utc::now().timestamp();
    sqlx::query!(
        "INSERT OR IGNORE INTO users (id, username, display_name, created_at) VALUES (?, ?, ?, ?)",
        id_str,
        username,
        display_name,
        now,
    )
    .execute(pool)
    .await?;
    Ok(new_id)
}

async fn fetch_credential_ids(
    pool: &SqlitePool,
    username: &str,
) -> anyhow::Result<Vec<webauthn_rs::prelude::CredentialID>> {
    let rows = sqlx::query!(
        "SELECT p.passkey_json FROM passkeys p JOIN users u ON p.user_id = u.id WHERE u.username = ?",
        username
    )
    .fetch_all(pool)
    .await?;

    let mut ids = Vec::new();
    for row in rows {
        if let Ok(pk) = serde_json::from_str::<Passkey>(&row.passkey_json) {
            ids.push(pk.cred_id().clone());
        }
    }
    Ok(ids)
}

async fn fetch_passkeys(pool: &SqlitePool, username: &str) -> anyhow::Result<Vec<Passkey>> {
    let rows = sqlx::query!(
        "SELECT p.passkey_json FROM passkeys p JOIN users u ON p.user_id = u.id WHERE u.username = ?",
        username
    )
    .fetch_all(pool)
    .await?;

    let mut passkeys = Vec::new();
    for row in rows {
        match serde_json::from_str::<Passkey>(&row.passkey_json) {
            Ok(pk) => passkeys.push(pk),
            Err(e) => error!("deserialize passkey: {e}"),
        }
    }
    Ok(passkeys)
}

async fn update_passkey_counter(
    pool: &SqlitePool,
    username: &str,
    auth_result: &AuthenticationResult,
) -> anyhow::Result<()> {
    // Load all passkeys, update the matching one
    let rows = sqlx::query!(
        "SELECT p.credential_id, p.passkey_json FROM passkeys p JOIN users u ON p.user_id = u.id WHERE u.username = ?",
        username
    )
    .fetch_all(pool)
    .await?;

    let now = Utc::now().timestamp();

    for row in rows {
        let mut pk: Passkey = match serde_json::from_str(&row.passkey_json) {
            Ok(p) => p,
            Err(_) => continue,
        };
        if pk.update_credential(auth_result).unwrap_or(false) {
            let updated_json = serde_json::to_string(&pk)?;
            sqlx::query!(
                "UPDATE passkeys SET passkey_json = ?, last_used_at = ? WHERE credential_id = ?",
                updated_json,
                now,
                row.credential_id,
            )
            .execute(pool)
            .await?;
            break;
        }
    }
    Ok(())
}

async fn audit(
    pool: &SqlitePool,
    user_id: Option<&str>,
    event: &str,
    detail: Option<&str>,
    ip: Option<&str>,
) {
    let now = Utc::now().timestamp();
    let _ = sqlx::query!(
        "INSERT INTO audit_log (user_id, event, detail, ip, created_at) VALUES (?, ?, ?, ?, ?)",
        user_id,
        event,
        detail,
        ip,
        now,
    )
    .execute(pool)
    .await;
}

fn base64_url_encode(bytes: &[u8]) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    URL_SAFE_NO_PAD.encode(bytes)
}
