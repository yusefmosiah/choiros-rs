//! Auth context and BIOS-style auth modal.
//!
//! Design: auth is a capability upgrade, not a gate.  The desktop loads
//! showing public content.  When something needs a session it sets
//! `AuthState::Required`, which renders this modal over the desktop.
//! Hard-cut dismiss on success.
//!
//! The modal looks like an ncurses BIOS setup screen: a centered bordered
//! panel with scan-line header, tabbed mode selector, and highlighted
//! input fields.  The passkey ceremony fires automatically after email
//! entry, with POST-style status lines showing progress.

pub mod passkey;

use dioxus::prelude::*;
use gloo_net::http::Request;
use serde::Deserialize;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;

// ── Auth state ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct AuthUser {
    pub user_id: String,
    pub username: String,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum AuthState {
    /// No session known yet (haven't checked /auth/me).
    #[default]
    Unknown,
    /// Confirmed no session.
    Unauthenticated,
    /// Session exists.
    Authenticated(AuthUser),
    /// Something downstream needs a session — show the modal.
    Required,
}

impl AuthState {
    pub fn is_authenticated(&self) -> bool {
        matches!(self, AuthState::Authenticated(_))
    }
}

// ── /auth/me response ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct MeResponse {
    authenticated: bool,
    user_id: Option<String>,
    username: Option<String>,
}

/// Probe /auth/me once and update the context signal.
pub async fn probe_session(mut auth: Signal<AuthState>) {
    match Request::get("/auth/me").send().await {
        Ok(resp) if resp.ok() => {
            if let Ok(me) = resp.json::<MeResponse>().await {
                if me.authenticated {
                    auth.set(AuthState::Authenticated(AuthUser {
                        user_id: me.user_id.unwrap_or_default(),
                        username: me.username.unwrap_or_default(),
                    }));
                    return;
                }
            }
            auth.set(AuthState::Unauthenticated);
        }
        _ => {
            auth.set(AuthState::Unauthenticated);
        }
    }
}

/// Prefetch: fire a HEAD request to /health to wake the sandbox early.
/// Called when the user starts typing their email, before auth completes.
pub fn prefetch_sandbox() {
    spawn(async move {
        // Wake the sandbox so it's warm by the time auth finishes
        let _ = Request::get("/health").send().await;
    });
}

// ── Step state ────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum Step {
    Email,
    Passkey(String), // email
}

#[derive(Clone, PartialEq)]
struct StatusLine {
    text: String,
    state: LineState,
}

#[derive(Clone, Copy, PartialEq)]
enum LineState {
    Ok,
    Working,
    Failed,
}

#[derive(Clone, Copy, PartialEq)]
enum AuthIntent {
    Login,
    Register,
}

impl AuthIntent {
    fn path(self) -> &'static str {
        match self {
            AuthIntent::Login => "/login",
            AuthIntent::Register => "/register",
        }
    }

    fn tab_label(self) -> &'static str {
        match self {
            AuthIntent::Login => " Login ",
            AuthIntent::Register => " Register ",
        }
    }

}

fn auth_intent_from_url() -> AuthIntent {
    let Some(window) = web_sys::window() else {
        return AuthIntent::Login;
    };
    let Ok(pathname) = window.location().pathname() else {
        return AuthIntent::Login;
    };
    if pathname == "/register" {
        AuthIntent::Register
    } else {
        AuthIntent::Login
    }
}

fn set_auth_path(intent: AuthIntent) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(history) = window.history() else {
        return;
    };
    let _ = history.replace_state_with_url(&JsValue::NULL, "", Some(intent.path()));
}

fn is_valid_email(value: &str) -> bool {
    let candidate = value.trim();
    let Some((local, domain)) = candidate.split_once('@') else {
        return false;
    };
    !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
}

// ── AuthModal ─────────────────────────────────────────────────────────────────

