/**
 * VM Lifecycle & Concurrency Report
 *
 * Comprehensive tests of Node B sandbox VM lifecycle:
 * - Cold boot timing
 * - Auto-ensure on request
 * - Health endpoint reliability
 * - Concurrent requests (same account)
 * - Multiple concurrent accounts
 * - Sandbox status transitions
 *
 * Run: PLAYWRIGHT_HYPERVISOR_BASE_URL=https://draft.choir-ip.com npx playwright test vm-lifecycle-report.spec.ts --reporter=list
 */

import { test, expect, type BrowserContext, type Page, type APIRequestContext } from "@playwright/test";

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
  return `vmtest_${Date.now()}_${Math.random().toString(36).slice(2, 8)}@example.com`;
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

  const me = await (await page.request.get("/auth/me")).json() as MeResponse;
  expect(me.authenticated).toBe(true);

  return { username, userId: me.user_id! };
}

async function getSandboxSnapshots(request: APIRequestContext): Promise<SandboxSnapshot[]> {
  const res = await request.get("/admin/sandboxes");
  if (!res.ok()) return [];
  return (await res.json()) as SandboxSnapshot[];
}

async function getSandboxStatus(request: APIRequestContext, userId: string, role = "live"): Promise<string | null> {
  const snapshots = await getSandboxSnapshots(request);
  const entry = snapshots.find((s) => s.user_id === userId && s.role === role);
  return entry?.status ?? null;
}

// ── Report accumulator ──────────────────────────────────────────────────────

const report: { test: string; metric: string; value: string | number; unit?: string }[] = [];

function recordMetric(testName: string, metric: string, value: string | number, unit?: string) {
  report.push({ test: testName, metric, value, unit });
  console.log(`[METRIC] ${testName} | ${metric}: ${value}${unit ? " " + unit : ""}`);
}

// ── Tests ────────────────────────────────────────────────────────────────────

