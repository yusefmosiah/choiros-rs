//! Writer styles and save-status rendering

use dioxus::prelude::*;

use super::types::SaveState;

pub const WRITER_STYLES: &str = r#"
/* ── Toolbar layout ── */
.writer-toolbar {
    display: flex;
    align-items: center;
    padding: 0.3rem 0.6rem;
    background: var(--titlebar-bg);
    border-bottom: 1px solid var(--border-color);
    flex-shrink: 0;
    gap: 0.25rem;
    min-height: 0;
    overflow: hidden;
}

.writer-toolbar-left {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    flex-shrink: 1;
    min-width: 0;
    overflow: hidden;
}

.writer-toolbar-center {
    display: flex;
    align-items: center;
    gap: 0.2rem;
    flex-shrink: 1;
    min-width: 0;
}

.writer-toolbar-spacer {
    flex: 1;
    min-width: 0.25rem;
}

.writer-toolbar-right {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    flex-shrink: 0;
}

.writer-toolbar-secondary {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    flex-shrink: 1;
}

@media (max-width: 520px) {
    .writer-toolbar-secondary {
        display: none;
    }
    .writer-toolbar-center {
        display: none;
    }
}

.writer-toolbar-btn {
    background: transparent;
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    cursor: pointer;
    padding: 0.25rem 0.55rem;
    border-radius: 0.375rem;
    font-size: 0.8rem;
    white-space: nowrap;
    flex-shrink: 0;
}

.writer-toolbar-btn:hover:not(:disabled) {
    border-color: color-mix(in srgb, var(--border-color) 60%, var(--accent-bg) 40%);
    color: var(--text-primary);
}

.writer-toolbar-btn:disabled {
    cursor: not-allowed;
    opacity: 0.5;
}

.writer-toolbar-btn-accent {
    background: var(--accent-bg);
    border: none;
    color: var(--accent-text);
    cursor: pointer;
    padding: 0.25rem 0.55rem;
    border-radius: 0.375rem;
    font-size: 0.8rem;
    white-space: nowrap;
    flex-shrink: 0;
    font-weight: 500;
}

.writer-toolbar-btn-accent:disabled {
    cursor: not-allowed;
    opacity: 0.5;
}

.writer-path-label {
    font-size: 0.78rem;
    color: var(--text-secondary);
    max-width: 180px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex-shrink: 1;
}

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

.writer-readonly-badge {
    font-size: 0.75rem;
    color: var(--warning-bg);
    padding: 0.125rem 0.375rem;
    background: color-mix(in srgb, var(--warning-bg) 12%, transparent);
    border-radius: 0.25rem;
}

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

.writer-impact-badge {
    font-size: 0.65rem;
    padding: 0.05rem 0.3rem;
    border-radius: 0.2rem;
    flex-shrink: 0;
}

