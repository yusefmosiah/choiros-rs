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

  const modal = page.getByTestId("auth-modal");
  await expect(modal).toBeVisible({ timeout: 60_000 });
  const input = page.getByTestId("auth-input");
  await expect(input).toBeVisible({ timeout: 60_000 });

  const username = uniqueUsername();
  const registerFinishResponse = page.waitForResponse(
    (resp) =>
      resp.url().includes("/auth/register/finish") &&
      resp.request().method() === "POST",
    { timeout: 45_000 }
  );

  await input.fill(username);
  await input.press("Enter");

  const resp = await registerFinishResponse;
  expect(resp.ok()).toBeTruthy();
  await expect(modal).toHaveCount(0, { timeout: 30_000 });

  const meAfter = await authMe(page);
  expect(meAfter.authenticated).toBe(true);
  return username;
}