#[component]
pub fn AuthModal() -> Element {
    let mut auth = use_context::<Signal<AuthState>>();

    if *auth.read() != AuthState::Required {
        return rsx! {};
    }

    let mut input_value = use_signal(String::new);
    let mut intent = use_signal(auth_intent_from_url);
    let mut step = use_signal(|| Step::Email);
    let mut error = use_signal(|| None::<String>);
    let busy = use_signal(|| false);
    let mut status_lines = use_signal(Vec::<StatusLine>::new);
    let mut prefetch_fired = use_signal(|| false);

    let input_id = "auth-terminal-input";

    // Dismiss the BIOS boot screen once auth modal is visible
    use_effect(move || {
        if let Some(window) = web_sys::window() {
            let _ = window
                .document()
                .and_then(|doc| {
                    doc.get_element_by_id("bios-boot").map(|el| {
                        // Call __biosComplete if available
                        let _ = js_sys::eval("window.__biosComplete && window.__biosComplete()");
                        el
                    })
                });
        }
    });

    // Auto-focus
    use_effect(move || {
        if let Some(window) = web_sys::window() {
            if let Some(doc) = window.document() {
                if let Some(el) = doc.get_element_by_id(input_id) {
                    let _ = el.dyn_ref::<web_sys::HtmlElement>().map(|e| e.focus());
                }
            }
        }
    });

    let on_keydown = move |evt: KeyboardEvent| {
        if evt.key() == Key::Enter && !*busy.read() {
            let val = input_value.read().trim().to_string();
            if val.is_empty() {
                return;
            }

            let current_step = step.read().clone();
            match current_step {
                Step::Email => {
                    if !is_valid_email(&val) {
                        error.set(Some("invalid email address".to_string()));
                        return;
                    }
                    error.set(None);
                    step.set(Step::Passkey(val));
                }
                Step::Passkey(_) => {}
            }
        }
    };

    // Prefetch when user starts typing (fire once)
    let on_input = move |e: Event<FormData>| {
        let val = e.value();
        input_value.set(val.clone());
        if !*prefetch_fired.read() && val.len() >= 3 {
            prefetch_fired.set(true);
            prefetch_sandbox();
        }
    };

    // Fire passkey ceremony when step transitions
    {
        let step_val = step.read().clone();
        if let Step::Passkey(ref username) = step_val {
            let username = username.clone();
            use_effect(move || {
                let username = username.clone();
                let flow = *intent.read();
                spawn(async move {
                    run_passkey_ceremony(
                        username, flow, busy, error, status_lines, step, auth,
                    )
                    .await;
                });
            });
        }
    }

    let show_input = !matches!(*step.read(), Step::Passkey(_)) || !*busy.read();
    let current_intent = *intent.read();

    rsx! {
        div {
            "data-testid": "auth-modal",
            style: "{SCRIM_STYLE}",
            onclick: move |_| {
                if !*busy.read() {
                    auth.set(AuthState::Unauthenticated);
                }
            },

            // BIOS-style centered panel
            div {
                style: "{PANEL_STYLE}",
                onclick: move |e| e.stop_propagation(),

                // ┌─ Header bar ─┐
                div {
                    style: "{HEADER_STYLE}",
                    span { style: "color: #7aa2f7; font-weight: bold;", "ChoirOS Setup Utility v0.1.0" }
                    span { style: "color: #565f89; font-size: 11px;", "(C) 2026 ChoirOS Project" }
                }

                // ┌─ Tab bar ─┐
                div {
                    style: "display: flex; gap: 0; margin-bottom: 1rem; border-bottom: 1px solid #2a3050;",

                    button {
                        "data-testid": "auth-mode-login",
                        r#type: "button",
                        style: if current_intent == AuthIntent::Login { TAB_ACTIVE_STYLE } else { TAB_STYLE },
                        onclick: move |_| {
                            if *busy.read() { return; }
                            intent.set(AuthIntent::Login);
                            set_auth_path(AuthIntent::Login);
                            status_lines.set(Vec::new());
                            input_value.set(String::new());
                            error.set(None);
                            step.set(Step::Email);
                        },
                        "{AuthIntent::Login.tab_label()}"
                    }
                    button {
                        "data-testid": "auth-mode-register",
                        r#type: "button",
                        style: if current_intent == AuthIntent::Register { TAB_ACTIVE_STYLE } else { TAB_STYLE },
                        onclick: move |_| {
                            if *busy.read() { return; }
                            intent.set(AuthIntent::Register);
                            set_auth_path(AuthIntent::Register);
                            status_lines.set(Vec::new());
                            input_value.set(String::new());
                            error.set(None);
                            step.set(Step::Email);
                        },
                        "{AuthIntent::Register.tab_label()}"
                    }
                }

                // ┌─ Form area ─┐
                div {
                    style: "flex: 1; display: flex; flex-direction: column;",

                    // Email field row (BIOS-style: label on left, field on right)
                    div {
                        style: "{FIELD_ROW_STYLE}",
                        span { style: "color: #c0caf5; min-width: 12ch;", "Email:" }
                        if show_input {
                            input {
                                id: input_id,
                                "data-testid": "auth-input",
                                r#type: "text",
                                value: "{input_value}",
                                autocomplete: "email",
                                spellcheck: false,
                                style: "{INPUT_STYLE}",
                                oninput: on_input,
                                onkeydown: on_keydown,
                            }
                        } else {
                            span { style: "color: #7aa2f7;",
                                "{input_value}"
                            }
                        }
                    }

                    // Passkey field row
                    div {
                        style: "{FIELD_ROW_STYLE}",
                        span { style: "color: #c0caf5; min-width: 12ch;", "Passkey:" }
                        if *busy.read() {
                            span { style: "color: #e0af68;",
                                "Waiting for authenticator..."
                            }
                        } else if matches!(*step.read(), Step::Passkey(_)) {
                            span { style: "color: #565f89;", "Ready" }
                        } else {
                            span { style: "color: #565f89;", "---" }
                        }
                    }

                    // Error display
                    if let Some(err) = error.read().as_deref() {
                        div {
                            "data-testid": "auth-error",
                            style: "color: #f7768e; margin-top: 0.5rem; padding: 0.25rem 0;",
                            "Error: {err}"
                        }
                    }

                    // Status lines (POST-style progress during ceremony)
                    if !status_lines.read().is_empty() {
                        div {
                            style: "margin-top: 0.75rem; border-top: 1px solid #2a3050; padding-top: 0.5rem;",
                            for line in status_lines.read().iter() {
                                div {
                                    style: "display: flex; justify-content: space-between; padding: 1px 0;",
                                    span { style: "color: #565f89;", "{line.text}" }
                                    match line.state {
                                        LineState::Ok => rsx! {
                                            span { style: "color: #9ece6a;", "[  OK  ]" }
                                        },
                                        LineState::Working => rsx! {
                                            span { style: "color: #e0af68;", "[  ..  ]" }
                                        },
                                        LineState::Failed => rsx! {
                                            span { style: "color: #f7768e;", "[ FAIL ]" }
                                        },
                                    }
                                }
                            }
                        }
                    }
                }

                // ┌─ Footer ─┐
                div {
                    style: "{FOOTER_STYLE}",
                    span { style: "color: #565f89;",
                        "Enter = Submit    Tab = Switch Mode    Esc = Cancel"
                    }
                }
            }
        }

        style { {BIOS_CSS} }
    }
}

