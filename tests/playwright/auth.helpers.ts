import { expect, type BrowserContext, type Page } from "@playwright/test";

interface VirtualAuthenticatorOptions {
  protocol: string;
  transport: string;
  hasResidentKey: boolean;
  hasUserVerification: boolean;
  isUserVerified: boolean;
}

interface MeResponse {
  authenticated: boolean;
  username?: string | null;
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
  return `writer_e2e_${Date.now()}_${Math.random().toString(36).slice(2, 8)}@example.com`;
}

async function authMe(page: Page): Promise<MeResponse> {
  const res = await page.request.get("/auth/me");
  if (!res.ok()) {
    return { authenticated: false };
  }
  return (await res.json()) as MeResponse;
}

export async function ensureAuthenticated(page: Page): Promise<string | null> {
  const me = await authMe(page);
  if (me.authenticated) {
    return me.username ?? null;
  }

  await addVirtualAuthenticator(page);
  await page.goto("/register");

  const username = uniqueUsername();
  const visibleInputs = page.locator("[data-testid='auth-input']:visible");
  await expect(visibleInputs.first()).toBeVisible({ timeout: 60_000 });

  const inputCount = await visibleInputs.count();
  for (let i = 0; i < inputCount; i++) {
    const input = visibleInputs.nth(i);
    try {
      await input.fill(username, { timeout: 10_000 });
      await input.press("Enter", { timeout: 10_000 });
    } catch {
      // Duplicate modal layers can race visibility/actionability; best-effort submit each visible input.
    }
  }

  await expect.poll(
    async () => {
      const meAfter = await authMe(page);
      return meAfter.authenticated;
    },
    {
      timeout: 90_000,
      message: "expected /auth/me to become authenticated after registration",
    }
  ).toBe(true);

  const meAfter = await authMe(page);
  expect(meAfter.authenticated).toBe(true);
  return username;
}
