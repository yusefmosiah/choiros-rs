//! WebAuthn passkey ceremony via JS interop.
//!
//! web-sys doesn't expose navigator.credentials.create/get with publicKey
//! options yet, so we call them via a thin inline JS shim.  All serialisation
//! (base64url ↔ ArrayBuffer) lives in the JS side; Rust only passes/receives
//! JSON strings to keep the boundary minimal.

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

/// Inject the base64url + ceremony helpers once into the page.
/// Idempotent — guarded by `window.__choiros_webauthn_ready`.
pub fn inject_shim() {
    let js = r#"
if (!window.__choiros_webauthn_ready) {
  window.__choiros_b64u_to_buf = function(s) {
    const pad = s.length % 4 === 0 ? '' : '='.repeat(4 - s.length % 4);
    const b64 = (s + pad).replace(/-/g, '+').replace(/_/g, '/');
    return Uint8Array.from(atob(b64), c => c.charCodeAt(0)).buffer;
  };
  window.__choiros_buf_to_b64u = function(buf) {
    const bytes = new Uint8Array(buf);
    let s = '';
    bytes.forEach(b => s += String.fromCharCode(b));
    return btoa(s).replace(/\+/g,'-').replace(/\//g,'_').replace(/=/g,'');
  };
  window.__choiros_decode_pk_opts = function(opts) {
    const o = Object.assign({}, opts);
    if (o.challenge)   o.challenge   = window.__choiros_b64u_to_buf(o.challenge);
    if (o.user?.id)    o.user = Object.assign({}, o.user, { id: window.__choiros_b64u_to_buf(o.user.id) });
    if (o.excludeCredentials) o.excludeCredentials = o.excludeCredentials.map(
      c => Object.assign({}, c, { id: window.__choiros_b64u_to_buf(c.id) }));
    if (o.allowCredentials)  o.allowCredentials  = o.allowCredentials.map(
      c => Object.assign({}, c, { id: window.__choiros_b64u_to_buf(c.id) }));
    return o;
  };
  window.__choiros_encode_cred = function(cred) {
    const r = cred.response;
    const out = { id: cred.id, rawId: window.__choiros_buf_to_b64u(cred.rawId), type: cred.type, response: {} };
    if (r.attestationObject) {
      out.response.attestationObject = window.__choiros_buf_to_b64u(r.attestationObject);
      out.response.clientDataJSON    = window.__choiros_buf_to_b64u(r.clientDataJSON);
      if (r.getTransports) out.transports = r.getTransports();
      const ext = cred.getClientExtensionResults ? cred.getClientExtensionResults() : {};
      out.clientExtensionResults = ext.credProps ? { credProps: ext.credProps } : {};
    } else {
      out.response.authenticatorData = window.__choiros_buf_to_b64u(r.authenticatorData);
      out.response.clientDataJSON    = window.__choiros_buf_to_b64u(r.clientDataJSON);
      out.response.signature         = window.__choiros_buf_to_b64u(r.signature);
      if (r.userHandle) out.response.userHandle = window.__choiros_buf_to_b64u(r.userHandle);
      out.clientExtensionResults = cred.getClientExtensionResults ? cred.getClientExtensionResults() : {};
    }
    return JSON.stringify(out);
  };
  window.__choiros_webauthn_ready = true;
}
"#;
    let _ = js_sys::eval(js);
}

/// Call navigator.credentials.create with the PublicKeyCredentialCreationOptions
/// JSON returned by the server (the full `{ publicKey: {...} }` object).
/// Returns the encoded credential JSON string ready to POST to /auth/register/finish.
pub async fn create_credential(options_json: &str) -> Result<String, String> {
    inject_shim();
    let js_code = format!(
        r#"(async function() {{
            const opts = {opts};
            const pk = window.__choiros_decode_pk_opts(opts.publicKey);
            const cred = await navigator.credentials.create({{ publicKey: pk }});
            return window.__choiros_encode_cred(cred);
        }})()"#,
        opts = options_json,
    );
    let promise = js_sys::eval(&js_code).map_err(|e| format!("{e:?}"))?;
    let promise: js_sys::Promise = promise.dyn_into().map_err(|e| format!("{e:?}"))?;
    let result = JsFuture::from(promise).await.map_err(|e| {
        // Extract .name + .message from DOMException if possible
        let obj = js_sys::Object::from(e);
        let name = js_sys::Reflect::get(&obj, &JsValue::from_str("name"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default();
        let msg = js_sys::Reflect::get(&obj, &JsValue::from_str("message"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default();
        if name.is_empty() {
            msg
        } else {
            format!("{name}: {msg}")
        }
    })?;
    result
        .as_string()
        .ok_or_else(|| "unexpected non-string result".to_string())
}

/// Call navigator.credentials.get with the PublicKeyCredentialRequestOptions
/// JSON returned by the server (the full `{ publicKey: {...} }` object).
/// Returns the encoded assertion JSON string ready to POST to /auth/login/finish.
pub async fn get_credential(options_json: &str) -> Result<String, String> {
    inject_shim();
    let js_code = format!(
        r#"(async function() {{
            const opts = {opts};
            const pk = window.__choiros_decode_pk_opts(opts.publicKey);
            const cred = await navigator.credentials.get({{ publicKey: pk }});
            return window.__choiros_encode_cred(cred);
        }})()"#,
        opts = options_json,
    );
    let promise = js_sys::eval(&js_code).map_err(|e| format!("{e:?}"))?;
    let promise: js_sys::Promise = promise.dyn_into().map_err(|e| format!("{e:?}"))?;
    let result = JsFuture::from(promise).await.map_err(|e| {
        let obj = js_sys::Object::from(e);
        let name = js_sys::Reflect::get(&obj, &JsValue::from_str("name"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default();
        let msg = js_sys::Reflect::get(&obj, &JsValue::from_str("message"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default();
        if name.is_empty() {
            msg
        } else {
            format!("{name}: {msg}")
        }
    })?;
    result
        .as_string()
        .ok_or_else(|| "unexpected non-string result".to_string())
}
