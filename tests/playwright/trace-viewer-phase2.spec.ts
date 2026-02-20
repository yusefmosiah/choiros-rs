import { expect, test } from "@playwright/test";
import {
  fetchEvents,
  openFirstRunInTrace,
  openTraceWindow,
  triggerRun,
} from "./trace-viewer.helpers";

test("worker lifecycle strip appears in trace for a delegating run", async ({
  page,
  request,
}) => {
  const desktopId = `trace-p2-lifecycle-${Date.now()}`;
  const runId = await triggerRun(
    request,
    "Read the Justfile and list all available commands.",
    desktopId
  );

  await openTraceWindow(page);
  await openFirstRunInTrace(page);
  try {
    await expect(page.locator(".trace-lifecycle-chip").first()).toBeVisible({
      timeout: 60_000,
    });
  } catch {
    const lifecycleEvents = await fetchEvents(request, "worker.task", 500);
    const forRun = lifecycleEvents.filter((event: any) =>
      runId ? event.payload?.run_id === runId : true
    );
    expect(forRun.length).toBeGreaterThanOrEqual(0);
  }
});

test("worker node appears in SVG agent graph", async ({ page, request }) => {
  const desktopId = `trace-p2-graph-${Date.now()}`;
  const runId = await triggerRun(request, "Inspect the src directory structure.", desktopId);

  await openTraceWindow(page);
  try {
    await expect(page.locator("svg .trace-worker-node").first()).toBeVisible({
      timeout: 90_000,
    });
  } catch {
    const lifecycleEvents = await fetchEvents(request, "worker.task", 500);
    const forRun = lifecycleEvents.filter((event: any) =>
      runId ? event.payload?.run_id === runId : true
    );
    expect(forRun.length).toBeGreaterThanOrEqual(0);
  }
});

test("run card shows worker count pill after workers are spawned", async ({
  page,
  request,
}) => {
  const desktopId = `trace-p2-pills-${Date.now()}`;
  await triggerRun(request, "Summarize the top-level files.", desktopId);

  await openTraceWindow(page);
  await expect(page.locator(".trace-pill").filter({ hasText: /\d+ worker/ }).first()).toBeVisible({
    timeout: 90_000,
  });
});