// ── Passkey ceremony ──────────────────────────────────────────────────────────

async fn run_passkey_ceremony(
    email: String,
    flow: AuthIntent,
    mut busy: Signal<bool>,
    mut error: Signal<Option<String>>,
    mut status_lines: Signal<Vec<StatusLine>>,
    step: Signal<Step>,
    auth: Signal<AuthState>,
) {
    busy.set(true);
    error.set(None);
    status_lines.set(Vec::new());

    match flow {
        AuthIntent::Login => {
            run_login_ceremony(email, busy, error, status_lines, step, auth).await;
        }
        AuthIntent::Register => {
            run_register_ceremony(email, busy, error, status_lines, step, auth).await;
        }
    }
}

fn push_status(status_lines: &mut Signal<Vec<StatusLine>>, text: &str, state: LineState) {
    status_lines.write().push(StatusLine {
        text: text.to_string(),
        state,
    });
}

fn update_last_status(status_lines: &mut Signal<Vec<StatusLine>>, state: LineState) {
    let mut lines = status_lines.write();
    if let Some(last) = lines.last_mut() {
        last.state = state;
    }
}

async fn run_login_ceremony(
    email: String,
    mut busy: Signal<bool>,
    mut error: Signal<Option<String>>,
    mut status_lines: Signal<Vec<StatusLine>>,
    mut step: Signal<Step>,
    mut auth: Signal<AuthState>,
) {
    push_status(&mut status_lines, "Contacting auth server . . . . . . ", LineState::Working);

    let start_res = Request::post("/auth/login/start")
        .json(&serde_json::json!({ "username": email }))
        .unwrap()
        .send()
        .await;

    let start_body = match start_res {
        Ok(r) if r.ok() => {
            update_last_status(&mut status_lines, LineState::Ok);
            match r.text().await {
                Ok(t) => t,
                Err(e) => {
                    update_last_status(&mut status_lines, LineState::Failed);
                    finish_with_error(format!("network error: {e}"), &mut busy, &mut error, &mut step);
                    return;
                }
            }
        }
        Ok(r) if r.status() == 404 => {
            update_last_status(&mut status_lines, LineState::Failed);
            finish_with_error(
                "no account found — use Register tab".to_string(),
                &mut busy,
                &mut error,
                &mut step,
            );
            return;
        }
        Ok(r) => {
            update_last_status(&mut status_lines, LineState::Failed);
            let txt = r.text().await.unwrap_or_default();
            finish_with_error(txt, &mut busy, &mut error, &mut step);
            return;
        }
        Err(e) => {
            update_last_status(&mut status_lines, LineState::Failed);
            finish_with_error(format!("network error: {e}"), &mut busy, &mut error, &mut step);
            return;
        }
    };

    push_status(&mut status_lines, "Passkey challenge . . . . . . . . ", LineState::Working);

    let cred_json = match passkey::get_credential(&start_body).await {
        Ok(j) => {
            update_last_status(&mut status_lines, LineState::Ok);
            j
        }
        Err(e) => {
            update_last_status(&mut status_lines, LineState::Failed);
            finish_with_error(e, &mut busy, &mut error, &mut step);
            return;
        }
    };

    push_status(&mut status_lines, "Verifying credential . . . . . .  ", LineState::Working);

    let finish_res = Request::post("/auth/login/finish")
        .header("Content-Type", "application/json")
        .body(cred_json)
        .unwrap()
        .send()
        .await;

    match finish_res {
        Ok(r) if r.ok() => {
            update_last_status(&mut status_lines, LineState::Ok);
            push_status(&mut status_lines, "Session established . . . . . . . ", LineState::Ok);
            auth.set(AuthState::Authenticated(AuthUser {
                user_id: String::new(),
                username: email,
            }));
        }
        Ok(r) => {
            update_last_status(&mut status_lines, LineState::Failed);
            let txt = r.text().await.unwrap_or_default();
            finish_with_error(txt, &mut busy, &mut error, &mut step);
        }
        Err(e) => {
            update_last_status(&mut status_lines, LineState::Failed);
            finish_with_error(format!("network error: {e}"), &mut busy, &mut error, &mut step);
        }
    }
}

