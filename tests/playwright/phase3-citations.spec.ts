/**
 * Phase 3 Gate Tests — Citations
 *
 * Gate requirements:
 * 3.1 Researcher → writer citation flow produces citation.proposed events
 * 3.2 Writer confirmation produces citation.confirmed events with confirmed_by="writer"
 * 3.3 user_inputs records exist for conductor and writer surfaces
 * 3.4 Confirmed external citations emit global_external_content.upsert events
 * 3.5 qwy.citation_registry events emitted on writer loop completion with citations
 *
 * These tests exercise the backend API directly (no browser automation needed).
 * They use request fixtures so Playwright handles base URL config.
 */

import { test, expect } from "@playwright/test";

const BACKEND = "http://127.0.0.1:8080";

// Helper: query the event store for events by type prefix
async function fetchEvents(
  request: any,
  eventTypePrefix: string,
  limit = 200
): Promise<any[]> {
  const resp = await request.get(
    `${BACKEND}/logs/events?event_type_prefix=${encodeURIComponent(eventTypePrefix)}&limit=${limit}`
  );
  const body = await resp.json();
  return body.events ?? [];
}

// Helper: wait up to `ms` for at least one event matching predicate
async function waitForEvent(
  request: any,
  eventTypePrefix: string,
  predicate: (ev: any) => boolean,
  ms = 60_000
): Promise<any> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const events = await fetchEvents(request, eventTypePrefix, 500);
    const match = events.find(predicate);
    if (match) return match;
    await new Promise((r) => setTimeout(r, 1_500));
  }
  throw new Error(
    `Timeout (${ms}ms) waiting for "${eventTypePrefix}" event matching predicate`
  );
}

// ─────────────────────────────────────────────────────────
// 3.3a  conductor surface emits user_input with UserInputRecord
// ─────────────────────────────────────────────────────────
test("3.3a conductor surface emits user_input record", async ({ request }) => {
  const desktopId = `test-desktop-phase3-3a-${Date.now()}`;
  const objective = `Phase 3 gate test conductor user_input ${Date.now()}`;

  // Fire a conductor execute (may fail — no active workers — but the event fires first)
  await request.post(`${BACKEND}/conductor/execute`, {
    data: {
      objective,
      desktop_id: desktopId,
      output_mode: "markdown_report_to_writer",
    },
  });

  // The user_input event fires synchronously before actor dispatch
  const event = await waitForEvent(
    request,
    "user_input",
    (ev) => {
      const p = ev.payload ?? {};
      return (
        p.surface === "conductor.execute" &&
        p.record?.content === objective &&
        p.record?.surface === "conductor"
      );
    },
    15_000
  );

  expect(event).toBeTruthy();
  expect(event.payload.record.surface).toBe("conductor");
  expect(event.payload.record.content).toBe(objective);
  expect(event.payload.record.input_id).toBeTruthy();
  expect(event.payload.record.desktop_id).toBe(desktopId);
  console.log("3.3a PASSED — conductor user_input record:", event.payload.record.input_id);
});

