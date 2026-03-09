/**
 * Heterogeneous E2E Load Test
 *
 * Simulates different user profiles with varying workload intensities
 * to measure per-user VM behavior under realistic mixed traffic.
 *
 * Profiles:
 *   idle    — register + authenticate, sit idle (baseline VM memory)
 *   light   — health checks, heartbeats, auth API calls
 *   medium  — single conductor prompt execution
 *   heavy   — multiple conductor prompts + writer flow + concurrent API
 *
 * Run against Node B (staging):
 *   PLAYWRIGHT_HYPERVISOR_BASE_URL=https://draft.choir-ip.com \
 *     npx playwright test heterogeneous-load-test.spec.ts --project=hypervisor
 */

import { test, expect, type BrowserContext, type Page } from "@playwright/test";

// ── Config ───────────────────────────────────────────────────────────────────

const PROFILES = {
  idle: 3,
  light: 3,
  medium: 2,
  heavy: 2,
} as const;

const TOTAL_USERS = Object.values(PROFILES).reduce((a, b) => a + b, 0);
const VM_READY_TIMEOUT = 90; // seconds to wait for sandbox health
const ADMIN_BASE =
  process.env.PLAYWRIGHT_HYPERVISOR_BASE_URL ?? "http://localhost:9090";

type ProfileName = keyof typeof PROFILES;

// ── Helpers ──────────────────────────────────────────────────────────────────

async function addVirtualAuthenticator(page: Page): Promise<string> {
  const cdp = await (page.context() as BrowserContext).newCDPSession(page);
  await cdp.send("WebAuthn.enable", { enableUI: false });
  const { authenticatorId } = await cdp.send(
    "WebAuthn.addVirtualAuthenticator",
    {
      options: {
        protocol: "ctap2",
        transport: "internal",
        hasResidentKey: true,
        hasUserVerification: true,
        isUserVerified: true,
      },
    }
  );
  return authenticatorId;
}

function uniqueUsername(profile: string): string {
  return `het_${profile}_${Date.now()}_${Math.random().toString(36).slice(2, 6)}@example.com`;
}

interface MeResponse {
  authenticated: boolean;
  user_id?: string | null;
}

async function registerAndLogin(
  page: Page,
  profile: string
): Promise<{ username: string; userId: string }> {
  const username = uniqueUsername(profile);
  await addVirtualAuthenticator(page);

  await page.goto("/register");
  await expect(page.getByTestId("auth-modal")).toBeVisible({ timeout: 60_000 });

  const finishWait = page.waitForResponse(
    (r) =>
      r.url().includes("/auth/register/finish") &&
      r.request().method() === "POST",
    { timeout: 45_000 }
  );

  const input = page.getByTestId("auth-input");
  await input.fill(username);
  await input.press("Enter");
  await finishWait;

  await expect(page.getByTestId("auth-modal")).toHaveCount(0, {
    timeout: 30_000,
  });

  const me = (await (await page.request.get("/auth/me")).json()) as MeResponse;
  expect(me.authenticated).toBe(true);
  return { username, userId: me.user_id! };
}

async function waitForSandbox(page: Page): Promise<{ ok: boolean; ms: number }> {
  const t0 = Date.now();
  for (let i = 0; i < VM_READY_TIMEOUT; i++) {
    try {
      const h = await page.request.get("/health", { timeout: 5_000 });
      if (h.ok()) return { ok: true, ms: Date.now() - t0 };
    } catch {
      /* keep polling */
    }
    await new Promise((r) => setTimeout(r, 1000));
  }
  return { ok: false, ms: Date.now() - t0 };
}

// ── Metrics ──────────────────────────────────────────────────────────────────

interface Metric {
  profile: string;
  user: number;
  metric: string;
  value: string | number;
  unit?: string;
}

const metrics: Metric[] = [];

function record(
  profile: string,
  user: number,
  metric: string,
  value: string | number,
  unit?: string
) {
  metrics.push({ profile, user, metric, value, unit });
  console.log(
    `[METRIC] ${profile}[${user}] ${metric}: ${value}${unit ? " " + unit : ""}`
  );
}

// ── User session holder ──────────────────────────────────────────────────────

interface UserSession {
  profile: ProfileName;
  index: number;
  context: BrowserContext;
  page: Page;
  userId: string;
  username: string;
}

// ── Workload functions ───────────────────────────────────────────────────────

