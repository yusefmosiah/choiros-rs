import { expect, test, type BrowserContext, type Page } from "@playwright/test";

interface VirtualAuthenticatorOptions {
  protocol: string;
  transport: string;
  hasResidentKey: boolean;
  hasUserVerification: boolean;
  isUserVerified: boolean;
}

async function addVirtualAuthenticator(
  page: Page,
  opts: Partial<VirtualAuthenticatorOptions> = {}
): Promise<string> {
  const cdp = await (page.context() as BrowserContext).newCDPSession(page);
  await cdp.send("WebAuthn.enable", { enableUI: false });
  const { authenticatorId } = await cdp.send("WebAuthn.addVirtualAuthenticator", {
    options: {
      protocol: opts.protocol ?? "ctap2",
      transport: opts.transport ?? "internal",
      hasResidentKey: opts.hasResidentKey ?? true,
      hasUserVerification: opts.hasUserVerification ?? true,
      isUserVerified: opts.isUserVerified ?? true,
      ...opts,
    },
  });
  return authenticatorId;
}

function uniqueUsername(): string {
  return `e2euser_${Date.now()}_${Math.random().toString(36).slice(2, 8)}@example.com`;
}

async function openAuthModal(page: Page, path: "/" | "/login" | "/register"): Promise<void> {
  await page.goto(path);
  await expect(page.getByTestId("auth-modal")).toBeVisible({ timeout: 60_000 });
  await expect(page.getByTestId("auth-input")).toBeVisible({ timeout: 60_000 });
}

async function submitUsername(page: Page, username: string): Promise<void> {
  const input = page.getByTestId("auth-input");
  await expect(input).toBeVisible({ timeout: 60_000 });
  await input.fill(username);
  await input.press("Enter");
}

async function registerViaModal(page: Page, username: string): Promise<void> {
  const registerFinishResponse = page.waitForResponse(
    (resp) =>
      resp.url().includes("/auth/register/finish") &&
      resp.request().method() === "POST",
    { timeout: 45_000 }
  );

  await submitUsername(page, username);

  const resp = await registerFinishResponse;
  expect(resp.ok()).toBeTruthy();
  await expect(page.getByTestId("auth-modal")).toHaveCount(0, { timeout: 30_000 });
}

test.fixme("writer opens before delegated run completes (localhost:9090 path)", async ({ page }) => {
  await addVirtualAuthenticator(page);
  await openAuthModal(page, "/register");
  await registerViaModal(page, uniqueUsername());

  const promptInput = page.getByPlaceholder(/Ask anything, paste URL/i);
  await expect(promptInput).toBeVisible({ timeout: 60_000 });

  const objectiveTag = `concurrency-${Date.now()}`;
  const objective =
    `what's the weather in boston right now? use researcher and return current conditions. ${objectiveTag}`;

  const traceLauncher = page
    .locator("button, [role='button'], .desktop-icon")
    .filter({ hasText: "Trace" })
    .first();
  await expect(traceLauncher).toBeVisible({ timeout: 60_000 });
  await traceLauncher.click({ force: true });
  const traceDialog = page.getByRole("dialog", { name: "Trace" }).last();
  await expect(traceDialog).toBeVisible({ timeout: 60_000 });

  await promptInput.fill(objective);
  await promptInput.press("Enter");

  const writerWindow = page.getByRole("dialog", { name: "Writer" }).last();
  await expect(writerWindow).toBeVisible({ timeout: 30_000 });

  // Concurrency gate: this run is active in Trace while Writer is already visible.
  await expect
    .poll(
      async () => {
        const traceText = (await traceDialog.innerText()).toLowerCase();
        const writerOpen = await writerWindow.isVisible();
        const hasRun = traceText.includes(objectiveTag);
        const hasRunning = /started|running/.test(traceText);
        return writerOpen && hasRun && hasRunning;
      },
      { timeout: 120_000, intervals: [1000, 2000, 3000] }
    )
    .toBe(true);

  // Then this run completes and writer contains weather-related output.
  await expect
    .poll(
      async () => {
        const traceText = (await traceDialog.innerText()).toLowerCase();
        const writerText = (await writerWindow.innerText()).toLowerCase();
        const hasRun = traceText.includes(objectiveTag);
        const completed = hasRun && /completed/.test(traceText);
        const hasWeatherOutput = /boston|temperature|weather|conditions|forecast/.test(writerText);
        return completed && hasWeatherOutput;
      },
      { timeout: 180_000, intervals: [1000, 2000, 3000] }
    )
    .toBe(true);
});
