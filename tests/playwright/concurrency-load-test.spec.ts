/**
 * Concurrency & Load Test Suite
 *
 * Tests:
 * 1. Concurrent user auth + sandbox startup (N users)
 * 2. Concurrent conductor prompts (real LLM calls)
 * 3. Writer edit + reprompt under concurrent load
 * 4. Hibernate/restore under concurrent access
 * 5. Capacity discovery: scale users until failure
 * 6. Mixed workload: auth + prompt + writer + idle simultaneously
 *
 * Run against Node B (staging):
 *   PLAYWRIGHT_HYPERVISOR_BASE_URL=https://draft.choir-ip.com \
 *     npx playwright test concurrency-load-test.spec.ts --project=hypervisor
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
  return `load_${Date.now()}_${Math.random().toString(36).slice(2, 8)}@example.com`;
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

  const me = (await (await page.request.get("/auth/me")).json()) as MeResponse;
  expect(me.authenticated).toBe(true);
  return { username, userId: me.user_id! };
}

async function getSandboxSnapshots(request: APIRequestContext): Promise<SandboxSnapshot[]> {
  const res = await request.get("/admin/sandboxes");
  if (!res.ok()) return [];
  return (await res.json()) as SandboxSnapshot[];
}

async function waitForSandboxRunning(
  request: APIRequestContext,
  userId: string,
  maxWaitSecs = 90
): Promise<boolean> {
  for (let i = 0; i < maxWaitSecs; i++) {
    const snapshots = await getSandboxSnapshots(request);
    if (snapshots.some((s) => s.user_id === userId && s.status === "running")) return true;
    await new Promise((r) => setTimeout(r, 1000));
  }
  return false;
}

async function waitForHealth(page: Page, maxWaitSecs = 120): Promise<{ ok: boolean; ms: number }> {
  const t0 = Date.now();
  for (let i = 0; i < maxWaitSecs; i++) {
    try {
      const res = await page.request.get("/health", { timeout: 5_000 });
      if (res.ok()) return { ok: true, ms: Date.now() - t0 };
    } catch {
      // timeout, keep polling
    }
    await new Promise((r) => setTimeout(r, 1000));
  }
  return { ok: false, ms: Date.now() - t0 };
}

// Metrics collection
const report: { test: string; metric: string; value: string | number; unit?: string }[] = [];

function recordMetric(testName: string, metric: string, value: string | number, unit?: string) {
  report.push({ test: testName, metric, value, unit });
  console.log(`[METRIC] ${testName} | ${metric}: ${value}${unit ? " " + unit : ""}`);
}

// ── Test 1: Concurrent Auth + Sandbox Startup ────────────────────────────────

test.describe("1. Concurrent auth + sandbox startup", () => {
  test("3 users register and start sandbox concurrently", async ({ browser }) => {
    const N = 3;
    const contexts = await Promise.all(
      Array.from({ length: N }, () => browser.newContext())
    );
    const pages = await Promise.all(contexts.map((ctx) => ctx.newPage()));

    // Register all users concurrently
    const regT0 = Date.now();
    const users = await Promise.all(pages.map((p) => registerAndLogin(p)));
    const regMs = Date.now() - regT0;

    recordMetric("concurrent-auth", "users", N);
    recordMetric("concurrent-auth", "total-register-time", regMs, "ms");
    recordMetric("concurrent-auth", "avg-register-time", Math.round(regMs / N), "ms");

    // Start all sandboxes concurrently
    const startT0 = Date.now();
    await Promise.all(
      users.map((u, i) => pages[i].request.post(`/admin/sandboxes/${u.userId}/live/start`))
    );

    // Wait for all to be running
    const runResults = await Promise.all(
      users.map((u, i) => waitForSandboxRunning(pages[i].request, u.userId))
    );
    const startMs = Date.now() - startT0;

    const running = runResults.filter(Boolean).length;
    recordMetric("concurrent-auth", "sandboxes-running", `${running}/${N}`);
    recordMetric("concurrent-auth", "total-start-time", startMs, "ms");

    // Verify health from each user's perspective
    const healthResults = await Promise.all(pages.map((p) => waitForHealth(p)));
    const healthOk = healthResults.filter((h) => h.ok).length;
    const avgHealthMs = Math.round(
      healthResults.reduce((a, h) => a + h.ms, 0) / healthResults.length
    );

    recordMetric("concurrent-auth", "health-ok", `${healthOk}/${N}`);
    recordMetric("concurrent-auth", "avg-health-time", avgHealthMs, "ms");

    expect(running).toBe(N);
    expect(healthOk).toBe(N);

    await Promise.all(contexts.map((ctx) => ctx.close()));
  });
});

// ── Test 2: Concurrent Conductor Prompts ─────────────────────────────────────

test.describe("2. Concurrent conductor prompts", () => {
  test("send 3 prompts concurrently via conductor/execute", async ({ browser }) => {
    const ctx = await browser.newContext();
    const page = await ctx.newPage();
    const { userId } = await registerAndLogin(page);

    // Ensure sandbox is running
    await page.request.post(`/admin/sandboxes/${userId}/live/start`);
    const health = await waitForHealth(page);
    expect(health.ok).toBe(true);

    const prompts = [
      "What is 2+2? Answer with just the number.",
      "List 3 primary colors. Be brief.",
      "What is the capital of France? One word answer.",
    ];

    // Send all prompts concurrently
    const executeT0 = Date.now();
    const results = await Promise.all(
      prompts.map(async (prompt, i) => {
        const t0 = Date.now();
        try {
          const res = await page.request.post("/conductor/execute", {
            data: { objective: prompt },
            timeout: 120_000,
          });
          return {
            i,
            status: res.status(),
            ms: Date.now() - t0,
            body: await res.text().catch(() => ""),
            error: null,
          };
        } catch (e: unknown) {
          return { i, status: 0, ms: Date.now() - t0, body: "", error: String(e) };
        }
      })
    );
    const totalMs = Date.now() - executeT0;

    for (const r of results) {
      const status = r.error ? `ERROR: ${r.error}` : `${r.status}`;
      console.log(`  prompt[${r.i}]: ${status} (${r.ms}ms)`);
      recordMetric("concurrent-prompts", `prompt-${r.i}-status`, r.status);
      recordMetric("concurrent-prompts", `prompt-${r.i}-time`, r.ms, "ms");
    }

    recordMetric("concurrent-prompts", "total-wall-time", totalMs, "ms");

    const ok = results.filter((r) => r.status >= 200 && r.status < 300).length;
    recordMetric("concurrent-prompts", "successful", `${ok}/${prompts.length}`);

    // At least 1 should succeed (shared sandbox may serialize)
    expect(ok).toBeGreaterThanOrEqual(1);

    await ctx.close();
  });
});

// ── Test 3: Writer Edit + Reprompt ───────────────────────────────────────────

test.describe("3. Writer edit + reprompt", () => {
  test("open writer, edit, save, then reprompt", async ({ browser }) => {
    const ctx = await browser.newContext();
    const page = await ctx.newPage();
    const { userId } = await registerAndLogin(page);

    // Ensure sandbox is running and healthy
    await page.request.post(`/admin/sandboxes/${userId}/live/start`);
    const health = await waitForHealth(page);
    expect(health.ok).toBe(true);

    // 1. Initial prompt to create a document
    const promptT0 = Date.now();
    const promptRes = await page.request.post("/conductor/execute", {
      data: { objective: "Write a short haiku about computers." },
      timeout: 120_000,
    });
    const promptMs = Date.now() - promptT0;
    recordMetric("writer-flow", "initial-prompt-status", promptRes.status());
    recordMetric("writer-flow", "initial-prompt-time", promptMs, "ms");

    // 2. Check that a run was created
    const runsRes = await page.request.get("/conductor/runs", { timeout: 10_000 });
    const runsData = runsRes.ok() ? await runsRes.json() : [];
    const runCount = Array.isArray(runsData) ? runsData.length : 0;
    recordMetric("writer-flow", "runs-after-prompt", runCount);

    // 3. Try to open the writer document
    const openT0 = Date.now();
    const openRes = await page.request.post("/writer/open", {
      data: { run_id: "latest" },
      timeout: 15_000,
    });
    const openMs = Date.now() - openT0;
    recordMetric("writer-flow", "open-status", openRes.status());
    recordMetric("writer-flow", "open-time", openMs, "ms");

    if (openRes.ok()) {
      const doc = await openRes.json();
      const contentLength = JSON.stringify(doc).length;
      recordMetric("writer-flow", "doc-content-length", contentLength, "chars");

      // 4. Save an edit
      const saveT0 = Date.now();
      const saveRes = await page.request.post("/writer/save", {
        data: {
          run_id: "latest",
          content: "# Edited Haiku\n\nSilicon dreams flow\nThrough circuits of light and code\nComputers haiku\n",
          revision: 1,
        },
        timeout: 15_000,
      });
      const saveMs = Date.now() - saveT0;
      recordMetric("writer-flow", "save-status", saveRes.status());
      recordMetric("writer-flow", "save-time", saveMs, "ms");

      // 5. Reprompt (AI revision)
      const repromptT0 = Date.now();
      const repromptRes = await page.request.post("/writer/prompt", {
        data: {
          run_id: "latest",
          prompt: "Make the haiku more poetic and add imagery about starlight.",
        },
        timeout: 120_000,
      });
      const repromptMs = Date.now() - repromptT0;
      recordMetric("writer-flow", "reprompt-status", repromptRes.status());
      recordMetric("writer-flow", "reprompt-time", repromptMs, "ms");
    }

    await ctx.close();
  });

  test("concurrent writer saves (conflict detection)", async ({ browser }) => {
    const ctx = await browser.newContext();
    const page = await ctx.newPage();
    const { userId } = await registerAndLogin(page);

    await page.request.post(`/admin/sandboxes/${userId}/live/start`);
    const health = await waitForHealth(page);
    expect(health.ok).toBe(true);

    // Create initial document
    await page.request.post("/conductor/execute", {
      data: { objective: "Write a test document with 3 bullet points." },
      timeout: 120_000,
    });

    // Try 3 concurrent saves to the same doc
    const saveT0 = Date.now();
    const saveResults = await Promise.all(
      [1, 2, 3].map(async (i) => {
        const t0 = Date.now();
        try {
          const res = await page.request.post("/writer/save", {
            data: {
              run_id: "latest",
              content: `# Concurrent Edit ${i}\n\nThis is version ${i}.`,
              revision: 1,
            },
            timeout: 15_000,
          });
          return { i, status: res.status(), ms: Date.now() - t0 };
        } catch (e: unknown) {
          return { i, status: 0, ms: Date.now() - t0, error: String(e) };
        }
      })
    );
    const saveMs = Date.now() - saveT0;

    for (const r of saveResults) {
      recordMetric("concurrent-saves", `save-${r.i}-status`, r.status);
      recordMetric("concurrent-saves", `save-${r.i}-time`, r.ms, "ms");
    }

    // At least one should succeed, others may get conflict (409)
    const succeeded = saveResults.filter((r) => r.status === 200).length;
    const conflicts = saveResults.filter((r) => r.status === 409).length;
    recordMetric("concurrent-saves", "succeeded", succeeded);
    recordMetric("concurrent-saves", "conflicts", conflicts);
    recordMetric("concurrent-saves", "total-wall-time", saveMs, "ms");

    await ctx.close();
  });
});

// ── Test 4: Hibernate Under Active Use ───────────────────────────────────────

test.describe("4. Hibernate under concurrent access", () => {
  test("hibernate while requests in flight, then restore", async ({ browser }) => {
    const ctx = await browser.newContext();
    const page = await ctx.newPage();
    const { userId } = await registerAndLogin(page);

    await page.request.post(`/admin/sandboxes/${userId}/live/start`);
    const health = await waitForHealth(page);
    expect(health.ok).toBe(true);

    // Fire a slow request (conductor/execute) and hibernate simultaneously
    const executePromise = page.request
      .post("/conductor/execute", {
        data: { objective: "Count to 10 slowly, one number per line." },
        timeout: 120_000,
      })
      .catch((e: unknown) => ({ status: () => 0, error: String(e) }));

    // Wait a moment for the request to start
    await page.waitForTimeout(1000);

    // Hibernate while the prompt is in flight
    const hibT0 = Date.now();
    const hibRes = await page.request.post(`/admin/sandboxes/${userId}/live/hibernate`, {
      timeout: 30_000,
    });
    const hibMs = Date.now() - hibT0;

    recordMetric("hibernate-active", "hibernate-status", hibRes.status());
    recordMetric("hibernate-active", "hibernate-time", hibMs, "ms");

    // Check what happened to the in-flight request
    const execResult = await executePromise;
    const execStatus = typeof execResult.status === "function" ? execResult.status() : 0;
    recordMetric("hibernate-active", "inflight-request-status", execStatus);

    // Now restore and verify
    const restoreT0 = Date.now();
    const restoreHealth = await waitForHealth(page);
    const restoreMs = Date.now() - restoreT0;

    recordMetric("hibernate-active", "restore-ok", restoreHealth.ok ? "yes" : "no");
    recordMetric("hibernate-active", "restore-time", restoreMs, "ms");

    expect(restoreHealth.ok).toBe(true);

    await ctx.close();
  });
});

// ── Test 5: Capacity Discovery ───────────────────────────────────────────────

test.describe("5. Capacity discovery", () => {
  test("scale users until failure: register N users, start sandboxes", async ({ browser }) => {
    const MAX_USERS = 8; // Start moderate, increase if all succeed
    const results: { i: number; regMs: number; startOk: boolean; healthOk: boolean }[] = [];

    for (let i = 0; i < MAX_USERS; i++) {
      const ctx = await browser.newContext();
      const page = await ctx.newPage();

      const regT0 = Date.now();
      let userId = "";
      try {
        const user = await registerAndLogin(page);
        userId = user.userId;
      } catch (e: unknown) {
        console.log(`  User ${i}: registration FAILED (${e})`);
        results.push({ i, regMs: Date.now() - regT0, startOk: false, healthOk: false });
        await ctx.close();
        break;
      }
      const regMs = Date.now() - regT0;

      // Start sandbox
      let startOk = false;
      try {
        await page.request.post(`/admin/sandboxes/${userId}/live/start`);
        startOk = await waitForSandboxRunning(page.request, userId, 60);
      } catch {
        startOk = false;
      }

      // Check health
      let healthOk = false;
      if (startOk) {
        const h = await waitForHealth(page, 60);
        healthOk = h.ok;
      }

      console.log(
        `  User ${i}: reg=${regMs}ms start=${startOk ? "ok" : "FAIL"} health=${healthOk ? "ok" : "FAIL"}`
      );
      results.push({ i, regMs, startOk, healthOk });
      await ctx.close();

      // Stop early if it's failing
      if (!startOk || !healthOk) {
        console.log(`  Stopping at user ${i} — sandbox not healthy`);
        break;
      }
    }

    const totalUsers = results.length;
    const okUsers = results.filter((r) => r.healthOk).length;
    const avgRegMs = Math.round(results.reduce((a, r) => a + r.regMs, 0) / totalUsers);

    recordMetric("capacity", "max-attempted", MAX_USERS);
    recordMetric("capacity", "total-registered", totalUsers);
    recordMetric("capacity", "healthy-sandboxes", okUsers);
    recordMetric("capacity", "avg-registration-time", avgRegMs, "ms");
    recordMetric("capacity", "first-failure-at", okUsers < totalUsers ? okUsers : "none");
  });
});

// ── Test 6: Mixed Workload ───────────────────────────────────────────────────

test.describe("6. Mixed concurrent workload", () => {
  test("auth + prompt + writer + heartbeat simultaneously", async ({ browser }) => {
    // User 1: already authenticated, sends prompts
    const ctx1 = await browser.newContext();
    const page1 = await ctx1.newPage();
    const { userId: uid1 } = await registerAndLogin(page1);
    await page1.request.post(`/admin/sandboxes/${uid1}/live/start`);
    await waitForHealth(page1);

    // User 2: registering fresh
    const ctx2 = await browser.newContext();
    const page2 = await ctx2.newPage();

    // Fire all these concurrently
    const t0 = Date.now();
    const [promptResult, regResult, heartbeatResult, healthResult] = await Promise.all([
      // User 1: conductor prompt
      page1.request
        .post("/conductor/execute", {
          data: { objective: "What is 1+1? Just the number." },
          timeout: 120_000,
        })
        .then((r) => ({ type: "prompt", status: r.status(), ms: Date.now() - t0 }))
        .catch((e: unknown) => ({ type: "prompt", status: 0, ms: Date.now() - t0, error: String(e) })),

      // User 2: fresh registration
      (async () => {
        try {
          const user = await registerAndLogin(page2);
          return { type: "register", status: 200, ms: Date.now() - t0, userId: user.userId };
        } catch (e: unknown) {
          return { type: "register", status: 0, ms: Date.now() - t0, error: String(e) };
        }
      })(),

      // User 1: heartbeat
      page1.request
        .post("/heartbeat", { timeout: 10_000 })
        .then((r) => ({ type: "heartbeat", status: r.status(), ms: Date.now() - t0 }))
        .catch((e: unknown) => ({ type: "heartbeat", status: 0, ms: Date.now() - t0, error: String(e) })),

      // User 1: health check
      page1.request
        .get("/health", { timeout: 10_000 })
        .then((r) => ({ type: "health", status: r.status(), ms: Date.now() - t0 }))
        .catch((e: unknown) => ({ type: "health", status: 0, ms: Date.now() - t0, error: String(e) })),
    ]);
    const totalMs = Date.now() - t0;

    for (const r of [promptResult, regResult, heartbeatResult, healthResult]) {
      recordMetric("mixed-workload", `${r.type}-status`, r.status);
      recordMetric("mixed-workload", `${r.type}-time`, r.ms, "ms");
    }
    recordMetric("mixed-workload", "total-wall-time", totalMs, "ms");

    // Health and heartbeat should always work
    expect(healthResult.status).toBe(200);
    expect(heartbeatResult.status).toBe(200);

    await Promise.all([ctx1.close(), ctx2.close()]);
  });
});

// ── Final Report ─────────────────────────────────────────────────────────────

test.afterAll(() => {
  console.log("\n╔══════════════════════════════════════════════════════════════════════════╗");
  console.log("║              CONCURRENCY & LOAD TEST REPORT                            ║");
  console.log("╠══════════════════════════════════════════════════════════════════════════╣");
  for (const r of report) {
    const val = `${r.value}${r.unit ? " " + r.unit : ""}`;
    console.log(`║  ${r.test.padEnd(22)} ${r.metric.padEnd(30)} ${val.padStart(15)} ║`);
  }
  console.log("╚══════════════════════════════════════════════════════════════════════════╝");
});
