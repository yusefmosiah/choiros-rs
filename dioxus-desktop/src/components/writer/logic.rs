//! Writer pure logic functions — no RSX, no signals

use crate::api::WriterOverlay;
use shared_types::PatchOp;

pub fn build_diff_ops(base: &str, edited: &str) -> Vec<PatchOp> {
    if base == edited {
        return Vec::new();
    }
    vec![
        PatchOp::Delete {
            pos: 0,
            len: base.chars().count() as u64,
        },
        PatchOp::Insert {
            pos: 0,
            text: edited.to_string(),
        },
    ]
}

pub fn extract_run_id_from_document_path(path: &str) -> Option<String> {
    let mut parts = path.split('/');
    match (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) {
        (Some("conductor"), Some("runs"), Some(run_id), Some("draft.md"), None) => {
            Some(run_id.to_string())
        }
        _ => None,
    }
}

pub fn overlay_note_lines(overlay: &WriterOverlay) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!("> [{}/{}]", overlay.author, overlay.kind));
    for op in &overlay.diff_ops {
        match op {
            PatchOp::Insert { text, .. } => {
                if !text.trim().is_empty() {
                    lines.push(format!("> + {}", text.trim()));
                }
            }
            PatchOp::Delete { len, .. } => {
                lines.push(format!("> - remove {} chars", len));
            }
            PatchOp::Replace { len, text, .. } => {
                if text.trim().is_empty() {
                    lines.push(format!("> ~ rewrite {} chars", len));
                } else {
                    lines.push(format!("> ~ rewrite {} chars -> {}", len, text.trim()));
                }
            }
            PatchOp::Retain { .. } => {}
        }
    }
    lines
}

pub fn normalize_version_ids(mut ids: Vec<u64>) -> Vec<u64> {
    ids.sort_unstable();
    ids.dedup();
    ids
}

pub fn selected_version_index(ids: &[u64], selected_version_id: Option<u64>) -> Option<usize> {
    if ids.is_empty() {
        return None;
    }
    selected_version_id
        .and_then(|id| ids.iter().position(|v| *v == id))
        .or(Some(ids.len() - 1))
}

pub fn reconcile_selected_version_id(ids: &[u64], selected_version_id: Option<u64>) -> Option<u64> {
    selected_version_index(ids, selected_version_id).and_then(|idx| ids.get(idx).copied())
}

/// Apply patch operations to content
pub fn apply_patch_ops(content: &str, ops: &[PatchOp]) -> String {
    let mut chars: Vec<char> = content.chars().collect();

    for op in ops {
        match op {
            PatchOp::Insert { pos, text } => {
                let pos = (*pos as usize).min(chars.len());
                let insert_chars: Vec<char> = text.chars().collect();
                chars.splice(pos..pos, insert_chars);
            }
            PatchOp::Delete { pos, len } => {
                let pos = (*pos as usize).min(chars.len());
                let end = (pos + *len as usize).min(chars.len());
                chars.drain(pos..end);
            }
            PatchOp::Replace { pos, len, text } => {
                let pos = (*pos as usize).min(chars.len());
                let end = (pos + *len as usize).min(chars.len());
                let replace_chars: Vec<char> = text.chars().collect();
                chars.splice(pos..end, replace_chars);
            }
            PatchOp::Retain { .. } => {}
        }
    }

    chars.into_iter().collect()
}

/// Check if there's a revision gap (missed patches)
pub fn has_revision_gap(current_revision: u64, patch_revision: u64) -> bool {
    patch_revision > current_revision + 1
}

/// Check if file is markdown based on mime type or extension
pub fn is_markdown(mime: &str, path: &str) -> bool {
    mime == "text/markdown" || path.ends_with(".md") || path.ends_with(".markdown")
}

/// Format a modified_at string (ISO 8601 or similar) into a compact human-readable form.
/// Returns an empty string if the input is blank or unparseable.
pub fn format_modified_date(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    // ISO 8601 like "2026-02-20T14:32:10Z" or "2026-02-20 14:32:10 UTC"
    // Take up to the minute: "YYYY-MM-DD HH:MM"
    let normalized = trimmed.replace('T', " ");
    let date_time = normalized.trim_end_matches('Z').trim();
    // Take first 16 chars: "YYYY-MM-DD HH:MM"
    if date_time.len() >= 16 {
        date_time[..16].to_string()
    } else {
        date_time.to_string()
    }
}
