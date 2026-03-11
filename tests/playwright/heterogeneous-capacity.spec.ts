/**
 * Heterogeneous Capacity Test (ADR-0014 Phase 6)
 *
 * Simulates production topology:
 *   - User tier:   ch-blk-2c-2g  (per-user sandboxes, isolation priority)
 *   - Worker tier:  ch-pmem-4c-4g (compute pool, performance priority via KSM)
 *
 * Ramps both tiers simultaneously and measures:
 *   - Boot time degradation curve per tier
 *   - Health latency per tier under mixed load
 *   - Memory consumption breakdown (user vs worker)
 *   - Degradation threshold (where p99 > 200ms or boot failures > 50%)
 *   - I/O workload latency under mixed load
 *
 * Config via env:
 *   USER_CLASS=ch-blk-2c-2g       (default)
 *   WORKER_CLASS=ch-pmem-4c-4g    (default)
 *   USER_BATCH=5                   (users added per round)
 *   WORKER_BATCH=2                 (workers added per round)
 *   MAX_ROUNDS=10                  (stop after N rounds)
 *   DEGRADATION_P99_MS=200         (p99 health latency threshold)
 *
 * Run:
 *   PLAYWRIGHT_HYPERVISOR_BASE_URL=https://draft.choir-ip.com \
 *     npx playwright test heterogeneous-capacity.spec.ts --project=stress
 */

import { test, expect, type BrowserContext, type Page } from "@playwright/test";

// ── Config ───────────────────────────────────────────────────────────────────

const USER_CLASS = process.env.USER_CLASS ?? "ch-blk-2c-2g";
const WORKER_CLASS = process.env.WORKER_CLASS ?? "ch-pmem-4c-4g";
const USER_BATCH = parseInt(process.env.USER_BATCH ?? "5", 10);
const WORKER_BATCH = parseInt(process.env.WORKER_BATCH ?? "2", 10);
const MAX_ROUNDS = parseInt(process.env.MAX_ROUNDS ?? "10", 10);
const DEGRADATION_P99_MS = parseInt(process.env.DEGRADATION_P99_MS ?? "200", 10);
const HEALTH_TIMEOUT_S = 30;
const HEALTH_SAMPLES = 5; // health checks per sampled VM

// ── Helpers ──────────────────────────────────────────────────────────────────

async function addVirtualAuthenticator(page: Page): Promise<void> {
  const cdp = await (page.context() as BrowserContext).newCDPSession(page);
  await cdp.send("WebAuthn.enable", { enableUI: false });
  await cdp.send("WebAuthn.addVirtualAuthenticator", {
    options: {
      protocol: "ctap2",
      transport: "internal",
      hasResidentKey: true,
      hasUserVerification: true,
      isUserVerified: true,
    },
  });
}

async function registerAndBoot(
  browser: import("@playwright/test").Browser,
  cls: string,
  tier: string,
  index: number
): Promise<{
  ctx: BrowserContext;
  page: Page;
  userId: string;
  bootMs: number;
  ok: boolean;
  tier: string;
}> {
  const ctx = await browser.newContext();
  const page = await ctx.newPage();

  try {
    const tag = cls.replace(/-/g, "").slice(0, 8);
    const username = `hc_${tier}_${tag}_${index}_${Date.now()}@test.choiros.dev`;
    await addVirtualAuthenticator(page);

    await page.goto("/register");
    await expect(page.getByTestId("auth-modal")).toBeVisible({ timeout: 30_000 });

    const finishWait = page.waitForResponse(
      (r) =>
        r.url().includes("/auth/register/finish") &&
        r.request().method() === "POST",
      { timeout: 30_000 }
    );

    const input = page.getByTestId("auth-input");
    await input.fill(username);
    await input.press("Enter");
    await finishWait;

    await expect(page.getByTestId("auth-modal")).toHaveCount(0, {
      timeout: 15_000,
    });

    // Set machine class
    await page.request.put("/profile/machine-class", {
      data: { class_name: cls },
      headers: { "Content-Type": "application/json" },
      timeout: 10_000,
    });

    const me = (await (await page.request.get("/auth/me")).json()) as {
      authenticated: boolean;
      user_id?: string;
    };

    // Wait for health
    const t0 = Date.now();
    for (let i = 0; i < HEALTH_TIMEOUT_S; i++) {
      try {
        const h = await page.request.get("/health", { timeout: 3_000 });
        if (h.ok())
          return {
            ctx,
            page,
            userId: me.user_id!,
            bootMs: Date.now() - t0,
            ok: true,
            tier,
          };
      } catch {
        /* poll */
      }
      await new Promise((r) => setTimeout(r, 1000));
    }

    return {
      ctx,
      page,
      userId: me.user_id ?? "",
      bootMs: Date.now() - t0,
      ok: false,
      tier,
    };
  } catch {
    return { ctx, page, userId: "", bootMs: 0, ok: false, tier };
  }
}

