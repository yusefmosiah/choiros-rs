import { expect, test, type APIRequestContext, type Locator } from "@playwright/test";
import { ensureAuthenticated } from "./auth.helpers";

const BACKEND = "http://127.0.0.1:8080";
const DESKTOP_ID = "default-desktop";

async function closeAllDesktopWindows(request: APIRequestContext) {
  const windowsResp = await request.get(`${BACKEND}/desktop/${DESKTOP_ID}/windows`);
  expect(windowsResp.ok()).toBeTruthy();
  const windowsJson = (await windowsResp.json()) as {
    success: boolean;
    windows: Array<{ id: string }>;
  };

  for (const win of windowsJson.windows) {
    await request.delete(`${BACKEND}/desktop/${DESKTOP_ID}/windows/${win.id}`);
  }
}

function windowByTitle(page: import("@playwright/test").Page, title: string): Locator {
  return page
    .locator(".floating-window")
    .filter({
      has: page.locator(".window-titlebar span", {
        hasText: title,
      }),
    })
    .first();
}

async function windowZIndex(window: Locator): Promise<number> {
  const style = (await window.getAttribute("style")) ?? "";
  const zIndexMatch = style.match(/z-index:\s*(\d+)/i);
  expect(zIndexMatch).toBeTruthy();
  return Number.parseInt(zIndexMatch![1], 10);
}

test("auto-opened writer does not trap focus; clicking another window brings it to top", async ({
  page,
}) => {
  await ensureAuthenticated(page);
  await closeAllDesktopWindows(page.request);
  await page.goto("/");

  const traceLauncher = page
    .locator("button, [role='button'], .desktop-icon")
    .filter({ hasText: "Trace" })
    .first();
  await expect(traceLauncher).toBeVisible({ timeout: 60_000 });
  await traceLauncher.click();

  const traceWindow = windowByTitle(page, "Trace");
  await expect(traceWindow).toBeVisible({ timeout: 60_000 });

  const promptInput = page.getByPlaceholder(/Ask anything, paste URL/i);
  await expect(promptInput).toBeVisible({ timeout: 60_000 });

  const marker = Date.now();
  await promptInput.fill(
    `Inspect this repository and produce a short architecture summary with actor boundaries. marker:${marker}`
  );
  await promptInput.press("Enter");

  const writerWindow = windowByTitle(page, "Writer");
  await expect(writerWindow).toBeVisible({ timeout: 120_000 });

  const writerZAfterOpen = await windowZIndex(writerWindow);
  const traceZAfterWriterOpen = await windowZIndex(traceWindow);
  expect(writerZAfterOpen).toBeGreaterThan(traceZAfterWriterOpen);

  await traceWindow.locator(".window-titlebar").first().click();

  await expect
    .poll(
      async () => {
        const writerZ = await windowZIndex(writerWindow);
        const traceZ = await windowZIndex(traceWindow);
        return traceZ - writerZ;
      },
      { timeout: 30_000, intervals: [300, 500, 1000] }
    )
    .toBeGreaterThan(0);

  await page.screenshot({
    path: "../artifacts/playwright/prompt-bar-writer-focus-final.png",
    fullPage: true,
  });
});
