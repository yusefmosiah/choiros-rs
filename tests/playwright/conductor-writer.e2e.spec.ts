import { expect, test, type APIRequestContext, type Locator } from "@playwright/test";
import { ensureAuthenticated } from "./auth.helpers";

const BACKEND = "http://127.0.0.1:8080";
const DESKTOP_ID = "default-desktop";

async function isVisible(locator: Locator): Promise<boolean> {
  try {
    return await locator.isVisible();
  } catch {
    return false;
  }
}

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

test("prompt bar routes to conductor and opens writer window", async ({ page }) => {
  await ensureAuthenticated(page);
  await closeAllDesktopWindows(page.request);
  await page.goto("/");

  const promptInput = page.getByPlaceholder(/Ask anything, paste URL/i);
  await expect(promptInput).toBeVisible();

  const marker = Date.now();
  const objective = `Inspect this repository and produce a short architecture summary with key actor boundaries. marker:${marker}`;
  await promptInput.fill(objective);
  await promptInput.press("Enter");

  const writerWindowTitle = page
    .locator(".floating-window .window-titlebar span")
    .filter({ hasText: "Writer" })
    .first();
  const toastIndicator = page.locator(".conductor-toast-indicator").first();
  const errorIndicator = page.locator(".conductor-error-indicator").first();

  await expect
    .poll(
      async () => {
        if (await isVisible(writerWindowTitle)) {
          return "writer";
        }
        if (await isVisible(toastIndicator)) {
          return "toast";
        }
        if (await isVisible(errorIndicator)) {
          return "error";
        }
        return "pending";
      },
      { timeout: 120_000, intervals: [500, 1000, 1500, 2000] }
    )
    .not.toBe("pending");

  const currentState = await (async () => {
    if (await isVisible(writerWindowTitle)) {
      return "writer";
    }
    if (await isVisible(toastIndicator)) {
      return "toast";
    }
    if (await isVisible(errorIndicator)) {
      return "error";
    }
    return "pending";
  })();

  expect(currentState).not.toBe("error");
  expect(["writer", "toast"]).toContain(currentState);

  if (currentState === "writer") {
    const proseBodyForRun = page
      .locator(".writer-prose-body")
      .filter({ hasText: `marker:${marker}` })
      .first();
    await expect(proseBodyForRun).toBeVisible();
    await expect(proseBodyForRun).toContainText(`marker:${marker}`);
    await expect(proseBodyForRun).not.toContainText(
      "Writer orchestration dispatched worker delegation."
    );
    await expect(proseBodyForRun).not.toContainText("Delegated capabilities:");
    await expect(proseBodyForRun).not.toContainText("Pending delegations:");
  }

  await page.screenshot({
    path: "../artifacts/playwright/final-state.png",
    fullPage: true,
  });
});
