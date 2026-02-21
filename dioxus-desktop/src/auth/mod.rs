//! Auth context and terminal-style auth modal.
//!
//! Design: auth is a capability upgrade, not a gate.  The desktop loads
//! showing public content.  When something needs a session it sets
//! `AuthState::Required`, which renders this modal over the desktop.
//! Hard-cut dismiss on success.
//!
//! The modal is a full-screen dark scrim with a terminal prompt at the
//! bottom.  One field at a time.  Lines scroll up as each field is
//! committed.  The winking tell: the cursor hesitates exactly once
//! before the passkey dialog fires, as if the system is deciding
//! whether to bother.

pub mod passkey;

use dioxus::prelude::*;
use gloo_net::http::Request;
use serde::Deserialize;
use wasm_bindgen::JsValue;
use wasm_bindgen::JsCast;

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
/// Called at startup from DesktopShell so the rest of the app knows
/// whether there's already a session without blocking render.
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

// ── Terminal history line ─────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
struct HistoryLine {
    prompt: &'static str,
    value: String,
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

    fn mode_label(self) -> &'static str {
        match self {
            AuthIntent::Login => "Log in",
            AuthIntent::Register => "Sign up",
        }
    }

    fn helper_copy(self) -> &'static str {
        match self {
            AuthIntent::Login => "Enter your email and complete your passkey login.",
            AuthIntent::Register => "Create an account with your email and a new passkey.",
        }
    }

    fn switch_label(self) -> &'static str {
        match self {
            AuthIntent::Login => "Need an account? Sign up",
            AuthIntent::Register => "Already have an account? Log in",
        }
    }

    fn switched(self) -> Self {
        match self {
            AuthIntent::Login => AuthIntent::Register,
            AuthIntent::Register => AuthIntent::Login,
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

/// Renders over the desktop when auth is required.
/// Consumes the `Signal<AuthState>` context provided by `DesktopShell`.
#[component]
pub fn AuthModal() -> Element {
    let mut auth = use_context::<Signal<AuthState>>();

    // Only render when Required
    if *auth.read() != AuthState::Required {
        return rsx! {};
    }

    // committed lines scrolled above the current input
    let mut history = use_signal(Vec::<HistoryLine>::new);
    // current input value
    let mut input_value = use_signal(String::new);
    let mut intent = use_signal(auth_intent_from_url);
    // which step we're on
    let mut step = use_signal(|| Step::Email);
    // error to show on the current line
    let mut error = use_signal(|| None::<String>);
    // are we waiting for the passkey dialog / network?
    let busy = use_signal(|| false);

    // auto-focus the input whenever it renders
    let input_id = "auth-terminal-input";

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
                        error.set(Some("enter a valid email address".to_string()));
                        return;
                    }
                    // commit the email line, move to passkey
                    history.write().push(HistoryLine {
                        prompt: "email:",
                        value: val.clone(),
                    });
                    input_value.set(String::new());
                    error.set(None);
                    step.set(Step::Passkey(val));
                }
                Step::Passkey(ref username) => {
                    // shouldn't be reachable — passkey fires automatically
                    let _ = username;
                }
            }
        }
    };

    // When step transitions to Passkey, fire the ceremony automatically.
    {
        let step_val = step.read().clone();
        if let Step::Passkey(ref username) = step_val {
            let username = username.clone();
            use_effect(move || {
                let username = username.clone();
                let flow = *intent.read();
                spawn(async move {
                    run_passkey_ceremony(username, flow, busy, error, history, step, auth).await;
                });
            });
        }
    }

    let current_prompt = match &*step.read() {
        Step::Email => "email:",
        Step::Passkey(_) => "",
    };

    let show_input = !matches!(*step.read(), Step::Passkey(_)) || !*busy.read();

    rsx! {
        // scrim
        div {
            "data-testid": "auth-modal",
            style: "
                position: fixed; inset: 0; z-index: 9999;
                background: rgba(0,0,0,0.82);
                display: flex; flex-direction: column; justify-content: flex-end;
                font-family: 'Menlo', 'Consolas', 'Monaco', monospace;
                font-size: 14px; line-height: 1.6;
                color: #e2e8f0;
                padding: 0 0 3.5rem 0;
            ",
            // click-outside to dismiss (if they decide not to auth)
            onclick: move |_| {
                if !*busy.read() {
                    // only dismiss if we're not mid-ceremony
                    auth.set(AuthState::Unauthenticated);
                }
            },

            // terminal lines container — bottom-anchored, scrolls up
            div {
                style: "
                    display: flex; flex-direction: column; justify-content: flex-end;
                    padding: 2rem 3rem;
                    gap: 0;
                    max-height: 100vh;
                    overflow: hidden;
                ",
                // stop click-through on the terminal content
                onclick: move |e| e.stop_propagation(),

                div {
                    style: "display:flex; gap: 1rem; align-items: baseline; margin-bottom: 0.35rem;",
                    strong {
                        "data-testid": "auth-mode-label",
                        style: "font-size: 15px; letter-spacing: 0.01em; color: #f8fafc;",
                        "{intent.read().mode_label()}"
                    }
                    span {
                        style: "font-size: 12px; color: #94a3b8;",
                        "{intent.read().helper_copy()}"
                    }
                }

                button {
                    "data-testid": "auth-mode-switch",
                    r#type: "button",
                    style: "
                        align-self: flex-start;
                        margin-bottom: 0.85rem;
                        background: transparent;
                        border: none;
                        color: #93c5fd;
                        cursor: pointer;
                        font: inherit;
                        padding: 0;
                    ",
                    onclick: move |_| {
                        if *busy.read() {
                            return;
                        }
                        let next_intent = intent.read().switched();
                        intent.set(next_intent);
                        set_auth_path(next_intent);
                        history.set(Vec::new());
                        input_value.set(String::new());
                        error.set(None);
                        step.set(Step::Email);
                    },
                    "{intent.read().switch_label()}"
                }

                // committed history lines
                for line in history.read().iter() {
                    div {
                        style: "display: flex; gap: 1ch; opacity: 0.45;",
                        span { style: "color: #64748b; user-select: none;", "{line.prompt}" }
                        span { "{line.value}" }
                    }
                }

                // error line if any
                if let Some(err) = error.read().as_deref() {
                    div {
                        "data-testid": "auth-error",
                        style: "color: #f87171; margin-top: 0.15rem; font-size: 12px; opacity: 0.8;",
                        "{err}"
                    }
                }

                // current input line
                if show_input {
                    div {
                        style: "display: flex; gap: 1ch; align-items: baseline; margin-top: 0.15rem;",
                        onclick: move |e| e.stop_propagation(),
                        span { style: "color: #64748b; user-select: none;", "{current_prompt}" }
                        input {
                            id: input_id,
                            "data-testid": "auth-input",
                            r#type: "text",
                            value: "{input_value}",
                            autocomplete: "email",
                            spellcheck: false,
                            style: "
                                background: transparent; border: none; outline: none;
                                color: #e2e8f0; font: inherit; caret-color: #e2e8f0;
                                min-width: 16ch; flex: 1;
                                padding: 0; margin: 0;
                            ",
                            oninput: move |e| input_value.set(e.value()),
                            onkeydown: on_keydown,
                        }
                    }
                }

                // busy: show a blinking cursor line while the passkey dialog is open
                if *busy.read() {
                    div {
                        "data-testid": "auth-busy",
                        style: "margin-top: 0.15rem; display: flex; gap: 1ch;",
                        span { style: "color: #64748b;", "·" }
                        span {
                            style: "
                                display: inline-block; width: 8px; height: 1em;
                                background: #e2e8f0; vertical-align: text-bottom;
                                animation: blink 1.1s step-end infinite;
                            ",
                        }
                    }
                }
            }
        }

        // blink keyframe injected once
        style { {BLINK_CSS} }
    }
}