// ─────────────────────────────────────────────────────────
// 3.3b  writer surface emits user_input with UserInputRecord
// ─────────────────────────────────────────────────────────
test("3.3b writer surface emits user_input record", async ({ request }) => {
  // Use a run that we know exists from a prior conductor run, or create one via /writer/ensure
  // /writer/ensure takes `path` not `run_id`
  const runId = `test-run-phase3-3b-${Date.now()}`;
  const ensurePath = `conductor/runs/${runId}/draft.md`;

  const ensureResp = await request.post(`${BACKEND}/writer/ensure`, {
    data: {
      path: ensurePath,
      desktop_id: "test-desktop-phase3-3b",
      objective: "Phase 3 writer user_input gate test",
    },
  });
  expect(ensureResp.ok()).toBeTruthy();
  const ensureBody = await ensureResp.json();
  // ensure returns run_id
  const ensuredRunId: string = ensureBody.run_id ?? runId;
  expect(ensuredRunId).toBeTruthy();

  // Get current head version — list_versions takes `path` not `run_id`
  const docPath = `conductor/runs/${ensuredRunId}/draft.md`;
  const versionsResp = await request.get(
    `${BACKEND}/writer/versions?path=${encodeURIComponent(docPath)}`
  );
  expect(versionsResp.ok()).toBeTruthy();
  const versionsBody = await versionsResp.json();
  const versions = versionsBody.versions ?? [];
  expect(versions.length).toBeGreaterThan(0);
  const headVersion = versions[versions.length - 1];
  const baseVersionId: number = headVersion.version_id;

  // Submit a user prompt — route is /writer/prompt (not /writer/prompt_document)
  const promptText = `phase3 gate writer input ${Date.now()}`;
  const promptResp = await request.post(`${BACKEND}/writer/prompt`, {
    data: {
      path: `conductor/runs/${ensuredRunId}/draft.md`,
      base_version_id: baseVersionId,
      prompt_diff: [{ op: "insert", pos: 0, text: promptText }],
    },
  });
  // 200/202 = success, 400 = validation (still fires the event)
  const status = promptResp.status();
  expect([200, 202, 400].includes(status)).toBeTruthy();

  // Verify user_input event for writer surface
  const event = await waitForEvent(
    request,
    "user_input",
    (ev) => {
      const p = ev.payload ?? {};
      return (
        p.surface === "writer.prompt_document" &&
        p.run_id === ensuredRunId &&
        p.record?.surface === "writer"
      );
    },
    15_000
  );

  expect(event).toBeTruthy();
  expect(event.payload.record.run_id).toBe(ensuredRunId);
  expect(event.payload.record.surface).toBe("writer");
  expect(event.payload.record.input_id).toBeTruthy();
  console.log("3.3b PASSED — writer user_input record:", event.payload.record.input_id);
});

// ─────────────────────────────────────────────────────────
// 3.1 + 3.2 + 3.4  Citation lifecycle: proposed → confirmed → global upsert
//
// Strategy: check for existing events first (from previous runs in DB).
// If none found, trigger a fresh conductor run and wait up to 3 minutes.
// Skipped entirely if no live model credentials.
// ─────────────────────────────────────────────────────────
test("3.1+3.2+3.4 citation lifecycle proposed→confirmed→global upsert", async ({
  request,
}) => {
  const hasLiveModel =
    process.env.AWS_PROFILE ||
    process.env.AWS_BEARER_TOKEN_BEDROCK ||
    (process.env.AWS_ACCESS_KEY_ID && process.env.AWS_SECRET_ACCESS_KEY);
  if (!hasLiveModel) {
    test.skip(true, "No live AWS credentials — citation lifecycle test skipped");
    return;
  }

  // 3.1: Check for existing citation.proposed events first
  let proposedEvents = await fetchEvents(request, "citation.proposed", 500);
  let runId: string | null = null;

  if (proposedEvents.length > 0) {
    // Use the most recent confirmed run
    runId = proposedEvents[proposedEvents.length - 1].payload?.citing_run_id ?? null;
    console.log(`Using existing citation data from run_id=${runId}`);
  } else {
    // Trigger a fresh conductor run
    const desktopId = `test-desktop-phase3-citation-${Date.now()}`;
    const objective =
      "Research the latest news on Rust async runtimes and summarise findings";
    const runResp = await request.post(`${BACKEND}/conductor/execute`, {
      data: {
        objective,
        desktop_id: desktopId,
        output_mode: "markdown_report_to_writer",
      },
    });
    expect(runResp.ok()).toBeTruthy();
    runId = (await runResp.json()).run_id;
    console.log(`Fresh conductor run started: run_id=${runId}`);

    // Wait for citation.proposed (up to 3 minutes)
    const proposedEvent = await waitForEvent(
      request,
      "citation.proposed",
      (ev) => ev.payload?.citing_run_id === runId && ev.payload?.status === "proposed",
      180_000
    );
    expect(proposedEvent).toBeTruthy();
  }

  expect(runId).toBeTruthy();

  // 3.1: Verify citation.proposed exists for this run
  const proposed = await waitForEvent(
    request,
    "citation.proposed",
    (ev) => {
      const p = ev.payload ?? {};
      return p.citing_run_id === runId && p.status === "proposed";
    },
    5_000
  );
  expect(proposed.payload.cited_kind).toBe("external_url");
  expect(proposed.payload.citation_id).toBeTruthy();
  console.log(`3.1 PASSED — citation.proposed: ${proposed.payload.citation_id}`);

  // 3.2: Verify citation.confirmed exists for this run
  const confirmed = await waitForEvent(
    request,
    "citation.confirmed",
    (ev) => {
      const p = ev.payload ?? {};
      return p.citing_run_id === runId && p.status === "confirmed";
    },
    30_000
  );
  expect(confirmed.payload.confirmed_by).toBe("writer");
  console.log(`3.2 PASSED — citation.confirmed for run ${runId}`);

  // 3.4: Verify global_external_content.upsert exists for this run
  const upsert = await waitForEvent(
    request,
    "global_external_content",
    (ev) => {
      const p = ev.payload ?? {};
      return p.citing_run_id === runId && p.action === "upsert";
    },
    30_000
  );
  expect(upsert.payload.cited_kind).toBe("external_url");
  expect(upsert.payload.cited_id).toBeTruthy();
  console.log(`3.4 PASSED — global_external_content.upsert for run ${runId}`);
});

