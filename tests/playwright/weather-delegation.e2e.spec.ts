import { expect, test, type APIRequestContext } from "@playwright/test";
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

test("weather prompt delegates researcher and produces writer enqueue activity", async ({
  page,
}) => {
  await ensureAuthenticated(page);
  await closeAllDesktopWindows(page.request);
  await page.goto("/");

  const promptInput = page.getByPlaceholder(/Ask anything, paste URL/i);
  await expect(promptInput).toBeVisible({ timeout: 60_000 });

  const objective =
    "whats the weather in boston right now? delegate_researcher and return current temperature and conditions.";
  await promptInput.fill(objective);
  await promptInput.press("Enter");

  const writerWindowTitle = page
    .locator(".floating-window .window-titlebar span")
    .filter({ hasText: "Writer" })
    .first();
  await expect(writerWindowTitle).toBeVisible({ timeout: 120_000 });

  const traceLauncher = page
    .locator("button, [role='button'], .desktop-icon")
    .filter({ hasText: "Trace" })
    .first();
  await expect(traceLauncher).toBeVisible({ timeout: 60_000 });
  await traceLauncher.click();

  const traceWindowTitle = page
    .locator(".floating-window .window-titlebar span")
    .filter({ hasText: "Trace" })
    .first();
  await expect(traceWindowTitle).toBeVisible({ timeout: 60_000 });
  await expect(page.getByText(/whats the weather in boston/i).first()).toBeVisible({
    timeout: 120_000,
  });

  await expect
    .poll(
      async () => {
        const bodyText = await page.locator("body").innerText();
        const hasObjective =
          bodyText.includes(
            "whats the weather in boston right now? delegate_researcher and return current temperature and conditions."
          ) || bodyText.includes("whats the weather in boston");
        const workerMatch = bodyText.match(/(\d+)\s+workers?/i);
        const hasWorkerActivity = workerMatch ? Number.parseInt(workerMatch[1], 10) > 0 : false;
        const toolCallsMatch = bodyText.match(/(\d+)\s+tool calls/i) ??
          bodyText.match(/(\d+)\s+tools/i);
        const hasToolCalls = toolCallsMatch
          ? Number.parseInt(toolCallsMatch[1], 10) > 0
          : false;
        const hasWeatherOutput =
          bodyText.includes("Temperature:") &&
          /Condition(s)?:/.test(bodyText) &&
          /Sources \(\d+\)/.test(bodyText);
        return hasObjective && hasWorkerActivity && hasToolCalls && hasWeatherOutput;
      },
      { timeout: 180_000, intervals: [1000, 2000, 3000] }
    )
    .toBe(true);

  await page.screenshot({
    path: "../artifacts/playwright/weather-delegation-final.png",
    fullPage: true,
  });
});