async function healthLatency(page: Page): Promise<number> {
  const t0 = Date.now();
  try {
    await page.request.get("/health", { timeout: 10_000 });
  } catch {
    /* slow */
  }
  return Date.now() - t0;
}

interface HostStats {
  memory_total_mb: number | null;
  memory_available_mb: number | null;
  vms_running: number;
  vms_total: number;
}

async function getHostStats(page: Page): Promise<HostStats | null> {
  try {
    const res = await page.request.get("/admin/stats", { timeout: 10_000 });
    if (res.ok()) return (await res.json()) as HostStats;
  } catch {
    /* best effort */
  }
  return null;
}

function percentile(sorted: number[], p: number): number {
  return sorted[Math.floor(sorted.length * p)] ?? 0;
}

// ── Metrics ──────────────────────────────────────────────────────────────────

interface Metric {
  step: string;
  metric: string;
  value: string | number;
  unit?: string;
}
const metrics: Metric[] = [];

function record(
  step: string,
  metric: string,
  value: string | number,
  unit?: string
) {
  metrics.push({ step, metric, value, unit });
  console.log(
    `[HETCAP] ${step.padEnd(14)} ${metric.padEnd(30)} ${value}${unit ? " " + unit : ""}`
  );
}

interface Session {
  tier: string;
  ctx: BrowserContext;
  page: Page;
  userId: string;
}

// ── Main Test ────────────────────────────────────────────────────────────────