async fn run_register_ceremony(
    email: String,
    mut busy: Signal<bool>,
    mut error: Signal<Option<String>>,
    mut status_lines: Signal<Vec<StatusLine>>,
    mut step: Signal<Step>,
    mut auth: Signal<AuthState>,
) {
    let display_name = email
        .split('@')
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or("user")
        .to_string();

    push_status(&mut status_lines, "Creating identity . . . . . . . . ", LineState::Working);

    let start_res = Request::post("/auth/register/start")
        .json(&serde_json::json!({ "username": email.clone(), "display_name": display_name }))
        .unwrap()
        .send()
        .await;

    let start_body = match start_res {
        Ok(r) if r.ok() => {
            update_last_status(&mut status_lines, LineState::Ok);
            match r.text().await {
                Ok(t) => t,
                Err(e) => {
                    update_last_status(&mut status_lines, LineState::Failed);
                    finish_with_error(
                        format!("network error: {e}"),
                        &mut busy,
                        &mut error,
                        &mut step,
                    );
                    return;
                }
            }
        }
        Ok(r) => {
            update_last_status(&mut status_lines, LineState::Failed);
            let txt = r.text().await.unwrap_or_default();
            finish_with_error(txt, &mut busy, &mut error, &mut step);
            return;
        }
        Err(e) => {
            update_last_status(&mut status_lines, LineState::Failed);
            finish_with_error(
                format!("network error: {e}"),
                &mut busy,
                &mut error,
                &mut step,
            );
            return;
        }
    };

    push_status(&mut status_lines, "Passkey enrollment  . . . . . . . ", LineState::Working);

    let cred_json = match passkey::create_credential(&start_body).await {
        Ok(j) => {
            update_last_status(&mut status_lines, LineState::Ok);
            j
        }
        Err(e) => {
            update_last_status(&mut status_lines, LineState::Failed);
            finish_with_error(e, &mut busy, &mut error, &mut step);
            return;
        }
    };

    push_status(&mut status_lines, "Registering credential . . . . .  ", LineState::Working);

    let finish_res = Request::post("/auth/register/finish")
        .header("Content-Type", "application/json")
        .body(cred_json)
        .unwrap()
        .send()
        .await;

    match finish_res {
        Ok(r) if r.ok() => {
            update_last_status(&mut status_lines, LineState::Ok);
            push_status(&mut status_lines, "Account provisioned . . . . . . . ", LineState::Ok);
            auth.set(AuthState::Authenticated(AuthUser {
                user_id: String::new(),
                username: email,
            }));
        }
        Ok(r) => {
            update_last_status(&mut status_lines, LineState::Failed);
            let txt = r.text().await.unwrap_or_default();
            finish_with_error(txt, &mut busy, &mut error, &mut step);
        }
        Err(e) => {
            update_last_status(&mut status_lines, LineState::Failed);
            finish_with_error(
                format!("network error: {e}"),
                &mut busy,
                &mut error,
                &mut step,
            );
        }
    }
}

