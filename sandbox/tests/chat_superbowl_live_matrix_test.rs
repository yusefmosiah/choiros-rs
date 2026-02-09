//! Live matrix test for Superbowl weather query quality across chat models and search providers.
//!
//! This test is intentionally integration-heavy and requires external provider credentials.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use ractor::Actor;
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tower::ServiceExt;

use sandbox::actors::event_store::EventStoreMsg;
use sandbox::actors::{EventStoreActor, EventStoreArguments};
use sandbox::api;
use sandbox::app_state::AppState;
use sandbox::runtime_env::ensure_tls_cert_env;

#[derive(Debug, Clone)]
struct MatrixCase {
    model: String,
    provider: String,
}

#[derive(Debug, Clone, Serialize)]
struct MatrixCaseResult {
    model: String,
    provider: String,
    executed: bool,
    skipped_reason: Option<String>,
    model_honored: bool,
    selected_model: String,
    non_blocking_flow: bool,
    signal_to_answer: bool,
    final_answer_quality: bool,
    polluted_followup: bool,
    used_web_search: bool,
    used_bash: bool,
    used_search_then_bash: bool,
    final_answer: String,
}

#[derive(Debug, Clone)]
struct ProviderEnvSnapshot {
    tavily: Option<String>,
    brave: Option<String>,
    exa: Option<String>,
}

fn test_chat_id() -> String {
    format!("test-superbowl-{}", uuid::Uuid::new_v4())
}

async fn setup_test_app() -> (
    axum::Router,
    tempfile::TempDir,
    ractor::ActorRef<EventStoreMsg>,
) {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("test_events.db");
    let db_path_str = db_path.to_str().expect("Invalid database path");

    let (event_store, _handle) = Actor::spawn(
        None,
        EventStoreActor,
        EventStoreArguments::File(db_path_str.to_string()),
    )
    .await
    .expect("Failed to create event store");

    let app_state = Arc::new(AppState::new(event_store.clone()));
    let ws_sessions: sandbox::api::websocket::WsSessions =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));

    let api_state = api::ApiState {
        app_state,
        ws_sessions,
    };

    let app = api::router().with_state(api_state);
    (app, temp_dir, event_store)
}

fn load_env_from_ancestors() {
    let cwd = match std::env::current_dir() {
        Ok(dir) => dir,
        Err(_) => return,
    };
    let mut current = cwd.clone();
    loop {
        let candidate = current.join(".env");
        if candidate.exists() {
            let _ = dotenvy::from_path(candidate);
            return;
        }
        if !current.pop() {
            break;
        }
    }
}

fn has_env(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

fn capture_provider_env() -> ProviderEnvSnapshot {
    ProviderEnvSnapshot {
        tavily: std::env::var("TAVILY_API_KEY").ok(),
        brave: std::env::var("BRAVE_API_KEY").ok(),
        exa: std::env::var("EXA_API_KEY").ok(),
    }
}

fn set_or_unset_env(name: &str, value: Option<&str>) {
    if let Some(v) = value {
        std::env::set_var(name, v);
    } else {
        std::env::remove_var(name);
    }
}

fn restore_provider_env(snapshot: &ProviderEnvSnapshot) {
    set_or_unset_env("TAVILY_API_KEY", snapshot.tavily.as_deref());
    set_or_unset_env("BRAVE_API_KEY", snapshot.brave.as_deref());
    set_or_unset_env("EXA_API_KEY", snapshot.exa.as_deref());
}

fn provider_available(snapshot: &ProviderEnvSnapshot, provider: &str) -> bool {
    let has_tavily = snapshot
        .tavily
        .as_deref()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let has_brave = snapshot
        .brave
        .as_deref()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let has_exa = snapshot
        .exa
        .as_deref()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    match provider {
        "tavily" => has_tavily,
        "brave" => has_brave,
        "exa" => has_exa,
        "all" => has_tavily && has_brave && has_exa,
        "auto" => has_tavily || has_brave || has_exa,
        _ => false,
    }
}

fn force_provider_env(provider: &str, snapshot: &ProviderEnvSnapshot) -> Result<(), &'static str> {
    if !provider_available(snapshot, provider) {
        return Err("missing provider credentials");
    }
    match provider {
        "tavily" => {
            set_or_unset_env("TAVILY_API_KEY", snapshot.tavily.as_deref());
            std::env::remove_var("BRAVE_API_KEY");
            std::env::remove_var("EXA_API_KEY");
        }
        "brave" => {
            std::env::remove_var("TAVILY_API_KEY");
            set_or_unset_env("BRAVE_API_KEY", snapshot.brave.as_deref());
            std::env::remove_var("EXA_API_KEY");
        }
        "exa" => {
            std::env::remove_var("TAVILY_API_KEY");
            std::env::remove_var("BRAVE_API_KEY");
            set_or_unset_env("EXA_API_KEY", snapshot.exa.as_deref());
        }
        "all" | "auto" => {
            restore_provider_env(snapshot);
        }
        _ => return Err("unknown provider"),
    }
    Ok(())
}

