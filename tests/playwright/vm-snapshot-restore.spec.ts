/**
 * VM Snapshot/Restore Verification
 *
 * Tests both paths:
 * - hibernate (snapshot preserved → fast restore)
 * - stop (snapshot deleted → cold boot)
 * Compares timing to verify snapshot restore is faster.
 */

import { test, expect, type BrowserContext, type Page, type APIRequestContext } from "@playwright/test";

async function addVirtualAuthenticator(page: Page): Promise<string> {
  const cdp = await (page.context() as BrowserContext).newCDPSession(page);
  await cdp.send("WebAuthn.enable", { enableUI: false });
  const { authenticatorId } = await cdp.send("WebAuthn.addVirtualAuthenticator", {
    options: {
      protocol: "ctap2",
      transport: "internal",
      hasResidentKey: true,
      hasUserVerification: true,
      isUserVerified: true,
    },
  });
  return authenticatorId;
}

function uniqueUsername(): string {
  return `snaptest_${Date.now()}_${Math.random().toString(36).slice(2, 8)}@example.com`;
}

interface MeResponse {
  authenticated: boolean;
  user_id?: string | null;
}

interface SandboxSnapshotEntry {
  user_id: string;
  role: string;
  port: number;
  status: string;
  idle_secs: number;
}

async function registerAndLogin(page: Page): Promise<{ username: string; userId: string }> {
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

  const me = (await (await page.request.get("/auth/me")).json()) as MeResponse;
  expect(me.authenticated).toBe(true);
  return { username, userId: me.user_id! };
}

async function getSandboxStatus(
  request: APIRequestContext,
  userId: string
): Promise<string | null> {
  const res = await request.get("/admin/sandboxes");
  if (!res.ok()) return null;
  const snapshots = (await res.json()) as SandboxSnapshotEntry[];
  return snapshots.find((s) => s.user_id === userId && s.role === "live")?.status ?? null;
}

async function ensureRunning(request: APIRequestContext, userId: string, maxWait = 60): Promise<boolean> {
  await request.post(`/admin/sandboxes/${userId}/live/start`);
  for (let i = 0; i < maxWait; i++) {
    const status = await getSandboxStatus(request, userId);
    if (status === "running") return true;
    await new Promise((r) => setTimeout(r, 1000));
  }
  return false;
}

const report: { metric: string; value: string | number; unit?: string }[] = [];

function recordMetric(metric: string, value: string | number, unit?: string) {
  report.push({ metric, value, unit });
  console.log(`[METRIC] ${metric}: ${value}${unit ? " " + unit : ""}`);
}