test.describe("Heterogeneous Capacity", () => {
  test.setTimeout(1200_000);

  test(`${USER_CLASS} + ${WORKER_CLASS} ramp`, async ({ browser }) => {
    record("config", "user-class", USER_CLASS);
    record("config", "worker-class", WORKER_CLASS);
    record("config", "user-batch", USER_BATCH);
    record("config", "worker-batch", WORKER_BATCH);
    record("config", "max-rounds", MAX_ROUNDS);
    record("config", "degradation-p99-threshold", DEGRADATION_P99_MS, "ms");

    // Baseline
    const baseCtx = await browser.newContext();
    const basePage = await baseCtx.newPage();
    await addVirtualAuthenticator(basePage);
    await basePage.goto("/register");
    await expect(basePage.getByTestId("auth-modal")).toBeVisible({
      timeout: 30_000,
    });
    const finishWait = basePage.waitForResponse(
      (r) =>
        r.url().includes("/auth/register/finish") &&
        r.request().method() === "POST",
      { timeout: 30_000 }
    );
    await basePage
      .getByTestId("auth-input")
      .fill(`hc_base_${Date.now()}@test.choiros.dev`);
    await basePage.getByTestId("auth-input").press("Enter");
    await finishWait;
    await expect(basePage.getByTestId("auth-modal")).toHaveCount(0, {
      timeout: 15_000,
    });

    const baseStats = await getHostStats(basePage);
    const baselineAvail = baseStats?.memory_available_mb ?? 30000;
    record("baseline", "memory-total", baseStats?.memory_total_mb ?? 0, "MB");
    record("baseline", "memory-available", baselineAvail, "MB");
    record("baseline", "vms-running", baseStats?.vms_running ?? 0);
    await baseCtx.close();

    const userSessions: Session[] = [];
    const workerSessions: Session[] = [];
    let totalUsers = 0;
    let totalWorkers = 0;
    let degradedRound: number | null = null;

    for (let round = 1; round <= MAX_ROUNDS; round++) {
      const stepName = `round-${round}`;
      console.log(
        `\n=== Round ${round}: +${USER_BATCH} users (${USER_CLASS}) + ${WORKER_BATCH} workers (${WORKER_CLASS}) ===`
      );

      // Boot user and worker batches concurrently
      const roundT0 = Date.now();
      const allBoots = await Promise.allSettled([
        ...Array.from({ length: USER_BATCH }, (_, i) =>
          registerAndBoot(browser, USER_CLASS, "user", totalUsers + i)
        ),
        ...Array.from({ length: WORKER_BATCH }, (_, i) =>
          registerAndBoot(browser, WORKER_CLASS, "worker", totalWorkers + i)
        ),
      ]);
      const roundMs = Date.now() - roundT0;

      let userOk = 0;
      let userFail = 0;
      let workerOk = 0;
      let workerFail = 0;
      const userBootTimes: number[] = [];
      const workerBootTimes: number[] = [];

      for (const r of allBoots) {
        if (r.status === "fulfilled") {
          if (r.value.ok) {
            if (r.value.tier === "user") {
              userOk++;
              userBootTimes.push(r.value.bootMs);
              userSessions.push({
                tier: "user",
                ctx: r.value.ctx,
                page: r.value.page,
                userId: r.value.userId,
              });
            } else {
              workerOk++;
              workerBootTimes.push(r.value.bootMs);
              workerSessions.push({
                tier: "worker",
                ctx: r.value.ctx,
                page: r.value.page,
                userId: r.value.userId,
              });
            }
          } else {
            if (r.value.tier === "user") userFail++;
            else workerFail++;
            await r.value.ctx.close();
          }
        } else {
          // Can't tell tier from rejected promise, count as user fail
          userFail++;
        }
      }

      totalUsers += userOk;
      totalWorkers += workerOk;

      record(stepName, "users-booted", `${userOk}/${USER_BATCH}`);
      record(stepName, "workers-booted", `${workerOk}/${WORKER_BATCH}`);
      record(stepName, "total-users", totalUsers);
      record(stepName, "total-workers", totalWorkers);
      record(stepName, "total-vms", totalUsers + totalWorkers);
      record(stepName, "round-wall-time", roundMs, "ms");

      if (userBootTimes.length > 0) {
        userBootTimes.sort((a, b) => a - b);
        record(
          stepName,
          "user-boot-median",
          percentile(userBootTimes, 0.5),
          "ms"
        );
      }
      if (workerBootTimes.length > 0) {
        workerBootTimes.sort((a, b) => a - b);
        record(
          stepName,
          "worker-boot-median",
          percentile(workerBootTimes, 0.5),
          "ms"
        );
      }

      // Host stats
      const anyPage =
        userSessions.length > 0
          ? userSessions[0].page
          : workerSessions.length > 0
            ? workerSessions[0].page
            : null;
      if (anyPage) {
        const stats = await getHostStats(anyPage);
        if (stats) {
          record(
            stepName,
            "memory-available",
            stats.memory_available_mb ?? 0,
            "MB"
          );
          record(stepName, "vms-running-host", stats.vms_running);

          const totalVms = totalUsers + totalWorkers;
          if (totalVms > 0) {
            const used = baselineAvail - (stats.memory_available_mb ?? 0);
            record(stepName, "mem/vm-avg-all", Math.round(used / totalVms), "MB");
          }
        }
      }

      // Health latency sweep — sample up to 3 from each tier
      const userSample = userSessions
        .slice()
        .sort(() => Math.random() - 0.5)
        .slice(0, 3);
      const workerSample = workerSessions
        .slice()
        .sort(() => Math.random() - 0.5)
        .slice(0, 3);

      const userLats: number[] = [];
      const workerLats: number[] = [];

      for (const s of userSample) {
        for (let h = 0; h < HEALTH_SAMPLES; h++) {
          userLats.push(await healthLatency(s.page));
        }
      }
      for (const s of workerSample) {
        for (let h = 0; h < HEALTH_SAMPLES; h++) {
          workerLats.push(await healthLatency(s.page));
        }
      }

      if (userLats.length > 0) {
        userLats.sort((a, b) => a - b);
        record(stepName, "user-health-p50", percentile(userLats, 0.5), "ms");
        record(stepName, "user-health-p99", percentile(userLats, 0.99), "ms");
      }
      if (workerLats.length > 0) {
        workerLats.sort((a, b) => a - b);
        record(
          stepName,
          "worker-health-p50",
          percentile(workerLats, 0.5),
          "ms"
        );
        record(
          stepName,
          "worker-health-p99",
          percentile(workerLats, 0.99),
          "ms"
        );
      }

      // Check degradation
      const allLats = [...userLats, ...workerLats].sort((a, b) => a - b);
      const p99 = allLats.length > 0 ? percentile(allLats, 0.99) : 0;
      const totalFails = userFail + workerFail;
      const totalAttempts = USER_BATCH + WORKER_BATCH;

      if (
        degradedRound === null &&
        (p99 > DEGRADATION_P99_MS || totalFails > totalAttempts / 2)
      ) {
        degradedRound = round;
        record(stepName, "DEGRADATION", `p99=${p99}ms, fails=${totalFails}`);
      }

      // Mark ceiling but keep going — degraded service is realistic
      if (totalFails >= totalAttempts) {
        record(stepName, "ceiling", "yes");
        // Don't break — continue to see how far we can push
      }
    }

    // ── I/O workload under mixed load ──────────────────────────────
    if (userSessions.length >= 1 && workerSessions.length >= 1) {
      console.log("\n=== I/O workload under mixed load ===");

      const ioResults = await Promise.allSettled([
        // User tier prompt
        ...userSessions.slice(0, 2).map(async (s, i) => {
          const t0 = Date.now();
          const res = await s.page.request.post("/conductor/execute", {
            data: {
              objective: "What is 2+2? Answer with just the number.",
              desktop_id: `hetcap-user-${Date.now()}-${i}`,
              output_mode: "auto",
            },
            timeout: 60_000,
          });
          return { tier: "user", status: res.status(), ms: Date.now() - t0 };
        }),
        // Worker tier prompt
        ...workerSessions.slice(0, 2).map(async (s, i) => {
          const t0 = Date.now();
          const res = await s.page.request.post("/conductor/execute", {
            data: {
              objective: "What is 2+2? Answer with just the number.",
              desktop_id: `hetcap-worker-${Date.now()}-${i}`,
              output_mode: "auto",
            },
            timeout: 60_000,
          });
          return { tier: "worker", status: res.status(), ms: Date.now() - t0 };
        }),
      ]);

      for (let i = 0; i < ioResults.length; i++) {
        const r = ioResults[i];
        if (r.status === "fulfilled") {
          record(
            "io-mixed",
            `${r.value.tier}-prompt-${i}`,
            `${r.value.status} ${r.value.ms}ms`
          );
        } else {
          record("io-mixed", `prompt-${i}-error`, String(r.reason).slice(0, 80));
        }
      }
    }

    // ── Summary ────────────────────────────────────────────────────
    record("summary", "total-users", totalUsers);
    record("summary", "total-workers", totalWorkers);
    record("summary", "total-vms", totalUsers + totalWorkers);
    record(
      "summary",
      "degradation-round",
      degradedRound ?? "none (no degradation)"
    );
    if (degradedRound) {
      record(
        "summary",
        "clean-capacity",
        `${(degradedRound - 1) * USER_BATCH} users + ${(degradedRound - 1) * WORKER_BATCH} workers`
      );
    }

    // ── Cleanup ────────────────────────────────────────────────────
    console.log("\n=== Cleanup ===");
    const allSessions = [...userSessions, ...workerSessions];
    await Promise.all(
      allSessions.map(async (s) => {
        try {
          await s.page.request.post(
            `/admin/sandboxes/${s.userId}/live/stop`,
            { timeout: 15_000 }
          );
        } catch {
          /* best effort */
        }
      })
    );
    await new Promise((r) => setTimeout(r, 5_000));

    if (allSessions.length > 0) {
      const finalStats = await getHostStats(allSessions[0].page);
      if (finalStats) {
        record(
          "cleanup",
          "memory-available",
          finalStats.memory_available_mb ?? 0,
          "MB"
        );
        record("cleanup", "vms-running", finalStats.vms_running);
      }
    }

    await Promise.all(allSessions.map((s) => s.ctx.close()));

    // At least the first round should succeed
    expect(totalUsers).toBeGreaterThanOrEqual(USER_BATCH);
    expect(totalWorkers).toBeGreaterThanOrEqual(WORKER_BATCH);
  });
});

// ── Report ───────────────────────────────────────────────────────────────────

test.afterAll(() => {
  if (metrics.length === 0) return;

  console.log(
    "\n╔══════════════════════════════════════════════════════════════════════════════╗"
  );
  console.log(
    `║  HETEROGENEOUS CAPACITY: ${USER_CLASS} + ${WORKER_CLASS}`.padEnd(78) +
      "║"
  );
  console.log(
    "╠══════════════════════════════════════════════════════════════════════════════╣"
  );

  let lastStep = "";
  for (const m of metrics) {
    if (m.step !== lastStep) {
      if (lastStep !== "")
        console.log(`║  ${"─".repeat(72)}║`);
      lastStep = m.step;
    }
    const label = `${m.step.padEnd(14)} ${m.metric}`;
    const val = `${m.value}${m.unit ? " " + m.unit : ""}`;
    console.log(`║  ${label.padEnd(48)} ${val.padStart(24)} ║`);
  }

  console.log(
    "╚══════════════════════════════════════════════════════════════════════════════╝"
  );
});