test.describe.serial("VM lifecycle report", () => {
  test("1. cold boot: register + first sandbox start timing", async ({ page }) => {
    const regStart = Date.now();
    const { userId } = await registerAndLogin(page);
    const regMs = Date.now() - regStart;
    recordMetric("cold-boot", "registration", regMs, "ms");

    // Trigger sandbox start
    const startTs = Date.now();
    const startRes = await page.request.post(`/admin/sandboxes/${userId}/live/start`);
    expect(startRes.ok() || startRes.status() === 409).toBeTruthy(); // 409 = already starting

    // Poll until running
    let running = false;
    for (let i = 0; i < 60; i++) {
      const status = await getSandboxStatus(page.request, userId);
      if (status === "running") {
        running = true;
        break;
      }
      await page.waitForTimeout(1000);
    }
    const bootMs = Date.now() - startTs;
    expect(running).toBe(true);
    recordMetric("cold-boot", "sandbox-start-to-running", bootMs, "ms");
  });

  test("2. health endpoint: timing and reliability (10 sequential hits)", async ({ page }) => {
    await registerAndLogin(page);

    // Ensure sandbox is running first
    const snapshots = await getSandboxSnapshots(page.request);
    if (!snapshots.some((s) => s.role === "live" && s.status === "running")) {
      // Previous test left a sandbox running; if not, wait
      await page.waitForTimeout(5000);
    }

    const timings: number[] = [];
    let successes = 0;
    let failures = 0;

    for (let i = 0; i < 10; i++) {
      const t0 = Date.now();
      try {
        const res = await page.request.get("/health", { timeout: 10_000 });
        const elapsed = Date.now() - t0;
        timings.push(elapsed);
        if (res.ok()) successes++;
        else failures++;
      } catch {
        const elapsed = Date.now() - t0;
        timings.push(elapsed);
        failures++;
      }
    }

    recordMetric("health-endpoint", "successes", successes);
    recordMetric("health-endpoint", "failures", failures);
    recordMetric("health-endpoint", "avg-latency", Math.round(timings.reduce((a, b) => a + b, 0) / timings.length), "ms");
    recordMetric("health-endpoint", "min-latency", Math.min(...timings), "ms");
    recordMetric("health-endpoint", "max-latency", Math.max(...timings), "ms");
  });

  test("3. concurrent requests: 5 parallel API calls from same account", async ({ page }) => {
    const { userId } = await registerAndLogin(page);

    // Ensure sandbox is running
    await page.request.post(`/admin/sandboxes/${userId}/live/start`);
    for (let i = 0; i < 45; i++) {
      const status = await getSandboxStatus(page.request, userId);
      if (status === "running") break;
      await page.waitForTimeout(1000);
    }

    // Fire 5 concurrent requests
    const endpoints = ["/health", "/api/events", "/api/desktop", "/logs/events", "/health"];
    const t0 = Date.now();
    const results = await Promise.all(
      endpoints.map(async (ep) => {
        const start = Date.now();
        try {
          const res = await page.request.get(ep, { timeout: 15_000 });
          return { endpoint: ep, status: res.status(), ms: Date.now() - start };
        } catch (e: unknown) {
          return { endpoint: ep, status: 0, ms: Date.now() - start, error: String(e) };
        }
      })
    );
    const totalMs = Date.now() - t0;

    let ok = 0;
    let bad = 0;
    for (const r of results) {
      console.log(`  ${r.endpoint} → ${r.status} (${r.ms}ms)`);
      // Accept sandbox-origin responses (not 502/503)
      if (r.status > 0 && r.status !== 502 && r.status !== 503) ok++;
      else bad++;
    }

    recordMetric("concurrent-same-account", "total-wall-time", totalMs, "ms");
    recordMetric("concurrent-same-account", "successful", ok);
    recordMetric("concurrent-same-account", "failed-502-503", bad);
  });

  test("4. concurrent requests: 10 parallel hits to same endpoint", async ({ page }) => {
    const { userId } = await registerAndLogin(page);

    await page.request.post(`/admin/sandboxes/${userId}/live/start`);
    for (let i = 0; i < 45; i++) {
      const status = await getSandboxStatus(page.request, userId);
      if (status === "running") break;
      await page.waitForTimeout(1000);
    }

    // 10 concurrent GETs to /health
    const t0 = Date.now();
    const results = await Promise.all(
      Array.from({ length: 10 }, (_, i) =>
        (async () => {
          const start = Date.now();
          try {
            const res = await page.request.get("/health", { timeout: 15_000 });
            return { i, status: res.status(), ms: Date.now() - start };
          } catch (e: unknown) {
            return { i, status: 0, ms: Date.now() - start, error: String(e) };
          }
        })()
      )
    );
    const totalMs = Date.now() - t0;

    const statuses = results.map((r) => r.status);
    const latencies = results.map((r) => r.ms);
    const ok = statuses.filter((s) => s === 200).length;

    recordMetric("concurrent-10-health", "total-wall-time", totalMs, "ms");
    recordMetric("concurrent-10-health", "200-OK", ok);
    recordMetric("concurrent-10-health", "non-200", 10 - ok);
    recordMetric("concurrent-10-health", "avg-latency", Math.round(latencies.reduce((a, b) => a + b, 0) / latencies.length), "ms");
    recordMetric("concurrent-10-health", "max-latency", Math.max(...latencies), "ms");
  });

  test("5. sandbox status API: list all sandboxes", async ({ page }) => {
    await registerAndLogin(page);
    const snapshots = await getSandboxSnapshots(page.request);

    recordMetric("sandbox-status", "total-sandboxes", snapshots.length);
    for (const s of snapshots) {
      console.log(`  user=${s.user_id.slice(0, 8)}... role=${s.role} status=${s.status} port=${s.port} idle=${s.idle_secs}s`);
    }

    const running = snapshots.filter((s) => s.status === "running").length;
    const stopped = snapshots.filter((s) => s.status === "stopped").length;
    recordMetric("sandbox-status", "running", running);
    recordMetric("sandbox-status", "stopped", stopped);
  });
});