async function workloadIdle(session: UserSession): Promise<void> {
  // Idle users just sit there after registration — their VM is booted but
  // no requests are made. This measures baseline VM memory.
  record(session.profile, session.index, "action", "idle (no requests)");
  // Wait 5s to let the VM settle
  await new Promise((r) => setTimeout(r, 5_000));
}

async function workloadLight(session: UserSession): Promise<void> {
  const { page, profile, index } = session;

  // 10 health checks
  let healthOk = 0;
  const t0 = Date.now();
  for (let i = 0; i < 10; i++) {
    try {
      const r = await page.request.get("/health", { timeout: 10_000 });
      if (r.ok()) healthOk++;
    } catch {
      /* count as failure */
    }
  }
  record(profile, index, "health-checks", `${healthOk}/10`);
  record(profile, index, "health-time", Date.now() - t0, "ms");

  // 5 heartbeats
  let heartbeatOk = 0;
  for (let i = 0; i < 5; i++) {
    try {
      const r = await page.request.post("/heartbeat", { timeout: 10_000 });
      if (r.ok()) heartbeatOk++;
    } catch {
      /* count */
    }
  }
  record(profile, index, "heartbeats", `${heartbeatOk}/5`);

  // 5 auth checks
  let authOk = 0;
  for (let i = 0; i < 5; i++) {
    try {
      const r = await page.request.get("/auth/me", { timeout: 10_000 });
      if (r.ok()) authOk++;
    } catch {
      /* count */
    }
  }
  record(profile, index, "auth-checks", `${authOk}/5`);
}

async function workloadMedium(session: UserSession): Promise<void> {
  const { page, profile, index } = session;

  // Single conductor prompt
  const t0 = Date.now();
  const res = await page.request.post("/conductor/execute", {
    data: {
      objective: "What is the capital of France? Answer in one word.",
      desktop_id: `het-medium-${Date.now()}-${index}`,
      output_mode: "auto",
    },
    timeout: 120_000,
  });
  const ms = Date.now() - t0;

  record(profile, index, "conductor-status", res.status());
  record(profile, index, "conductor-time", ms, "ms");

  if (res.ok()) {
    const body = await res.text();
    record(profile, index, "response-length", body.length, "chars");
  }

  // Check runs
  try {
    const runsRes = await page.request.get("/conductor/runs", {
      timeout: 10_000,
    });
    if (runsRes.ok()) {
      const runs = await runsRes.json();
      record(
        profile,
        index,
        "runs-count",
        Array.isArray(runs) ? runs.length : 0
      );
    }
  } catch {
    /* non-critical */
  }
}

async function workloadHeavy(session: UserSession): Promise<void> {
  const { page, profile, index } = session;

  // 1. First conductor prompt (math)
  const t0 = Date.now();
  const res1 = await page.request.post("/conductor/execute", {
    data: {
      objective:
        "Calculate the first 10 prime numbers. List them separated by commas.",
      desktop_id: `het-heavy-a-${Date.now()}-${index}`,
      output_mode: "auto",
    },
    timeout: 120_000,
  });
  record(profile, index, "prompt-1-status", res1.status());
  record(profile, index, "prompt-1-time", Date.now() - t0, "ms");

  // 2. Second conductor prompt (creative writing → exercises writer)
  const t1 = Date.now();
  const res2 = await page.request.post("/conductor/execute", {
    data: {
      objective:
        "Write a short paragraph about the history of computing. Include at least 3 facts.",
      desktop_id: `het-heavy-b-${Date.now()}-${index}`,
      output_mode: "auto",
    },
    timeout: 120_000,
  });
  record(profile, index, "prompt-2-status", res2.status());
  record(profile, index, "prompt-2-time", Date.now() - t1, "ms");

  // 3. Concurrent burst: health + heartbeat + auth + runs check
  const burstT0 = Date.now();
  const [h, hb, auth, runs] = await Promise.allSettled([
    page.request.get("/health", { timeout: 30_000 }),
    page.request.post("/heartbeat", { timeout: 10_000 }),
    page.request.get("/auth/me", { timeout: 10_000 }),
    page.request.get("/conductor/runs", { timeout: 10_000 }),
  ]);
  record(profile, index, "burst-time", Date.now() - burstT0, "ms");
  record(
    profile,
    index,
    "burst-results",
    [h, hb, auth, runs]
      .map((r, i) => {
        const names = ["health", "heartbeat", "auth", "runs"];
        const ok =
          r.status === "fulfilled" && "ok" in r.value && r.value.ok();
        return `${names[i]}:${ok ? "ok" : "fail"}`;
      })
      .join(" ")
  );

  // 4. Third conductor prompt (analysis — heaviest)
  const t2 = Date.now();
  const res3 = await page.request.post("/conductor/execute", {
    data: {
      objective:
        "Compare and contrast three programming languages: Rust, Python, and JavaScript. " +
        "Give pros and cons of each in a structured format.",
      desktop_id: `het-heavy-c-${Date.now()}-${index}`,
      output_mode: "auto",
    },
    timeout: 180_000,
  });
  record(profile, index, "prompt-3-status", res3.status());
  record(profile, index, "prompt-3-time", Date.now() - t2, "ms");

  // Total
  record(profile, index, "total-heavy-time", Date.now() - t0, "ms");

  // Final runs count
  try {
    const runsRes = await page.request.get("/conductor/runs", {
      timeout: 10_000,
    });
    if (runsRes.ok()) {
      const r = await runsRes.json();
      record(
        profile,
        index,
        "final-runs-count",
        Array.isArray(r) ? r.length : 0
      );
    }
  } catch {
    /* non-critical */
  }
}

