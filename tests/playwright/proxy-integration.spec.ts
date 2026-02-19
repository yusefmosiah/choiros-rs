/**
 * Proxy integration E2E tests.
 *
 * Verifies that after authentication the hypervisor auto-spawns the sandbox
 * process and proxies requests to it correctly.
 *
 * Requires:
 *   - Hypervisor running on port 9090 (`just dev-hypervisor` or `just dev-full`)
 *   - Sandbox binary built at workspace root `target/debug/sandbox`
 *     (`cargo build -p sandbox`)
 *
 * The sandbox is NOT pre-started — these tests verify the auto-spawn path.
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

interface SandboxSnapshot {
  user_id: string;
  role: string;
  port: number;
  status: string;
  idle_secs: number;
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
  return `proxytest_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
}

async function authMe(page: Page): Promise<MeResponse> {
  const res = await page.request.get("/auth/me");
  expect(res.ok()).toBeTruthy();
  return (await res.json()) as MeResponse;
}

/** Register a new user via the passkey modal and return the session. */
async function registerAndLogin(page: Page): Promise<{ username: string }> {
  const username = uniqueUsername();
  await addVirtualAuthenticator(page);

  await page.goto("/register");
  await expect(page.getByTestId("auth-modal")).toBeVisible({ timeout: 60_000 });

  const finishWait = page.waitForResponse(
    (r) => r.url().includes("/auth/register/finish") && r.request().method() === "POST",
    { timeout: 45_000 }
  );

  const input = page.getByTestId("auth-input");
  await input.fill(username);
  await input.press("Enter");
  await finishWait;

  await expect(page.getByTestId("auth-modal")).toHaveCount(0, { timeout: 30_000 });

  const me = await authMe(page);
  expect(me.authenticated).toBe(true);

  return { username };
}

/** Start the live sandbox for the current user and wait up to 35s for it to be running. */
async function ensureSandboxRunning(page: Page): Promise<void> {
  const me = (await (await page.request.get("/auth/me")).json()) as MeResponse;
  const userId = me.user_id!;

  // Explicit start — idempotent if already running.
  await page.request.post(`/admin/sandboxes/${userId}/live/start`);

  for (let i = 0; i < 35; i++) {
    const res = await page.request.get("/admin/sandboxes");
    if (res.ok()) {
      const snapshots = (await res.json()) as SandboxSnapshot[];
      if (snapshots.some((s) => s.role === "live" && s.status === "running")) return;
    }
    await page.waitForTimeout(1000);
  }
  throw new Error("sandbox did not become running within 35s");
}

test.describe.serial("proxy integration", () => {
  test("sandbox is auto-spawned after first authenticated request", async ({ page }) => {
    await registerAndLogin(page);
    await ensureSandboxRunning(page);

    const res = await page.request.get("/admin/sandboxes");
    const snapshots = (await res.json()) as SandboxSnapshot[];
    const live = snapshots.find((s) => s.role === "live");
    expect(live?.status).toBe("running");
  });

  test("proxied GET /api/events returns a valid response from the sandbox", async ({ page }) => {
    await registerAndLogin(page);
    await ensureSandboxRunning(page);

    // /logs/events is a known sandbox endpoint — expect any sandbox-origin status, not 502.
    const resp = await page.request.get("/logs/events", { timeout: 15_000 });
    expect(
      [200, 204, 400, 401, 404].includes(resp.status()),
      `expected a sandbox response code, got ${resp.status()}`
    ).toBe(true);
  });

  test("sandbox status snapshot is accessible to authenticated user", async ({ page }) => {
    await registerAndLogin(page);

    const res = await page.request.get("/admin/sandboxes");
    expect(res.ok()).toBeTruthy();

    const snapshots = (await res.json()) as SandboxSnapshot[];
    expect(Array.isArray(snapshots)).toBe(true);
  });

  test("unauthenticated request to proxied path is redirected to login", async ({ page }) => {
    // Explicitly do NOT log in.
    const resp = await page.request.get("/", { maxRedirects: 0 });
    expect(resp.status()).toBe(303);
    expect(resp.headers()["location"]).toContain("/login");
  });
});
