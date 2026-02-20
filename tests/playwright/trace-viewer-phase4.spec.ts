import { expect, test } from "@playwright/test";
import { openFirstRunInTrace, openTraceWindow, triggerRun } from "./trace-viewer.helpers";

test("duration bar appears in span cards", async ({ page, request }) => {
  const desktopId = `trace-p4-dur-${Date.now()}`;
  await triggerRun(request, "Read the top-level Cargo.toml.", desktopId);

  await openTraceWindow(page);
  await openFirstRunInTrace(page);
  await expect(page.locator(".trace-duration-bar").first()).toBeVisible({ timeout: 30_000 });
});

test("sparkline appears on run list rows", async ({ page, request }) => {
  const desktopId = `trace-p4-spark-${Date.now()}`;
  await triggerRun(request, "List the contents of the sandbox directory.", desktopId);

  await openTraceWindow(page);
  await page.getByRole("button", { name: /Runs|Hide Runs/i }).first().click();
  await expect(page.locator(".trace-run-sparkline").first()).toBeVisible({ timeout: 90_000 });
});

test("total duration and token pills appear on run cards", async ({ page, request }) => {
  const desktopId = `trace-p4-pills-${Date.now()}`;
  await triggerRun(request, "Describe the actor system architecture.", desktopId);

  await openTraceWindow(page);
  await expect(page.locator(".trace-pill").filter({ hasText: /tok/ }).first()).toBeVisible({
    timeout: 90_000,
  });
  await expect(page.locator(".trace-pill").filter({ hasText: /\d+\.\d+s/ }).first()).toBeVisible({
    timeout: 90_000,
  });
});
