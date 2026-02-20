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

pub const INLINE_OVERLAY_MARKER: &str = "\n\n--- pending suggestions ---\n";

pub fn compose_editor_text(base: &str, overlays: &[WriterOverlay]) -> String {
    if overlays.is_empty() {
        return base.to_string();
    }
    let mut out = String::from(base);
    out.push_str(INLINE_OVERLAY_MARKER);
    for overlay in overlays {
        for line in overlay_note_lines(overlay) {
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

pub fn strip_inline_overlay_block(text: &str) -> String {
    if let Some(index) = text.find(INLINE_OVERLAY_MARKER) {
        return text[..index].to_string();
    }
    text.to_string()
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
