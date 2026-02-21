import { expect, test, type Locator } from "@playwright/test";

async function isVisible(locator: Locator): Promise<boolean> {
  try {
    return await locator.isVisible();
  } catch {
    return false;
  }
}

test("prompt bar 'hi' returns immediate response without opening writer", async ({ page }) => {
  await page.goto("/");

  const promptInput = page.getByPlaceholder(/Ask anything, paste URL/i);
  await expect(promptInput).toBeVisible();

  await promptInput.fill("hi");
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
        if (await isVisible(errorIndicator)) {
          return "error";
        }
        if (await isVisible(toastIndicator)) {
          return "toast";
        }
        if (await isVisible(writerWindowTitle)) {
          return "writer";
        }
        return "pending";
      },
      { timeout: 120_000, intervals: [500, 1000, 1500, 2000] }
    )
    .not.toBe("pending");

  expect(await isVisible(errorIndicator)).toBe(false);
  expect(await isVisible(toastIndicator)).toBe(true);
  expect(await isVisible(writerWindowTitle)).toBe(false);

  await page.screenshot({
    path: "../artifacts/playwright/immediate-response-final-state.png",
    fullPage: true,
  });
});
