import { expect, test, type Locator } from "@playwright/test";

async function isVisible(locator: Locator): Promise<boolean> {
  try {
    return await locator.isVisible();
  } catch {
    return false;
  }
}

test("prompt bar routes to conductor and opens writer window", async ({ page }) => {
  await page.goto("/");

  const promptInput = page.getByPlaceholder(/Ask anything, paste URL/i);
  await expect(promptInput).toBeVisible();

  const objective =
    "Inspect this repository and produce a short architecture summary with key actor boundaries.";
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

  await page.screenshot({
    path: "../artifacts/playwright/final-state.png",
    fullPage: true,
  });
});
