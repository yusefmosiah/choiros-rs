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
    // which step we're on
    let mut step = use_signal(|| Step::Username);
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
                Step::Username => {
                    // commit the username line, move to passkey
                    history.write().push(HistoryLine {
                        prompt: "login:",
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
                spawn(async move {
                    run_passkey_ceremony(username, busy, error, history, step, auth).await;
                });
            });
        }
    }

    let current_prompt = match &*step.read() {
        Step::Username => "login:",
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
                            autocomplete: "username",
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
    Username,
    Passkey(String), // username
}

// ── Passkey ceremony ──────────────────────────────────────────────────────────

async fn run_passkey_ceremony(
    username: String,
    mut busy: Signal<bool>,
    mut error: Signal<Option<String>>,
    mut history: Signal<Vec<HistoryLine>>,
    mut step: Signal<Step>,
    mut auth: Signal<AuthState>,
) {
    busy.set(true);
    error.set(None);

    // 1. Try login/start
    let start_res = Request::post("/auth/login/start")
        .json(&serde_json::json!({ "username": username }))
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
            // Unknown user — try register/start instead
            run_register_ceremony(username, busy, error, history, step, auth).await;
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

    // 2. Call navigator.credentials.get
    let cred_json = match passkey::get_credential(&start_body).await {
        Ok(j) => j,
        Err(e) => {
            finish_with_error(e, &mut busy, &mut error, &mut step);
            return;
        }
    };

    // 3. Finish
    let finish_res = Request::post("/auth/login/finish")
        .header("Content-Type", "application/json")
        .body(cred_json)
        .unwrap()
        .send()
        .await;

    match finish_res {
        Ok(r) if r.ok() => {
            // Hard cut
            history.write().push(HistoryLine {
                prompt: "login:",
                value: username.clone(),
            });
            auth.set(AuthState::Authenticated(AuthUser {
                user_id: String::new(), // will be refreshed on next probe
                username,
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
    username: String,
    mut busy: Signal<bool>,
    mut error: Signal<Option<String>>,
    mut history: Signal<Vec<HistoryLine>>,
    mut step: Signal<Step>,
    mut auth: Signal<AuthState>,
) {
    // New user — register
    let start_res = Request::post("/auth/register/start")
        .json(
            &serde_json::json!({ "username": username.clone(), "display_name": username.clone() }),
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
                prompt: "login:",
                value: username.clone(),
            });
            auth.set(AuthState::Authenticated(AuthUser {
                user_id: String::new(),
                username,
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
    step.set(Step::Username);
}

// ── CSS ───────────────────────────────────────────────────────────────────────

const BLINK_CSS: &str = r#"
@keyframes blink {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0; }
}
"#;