fn has_bedrock_auth() -> bool {
    has_env("AWS_BEARER_TOKEN_BEDROCK")
        || has_env("AWS_PROFILE")
        || (has_env("AWS_ACCESS_KEY_ID") && has_env("AWS_SECRET_ACCESS_KEY"))
}

fn bedrock_tls_ready() -> bool {
    ensure_tls_cert_env().is_some()
}

fn model_check(model: &str) -> Result<(), &'static str> {
    match model {
        "ClaudeBedrockOpus46"
        | "ClaudeBedrockOpus45"
        | "ClaudeBedrockSonnet45"
        | "ClaudeBedrockHaiku45" => {
            if has_bedrock_auth() && bedrock_tls_ready() {
                Ok(())
            } else {
                Err("missing Bedrock credentials or TLS cert bundle")
            }
        }
        "KimiK25" | "KimiK25Fallback" => {
            if has_env("ANTHROPIC_API_KEY") {
                Ok(())
            } else {
                Err("missing ANTHROPIC_API_KEY")
            }
        }
        "ZaiGLM47" | "ZaiGLM47Flash" | "ZaiGLM47Air" => {
            if has_env("ZAI_API_KEY") {
                Ok(())
            } else {
                Err("missing ZAI_API_KEY")
            }
        }
        _ => Err("model not configured in matrix harness"),
    }
}

fn normalized_keywords(text: &str) -> std::collections::HashSet<String> {
    const STOPWORDS: &[&str] = &[
        "a",
        "an",
        "the",
        "and",
        "or",
        "but",
        "if",
        "then",
        "else",
        "is",
        "are",
        "was",
        "were",
        "be",
        "been",
        "being",
        "to",
        "of",
        "in",
        "on",
        "at",
        "for",
        "from",
        "by",
        "with",
        "about",
        "as",
        "into",
        "through",
        "after",
        "before",
        "between",
        "it",
        "its",
        "this",
        "that",
        "these",
        "those",
        "i",
        "you",
        "we",
        "they",
        "he",
        "she",
        "them",
        "our",
        "your",
        "my",
        "me",
        "what",
        "which",
        "who",
        "whom",
        "when",
        "where",
        "why",
        "how",
        "can",
        "could",
        "should",
        "would",
        "will",
        "just",
        "please",
        "today",
        "tomorrow",
        "yesterday",
    ];
    let mut set = std::collections::HashSet::new();
    for raw in text.split_whitespace() {
        let token = raw
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        if token.len() < 4 || STOPWORDS.contains(&token.as_str()) {
            continue;
        }
        set.insert(token);
    }
    set
}

