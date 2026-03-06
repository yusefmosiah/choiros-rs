# PDF App Implementation Guide (Deferred)

Date: 2026-02-08
Status: Deferred design gate (no implementation in this phase)

## Purpose

Define a concrete, testable implementation plan for a PDF app without starting build work yet.

## Deferred Scope

- Open/render PDF documents.
- Page navigation and viewport controls.
- Basic text selection/extraction.
- Read-only integration with directives and researcher outputs.

## Explicitly Out of Scope (for this phase)

- Annotation workflows.
- In-place PDF editing.
- OCR and advanced document intelligence.
- Persistent per-user PDF metadata before identity/scope hardening is complete.

## Planned Deliverables

- Capability/API contract for PDF open/render/read actions.
- Event schema for PDF lifecycle:
  - `pdf.opened`
  - `pdf.page_changed`
  - `pdf.extracted`
- Test plan:
  - render correctness
  - navigation correctness
  - deterministic extraction checks
- UX notes for mobile + desktop windowing behavior.

## Gate

This guide must be reviewed and accepted before any PDF app implementation starts.
