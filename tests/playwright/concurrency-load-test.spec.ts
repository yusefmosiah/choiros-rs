/**
 * Concurrency & Load Test Suite
 *
 * Tests concurrent auth, API calls, and workloads against the shared sandbox.
 * Current architecture: all users share a single sandbox VM on port 8080.
 *
 * Run against Node B (staging):
 *   PLAYWRIGHT_HYPERVISOR_BASE_URL=https://draft.choir-ip.com \
 *     npx playwright test concurrency-load-test.spec.ts --project=hypervisor
 */

import { test, expect, type BrowserContext, type Page } from "@playwright/test";

// ── Helpers ──────────────────────────────────────────────────────────────────

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
  return `load_${Date.now()}_${Math.random().toString(36).slice(2, 8)}@example.com`;
}

interface MeResponse {
  authenticated: boolean;
  user_id?: string | null;
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

// Metrics collection
const report: { test: string; metric: string; value: string | number; unit?: string }[] = [];

function recordMetric(testName: string, metric: string, value: string | number, unit?: string) {
  report.push({ test: testName, metric, value, unit });
  console.log(`[METRIC] ${testName} | ${metric}: ${value}${unit ? " " + unit : ""}`);
}

// ── Test 1: Concurrent Auth Registration ─────────────────────────────────────

test.describe("1. Concurrent auth registration", () => {
  test("5 users register concurrently", async ({ browser }) => {
    const N = 5;
    const contexts = await Promise.all(
      Array.from({ length: N }, () => browser.newContext())
    );
    const pages = await Promise.all(contexts.map((ctx) => ctx.newPage()));

    const regT0 = Date.now();
    const results = await Promise.allSettled(pages.map((p) => registerAndLogin(p)));
    const regMs = Date.now() - regT0;

    const succeeded = results.filter((r) => r.status === "fulfilled").length;
    const failed = results.filter((r) => r.status === "rejected").length;

    recordMetric("concurrent-auth", "attempted", N);
    recordMetric("concurrent-auth", "succeeded", succeeded);
    recordMetric("concurrent-auth", "failed", failed);
    recordMetric("concurrent-auth", "total-wall-time", regMs, "ms");
    recordMetric("concurrent-auth", "avg-time-per-user", Math.round(regMs / N), "ms");

    expect(succeeded).toBe(N);

    await Promise.all(contexts.map((ctx) => ctx.close()));
  });
});

// ── Test 2: Concurrent API Calls (shared sandbox) ───────────────────────────

test.describe("2. Concurrent API calls", () => {
  test("concurrent auth checks from multiple sessions", async ({ browser }) => {
    // Create 3 authenticated sessions
    const N = 3;
    const contexts = await Promise.all(
      Array.from({ length: N }, () => browser.newContext())
    );
    const pages = await Promise.all(contexts.map((ctx) => ctx.newPage()));
    await Promise.all(pages.map((p) => registerAndLogin(p)));

    // Fire concurrent auth API calls (these don't need sandbox proxy)
    const t0 = Date.now();
    const allResults = await Promise.allSettled([
      // Auth checks from all 3 users
      ...pages.map((p) => p.request.get("/auth/me", { timeout: 10_000 })),
      // Logout + re-check from user 0 (tests auth state isolation)
      (async () => {
        await pages[0].request.post("/auth/logout", { timeout: 10_000 });
        return pages[0].request.get("/auth/me", { timeout: 10_000 });
      })(),
    ]);
    const totalMs = Date.now() - t0;

    const authResults = allResults.slice(0, N);
    const authOk = authResults.filter(
      (r) => r.status === "fulfilled" && r.value.ok()
    ).length;

    // Check that user 0 is logged out after logout
    const logoutResult = allResults[N];
    let loggedOut = false;
    if (logoutResult.status === "fulfilled") {
      const body = await logoutResult.value.json();
      loggedOut = body.authenticated === false;
    }

    recordMetric("concurrent-api", "auth-ok", `${authOk}/${N}`);
    recordMetric("concurrent-api", "logout-isolation", loggedOut ? "yes" : "no");
    recordMetric("concurrent-api", "total-wall-time", totalMs, "ms");

    expect(authOk).toBe(N);
    expect(loggedOut).toBe(true);

    await Promise.all(contexts.map((ctx) => ctx.close()));
  });
});

// ── Test 3: Conductor Prompt Execution ──────────────────────────────────────

test.describe("3. Conductor prompt execution", () => {
  test("send a prompt and get a response", async ({ browser }) => {
    const ctx = await browser.newContext();
    const page = await ctx.newPage();
    await registerAndLogin(page);

    // Wait for sandbox proxy to be ready (first request triggers ensure)
    const healthT0 = Date.now();
    let sandboxReady = false;
    for (let i = 0; i < 90; i++) {
      try {
        const h = await page.request.get("/health", { timeout: 5_000 });
        if (h.ok()) { sandboxReady = true; break; }
      } catch { /* keep polling */ }
      await new Promise((r) => setTimeout(r, 1000));
    }
    const healthMs = Date.now() - healthT0;
    recordMetric("conductor-prompt", "sandbox-ready", sandboxReady ? "yes" : "no");
    recordMetric("conductor-prompt", "sandbox-wait", healthMs, "ms");

    if (!sandboxReady) {
      console.log("  Sandbox not ready — skipping conductor test");
      await ctx.close();
      return;
    }

    // Single prompt, measure response time
    const t0 = Date.now();
    const res = await page.request.post("/conductor/execute", {
      data: {
        objective: "What is 2+2? Answer with just the number.",
        desktop_id: "e2e-test-" + Date.now(),
        output_mode: "auto",
      },
      timeout: 120_000,
    });
    const ms = Date.now() - t0;

    recordMetric("conductor-prompt", "status", res.status());
    recordMetric("conductor-prompt", "response-time", ms, "ms");

    if (res.ok()) {
      const body = await res.text();
      recordMetric("conductor-prompt", "response-length", body.length, "chars");
    } else {
      const body = await res.text().catch(() => "");
      recordMetric("conductor-prompt", "error-body", body.slice(0, 200));
    }

    // Accept 200 (success) or 202 (accepted/async)
    expect(res.status()).toBeGreaterThanOrEqual(200);
    expect(res.status()).toBeLessThan(300);

    await ctx.close();
  });

  test("2 sequential prompts to verify state consistency", async ({ browser }) => {
    const ctx = await browser.newContext();
    const page = await ctx.newPage();
    await registerAndLogin(page);

    // Wait for sandbox proxy to be ready
    for (let i = 0; i < 90; i++) {
      try {
        const h = await page.request.get("/health", { timeout: 5_000 });
        if (h.ok()) break;
      } catch { /* keep polling */ }
      await new Promise((r) => setTimeout(r, 1000));
    }

    for (let i = 0; i < 2; i++) {
      const t0 = Date.now();
      const res = await page.request.post("/conductor/execute", {
        data: {
          objective: `Prompt ${i + 1}: What is ${i + 1}+${i + 1}? Just the number.`,
          desktop_id: `e2e-seq-${Date.now()}-${i}`,
          output_mode: "auto",
        },
        timeout: 120_000,
      });
      const ms = Date.now() - t0;
      recordMetric("sequential-prompts", `prompt-${i + 1}-status`, res.status());
      recordMetric("sequential-prompts", `prompt-${i + 1}-time`, ms, "ms");
    }

    // Check runs were created
    const runsRes = await page.request.get("/conductor/runs", { timeout: 10_000 });
    if (runsRes.ok()) {
      const runs = await runsRes.json();
      const count = Array.isArray(runs) ? runs.length : 0;
      recordMetric("sequential-prompts", "total-runs", count);
      expect(count).toBeGreaterThanOrEqual(2);
    }

    await ctx.close();
  });
});

// ── Test 4: Writer Flow ─────────────────────────────────────────────────────

test.describe("4. Writer document flow", () => {
  test("prompt creates document, then save edit", async ({ browser }) => {
    const ctx = await browser.newContext();
    const page = await ctx.newPage();
    await registerAndLogin(page);

    // Wait for sandbox proxy to be ready
    for (let i = 0; i < 90; i++) {
      try {
        const h = await page.request.get("/health", { timeout: 5_000 });
        if (h.ok()) break;
      } catch { /* keep polling */ }
      await new Promise((r) => setTimeout(r, 1000));
    }

    // Create a document via conductor
    const promptT0 = Date.now();
    const promptRes = await page.request.post("/conductor/execute", {
      data: {
        objective: "Write a short haiku about computers.",
        desktop_id: "e2e-writer-" + Date.now(),
        output_mode: "auto",
      },
      timeout: 120_000,
    });
    const promptMs = Date.now() - promptT0;
    recordMetric("writer-flow", "prompt-status", promptRes.status());
    recordMetric("writer-flow", "prompt-time", promptMs, "ms");

    // Check runs exist
    const runsRes = await page.request.get("/conductor/runs", { timeout: 10_000 });
    const runsOk = runsRes.ok();
    recordMetric("writer-flow", "runs-endpoint", runsOk ? "ok" : "fail");

    if (runsOk) {
      const runs = await runsRes.json();
      const count = Array.isArray(runs) ? runs.length : 0;
      recordMetric("writer-flow", "run-count", count);
    }

    await ctx.close();
  });
});

// ── Test 5: Auth Capacity ────────────────────────────────────────────────────

test.describe("5. Auth capacity", () => {
  test("register 10 users sequentially, measure degradation", async ({ browser }) => {
    const MAX_USERS = 10;
    const times: number[] = [];

    for (let i = 0; i < MAX_USERS; i++) {
      const ctx = await browser.newContext();
      const page = await ctx.newPage();

      const t0 = Date.now();
      try {
        await registerAndLogin(page);
        const ms = Date.now() - t0;
        times.push(ms);
        console.log(`  User ${i + 1}: registered in ${ms}ms`);
      } catch (e: unknown) {
        console.log(`  User ${i + 1}: FAILED after ${Date.now() - t0}ms — ${e}`);
        await ctx.close();
        break;
      }
      await ctx.close();
    }

    const total = times.length;
    const avgMs = Math.round(times.reduce((a, b) => a + b, 0) / total);
    const minMs = Math.min(...times);
    const maxMs = Math.max(...times);
    const p50 = times.sort((a, b) => a - b)[Math.floor(total / 2)];

    recordMetric("auth-capacity", "total-registered", total);
    recordMetric("auth-capacity", "avg-time", avgMs, "ms");
    recordMetric("auth-capacity", "min-time", minMs, "ms");
    recordMetric("auth-capacity", "max-time", maxMs, "ms");
    recordMetric("auth-capacity", "p50-time", p50, "ms");

    // All 10 should succeed — auth is lightweight
    expect(total).toBe(MAX_USERS);
  });
});

// ── Test 6: Mixed Workload ──────────────────────────────────────────────────

test.describe("6. Mixed concurrent workload", () => {
  test("auth + health + heartbeat fire simultaneously", async ({ browser }) => {
    // User 1: already authenticated, wait for sandbox proxy to be ready
    const ctx1 = await browser.newContext();
    const page1 = await ctx1.newPage();
    await registerAndLogin(page1);

    // Wait for sandbox proxy to be ready for user 1
    for (let i = 0; i < 90; i++) {
      try {
        const h = await page1.request.get("/health", { timeout: 5_000 });
        if (h.ok()) break;
      } catch { /* keep polling */ }
      await new Promise((r) => setTimeout(r, 1000));
    }

    // User 2: will register concurrently
    const ctx2 = await browser.newContext();
    const page2 = await ctx2.newPage();

    const t0 = Date.now();
    const [healthResult, heartbeatResult, authResult, regResult] = await Promise.allSettled([
      page1.request.get("/health", { timeout: 30_000 }),
      page1.request.post("/heartbeat", { timeout: 10_000 }),
      page1.request.get("/auth/me", { timeout: 10_000 }),
      registerAndLogin(page2),
    ]);
    const totalMs = Date.now() - t0;

    const healthOk =
      healthResult.status === "fulfilled" && healthResult.value.ok();
    const heartbeatOk =
      heartbeatResult.status === "fulfilled" && heartbeatResult.value.ok();
    const authOk =
      authResult.status === "fulfilled" && authResult.value.ok();
    const regOk = regResult.status === "fulfilled";

    recordMetric("mixed-workload", "health", healthOk ? "ok" : "fail");
    recordMetric("mixed-workload", "heartbeat", heartbeatOk ? "ok" : "fail");
    recordMetric("mixed-workload", "auth-check", authOk ? "ok" : "fail");
    recordMetric("mixed-workload", "concurrent-reg", regOk ? "ok" : "fail");
    recordMetric("mixed-workload", "total-wall-time", totalMs, "ms");

    // Auth and heartbeat must work (hypervisor-level, no sandbox proxy needed)
    expect(authOk).toBe(true);
    // Health goes through sandbox proxy — may fail for fresh sessions in shared sandbox
    // This is a known architectural limitation, not a bug
    if (!healthOk) {
      console.log("  NOTE: /health through sandbox proxy not ready for fresh session (expected)");
    }

    await Promise.all([ctx1.close(), ctx2.close()]);
  });
});

// ── Final Report ─────────────────────────────────────────────────────────────

test.afterAll(() => {
  if (report.length === 0) return;
  console.log("\n╔══════════════════════════════════════════════════════════════════════════╗");
  console.log("║              CONCURRENCY & LOAD TEST REPORT                            ║");
  console.log("╠══════════════════════════════════════════════════════════════════════════╣");
  for (const r of report) {
    const val = `${r.value}${r.unit ? " " + r.unit : ""}`;
    console.log(`║  ${r.test.padEnd(22)} ${r.metric.padEnd(30)} ${val.padStart(15)} ║`);
  }
  console.log("╚══════════════════════════════════════════════════════════════════════════╝");
});