fn looks_like_final_answer_for_prompt(prompt: &str, text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let uncertainty_hit = lower.contains("could you clarify")
        || lower.contains("which one do you mean")
        || lower.contains("i need more information")
        || lower.contains("i'm searching")
        || lower.contains("running in the background")
        || lower.contains("would you like me to search")
        || lower.contains("does not yet directly answer the objective")
        || lower.contains("could not complete that request yet");
    let tool_dump_hit = lower.starts_with("research results for '")
        || lower.starts_with("research results for \"")
        || lower.contains(" via tavily:")
        || lower.contains(" via brave:")
        || lower.contains(" via exa:")
        || lower.contains("\n- https://")
        || lower.contains("\n- http://");
    if uncertainty_hit || tool_dump_hit || lower.trim().is_empty() {
        return false;
    }
    let prompt_keywords = normalized_keywords(prompt);
    if prompt_keywords.is_empty() {
        return true;
    }
    let answer_keywords = normalized_keywords(text);
    prompt_keywords
        .iter()
        .any(|keyword| answer_keywords.contains(keyword))
}

fn matrix_profile() -> String {
    std::env::var("CHOIR_SUPERBOWL_MATRIX_PROFILE")
        .unwrap_or_else(|_| "fast".to_string())
        .trim()
        .to_ascii_lowercase()
}

async fn scoped_events(
    event_store: &ractor::ActorRef<EventStoreMsg>,
    actor_id: &str,
    session_id: &str,
    thread_id: &str,
) -> Vec<shared_types::Event> {
    ractor::call!(event_store, |reply| {
        EventStoreMsg::GetEventsForActorWithScope {
            actor_id: actor_id.to_string(),
            session_id: session_id.to_string(),
            thread_id: thread_id.to_string(),
            since_seq: 0,
            reply,
        }
    })
    .ok()
    .and_then(|v| v.ok())
    .unwrap_or_default()
}