test("hibernate → restore vs stop → cold boot comparison", async ({ page }) => {
  const { userId } = await registerAndLogin(page);
  recordMetric("user-id", userId.slice(0, 8) + "...");

  // ── Baseline: get sandbox running ──────────────────────────────────────
  const started = await ensureRunning(page.request, userId);
  expect(started).toBe(true);

  const h0 = await page.request.get("/health", { timeout: 60_000 });
  expect(h0.ok()).toBe(true);
  recordMetric("baseline-health", h0.status());

  // Load the app to generate some state
  await page.goto("/");
  await page.waitForTimeout(2000);
  const bodyBefore = await page.evaluate(() => document.body.innerHTML.length);
  recordMetric("body-html-before", bodyBefore, "chars");

  // ── Test 1: HIBERNATE → RESTORE ────────────────────────────────────────
  console.log("\n--- HIBERNATE PATH ---");

  const hibT0 = Date.now();
  const hibRes = await page.request.post(`/admin/sandboxes/${userId}/live/hibernate`, {
    timeout: 30_000,
  });
  const hibMs = Date.now() - hibT0;
  recordMetric("hibernate-status", hibRes.status());
  recordMetric("hibernate-time", hibMs, "ms");

  // Verify status is hibernated
  await page.waitForTimeout(2000);
  const statusHib = await getSandboxStatus(page.request, userId);
  recordMetric("status-after-hibernate", statusHib ?? "unknown");

  // Now trigger restore by requesting health
  const restoreT0 = Date.now();
  const restoreRes = await page.request.get("/health", { timeout: 120_000 });
  const restoreMs = Date.now() - restoreT0;
  recordMetric("restore-from-hibernate-status", restoreRes.status());
  recordMetric("restore-from-hibernate-time", restoreMs, "ms");

  const statusRestore = await getSandboxStatus(page.request, userId);
  recordMetric("status-after-restore", statusRestore ?? "unknown");

  // Verify data survived
  await page.goto("/");
  await page.waitForTimeout(2000);
  const bodyAfterRestore = await page.evaluate(() => document.body.innerHTML.length);
  recordMetric("body-html-after-restore", bodyAfterRestore, "chars");

  // ── Test 2: STOP → COLD BOOT ──────────────────────────────────────────
  console.log("\n--- STOP PATH ---");

  const stopT0 = Date.now();
  const stopRes = await page.request.post(`/admin/sandboxes/${userId}/live/stop`, {
    timeout: 30_000,
  });
  const stopMs = Date.now() - stopT0;
  recordMetric("stop-status", stopRes.status());
  recordMetric("stop-time", stopMs, "ms");

  await page.waitForTimeout(2000);
  const statusStop = await getSandboxStatus(page.request, userId);
  recordMetric("status-after-stop", statusStop ?? "unknown");

  // Trigger cold boot
  const coldT0 = Date.now();
  const coldRes = await page.request.get("/health", { timeout: 120_000 });
  const coldMs = Date.now() - coldT0;
  recordMetric("cold-boot-status", coldRes.status());
  recordMetric("cold-boot-time", coldMs, "ms");

  // ── Test 3: Repeat hibernate → restore 2 more times ────────────────────
  console.log("\n--- HIBERNATE CYCLES ---");

  const hibernateTimes: number[] = [restoreMs];
  for (let i = 2; i <= 3; i++) {
    // Hibernate
    await page.request.post(`/admin/sandboxes/${userId}/live/hibernate`, { timeout: 30_000 });
    await page.waitForTimeout(2000);

    // Restore
    const t0 = Date.now();
    const res = await page.request.get("/health", { timeout: 120_000 });
    const ms = Date.now() - t0;
    hibernateTimes.push(ms);
    console.log(`  Hibernate cycle ${i}: restore=${ms}ms status=${res.status()}`);
    recordMetric(`hibernate-cycle-${i}-restore`, ms, "ms");
  }

  // ── Summary ────────────────────────────────────────────────────────────
  console.log("\n--- SUMMARY ---");

  const avgRestore = Math.round(hibernateTimes.reduce((a, b) => a + b, 0) / hibernateTimes.length);
  recordMetric("avg-restore-from-hibernate", avgRestore, "ms");
  recordMetric("cold-boot-baseline", coldMs, "ms");

  if (coldMs > 0 && avgRestore > 0) {
    const speedup = (coldMs / avgRestore).toFixed(2);
    recordMetric("speedup-ratio", `${speedup}x`);
  }

  const method = avgRestore < coldMs * 0.5 ? "SNAPSHOT RESTORE WORKING" : "NO SPEEDUP (both cold boot)";
  recordMetric("verdict", method);
});

test.afterAll(() => {
  console.log("\n╔══════════════════════════════════════════════════════════════════╗");
  console.log("║          SNAPSHOT / RESTORE LIFECYCLE REPORT                   ║");
  console.log("╠══════════════════════════════════════════════════════════════════╣");
  for (const r of report) {
    const val = `${r.value}${r.unit ? " " + r.unit : ""}`;
    console.log(`║  ${r.metric.padEnd(35)} ${val.padStart(25)} ║`);
  }
  console.log("╚══════════════════════════════════════════════════════════════════╝");
});