.writer-impact--high   { background: color-mix(in srgb, var(--danger-bg) 15%, transparent); color: #f87171; }
.writer-impact--medium { background: color-mix(in srgb, var(--warning-bg) 15%, transparent); color: #fbbf24; }
.writer-impact--low    { background: color-mix(in srgb, var(--success-bg) 12%, transparent); color: var(--success-bg); }

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

.writer-doc-card-footer {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    margin-top: 0.1rem;
    flex-wrap: wrap;
}

.writer-doc-card-meta {
    font-size: 0.65rem;
    color: var(--text-muted, var(--text-secondary));
    opacity: 0.8;
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

/* ── Marginalia layout ── */
.writer-layout {
    display: grid;
    grid-template-columns: 180px 1fr 180px;
    grid-template-rows: 1fr;
    flex: 1;
    overflow: hidden;
}

.writer-margin {
    overflow-y: auto;
    padding: 0.5rem;
    background: color-mix(in srgb, var(--bg-secondary) 95%, transparent);
    border-right: 1px solid var(--border-color);
}

.writer-margin-right {
    border-right: none;
    border-left: 1px solid var(--border-color);
}

.writer-margin-empty {
    font-size: 0.74rem;
    color: var(--text-secondary);
    opacity: 0.8;
    padding: 0.5rem;
}

.writer-margin-card {
    border-left: 2px solid var(--accent-bg);
    padding: 0.4rem 0.5rem;
    margin-bottom: 0.4rem;
    font-size: 0.72rem;
    color: var(--text-secondary);
    position: relative;
    background: color-mix(in srgb, var(--bg-secondary) 92%, transparent);
}

.writer-margin-card::after {
    content: "";
    position: absolute;
    right: -16px;
    width: 16px;
    height: 1px;
    background: var(--border-color);
    top: 50%;
}

.writer-margin-card-actions {
    display: flex;
    gap: 0.35rem;
    margin-top: 0.35rem;
}

.writer-margin-card-btn {
    background: transparent;
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    cursor: pointer;
    border-radius: 0.25rem;
    padding: 0.1rem 0.35rem;
    font-size: 0.68rem;
}

.writer-margin-card-btn:hover {
    color: var(--text-primary);
}

.writer-prose-column {
    display: flex;
    flex-direction: column;
    min-width: 0;
    overflow: hidden;
    position: relative;
}

.writer-prose-container {
    flex: 1;
    overflow: auto;
    position: relative;
    padding: 0.8rem;
}

.writer-prose-body {
    max-width: 680px;
    min-height: calc(100% - 1rem);
    margin: 0 auto;
    padding: 1rem;
    border: 1px solid var(--border-color);
    border-radius: 0.5rem;
    background: var(--window-bg);
    color: var(--text-primary);
    line-height: 1.65;
    font-size: 0.95rem;
    outline: none;
}

.writer-prose-body:focus {
    border-color: color-mix(in srgb, var(--accent-bg) 50%, var(--border-color) 50%);
}

.writer-prose-body h1,
.writer-prose-body h2,
.writer-prose-body h3 {
    margin: 0.35rem 0 0.7rem;
    line-height: 1.2;
}

.writer-prose-body p {
    margin: 0.35rem 0 0.8rem;
}

.writer-note-toggle {
    position: absolute;
    top: 0.8rem;
    right: 0.8rem;
    z-index: 11;
    border: 1px solid var(--border-color);
    background: color-mix(in srgb, var(--bg-secondary) 90%, transparent);
    color: var(--text-secondary);
    border-radius: 999px;
    padding: 0.18rem 0.45rem;
    font-size: 0.72rem;
    cursor: pointer;
    display: none;
}

.writer-bubble {
    position: absolute;
    right: 4px;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: color-mix(in srgb, var(--accent-bg) 40%, transparent);
    border: none;
    font-size: 10px;
    cursor: pointer;
    transition: opacity 0.2s;
    z-index: 10;
    display: none;
}

.writer-prose-container:focus-within .writer-bubble {
    opacity: 0.15;
}

.writer-bottom-sheet-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.35);
    z-index: 90;
}

.writer-bottom-sheet {
    position: fixed;
    bottom: 0;
    left: 0;
    right: 0;
    max-height: 60vh;
    background: var(--bg-secondary);
    border-top: 1px solid var(--border-color);
    border-radius: 12px 12px 0 0;
    padding: 1rem;
    overflow-y: auto;
    z-index: 100;
}

@media (max-width: 900px) {
    .writer-layout {
        grid-template-columns: 0 1fr 180px;
    }
    .writer-margin-left {
        overflow: hidden;
        width: 0;
        padding: 0;
        border-right: none;
    }
    .writer-margin-right {
        position: absolute;
        top: 0;
        right: 0;
        bottom: 0;
        width: 180px;
        transform: translateX(100%);
        transition: transform 0.2s ease;
        z-index: 20;
        background: var(--bg-secondary);
    }
    .writer-margin-right.is-open {
        transform: translateX(0%);
    }
    .writer-note-toggle {
        display: inline-flex;
        align-items: center;
        gap: 0.2rem;
    }
}

@media (max-width: 640px) {
    .writer-layout {
        grid-template-columns: 0 1fr 0;
    }
    .writer-margin-right {
        display: none;
    }
    .writer-note-toggle {
        display: none;
    }
    .writer-bubble {
        display: block;
    }
}
"#;

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