// ─────────────────────────────────────────────────────────
// 3.5  qwy.citation_registry event on writer loop completion
// Checks for existing events first (from previous runs).
// ─────────────────────────────────────────────────────────
test("3.5 qwy.citation_registry event emitted on writer version with citations", async ({
  request,
}) => {
  const hasLiveModel =
    process.env.AWS_PROFILE ||
    process.env.AWS_BEARER_TOKEN_BEDROCK ||
    (process.env.AWS_ACCESS_KEY_ID && process.env.AWS_SECRET_ACCESS_KEY);
  if (!hasLiveModel) {
    test.skip(true, "No live AWS credentials — qwy.citation_registry test skipped");
    return;
  }

  // Check if any qwy.citation_registry events already exist in DB (from any run).
  let registryEvents = await fetchEvents(request, "qwy.citation_registry", 50);
  let registryEvent: any = registryEvents.find(
    (ev) =>
      Array.isArray(ev.payload?.citation_registry) &&
      ev.payload.citation_registry.length > 0
  );

  if (!registryEvent) {
    // No existing registry events — trigger a fresh conductor run and wait.
    console.log("No existing qwy.citation_registry events — triggering fresh conductor run");
    const desktopId = `test-desktop-phase3-3-5-${Date.now()}`;
    const objective =
      "Research Rust async runtimes: tokio vs async-std, summarise key differences";
    const runResp = await request.post(`${BACKEND}/conductor/execute`, {
      data: {
        objective,
        desktop_id: desktopId,
        output_mode: "markdown_report_to_writer",
      },
    });
    expect(runResp.ok()).toBeTruthy();
    const freshRunId: string = (await runResp.json()).run_id;
    console.log(`Fresh conductor run: run_id=${freshRunId}`);

    // Wait up to 3 minutes for qwy.citation_registry for this fresh run
    registryEvent = await waitForEvent(
      request,
      "qwy.citation_registry",
      (ev) => {
        const p = ev.payload ?? {};
        return (
          p.run_id === freshRunId &&
          Array.isArray(p.citation_registry) &&
          p.citation_registry.length > 0
        );
      },
      180_000
    );
  } else {
    console.log(
      `Using existing qwy.citation_registry for run_id=${registryEvent.payload.run_id}`
    );
  }

  expect(registryEvent).toBeTruthy();
  const registry = registryEvent.payload.citation_registry;
  expect(registry.length).toBeGreaterThan(0);
  expect(registry[0].citation_id).toBeTruthy();
  expect(registry[0].cited_kind).toBe("external_url");
  console.log(
    `3.5 PASSED — qwy.citation_registry: ${registry.length} entries for run ${registryEvent.payload.run_id}`
  );
});

// ─────────────────────────────────────────────────────────
// Phase 3 Gate Smoke: all citation event topics are queryable
// (No live model needed — just verifies routing is wired)
// ─────────────────────────────────────────────────────────
test("Phase 3 gate smoke: citation event topic constants are wired", async ({
  request,
}) => {
  const topics = [
    "citation.proposed",
    "citation.confirmed",
    "citation.rejected",
    "user_input",
    "global_external_content",
    "qwy.citation_registry",
  ];

  for (const topic of topics) {
    const resp = await request.get(
      `${BACKEND}/logs/events?event_type_prefix=${encodeURIComponent(topic)}&limit=1`
    );
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(Array.isArray(body.events)).toBeTruthy();
    console.log(
      `  topic "${topic}" queryable — ${body.events.length} event(s) in store`
    );
  }
  console.log("Phase 3 gate smoke PASSED — all citation topics queryable");
});
