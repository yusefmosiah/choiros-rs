# Writer Contract and Implementation Guide

Date: 2026-03-09
Kind: Guide
Status: Active
Requires: [ADR-0001]

## Narrative Summary (1-minute read)

The old Writer API contract is no longer the right model for ChoirOS. Writer is
not primarily a stateless file editor with optimistic save conflicts. It is the
runtime for a living document whose versions tell the story of a run.

The important contract is:

- the first user prompt is a real version,
- versions are immutable snapshots in the run history,
- users, Writer, and future collaborators may author revisions,
- one runtime commit path records accepted versions in order,
- Researcher and Terminal usually send evidence, progress, artifacts, and
  proposals, not direct canonical diffs,
- Writer owns canon and the readable presentation of marginalia,
- `.writer_revisions` is transitional legacy machinery for the generic
  `/writer/open` and `/writer/save` path and should be removed after that path
  is migrated.

This guide defines the target contract and the implementation sequence to get
there without guessing from churn-era behavior.

## What Changed

- Replaced the obsolete "generic file editor" contract with the current
  living-document contract.
- Defined version authorship separately from version commit authority.
- Defined explicit lanes for canon, marginalia, artifacts, and proposals.
- Marked `.writer_revisions` as transitional, not canonical.
- Added an implementation plan and acceptance criteria for removing
  `.writer_revisions` safely.

## What To Do Next

1. Make the prompt bar flow and blank Writer window flow create the same initial
   user-authored seed version.
2. Separate run status from document versions so `running` and `done` stop being
   inferred from document mutations.
3. Stop treating Researcher and Terminal ingress as canonical diffs by default.
4. Migrate `/writer/open` and `/writer/save` off `.writer_revisions`.
5. Delete `.writer_revisions` code and docs only after the live API path is
   moved.

## 1) Current Reality

### Current fact

Run-scoped Writer documents are already backed by:

- `conductor/runs/{run_id}/draft.md`
- `conductor/runs/{run_id}/draft.writer-state.json`

The sidecar currently stores versions, overlays, and source references. This is
the real run-document persistence model.

### Current fact

`.writer_revisions` is still live only because the generic Writer HTTP API
still uses it for optimistic revision tracking on `/writer/open` and
`/writer/save`. It is not the canonical run-document history mechanism.

### Current fact

The current system still has contract bugs:

- the initial prompt/version rendering is inconsistent,
- `running` and `done` status display is unreliable,
- marginalia is too thin and too raw,
- version history and source links are not yet coherent enough to serve as the
  readable story of a run.

This guide treats those as contract defects, not just UI bugs.

## 2) Core Writer Contract

### 2.1 Writer is a living-document runtime

Writer is not best understood as a plain file editor. Its primary job is to
maintain the canonical living document for a run and to present the progression
of that run in a readable form.

### 2.2 Versions are the canonical story of the run

A `Version` is an immutable snapshot in document history.

Required rules:

- the first user prompt is a real version,
- every version has a stable `version_id`,
- every version has a `parent_version_id` except the seed,
- every version must be renderable on its own,
- version history must remain navigable even after later revisions supersede the
  current head.

The first visible version must not be an empty system placeholder. If the user
starts a run with a prompt, that prompt is version `0` or `1` depending on the
chosen indexing scheme, and it must render as an actual document state.

### 2.3 Authorship is broader than Writer

Writer is not the only possible author of a version.

Version authors may be:

- `user`
- `writer`
- `collaborator`
- future privileged agents, if explicitly allowed

The important distinction is:

- authorship may be plural,
- commit authority must remain centralized.

One runtime commit path must enforce:

- parent linkage,
- ordering,
- moderation and policy,
- visibility,
- notification behavior.

### 2.4 Worker updates are not usually versions

Researcher and Terminal do not normally create canonical versions directly.

Their default job is to provide inputs to Writer:

- evidence,
- progress,
- artifacts,
- results,
- optional structured proposals.

Writer decides whether those inputs remain marginalia, stay as artifacts only,
or become a new canonical version.

## 3) Document Lanes

Writer should maintain four explicit lanes.

### 3.1 Canon

Canonical document content.

- immutable version snapshots,
- readable as the main document body,
- only changed through committed versions.

### 3.2 Marginalia

Readable side content attached to a version or version range.

Examples:

- researcher findings,
- terminal progress,
- verification notes,
- rationale,
- source summaries,
- moderation or collaboration notes.

Marginalia is Writer-owned presentation, not raw metadata dumps.

### 3.3 Artifacts

Run files that are useful but not canonical document content.

Examples:

- research reports,
- fetched source captures,
- test logs,
- build outputs,
- generated notes.

### 3.4 Proposals

Candidate revisions not yet committed as canonical versions.

These are useful for:

- collaborator suggestions,
- researcher-proposed insertions,
- future moderated or private revisions.

## 4) Worker Ingress Contract

### 4.1 User input

User input is version-capable.

Examples:

- initial prompt,
- explicit revision,
- direct edit in Writer,
- future collaborator-submitted revision.

These may become committed versions through the Writer runtime path.

### 4.2 Researcher ingress

Researcher should usually send an evidence packet, not a canonical diff.

Evidence packet fields should include:

- `run_id`
- `base_version_id` when relevant
- `summary`
- `citations`
- `source_refs`
- `confidence`
- `section_hint` optional
- `importance` optional
- artifact refs

Researcher should not be responsible for deciding where canon text goes.

### 4.3 Terminal ingress

Terminal should usually send:

