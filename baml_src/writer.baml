// ChoirOS Writer BAML Contracts
// Changeset summarization for Marginalia observation UX.

// ============================================================================
// Input Types
// ============================================================================

class ChangesetInput {
  patch_id string
  loop_id string?
  before_content string   // document content before the patch (may be empty)
  after_content string    // document content after the patch
  ops_json string         // JSON-serialized Vec<PatchOp> for context
  source string           // who/what produced the change (writer, user, conductor, etc.)
}

// ============================================================================
// Output Types
// ============================================================================

enum ImpactLevel {
  Low
  Medium
  High
}

class ChangesetSummaryOutput {
  summary string        // 1-2 sentence human-readable description of what changed
  impact ImpactLevel    // estimated scope of change
  op_taxonomy string[]  // list of change categories present (e.g. ["insert", "structural_rewrite", "clarification"])
}

// ============================================================================
// Function
// ============================================================================

function SummarizeChangeset(input: ChangesetInput) -> ChangesetSummaryOutput {
  client FastResponse
  prompt #"
    You are the ChoirOS Marginalia observer. Your job is to produce a compact,
    human-readable summary of what changed in a document patch.

    Patch ID: {{ input.patch_id }}
    Source: {{ input.source }}
    Loop ID: {{ input.loop_id }}

    Before (may be empty if this is the first version):
    {{ input.before_content }}

    After:
    {{ input.after_content }}

    Raw ops (JSON for reference):
    {{ input.ops_json }}

    Instructions:
    - Write a 1-2 sentence summary describing what changed and why it matters.
    - Choose ImpactLevel:
      - Low: minor wording tweaks, small additions or deletions
      - Medium: one or more paragraphs changed, structural reorganization of a section
      - High: large-scale rewrite, major content addition or deletion, objective changed
    - List op_taxonomy categories that apply (pick from: insert, delete, replace, structural_rewrite,
      clarification, expansion, condensation, reformatting, factual_update, objective_change).
    - Keep the summary factual; do not speculate about intent beyond what the diff shows.

    {{ ctx.output_format }}
  "#
}
