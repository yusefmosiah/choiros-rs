//! Writer styles, preview helpers, and save-status rendering

use dioxus::prelude::*;
use crate::api::writer_preview;
use super::types::SaveState;
use super::logic::is_markdown;

pub const WRITER_STYLES: &str = r#"
/* ── Writer status chip ── */
.writer-status-chip {
    font-size: 0.75rem;
    padding: 0.125rem 0.375rem;
    background: var(--hover-bg);
    border-radius: 0.25rem;
    display: flex;
    align-items: center;
    gap: 0.25rem;
}

.writer-status--initializing { color: var(--text-secondary); }
.writer-status--running       { color: var(--accent-bg); }
.writer-status--waiting       { color: var(--warning-bg); }
.writer-status--completing    { color: var(--success-bg); }
.writer-status--completed     { color: var(--success-bg); }
.writer-status--failed        { color: var(--danger-bg); }
.writer-status--blocked       { color: var(--danger-bg); }

/* ── Read-only badge ── */
.writer-readonly-badge {
    font-size: 0.75rem;
    color: var(--warning-bg);
    padding: 0.125rem 0.375rem;
    background: color-mix(in srgb, var(--warning-bg) 12%, transparent);
    border-radius: 0.25rem;
}

/* ── Provenance / version source badge ── */
.writer-provenance-badge {
    font-size: 0.7rem;
    padding: 0.1rem 0.4rem;
    border-radius: 0.25rem;
    white-space: nowrap;
}

.writer-provenance--ai {
    background: color-mix(in srgb, #6366f1 15%, transparent);
    color: #818cf8;
}

.writer-provenance--user {
    background: color-mix(in srgb, var(--success-bg) 15%, transparent);
    color: var(--success-bg);
}

.writer-provenance--system {
    background: color-mix(in srgb, var(--text-muted) 15%, transparent);
    color: var(--text-muted);
}

:root[data-theme="light"] .writer-provenance--ai {
    background: color-mix(in srgb, #6366f1 12%, transparent);
    color: #4f46e5;
}

:root[data-theme="light"] .writer-provenance--user {
    background: color-mix(in srgb, var(--success-bg) 12%, transparent);
    color: var(--success-bg);
}

/* ── New-version banner ── */
.writer-new-version-banner {
    padding: 0.6rem 1rem;
    background: color-mix(in srgb, var(--accent-bg) 12%, transparent);
    color: var(--text-primary);
    font-size: 0.85rem;
    border-bottom: 1px solid var(--border-color);
    display: flex;
    align-items: center;
    justify-content: space-between;
}

/* ── Changeset panel ── */
.writer-changeset-panel {
    padding: 0.4rem 1rem;
    background: color-mix(in srgb, #6366f1 6%, transparent);
    border-bottom: 1px solid var(--border-color);
    font-size: 0.78rem;
    color: var(--text-secondary);
    max-height: 5rem;
    overflow-y: auto;
}

:root[data-theme="light"] .writer-changeset-panel {
    background: color-mix(in srgb, #6366f1 8%, var(--bg-secondary) 92%);
}

/* ── Changeset impact badges ── */
.writer-impact-badge {
    font-size: 0.65rem;
    padding: 0.05rem 0.3rem;
    border-radius: 0.2rem;
    flex-shrink: 0;
}

.writer-impact--high   { background: color-mix(in srgb, var(--danger-bg)  15%, transparent); color: #f87171; }
.writer-impact--medium { background: color-mix(in srgb, var(--warning-bg) 15%, transparent); color: #fbbf24; }
.writer-impact--low    { background: color-mix(in srgb, var(--success-bg) 12%, transparent); color: var(--success-bg); }

:root[data-theme="light"] .writer-impact--high   { color: var(--danger-text); }
:root[data-theme="light"] .writer-impact--medium { color: var(--warning-bg); }
:root[data-theme="light"] .writer-impact--low    { color: var(--success-bg); }

/* ── Overview grid ── */
.writer-overview-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 0.75rem;
    padding: 0.75rem;
    overflow-y: auto;
    flex: 1;
}

.writer-doc-card {
    border: 1px solid var(--border-color);
    border-radius: 10px;
    background: var(--bg-secondary);
    padding: 0.65rem 0.75rem;
    cursor: pointer;
    transition: border-color 0.15s, box-shadow 0.15s;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
}

.writer-doc-card:hover {
    border-color: color-mix(in srgb, var(--border-color) 50%, var(--accent-bg) 50%);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.18);
}

.writer-doc-card-title {
    font-size: 0.82rem;
    font-weight: 600;
    color: var(--text-primary);
    line-height: 1.35;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
}

.writer-doc-card-path {
    font-size: 0.7rem;
    color: var(--text-secondary);
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
}

.writer-doc-card-footer {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    margin-top: 0.1rem;
}

.writer-back-btn {
    background: transparent;
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    border-radius: 0.4rem;
    padding: 0.25rem 0.55rem;
    font-size: 0.72rem;
    cursor: pointer;
}

.writer-back-btn:hover {
    border-color: color-mix(in srgb, var(--border-color) 60%, var(--accent-bg) 40%);
    color: var(--text-primary);
}
"#;

/// Update the preview HTML
pub async fn update_preview(
    content: String,
    mime: &str,
    path: &str,
    preview_html: &mut Signal<String>,
) {
    if !is_markdown(mime, path) {
        preview_html.set(format!("<pre>{}</pre>", html_escape(&content)));
        return;
    }

    match writer_preview(Some(&content), Some(path)).await {
        Ok(response) => {
            preview_html.set(response.html);
        }
        Err(e) => {
            dioxus_logger::tracing::error!("Preview failed: {}", e);
            preview_html.set(format!("<pre>Error rendering preview: {}</pre>", e));
        }
    }
}

/// Simple HTML escape for non-markdown files
pub fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Render save status indicator
pub fn render_save_status(save_state: &SaveState, on_dismiss_saved: EventHandler<()>) -> Element {
    match save_state {
        SaveState::Clean => rsx! {
            span { style: "font-size: 0.875rem; color: var(--text-secondary);", "" }
        },
        SaveState::Dirty => rsx! {
            span { style: "font-size: 0.875rem; color: var(--warning-bg);", "Modified" }
        },
        SaveState::Saving => rsx! {
            span { style: "font-size: 0.875rem; color: var(--accent-bg);", "Saving..." }
        },
        SaveState::Saved => rsx! {
            div {
                style: "display: flex; align-items: center; gap: 0.5rem;",
                span { style: "font-size: 0.875rem; color: var(--success-bg);", "Saved" }
                button {
                    style: "background: transparent; border: none; color: var(--text-secondary); cursor: pointer; font-size: 0.75rem;",
                    onclick: move |_| on_dismiss_saved.call(()),
                    "Dismiss"
                }
            }
        },
        SaveState::Conflict { .. } => rsx! {
            span { style: "font-size: 0.875rem; color: var(--danger-text); font-weight: bold;", "CONFLICT!" }
        },
        SaveState::Error(_) => rsx! {
            span { style: "font-size: 0.875rem; color: var(--danger-text);", "Error" }
        },
    }
}