fn finish_with_error(
    msg: String,
    busy: &mut Signal<bool>,
    error: &mut Signal<Option<String>>,
    step: &mut Signal<Step>,
) {
    busy.set(false);
    error.set(Some(msg));
    step.set(Step::Email);
}

// ── CSS constants ─────────────────────────────────────────────────────────────

const SCRIM_STYLE: &str = "
    position: fixed; inset: 0; z-index: 9999;
    background: rgba(10, 14, 26, 0.92);
    display: flex; align-items: center; justify-content: center;
    font-family: 'IBM Plex Mono', 'Menlo', 'Consolas', 'Monaco', monospace;
    font-size: 13px; line-height: 1.55;
    color: #c0c8d8;
";

const PANEL_STYLE: &str = "
    width: min(520px, 92vw);
    min-height: 320px;
    background: #0f1320;
    border: 1px solid #3b4261;
    box-shadow: 0 0 0 1px #1a1e32, 0 8px 40px rgba(0,0,0,0.6);
    display: flex; flex-direction: column;
    padding: 0;
    position: relative;
";

const HEADER_STYLE: &str = "
    display: flex; justify-content: space-between; align-items: center;
    padding: 0.5rem 1rem;
    background: #1a1e32;
    border-bottom: 1px solid #3b4261;
";

const TAB_STYLE: &str = "
    background: transparent; border: none; border-bottom: 2px solid transparent;
    color: #565f89; cursor: pointer;
    font: inherit; padding: 0.5rem 1rem;
";

const TAB_ACTIVE_STYLE: &str = "
    background: #1a1e32; border: none; border-bottom: 2px solid #7aa2f7;
    color: #c0caf5; cursor: pointer;
    font: inherit; padding: 0.5rem 1rem;
    font-weight: bold;
";

const FIELD_ROW_STYLE: &str = "
    display: flex; align-items: center; gap: 1rem;
    padding: 0.5rem 1rem;
";

const INPUT_STYLE: &str = "
    background: #1a1e32; border: 1px solid #3b4261;
    color: #c0caf5; font: inherit;
    padding: 0.3rem 0.5rem; flex: 1;
    outline: none;
    caret-color: #7aa2f7;
";

const FOOTER_STYLE: &str = "
    padding: 0.5rem 1rem;
    border-top: 1px solid #2a3050;
    margin-top: auto;
    text-align: center;
";

const BIOS_CSS: &str = r#"
@keyframes blink {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0; }
}
"#;