const WORKLOADS: Record<ProfileName, (s: UserSession) => Promise<void>> = {
  idle: workloadIdle,
  light: workloadLight,
  medium: workloadMedium,
  heavy: workloadHeavy,
};

// ── Main Test ────────────────────────────────────────────────────────────────

test.describe("Heterogeneous per-user VM load test", () => {
  // Generous timeout: heavy users run 3 conductor prompts sequentially
  test.setTimeout(600_000); // 10 minutes

  test("mixed workload across user profiles", async ({ browser }) => {
    const sessions: UserSession[] = [];

    // ── Phase 1: Register all users ──────────────────────────────────────
    console.log(`\n=== Phase 1: Registering ${TOTAL_USERS} users ===`);

    const profileEntries: { profile: ProfileName; index: number }[] = [];
    for (const [profile, count] of Object.entries(PROFILES)) {
      for (let i = 0; i < count; i++) {
        profileEntries.push({ profile: profile as ProfileName, index: i });
      }
    }

    // Register all users concurrently
    const regT0 = Date.now();
    const regResults = await Promise.allSettled(
      profileEntries.map(async ({ profile, index }) => {
        const context = await browser.newContext();
        const page = await context.newPage();
        const { username, userId } = await registerAndLogin(page, profile);
        return { profile, index, context, page, userId, username } as UserSession;
      })
    );
    const regMs = Date.now() - regT0;

    for (const result of regResults) {
      if (result.status === "fulfilled") {
        sessions.push(result.value);
        record(
          result.value.profile,
          result.value.index,
          "registered",
          "yes"
        );
      } else {
        console.error(`Registration failed: ${result.reason}`);
      }
    }

    console.log(
      `Registered ${sessions.length}/${TOTAL_USERS} users in ${regMs}ms`
    );
    record("global", 0, "registration-total", regMs, "ms");
    record("global", 0, "registered-count", sessions.length);

    expect(sessions.length).toBeGreaterThanOrEqual(
      Math.floor(TOTAL_USERS * 0.8)
    );

    // ── Phase 2: Wait for all VMs to be ready ────────────────────────────
    console.log("\n=== Phase 2: Waiting for VMs ===");

    // Only non-idle users need sandbox readiness (idle users don't make
    // sandbox-proxied requests, but their VMs still boot via ensure_running)
    const vmT0 = Date.now();
    const vmResults = await Promise.allSettled(
      sessions.map(async (s) => {
        const { ok, ms } = await waitForSandbox(s.page);
        record(s.profile, s.index, "vm-ready", ok ? "yes" : "no");
        record(s.profile, s.index, "vm-wait", ms, "ms");
        return ok;
      })
    );
    const vmMs = Date.now() - vmT0;

    const vmsReady = vmResults.filter(
      (r) => r.status === "fulfilled" && r.value
    ).length;
    console.log(`VMs ready: ${vmsReady}/${sessions.length} in ${vmMs}ms`);
    record("global", 0, "vm-ready-total", vmMs, "ms");
    record("global", 0, "vms-ready-count", vmsReady);

    // ── Phase 2.5: Snapshot admin registry ───────────────────────────────
    try {
      const adminRes = await sessions[0].page.request.get(
        "/admin/sandboxes",
        { timeout: 10_000 }
      );
      if (adminRes.ok()) {
        const snap = await adminRes.json();
        const userCount = Object.keys(snap).length;
        let totalEntries = 0;
        for (const u of Object.values(snap) as Array<{ roles?: object }>) {
          totalEntries += Object.keys(u.roles ?? {}).length;
        }
        record("global", 0, "registry-users", userCount);
        record("global", 0, "registry-entries", totalEntries);
        console.log(
          `Admin registry: ${userCount} users, ${totalEntries} entries`
        );
      }
    } catch {
      console.log("  (admin/sandboxes not accessible)");
    }

    // ── Phase 3: Execute heterogeneous workloads ─────────────────────────
    console.log("\n=== Phase 3: Running workloads ===");

    const workT0 = Date.now();
    const workResults = await Promise.allSettled(
      sessions.map(async (s) => {
        const fn = WORKLOADS[s.profile];
        const t0 = Date.now();
        try {
          await fn(s);
          record(s.profile, s.index, "workload-status", "complete");
          record(s.profile, s.index, "workload-time", Date.now() - t0, "ms");
        } catch (e) {
          record(s.profile, s.index, "workload-status", "failed");
          record(
            s.profile,
            s.index,
            "workload-error",
            String(e).slice(0, 200)
          );
        }
      })
    );
    const workMs = Date.now() - workT0;

    const workOk = workResults.filter(
      (r) => r.status === "fulfilled"
    ).length;
    console.log(
      `Workloads done: ${workOk}/${sessions.length} in ${workMs}ms`
    );
    record("global", 0, "workload-total", workMs, "ms");

    // ── Phase 4: Post-workload snapshot ──────────────────────────────────
    console.log("\n=== Phase 4: Post-workload snapshot ===");

    try {
      const adminRes = await sessions[0].page.request.get(
        "/admin/sandboxes",
        { timeout: 10_000 }
      );
      if (adminRes.ok()) {
        const snap = await adminRes.json();
        const entries = Object.entries(snap) as Array<
          [string, { roles?: Record<string, { status?: string; port?: number }> }]
        >;
        let running = 0;
        let hibernated = 0;
        let stopped = 0;
        for (const [, u] of entries) {
          for (const [, e] of Object.entries(u.roles ?? {})) {
            if (e.status === "Running") running++;
            else if (e.status === "Hibernated") hibernated++;
            else stopped++;
          }
        }
        record("global", 0, "post-running", running);
        record("global", 0, "post-hibernated", hibernated);
        record("global", 0, "post-stopped", stopped);
      }
    } catch {
      /* non-critical */
    }

    // ── Cleanup ──────────────────────────────────────────────────────────
    await Promise.all(sessions.map((s) => s.context.close()));
  });
});

