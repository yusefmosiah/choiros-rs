import { expect, test } from "@playwright/test";
import { openFirstRunInTrace, openTraceWindow, triggerRun } from "./trace-viewer.helpers";

test("trajectory grid appears in run detail", async ({ page, request }) => {
  const desktopId = `trace-p3-grid-${Date.now()}`;
  await triggerRun(
    request,
    "List all Rust source files in the sandbox/src/actors directory.",
    desktopId
  );

  await openTraceWindow(page);
  await openFirstRunInTrace(page);
  await expect(page.locator(".trace-traj-grid").first()).toBeVisible({ timeout: 30_000 });
  await expect(page.locator(".trace-traj-grid circle").first()).toBeVisible({
    timeout: 15_000,
  });
});

test("trajectory grid mode toggle switches between Status and Duration", async ({
  page,
  request,
}) => {
  const desktopId = `trace-p3-mode-${Date.now()}`;
  await triggerRun(request, "Read Cargo.toml and list workspace members.", desktopId);

  await openTraceWindow(page);
  await openFirstRunInTrace(page);
  await expect(page.locator(".trace-traj-grid").first()).toBeVisible({ timeout: 30_000 });

  await page.getByRole("button", { name: "Duration" }).first().click();
  await expect(page.locator(".trace-traj-grid").first()).toBeVisible({ timeout: 5_000 });
  await page.screenshot({
    path: "../artifacts/playwright/traj-grid-duration.png",
    fullPage: true,
  });
});

test("clicking a trajectory cell highlights a corresponding span card", async ({
  page,
  request,
}) => {
  const desktopId = `trace-p3-click-${Date.now()}`;
  await triggerRun(request, "Read the README if one exists.", desktopId);

  await openTraceWindow(page);
  await openFirstRunInTrace(page);
  await expect(page.locator(".trace-traj-grid").first()).toBeVisible({ timeout: 30_000 });

  const firstCell = page.locator(".trace-traj-grid circle").first();
  await firstCell.click();
  await expect
    .poll(
      async () => {
        const selectedCard = await page.locator(".trace-call-card--selected").count();
        const selectedLoop = await page
          .locator(".trace-loop-group.trace-call-card--selected")
          .count();
        return selectedCard + selectedLoop;
      },
      { timeout: 10_000, intervals: [500, 1000] }
    )
    .toBeGreaterThan(0);
});
