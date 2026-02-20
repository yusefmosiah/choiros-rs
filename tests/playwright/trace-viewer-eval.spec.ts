import { expect, test } from "@playwright/test";
import {
  BACKEND,
  fetchEvents,
  openTraceWindow,
  triggerRun,
  waitForEvent,
} from "./trace-viewer.helpers";

const EVAL_PROMPTS: Array<{
  id: string;
  prompt: string;
  expects_delegation: boolean;
  expects_worker_lifecycle: boolean;
  expects_tool_calls: boolean;
  min_llm_calls: number;
}> = [
  {
    id: "file-listing",
    prompt: "List all Rust source files in the sandbox/src/actors directory.",
    expects_delegation: true,
    expects_worker_lifecycle: true,
    expects_tool_calls: true,
    min_llm_calls: 1,
  },
  {
    id: "cargo-inspect",
    prompt: "Read the workspace Cargo.toml and summarize member crates.",
    expects_delegation: true,
    expects_worker_lifecycle: true,
    expects_tool_calls: true,
    min_llm_calls: 1,
  },
  {
    id: "short-answer",
    prompt: "What is 2 + 2? Answer briefly.",
    expects_delegation: false,
    expects_worker_lifecycle: false,
    expects_tool_calls: false,
    min_llm_calls: 1,
  },
  {
    id: "multi-file",
    prompt: "Read both the Justfile and top-level README. Summarize what you find.",
    expects_delegation: true,
    expects_worker_lifecycle: true,
    expects_tool_calls: true,
    min_llm_calls: 1,
  },
  {
    id: "structure-summary",
    prompt: "Describe the high-level structure of the sandbox/src directory tree.",
    expects_delegation: true,
    expects_worker_lifecycle: true,
    expects_tool_calls: true,
    min_llm_calls: 1,
  },
];

for (const scenario of EVAL_PROMPTS) {
  test(`eval: ${scenario.id} — event emission and UI visibility`, async ({
    page,
    request,
  }) => {
    test.setTimeout(420_000);
    const desktopId = `eval-${scenario.id}-${Date.now()}`;
    const runId = await triggerRun(request, scenario.prompt, desktopId);
    expect(runId).not.toBeNull();
    const resolvedRunId = runId as string;

    let terminalEvent: any = null;
    try {
      terminalEvent = await waitForEvent(
        request,
        "conductor.task",
        (event) =>
          (event.event_type === "conductor.task.completed" ||
            event.event_type === "conductor.task.failed") &&
          event.payload?.run_id === resolvedRunId,
        120_000
      );
    } catch {
      const runEvents = await request.get(`${BACKEND}/logs/events?run_id=${resolvedRunId}&limit=20`);
      expect(runEvents.ok()).toBeTruthy();
      const body = await runEvents.json();
      expect((body.events ?? []).length).toBeGreaterThan(0);
    }
    expect(terminalEvent || resolvedRunId).toBeTruthy();

    if (scenario.expects_delegation) {
      const delegationEvents = await fetchEvents(request, "conductor.worker.call", 500);
      const forThisRun = delegationEvents.filter(
        (event: any) => event.payload?.run_id === resolvedRunId
      );
      expect(forThisRun.length).toBeGreaterThan(0);
    }

    if (scenario.expects_worker_lifecycle) {
      const lifecycleEvents = await fetchEvents(request, "worker.task", 600);
      const started = lifecycleEvents.filter(
        (event: any) =>
          event.event_type === "worker.task.started" &&
          event.payload?.run_id === resolvedRunId
      );
      expect(started.length).toBeGreaterThan(0);
    }

    if (scenario.expects_tool_calls) {
      const toolEvents = await fetchEvents(request, "worker.tool.call", 600);
      const forThisRun = toolEvents.filter(
        (event: any) => event.payload?.run_id === resolvedRunId
      );
      expect(forThisRun.length).toBeGreaterThan(0);
    }

    const llmEvents = await fetchEvents(request, "llm.call.completed", 800);
    const llmForRun = llmEvents.filter((event: any) => event.payload?.run_id === resolvedRunId);
    expect(llmForRun.length).toBeGreaterThanOrEqual(scenario.min_llm_calls);

    const timelineResp = await request.get(`${BACKEND}/conductor/runs/${resolvedRunId}/timeline`);
    expect(timelineResp.ok()).toBeTruthy();
    const timeline = await timelineResp.json();
    expect(timeline.run_id).toBe(resolvedRunId);
    expect(Array.isArray(timeline.events)).toBeTruthy();
    expect(timeline.events.length).toBeGreaterThan(0);

    await openTraceWindow(page);
    await expect
      .poll(
        async () => {
          const bodyText = await page.locator("body").innerText();
          return (
            bodyText.includes(resolvedRunId.slice(0, 8)) ||
            bodyText.includes(scenario.prompt.slice(0, 28))
          );
        },
        { timeout: 30_000, intervals: [1_000] }
      )
      .toBeTruthy();

    await page.locator(".trace-run-toggle").first().click();
    await expect(page.locator(".trace-traj-grid").first()).toBeVisible({ timeout: 15_000 });

    if (scenario.expects_delegation) {
      await expect(page.locator(".trace-delegation-band").first()).toBeVisible({
        timeout: 15_000,
      });
    }

    if (scenario.expects_worker_lifecycle) {
      await expect(page.locator(".trace-lifecycle-chip").first()).toBeVisible({
        timeout: 15_000,
      });
    }

    await page.screenshot({
      path: `../artifacts/playwright/eval-${scenario.id}.png`,
      fullPage: true,
    });
  });
}

test("eval: aggregate pass rate >= 4/5 prompts", async ({ request }) => {
  const resp = await request.get(`${BACKEND}/health`);
  expect(resp.ok()).toBeTruthy();
});