// ── Final Report ─────────────────────────────────────────────────────────────

test.afterAll(() => {
  if (metrics.length === 0) return;

  console.log(
    "\n╔════════════════════════════════════════════════════════════════════════════════╗"
  );
  console.log(
    "║           HETEROGENEOUS PER-USER VM LOAD TEST REPORT                         ║"
  );
  console.log(
    "╠════════════════════════════════════════════════════════════════════════════════╣"
  );
  console.log(
    `║  Profiles: idle=${PROFILES.idle} light=${PROFILES.light} medium=${PROFILES.medium} heavy=${PROFILES.heavy}  (total=${TOTAL_USERS})`.padEnd(
      81
    ) + "║"
  );
  console.log(
    "╠════════════════════════════════════════════════════════════════════════════════╣"
  );

  // Group by profile
  const profiles = ["global", "idle", "light", "medium", "heavy"];
  for (const p of profiles) {
    const pMetrics = metrics.filter((m) => m.profile === p);
    if (pMetrics.length === 0) continue;

    console.log(
      `║  ── ${p.toUpperCase()} ${"─".repeat(72 - p.length)}║`
    );
    for (const m of pMetrics) {
      const label =
        p === "global"
          ? m.metric
          : `[${m.user}] ${m.metric}`;
      const val = `${m.value}${m.unit ? " " + m.unit : ""}`;
      console.log(
        `║    ${label.padEnd(45)} ${val.padStart(30)} ║`
      );
    }
  }

  console.log(
    "╚════════════════════════════════════════════════════════════════════════════════╝"
  );
});
