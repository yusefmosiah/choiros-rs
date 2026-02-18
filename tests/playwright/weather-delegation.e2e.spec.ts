import { expect, test } from "@playwright/test";

test("weather prompt delegates researcher and produces writer enqueue activity", async ({
  page,
}) => {
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
  await expect(page.getByText("Run Graph").first()).toBeVisible({ timeout: 120_000 });

  await expect(page.getByText(/Researcher/i).first()).toBeVisible({ timeout: 180_000 });

  await expect
    .poll(
      async () => {
        const bodyText = await page.locator("body").innerText();
        const hasObjective =
          bodyText.includes(
            "whats the weather in boston right now? delegate_researcher and return current temperature and conditions."
          ) || bodyText.includes("whats the weather in boston");
        const hasResearcherNode = /Researcher\s*\(\d+\)|\nResearcher\n/i.test(bodyText);
        const toolCallsMatch = bodyText.match(/(\d+)\s+tool calls/i);
        const hasToolCalls = toolCallsMatch
          ? Number.parseInt(toolCallsMatch[1], 10) > 0
          : false;
        return hasObjective && hasResearcherNode && hasToolCalls;
      },
      { timeout: 180_000, intervals: [1000, 2000, 3000] }
    )
    .toBe(true);

  await page.screenshot({
    path: "../artifacts/playwright/weather-delegation-final.png",
    fullPage: true,
  });
});
