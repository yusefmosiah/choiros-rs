/**
 * VM Lifecycle Stress Tests — Deeper exploration of Node B behavior
 *
 * Tests:
 * - Port sharing discovery (are all sandboxes the same process?)
 * - Rapid sequential registrations
 * - WASM app rendering verification (not just HTTP 200)
 * - Desktop API through proxy
 * - Long-polling / event stream behavior
 * - Multiple concurrent page navigations with full rendering
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
  return `stress_${Date.now()}_${Math.random().toString(36).slice(2, 8)}@example.com`;
}

interface MeResponse {
  authenticated: boolean;
  user_id?: string | null;
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

const report: { test: string; metric: string; value: string | number; unit?: string }[] = [];

function recordMetric(testName: string, metric: string, value: string | number, unit?: string) {
  report.push({ test: testName, metric, value, unit });
  console.log(`[METRIC] ${testName} | ${metric}: ${value}${unit ? " " + unit : ""}`);
}

// ── Tests ────────────────────────────────────────────────────────────────────

test.describe("VM stress & discovery", () => {
  test("1. port allocation discovery: do different users get different ports?", async ({ browser }) => {
    const contexts = await Promise.all(
      Array.from({ length: 3 }, () => browser.newContext())
    );
    const pages = await Promise.all(contexts.map((ctx) => ctx.newPage()));
    const users = await Promise.all(pages.map((p) => registerAndLogin(p)));

    // Start all sandboxes
    await Promise.all(
      users.map((u, i) => pages[i].request.post(`/admin/sandboxes/${u.userId}/live/start`))
    );
    await pages[0].waitForTimeout(2000);

    // Get snapshots from first page (admin endpoint)
    const snapshots = await getSandboxSnapshots(pages[0].request);
    const userPorts = new Map<string, number>();
    for (const u of users) {
      const entry = snapshots.find((s) => s.user_id === u.userId && s.role === "live");
      if (entry) userPorts.set(u.userId.slice(0, 8), entry.port);
    }

    const uniquePorts = new Set(userPorts.values());
    console.log(`  User ports: ${JSON.stringify(Object.fromEntries(userPorts))}`);
    console.log(`  Unique ports: ${[...uniquePorts].join(", ")}`);

    recordMetric("port-discovery", "users-tested", 3);
    recordMetric("port-discovery", "unique-ports", uniquePorts.size);
    recordMetric("port-discovery", "shared-port", uniquePorts.size === 1 ? "yes" : "no");

    await Promise.all(contexts.map((ctx) => ctx.close()));
  });

  test("2. rapid sequential registrations: 5 users back-to-back", async ({ browser }) => {
    const timings: number[] = [];

    for (let i = 0; i < 5; i++) {
      const ctx = await browser.newContext();
      const page = await ctx.newPage();
      const t0 = Date.now();
      const { userId } = await registerAndLogin(page);
      timings.push(Date.now() - t0);
      console.log(`  User ${i}: registered in ${timings[i]}ms (${userId.slice(0, 8)}...)`);
      await ctx.close();
    }

    recordMetric("rapid-register", "total-users", 5);
    recordMetric("rapid-register", "avg-time", Math.round(timings.reduce((a, b) => a + b, 0) / timings.length), "ms");
    recordMetric("rapid-register", "min-time", Math.min(...timings), "ms");
    recordMetric("rapid-register", "max-time", Math.max(...timings), "ms");
  });

  test("3. full WASM render verification: app loads and shows desktop", async ({ page }) => {
    await registerAndLogin(page);

    const t0 = Date.now();
    await page.goto("/");

    // Wait for WASM to boot and render the desktop
    // Check that body has meaningful content (not just style/script tags)
    const desktopRendered = await page.evaluate(() => {
      return new Promise<boolean>((resolve) => {
        const check = () => {
          // Look for rendered DOM elements beyond script/style
          const mainEl = document.getElementById("main");
          if (mainEl && mainEl.children.length > 0) {
            const hasVisibleContent = Array.from(mainEl.querySelectorAll("*")).some(
              (el) => !["SCRIPT", "STYLE"].includes(el.tagName) && el.getBoundingClientRect().height > 0
            );
            if (hasVisibleContent) { resolve(true); return; }
          }
          setTimeout(check, 200);
        };
        setTimeout(() => resolve(false), 30000);
        check();
      });
    });

    const renderMs = Date.now() - t0;

    // Check for console errors
    const consoleErrors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") consoleErrors.push(msg.text());
    });

    // Take a snapshot of what's rendered
    const bodyHTML = await page.evaluate(() => document.body.innerHTML.length);
    const scriptCount = await page.evaluate(() => document.querySelectorAll("script").length);

    recordMetric("wasm-render", "desktop-rendered", desktopRendered ? "yes" : "no");
    recordMetric("wasm-render", "render-time", renderMs, "ms");
    recordMetric("wasm-render", "body-html-length", bodyHTML, "chars");
    recordMetric("wasm-render", "script-tags", scriptCount);

    // Verify it's not a blank page
    expect(bodyHTML).toBeGreaterThan(100);
  });

  test("4. desktop API depth: create desktop + list + verify through proxy", async ({ page }) => {
    const { userId } = await registerAndLogin(page);

    // Ensure sandbox running
    await page.request.post(`/admin/sandboxes/${userId}/live/start`);
    for (let i = 0; i < 45; i++) {
      const snapshots = await getSandboxSnapshots(page.request);
      if (snapshots.some((s) => s.user_id === userId && s.status === "running")) break;
      await page.waitForTimeout(1000);
    }

    // List desktops
    const listT0 = Date.now();
    const listRes = await page.request.get("/api/desktop", { timeout: 10_000 });
    const listMs = Date.now() - listT0;

    recordMetric("desktop-api", "list-status", listRes.status());
    recordMetric("desktop-api", "list-latency", listMs, "ms");

    if (listRes.ok()) {
      const desktops = await listRes.json();
      recordMetric("desktop-api", "desktop-count", Array.isArray(desktops) ? desktops.length : "n/a");
    }

    // Get events
    const eventsT0 = Date.now();
    const eventsRes = await page.request.get("/logs/events?limit=5", { timeout: 15_000 });
    const eventsMs = Date.now() - eventsT0;

    recordMetric("desktop-api", "events-status", eventsRes.status());
    recordMetric("desktop-api", "events-latency", eventsMs, "ms");
  });

  test("5. concurrent full page renders: 4 users load app simultaneously", async ({ browser }) => {
    // Create 4 contexts and register
    const contexts = await Promise.all(
      Array.from({ length: 4 }, () => browser.newContext())
    );
    const pages = await Promise.all(contexts.map((ctx) => ctx.newPage()));
    await Promise.all(pages.map((p) => registerAndLogin(p)));

    // All 4 navigate to / at the same time
    const t0 = Date.now();
    const results = await Promise.all(
      pages.map(async (p, i) => {
        const start = Date.now();
        try {
          await p.goto("/", { timeout: 45_000, waitUntil: "domcontentloaded" });
          // Wait for WASM to hydrate
          const hasContent = await p.evaluate(() => {
            return new Promise<boolean>((resolve) => {
              const check = () => {
                if (document.body.innerHTML.length > 200) {
                  resolve(true);
                  return;
                }
                setTimeout(check, 200);
              };
              setTimeout(() => resolve(false), 15000);
              check();
            });
          });
          return { user: i, ms: Date.now() - start, rendered: hasContent, error: null };
        } catch (e: unknown) {
          return { user: i, ms: Date.now() - start, rendered: false, error: String(e) };
        }
      })
    );
    const totalMs = Date.now() - t0;

    for (const r of results) {
      console.log(`  user${r.user}: rendered=${r.rendered} (${r.ms}ms)${r.error ? " error=" + r.error : ""}`);
    }

    const rendered = results.filter((r) => r.rendered).length;
    const avgMs = Math.round(results.reduce((a, r) => a + r.ms, 0) / results.length);

    recordMetric("concurrent-render", "users", 4);
    recordMetric("concurrent-render", "rendered-ok", rendered);
    recordMetric("concurrent-render", "avg-render-time", avgMs, "ms");
    recordMetric("concurrent-render", "total-wall-time", totalMs, "ms");

    await Promise.all(contexts.map((ctx) => ctx.close()));
  });

  test("6. sandbox recovery: stop sandbox, verify auto-restart on request", async ({ page }) => {
    const { userId } = await registerAndLogin(page);

    // Start sandbox
    await page.request.post(`/admin/sandboxes/${userId}/live/start`);
    for (let i = 0; i < 45; i++) {
      const snapshots = await getSandboxSnapshots(page.request);
      if (snapshots.some((s) => s.user_id === userId && s.status === "running")) break;
      await page.waitForTimeout(1000);
    }

    // Verify it's running
    const healthBefore = await page.request.get("/health", { timeout: 10_000 });
    recordMetric("sandbox-recovery", "health-before-stop", healthBefore.status());

    // Stop it
    const stopRes = await page.request.post(`/admin/sandboxes/${userId}/live/stop`);
    recordMetric("sandbox-recovery", "stop-status", stopRes.status());

    await page.waitForTimeout(2000);

    // Verify it's stopped
    const statusAfterStop = await (async () => {
      const snapshots = await getSandboxSnapshots(page.request);
      return snapshots.find((s) => s.user_id === userId && s.role === "live")?.status ?? "unknown";
    })();
    recordMetric("sandbox-recovery", "status-after-stop", statusAfterStop);

    // Now make a request that should trigger auto-ensure
    const autoRestartT0 = Date.now();
    const healthAfter = await page.request.get("/health", { timeout: 60_000 });
    const autoRestartMs = Date.now() - autoRestartT0;

    recordMetric("sandbox-recovery", "auto-restart-health-status", healthAfter.status());
    recordMetric("sandbox-recovery", "auto-restart-time", autoRestartMs, "ms");
  });
});

// Print final report
test.afterAll(() => {
  console.log("\n╔══════════════════════════════════════════════════════════════════╗");
  console.log("║          VM STRESS & DISCOVERY REPORT                          ║");
  console.log("╠══════════════════════════════════════════════════════════════════╣");
  for (const r of report) {
    const val = `${r.value}${r.unit ? " " + r.unit : ""}`;
    console.log(`║  ${r.test.padEnd(25)} ${r.metric.padEnd(30)} ${val.padStart(8)} ║`);
  }
  console.log("╚══════════════════════════════════════════════════════════════════╝");
});