test.describe("VM multi-account concurrency", () => {
  test("6. three users register and start sandboxes concurrently", async ({ browser }) => {
    // Create 3 isolated browser contexts
    const contexts = await Promise.all(
      Array.from({ length: 3 }, () => browser.newContext())
    );
    const pages = await Promise.all(contexts.map((ctx) => ctx.newPage()));

    // Register all 3 concurrently
    const t0 = Date.now();
    const users = await Promise.all(pages.map((p) => registerAndLogin(p)));
    const regMs = Date.now() - t0;
    recordMetric("multi-account", "3-concurrent-registrations", regMs, "ms");

    // Start all 3 sandboxes concurrently
    const startT0 = Date.now();
    await Promise.all(
      users.map((u, i) => pages[i].request.post(`/admin/sandboxes/${u.userId}/live/start`))
    );

    // Wait for all to be running
    const allRunning = await Promise.all(
      users.map(async (u, i) => {
        for (let j = 0; j < 60; j++) {
          const status = await getSandboxStatus(pages[i].request, u.userId);
          if (status === "running") return { userId: u.userId, ms: Date.now() - startT0 };
          await pages[i].waitForTimeout(1000);
        }
        return { userId: u.userId, ms: -1 }; // Timeout
      })
    );
    const startMs = Date.now() - startT0;

    for (const r of allRunning) {
      console.log(`  user=${r.userId.slice(0, 8)}... boot=${r.ms}ms`);
      recordMetric("multi-account", `user-${r.userId.slice(0, 8)}-boot`, r.ms, "ms");
    }

    const succeeded = allRunning.filter((r) => r.ms > 0).length;
    recordMetric("multi-account", "sandboxes-started", succeeded);
    recordMetric("multi-account", "total-concurrent-start-time", startMs, "ms");

    // Now hit all 3 health endpoints concurrently
    const healthResults = await Promise.all(
      pages.map(async (p, i) => {
        const start = Date.now();
        try {
          const res = await p.request.get("/health", { timeout: 10_000 });
          return { user: users[i].userId.slice(0, 8), status: res.status(), ms: Date.now() - start };
        } catch {
          return { user: users[i].userId.slice(0, 8), status: 0, ms: Date.now() - start };
        }
      })
    );

    for (const r of healthResults) {
      console.log(`  user=${r.user}... health=${r.status} (${r.ms}ms)`);
    }

    const healthOk = healthResults.filter((r) => r.status === 200).length;
    recordMetric("multi-account", "concurrent-health-ok", healthOk);

    // Cleanup contexts
    await Promise.all(contexts.map((ctx) => ctx.close()));
  });

  test("7. page load race: two users load the app simultaneously", async ({ browser }) => {
    const contexts = await Promise.all([browser.newContext(), browser.newContext()]);
    const pages = await Promise.all(contexts.map((ctx) => ctx.newPage()));

    // Register both
    await Promise.all(pages.map((p) => registerAndLogin(p)));

    // Both navigate to / simultaneously
    const t0 = Date.now();
    const loadResults = await Promise.all(
      pages.map(async (p, i) => {
        const start = Date.now();
        try {
          await p.goto("/", { timeout: 60_000, waitUntil: "networkidle" });
          return { user: i, ms: Date.now() - start, ok: true };
        } catch (e: unknown) {
          return { user: i, ms: Date.now() - start, ok: false, error: String(e) };
        }
      })
    );
    const totalMs = Date.now() - t0;

    for (const r of loadResults) {
      console.log(`  user${r.user} page-load: ${r.ok ? "OK" : "FAILED"} (${r.ms}ms)`);
      recordMetric("page-load-race", `user${r.user}-load`, r.ms, "ms");
    }
    recordMetric("page-load-race", "total-wall-time", totalMs, "ms");
    recordMetric("page-load-race", "succeeded", loadResults.filter((r) => r.ok).length);

    await Promise.all(contexts.map((ctx) => ctx.close()));
  });

  test("8. WebSocket connection: verify ws upgrade through proxy", async ({ page }) => {
    const { userId } = await registerAndLogin(page);

    // Ensure sandbox running
    await page.request.post(`/admin/sandboxes/${userId}/live/start`);
    for (let i = 0; i < 45; i++) {
      const status = await getSandboxStatus(page.request, userId);
      if (status === "running") break;
      await page.waitForTimeout(1000);
    }

    // Try to establish WebSocket via page eval
    const wsResult = await page.evaluate(async () => {
      return new Promise<{ connected: boolean; messages: number; error?: string }>((resolve) => {
        const protocol = location.protocol === "https:" ? "wss:" : "ws:";
        const ws = new WebSocket(`${protocol}//${location.host}/ws`);
        let messages = 0;

        const timeout = setTimeout(() => {
          ws.close();
          resolve({ connected: true, messages });
        }, 5000);

        ws.onopen = () => {
          // Connection established — listen for messages briefly
        };

        ws.onmessage = () => {
          messages++;
        };

        ws.onerror = (e) => {
          clearTimeout(timeout);
          resolve({ connected: false, messages, error: "ws error" });
        };

        ws.onclose = () => {
          clearTimeout(timeout);
          resolve({ connected: messages > 0 || ws.readyState !== WebSocket.CONNECTING, messages });
        };
      });
    });

    recordMetric("websocket", "connected", wsResult.connected ? "yes" : "no");
    recordMetric("websocket", "messages-in-5s", wsResult.messages);
    if (wsResult.error) recordMetric("websocket", "error", wsResult.error);
  });
});

// Print final report
test.afterAll(() => {
  console.log("\n╔══════════════════════════════════════════════════════════════════╗");
  console.log("║              VM LIFECYCLE & CONCURRENCY REPORT                 ║");
  console.log("╠══════════════════════════════════════════════════════════════════╣");
  for (const r of report) {
    const val = `${r.value}${r.unit ? " " + r.unit : ""}`;
    console.log(`║  ${r.test.padEnd(25)} ${r.metric.padEnd(30)} ${val.padStart(8)} ║`);
  }
  console.log("╚══════════════════════════════════════════════════════════════════╝");
});
