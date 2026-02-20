import { expect, test } from "@playwright/test";
import { fetchEvents, openTraceWindow, triggerRun } from "./trace-viewer.helpers";

test("conductor.worker.call is emitted for a delegating prompt", async ({ request }) => {
  const desktopId = `trace-p1-delegation-${Date.now()}`;
  await triggerRun(request, "List the files in the src directory.", desktopId);

  const deadline = Date.now() + 60_000;
  let events: any[] = [];
  while (Date.now() < deadline) {
    events = await fetchEvents(request, "conductor.worker.call");
    if (events.length > 0) break;
    await new Promise((resolve) => setTimeout(resolve, 1_500));
  }
  expect(events.length).toBeGreaterThan(0);

  const ev =
    events.find((event: any) => {
      const runId = event.payload?.run_id ?? event.payload?.data?.run_id;
      return typeof runId === "string" && typeof event.payload?.worker_type === "string";
    }) ?? events[0];
  const runId = ev.payload?.run_id ?? ev.payload?.data?.run_id;
  expect(typeof runId).toBe("string");
  expect(typeof ev.payload?.worker_type).toBe("string");
  expect((ev.payload?.worker_type ?? "").length).toBeGreaterThan(0);
});

test("delegation timeline band appears in trace window after run", async ({
  page,
  request,
}) => {
  const desktopId = `trace-p1-ui-${Date.now()}`;
  await triggerRun(request, "Summarize the Justfile.", desktopId);

  await openTraceWindow(page);
  await expect(page.locator(".trace-delegation-band").first()).toBeVisible({
    timeout: 90_000,
  });
});

test("run status badge appears for the run", async ({
  page,
  request,
}) => {
  const desktopId = `trace-p1-status-${Date.now()}`;
  await triggerRun(request, "Echo hello world.", desktopId);

  await openTraceWindow(page);
  await expect
    .poll(
      async () => {
        const completed = await page.locator(".trace-run-status--completed").count();
        const failed = await page.locator(".trace-run-status--failed").count();
        const inflight = await page.locator(".trace-run-status--in-progress").count();
        return completed + failed + inflight;
      },
      { timeout: 120_000, intervals: [2_000] }
    )
    .toBeGreaterThan(0);
});