async fn run_case(
    app: &axum::Router,
    event_store: &ractor::ActorRef<EventStoreMsg>,
    case: &MatrixCase,
    provider_env: &ProviderEnvSnapshot,
) -> MatrixCaseResult {
    if let Err(reason) = model_check(&case.model) {
        return MatrixCaseResult {
            model: case.model.clone(),
            provider: case.provider.clone(),
            executed: false,
            skipped_reason: Some(reason.to_string()),
            model_honored: false,
            selected_model: String::new(),
            non_blocking_flow: false,
            signal_to_answer: false,
            final_answer_quality: false,
            polluted_followup: false,
            used_web_search: false,
            used_bash: false,
            used_search_then_bash: false,
            final_answer: String::new(),
        };
    }
    if let Err(reason) = force_provider_env(&case.provider, provider_env) {
        return MatrixCaseResult {
            model: case.model.clone(),
            provider: case.provider.clone(),
            executed: false,
            skipped_reason: Some(reason.to_string()),
            model_honored: false,
            selected_model: String::new(),
            non_blocking_flow: false,
            signal_to_answer: false,
            final_answer_quality: false,
            polluted_followup: false,
            used_web_search: false,
            used_bash: false,
            used_search_then_bash: false,
            final_answer: String::new(),
        };
    }

    let chat_id = test_chat_id();
    let session_id = format!("session:{chat_id}");
    let thread_id = format!("thread:{chat_id}");

    let prompt_text = std::env::var("CHOIR_SUPERBOWL_MATRIX_PROMPT")
        .unwrap_or_else(|_| "As of today, whats the weather for the superbowl?".to_string());
    let message_req = json!({
        "actor_id": chat_id,
        "user_id": "test-user",
        "text": prompt_text,
        "model": case.model,
        "session_id": session_id,
        "thread_id": thread_id
    });

    let req = Request::builder()
        .method("POST")
        .uri("/chat/send")
        .header("content-type", "application/json")
        .body(Body::from(message_req.to_string()))
        .expect("build /chat/send request");
    let response = app.clone().oneshot(req).await.expect("send chat request");
    if response.status() != StatusCode::OK {
        return MatrixCaseResult {
            model: case.model.clone(),
            provider: case.provider.clone(),
            executed: true,
            skipped_reason: Some(format!(
                "chat send failed with status {}",
                response.status()
            )),
            model_honored: false,
            selected_model: String::new(),
            non_blocking_flow: false,
            signal_to_answer: false,
            final_answer_quality: false,
            polluted_followup: false,
            used_web_search: false,
            used_bash: false,
            used_search_then_bash: false,
            final_answer: String::new(),
        };
    }

    let default_case_timeout_ms = if matrix_profile() == "full" {
        60_000
    } else {
        35_000
    };
    let case_timeout_ms = std::env::var("CHOIR_SUPERBOWL_CASE_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default_case_timeout_ms)
        .clamp(10_000, 180_000);
    let deadline = Instant::now() + Duration::from_millis(case_timeout_ms);
    let mut final_events = Vec::new();
    let mut last_event_count = 0usize;
    let mut last_change = Instant::now();
    while Instant::now() < deadline {
        let events = scoped_events(event_store, &chat_id, &session_id, &thread_id).await;
        if events.len() != last_event_count {
            last_event_count = events.len();
            last_change = Instant::now();
        }
        let research_done_seq = events
            .iter()
            .filter(|e| {
                e.event_type == shared_types::EVENT_TOPIC_RESEARCH_TASK_COMPLETED
                    || e.event_type == shared_types::EVENT_TOPIC_RESEARCH_TASK_FAILED
            })
            .map(|e| e.seq)
            .max();
        let has_post_research_assistant = research_done_seq.is_some_and(|seq| {
            events
                .iter()
                .any(|e| e.event_type == shared_types::EVENT_CHAT_ASSISTANT_MSG && e.seq > seq)
        });
        let saw_tool_or_worker_activity = events.iter().any(|e| {
            e.event_type == shared_types::EVENT_CHAT_TOOL_CALL
                || e.event_type == shared_types::EVENT_CHAT_TOOL_RESULT
                || e.event_type == shared_types::EVENT_TOPIC_WORKER_TASK_STARTED
                || e.event_type == shared_types::EVENT_TOPIC_WORKER_TASK_PROGRESS
                || e.event_type == shared_types::EVENT_TOPIC_WORKER_TASK_COMPLETED
                || e.event_type == shared_types::EVENT_TOPIC_WORKER_TASK_FAILED
                || e.event_type == shared_types::EVENT_TOPIC_RESEARCH_TASK_STARTED
                || e.event_type == shared_types::EVENT_TOPIC_RESEARCH_TASK_PROGRESS
                || e.event_type == shared_types::EVENT_TOPIC_RESEARCH_TASK_COMPLETED
                || e.event_type == shared_types::EVENT_TOPIC_RESEARCH_TASK_FAILED
        });
        if has_post_research_assistant {
            final_events = events;
            break;
        }
        // After research completion, allow a longer quiet window so followup
        // synthesis/tool chaining can land before we snapshot the run.
        let quiet_break_seconds = if matrix_profile() == "full" { 15 } else { 6 };
        let post_research_quiet_break_seconds = if matrix_profile() == "full" { 30 } else { 14 };
        let quiet_break_limit = if research_done_seq.is_some() {
            post_research_quiet_break_seconds
        } else {
            quiet_break_seconds
        };
        if last_change.elapsed() > Duration::from_secs(quiet_break_limit)
            && (saw_tool_or_worker_activity
                || Instant::now() + Duration::from_millis(200) >= deadline)
        {
            final_events = events;
            break;
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    if final_events.is_empty() {
        final_events = scoped_events(event_store, &chat_id, &session_id, &thread_id).await;
    }

    let deferred_seq = final_events
        .iter()
        .find(|e| {
            if e.event_type != shared_types::EVENT_CHAT_TOOL_RESULT {
                return false;
            }
            e.payload
                .get("deferred")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                || e.payload
                    .get("output")
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_ascii_lowercase().contains("running in the background"))
                    .unwrap_or(false)
        })
        .map(|e| e.seq);

    let mut first_web_search_seq: Option<i64> = None;
    let mut used_web_search = false;
    let mut used_bash = false;
    let mut used_search_then_bash = false;
    for event in &final_events {
        if event.event_type != shared_types::EVENT_CHAT_TOOL_CALL {
            continue;
        }
        let tool_name = event
            .payload
            .get("tool_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if tool_name == "web_search" {
            used_web_search = true;
            if first_web_search_seq.is_none() {
                first_web_search_seq = Some(event.seq);
            }
        } else if tool_name == "bash" {
            used_bash = true;
            if first_web_search_seq.is_some_and(|seq| event.seq > seq) {
                used_search_then_bash = true;
            }
        }
    }

    let selected_model = final_events
        .iter()
        .rev()
        .find(|e| e.event_type == shared_types::EVENT_MODEL_SELECTION)
        .and_then(|e| e.payload.get("model_used"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let model_honored = !selected_model.is_empty() && selected_model == case.model;

    let research_done_seq = final_events
        .iter()
        .filter(|e| {
            e.event_type == shared_types::EVENT_TOPIC_RESEARCH_TASK_COMPLETED
                || e.event_type == shared_types::EVENT_TOPIC_RESEARCH_TASK_FAILED
        })
        .map(|e| e.seq)
        .max();

    let mut assistant_messages: Vec<(i64, String, bool, bool)> = final_events
        .iter()
        .filter(|e| e.event_type == shared_types::EVENT_CHAT_ASSISTANT_MSG)
        .map(|e| {
            (
                e.seq,
                e.payload
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                e.payload
                    .get("async_followup")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                e.payload
                    .get("deferred_status")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            )
        })
        .collect();
    assistant_messages.sort_by_key(|(seq, _, _, _)| *seq);

    let signal_to_answer = research_done_seq.is_some_and(|seq| {
        assistant_messages
            .iter()
            .any(|(msg_seq, text, async_followup, _)| {
                *msg_seq > seq
                    && (*async_followup || looks_like_final_answer_for_prompt(&prompt_text, text))
            })
    });

    let non_blocking_flow = research_done_seq
        .is_some_and(|seq| deferred_seq.is_some_and(|d| d < seq) && signal_to_answer);

    let polluted_followup = research_done_seq.is_some_and(|seq| {
        assistant_messages
            .iter()
            .any(|(msg_seq, text, async_followup, deferred_status)| {
                let lower = text.to_ascii_lowercase();
                *msg_seq > seq
                    && !*async_followup
                    && !*deferred_status
                    && (lower.contains("running in the background")
                        || lower.contains("i'm searching")
                        || lower.contains("research results for '")
                        || lower.starts_with("async research update"))
            })
    });

    let final_answer = assistant_messages
        .iter()
        .rev()
        .find(|(_, text, _, deferred_status)| {
            let lower = text.to_ascii_lowercase();
            !*deferred_status
                && !lower.starts_with("async research update")
                && !lower.contains("working on it now")
                && !lower.contains("running in the background")
                && !lower.contains("i'm searching")
        })
        .map(|(_, text, _, _)| text.clone())
        .unwrap_or_default();
    let final_answer_quality = looks_like_final_answer_for_prompt(&prompt_text, &final_answer);

    MatrixCaseResult {
        model: case.model.clone(),
        provider: case.provider.clone(),
        executed: true,
        skipped_reason: None,
        model_honored,
        selected_model,
        non_blocking_flow,
        signal_to_answer,
        final_answer_quality,
        polluted_followup,
        used_web_search,
        used_bash,
        used_search_then_bash,
        final_answer,
    }
}

#[tokio::test]
async fn test_chat_superbowl_weather_live_model_provider_matrix() {
    load_env_from_ancestors();
    let _ = ensure_tls_cert_env();
    std::env::set_var("BAML_LOG", "ERROR");
    let provider_env = capture_provider_env();
    std::env::set_var("CHOIR_DELEGATED_TOOL_SOFT_WAIT_MS", "100");
    std::env::set_var("CHOIR_RESEARCHER_AUTO_PROVIDER_MODE", "parallel");

    let (app, _temp_dir, event_store) = setup_test_app().await;

    let profile = matrix_profile();
    let mut cases = Vec::new();
    let default_models = if profile == "full" {
        "KimiK25,ZaiGLM47Flash,ZaiGLM47"
    } else {
        "KimiK25,ZaiGLM47Flash"
    };
    let default_providers = if profile == "full" {
        "auto,tavily,brave,exa,all"
    } else {
        "auto,tavily"
    };
    let models_raw = std::env::var("CHOIR_SUPERBOWL_MATRIX_MODELS")
        .unwrap_or_else(|_| default_models.to_string());
    let providers_raw = std::env::var("CHOIR_SUPERBOWL_MATRIX_PROVIDERS")
        .unwrap_or_else(|_| default_providers.to_string());
    let max_cases = std::env::var("CHOIR_SUPERBOWL_MATRIX_MAX_CASES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or_else(|| if profile == "full" { usize::MAX } else { 4 });
    let candidate_models = models_raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    let providers = providers_raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();

    for model in candidate_models {
        for provider in &providers {
            if cases.len() >= max_cases {
                break;
            }
            cases.push(MatrixCase {
                model: model.to_string(),
                provider: (*provider).to_string(),
            });
        }
        if cases.len() >= max_cases {
            break;
        }
    }

    let mut results = Vec::new();
    for case in &cases {
        let result = run_case(&app, &event_store, case, &provider_env).await;
        restore_provider_env(&provider_env);
        eprintln!(
            "CASE model={} provider={} selected_model={} honored={} executed={} non_blocking={} signal_to_answer={} final_quality={} polluted={} web_search={} bash={} search_then_bash={} skip={:?}",
            result.model,
            result.provider,
            result.selected_model,
            result.model_honored,
            result.executed,
            result.non_blocking_flow,
            result.signal_to_answer,
            result.final_answer_quality,
            result.polluted_followup,
            result.used_web_search,
            result.used_bash,
            result.used_search_then_bash,
            result.skipped_reason
        );
        if result.executed {
            eprintln!(
                "FINAL model={} provider={} => {}",
                result.model, result.provider, result.final_answer
            );
        }
        results.push(result);
    }

    let executed = results.iter().filter(|r| r.executed).collect::<Vec<_>>();
    assert!(
        !executed.is_empty(),
        "No live matrix cases executed; likely missing credentials."
    );

    let any_non_blocking = executed.iter().any(|r| r.non_blocking_flow);
    let any_signal_to_answer = executed.iter().any(|r| r.signal_to_answer);
    let any_quality = executed.iter().any(|r| r.final_answer_quality);
    let any_model_honored = executed.iter().any(|r| r.model_honored);
    let any_search_then_bash = executed.iter().any(|r| r.used_search_then_bash);
    let pollution_count = executed.iter().filter(|r| r.polluted_followup).count();
    let strict_pass_count = executed
        .iter()
        .filter(|r| {
            r.model_honored && r.non_blocking_flow && r.signal_to_answer && r.final_answer_quality
        })
        .count();

    eprintln!(
        "SUMMARY executed={} model_honored={} non_blocking={} signal_to_answer={} quality={} strict_passes={} polluted_count={} search_then_bash={}",
        executed.len(),
        any_model_honored,
        any_non_blocking,
        any_signal_to_answer,
        any_quality,
        strict_pass_count,
        pollution_count,
        any_search_then_bash
    );

    assert!(
        any_non_blocking,
        "Expected at least one non-blocking background->signal flow case."
    );
    assert!(
        any_signal_to_answer,
        "Expected at least one case with a post-completion assistant answer."
    );
    assert!(
        any_model_honored,
        "Expected at least one case where requested chat model was honored."
    );
    assert!(
        executed.iter().any(|r| r.used_web_search),
        "Expected at least one case to invoke web_search in no-hint prompt matrix."
    );
    assert_eq!(
        pollution_count, 0,
        "Expected no polluted follow-up messages after completion."
    );
}