// ── Step state ────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum Step {
    Email,
    Passkey(String), // email
}

// ── Passkey ceremony ──────────────────────────────────────────────────────────

async fn run_passkey_ceremony(
    email: String,
    flow: AuthIntent,
    mut busy: Signal<bool>,
    mut error: Signal<Option<String>>,
    history: Signal<Vec<HistoryLine>>,
    step: Signal<Step>,
    auth: Signal<AuthState>,
) {
    busy.set(true);
    error.set(None);

    match flow {
        AuthIntent::Login => {
            run_login_ceremony(email, busy, error, history, step, auth).await;
        }
        AuthIntent::Register => {
            run_register_ceremony(email, busy, error, history, step, auth).await;
        }
    }
}

async fn run_login_ceremony(
    email: String,
    mut busy: Signal<bool>,
    mut error: Signal<Option<String>>,
    mut history: Signal<Vec<HistoryLine>>,
    mut step: Signal<Step>,
    mut auth: Signal<AuthState>,
) {
    let start_res = Request::post("/auth/login/start")
        .json(&serde_json::json!({ "username": email }))
        .unwrap()
        .send()
        .await;

    let start_body = match start_res {
        Ok(r) if r.ok() => match r.text().await {
            Ok(t) => t,
            Err(e) => {
                finish_with_error(
                    format!("network error: {e}"),
                    &mut busy,
                    &mut error,
                    &mut step,
                );
                return;
            }
        },
        Ok(r) if r.status() == 404 => {
            finish_with_error(
                "no account found for this email; use sign up first".to_string(),
                &mut busy,
                &mut error,
                &mut step,
            );
            return;
        }
        Ok(r) => {
            let txt = r.text().await.unwrap_or_default();
            finish_with_error(txt, &mut busy, &mut error, &mut step);
            return;
        }
        Err(e) => {
            finish_with_error(
                format!("network error: {e}"),
                &mut busy,
                &mut error,
                &mut step,
            );
            return;
        }
    };

    let cred_json = match passkey::get_credential(&start_body).await {
        Ok(j) => j,
        Err(e) => {
            finish_with_error(e, &mut busy, &mut error, &mut step);
            return;
        }
    };

    let finish_res = Request::post("/auth/login/finish")
        .header("Content-Type", "application/json")
        .body(cred_json)
        .unwrap()
        .send()
        .await;

    match finish_res {
        Ok(r) if r.ok() => {
            history.write().push(HistoryLine {
                prompt: "email:",
                value: email.clone(),
            });
            auth.set(AuthState::Authenticated(AuthUser {
                user_id: String::new(),
                username: email,
            }));
        }
        Ok(r) => {
            let txt = r.text().await.unwrap_or_default();
            finish_with_error(txt, &mut busy, &mut error, &mut step);
        }
        Err(e) => {
            finish_with_error(
                format!("network error: {e}"),
                &mut busy,
                &mut error,
                &mut step,
            );
        }
    }
}

