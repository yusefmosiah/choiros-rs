/**
 * Modal-based auth E2E tests for the hypervisor + Dioxus WASM frontend.
 *
 * These tests target the terminal auth modal (not the legacy BIOS HTML forms).
 */

import { test, expect, type BrowserContext, type Page } from "@playwright/test";

interface VirtualAuthenticatorOptions {
  protocol: string;
  transport: string;
  hasResidentKey: boolean;
  hasUserVerification: boolean;
  isUserVerified: boolean;
}

interface MeResponse {
  authenticated: boolean;
  user_id?: string | null;
  username?: string | null;
}

interface RegisterFinishResponse {
  recovery_codes: string[];
  is_first_passkey: boolean;
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
  return `e2euser_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
}

async function authMe(page: Page): Promise<MeResponse> {
  const res = await page.request.get("/auth/me");
  expect(res.ok()).toBeTruthy();
  return (await res.json()) as MeResponse;
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

async function registerViaModal(
  page: Page,
  username: string
): Promise<RegisterFinishResponse> {
  const registerFinishResponse = page.waitForResponse(
    (resp) =>
      resp.url().includes("/auth/register/finish") &&
      resp.request().method() === "POST",
    { timeout: 45_000 }
  );

  await submitUsername(page, username);

  const resp = await registerFinishResponse;
  expect(resp.ok()).toBeTruthy();

  const payload = (await resp.json()) as RegisterFinishResponse;
  await expect(page.getByTestId("auth-modal")).toHaveCount(0, { timeout: 30_000 });
  return payload;
}

async function loginViaModal(page: Page, username: string): Promise<void> {
  const loginFinishResponse = page.waitForResponse(
    (resp) =>
      resp.url().includes("/auth/login/finish") &&
      resp.request().method() === "POST",
    { timeout: 45_000 }
  );

  await submitUsername(page, username);

  const resp = await loginFinishResponse;
  expect(resp.ok()).toBeTruthy();
  await expect(page.getByTestId("auth-modal")).toHaveCount(0, { timeout: 30_000 });
}

test.describe("WASM auth modal", () => {
  test("/login boots with auth modal and single input", async ({ page }) => {
    await openAuthModal(page, "/login");
    await expect(page.locator("span", { hasText: "login:" }).first()).toBeVisible();

    const me = await authMe(page);
    expect(me.authenticated).toBe(false);
  });

  test("/register boots with auth modal and single input", async ({ page }) => {
    await openAuthModal(page, "/register");
    await expect(page.locator("span", { hasText: "login:" }).first()).toBeVisible();

    const me = await authMe(page);
    expect(me.authenticated).toBe(false);
  });

  test("unknown user from modal auto-registers with passkey and logs in", async ({ page }) => {
    await addVirtualAuthenticator(page);
    const username = uniqueUsername();

    await openAuthModal(page, "/register");
    const finish = await registerViaModal(page, username);

    expect(finish.is_first_passkey).toBe(true);
    expect(finish.recovery_codes).toHaveLength(10);

    const me = await authMe(page);
    expect(me.authenticated).toBe(true);
    expect(me.username).toBe(username);
  });

  test("register then logout then login succeeds in same browser context", async ({ page }) => {
    await addVirtualAuthenticator(page);
    const username = uniqueUsername();

    await openAuthModal(page, "/register");
    await registerViaModal(page, username);

    const logout = await page.request.post("/auth/logout");
    expect(logout.status()).toBeGreaterThanOrEqual(200);
    expect(logout.status()).toBeLessThan(400);

    let me = await authMe(page);
    expect(me.authenticated).toBe(false);

    await openAuthModal(page, "/login");
    await loginViaModal(page, username);

    me = await authMe(page);
    expect(me.authenticated).toBe(true);
    expect(me.username).toBe(username);
  });

  test("logout clears authenticated session", async ({ page }) => {
    await addVirtualAuthenticator(page);
    const username = uniqueUsername();

    await openAuthModal(page, "/register");
    await registerViaModal(page, username);

    let me = await authMe(page);
    expect(me.authenticated).toBe(true);

    await page.request.post("/auth/logout");
    me = await authMe(page);
    expect(me.authenticated).toBe(false);
  });

  test("valid recovery code is accepted once", async ({ page }) => {
    await addVirtualAuthenticator(page);
    const username = uniqueUsername();

    await openAuthModal(page, "/register");
    const finish = await registerViaModal(page, username);
    expect(finish.recovery_codes).toHaveLength(10);

    await page.request.post("/auth/logout");

    const firstUse = await page.request.post("/auth/recovery", {
      data: { username, code: finish.recovery_codes[0] },
    });
    expect(firstUse.status()).toBe(200);

    const reused = await page.request.post("/auth/recovery", {
      data: { username, code: finish.recovery_codes[0] },
    });
    expect(reused.status()).toBe(401);
  });

  test("invalid recovery code is rejected", async ({ page }) => {
    const res = await page.request.post("/auth/recovery", {
      data: {
        username: "nonexistent_user_xyz",
        code: "wrong-wrong-wrong-wrong",
      },
    });

    expect(res.status()).toBe(401);
  });
});