- progress updates,
- execution summaries,
- verification results,
- artifact refs,
- optional structured proposals.

Terminal owns execution truth, not canonical document structure.

### 4.4 Structured proposals

When a worker wants to suggest document text, it should prefer a structured
proposal over a raw diff.

Example shape:

- `proposal_kind`
- `section_hint`
- `content`
- `citations`
- `source_refs`
- `base_version_id`

Writer may convert an accepted proposal into a canonical revision.

## 5) Run State Is Not Version State

The UI must stop inferring run status from whether a new version has appeared.

Run state should be tracked separately from document versions.

Recommended states:

- `idle`
- `queued`
- `running`
- `waiting_for_worker`
- `waiting_for_user`
- `blocked`
- `completed`
- `failed`
- `superseded`

This separation is required to fix the current broken `running...` and `done`
display behavior.

## 6) Prompting and Context Rules

### 6.1 Prompt bar and Writer window must be equivalent

These two entry paths should have the same semantics:

- prompt bar into Conductor,
- opening a new Writer window, typing, and prompting for the first time.

Both should:

- create or open the run-scoped document,
- commit an initial user-authored seed version,
- start orchestration against that head version.

### 6.2 Writer context budget

Writer should not receive many full historical versions.

Preferred prompt input:

- full current head version,
- compact summary or diff from the previous version,
- bounded marginalia digest since the previous version,
- relevant evidence and artifact summaries,
- source-set summary for the head version.

At most one full prior version should be included when diff context is
insufficient.

### 6.3 Canonical version writes should be serialized

New inputs may arrive while Writer is still working, but canonical version
creation should remain serialized.

Queue new inputs against a known base. Do not allow multiple overlapping
canonical writes against the same run head.

## 7) Source and Link Contract

Sources and links should be version-aware.

Required rules:

- every version should have a stable source set,
- source display should be attached to a version or proposal context,
- observed sources and selected sources must be distinguishable,
- citations in canon should be traceable back to artifacts or evidence packets.

The sidecar already carries `selected_source_refs` and `observed_source_refs`.
The problem is not storage existence but contract clarity and presentation.

## 8) File Representation

The active target representation for run documents is:

- one visible canonical file per run: `draft.md`
- one sidecar state file per run: `draft.writer-state.json`

The sidecar should hold:

- version history,
- head pointer,
- marginalia and proposal state,
- source sets,
- run-linked document metadata.

The system should not create one separate on-disk file per version for the
normal run-document path.

`.writer_revisions` is transitional and should not be described as the model for
run history.

## 9) Transitional Compatibility Contract

The current generic Writer HTTP API still exposes:

- `POST /writer/open`
- `POST /writer/save`
- `POST /writer/save-version`
- version and overlay queries for run documents

Transitional rule:

- `/writer/open` and `/writer/save` may keep serving compatibility behavior
  while migration is underway,
- but the active docs must no longer describe `.writer_revisions` as the target
  architecture,
- and run documents must be documented in terms of `draft.md` plus
  `draft.writer-state.json`.

## 10) Implementation Guide

### Phase 1: Correct the docs and runtime framing

- stop documenting `.writer_revisions` as canonical Writer storage,
- document run docs as `draft.md` plus `draft.writer-state.json`,
- document the first prompt as a real version,
- document Writer-owned marginalia and separate run state.

### Phase 2: Fix initial version creation

- make prompt-bar entry and blank Writer-window entry use the same seed-version
  path,
- remove any empty placeholder version behavior for new run documents,
- ensure the first visible document state is renderable immediately.

### Phase 3: Separate run state from document history

- introduce explicit run status state,
- stop using version appearance as status inference,
- make `running`, `waiting`, `completed`, and `failed` durable and observable.

### Phase 4: Fix ingress semantics

- keep user edits and prompt-derived revisions as version-capable inputs,
- convert Researcher default output to evidence packets,
- convert Terminal default output to progress, result, and artifact packets,
- use structured proposals instead of raw worker diffs where textual suggestions
  are needed.

### Phase 5: Upgrade marginalia

- make marginalia readable and version-aware,
- group updates by meaning instead of exposing raw metadata blobs,
- attach progress, evidence, and verification to versions or version ranges.

### Phase 6: Migrate off `.writer_revisions`

- move `/writer/open` and `/writer/save` revision behavior onto the real
  document runtime state,
- update desktop client assumptions,
- update integration tests,
- remove sidecar revision helper code,
- delete `.writer_revisions` from docs and runtime.

## 11) Acceptance Criteria

The contract is only correct if these are true.

1. The first user prompt is committed and visible as the first version.
2. Prompt-bar and new-Writer-window entry create equivalent initial state.
3. Opening any version shows a coherent snapshot, not a blank placeholder.
4. Run status can show `running` before a second canonical version exists.
5. Researcher updates do not directly mutate canon by default.
6. Terminal updates do not directly mutate canon by default.
7. Writer can promote worker input into new canonical versions when appropriate.
8. Source links can be traced from visible document content back to evidence or
   artifacts.
9. The system works with memory absent or empty.
10. `.writer_revisions` can be deleted without breaking the supported Writer
    flows.

## 12) Notes on Current Code

This guide is grounded in the current implementation split:

- run-document state lives in `sandbox/src/actors/writer/document_runtime/`
- generic Writer API revision sidecars still live in `sandbox/src/api/writer.rs`
- desktop still calls `/writer/open` and `/writer/save`

That split is why `.writer_revisions` removal is a migration step, not a single
delete.