async fn run_register_ceremony(
    email: String,
    mut busy: Signal<bool>,
    mut error: Signal<Option<String>>,
    mut history: Signal<Vec<HistoryLine>>,
    mut step: Signal<Step>,
    mut auth: Signal<AuthState>,
) {
    let display_name = email
        .split('@')
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or("user")
        .to_string();

    let start_res = Request::post("/auth/register/start")
        .json(
            &serde_json::json!({ "username": email.clone(), "display_name": display_name }),
        )
        .unwrap()
        .send()
        .await;

    let start_body = match start_res {
        Ok(r) if r.ok() => match r.text().await {
            Ok(t) => t,
            Err(e) => {
                finish_with_error(
                    format!("network error: {e}"),
                    &mut busy,
                    &mut error,
                    &mut step,
                );
                return;
            }
        },
        Ok(r) => {
            let txt = r.text().await.unwrap_or_default();
            finish_with_error(txt, &mut busy, &mut error, &mut step);
            return;
        }
        Err(e) => {
            finish_with_error(
                format!("network error: {e}"),
                &mut busy,
                &mut error,
                &mut step,
            );
            return;
        }
    };

    let cred_json = match passkey::create_credential(&start_body).await {
        Ok(j) => j,
        Err(e) => {
            finish_with_error(e, &mut busy, &mut error, &mut step);
            return;
        }
    };

    let finish_res = Request::post("/auth/register/finish")
        .header("Content-Type", "application/json")
        .body(cred_json)
        .unwrap()
        .send()
        .await;

    match finish_res {
        Ok(r) if r.ok() => {
            history.write().push(HistoryLine {
                prompt: "email:",
                value: email.clone(),
            });
            auth.set(AuthState::Authenticated(AuthUser {
                user_id: String::new(),
                username: email,
            }));
        }
        Ok(r) => {
            let txt = r.text().await.unwrap_or_default();
            finish_with_error(txt, &mut busy, &mut error, &mut step);
        }
        Err(e) => {
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

// ── CSS ───────────────────────────────────────────────────────────────────────

const BLINK_CSS: &str = r#"
@keyframes blink {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0; }
}
"#;
