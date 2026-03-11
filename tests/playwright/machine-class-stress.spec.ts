/**
 * Machine Class Stress Test (ADR-0014 Phase 6)
 *
 * Ramps up VMs of a single machine class to measure:
 *   - Boot time degradation curve
 *   - Per-VM memory footprint
 *   - Health latency under load
 *   - Ceiling (max VMs before failure)
 *   - I/O workload latency (conductor prompt)
 *
 * Run one class at a time for clean comparison:
 *   MACHINE_CLASS=ch-pmem-2c-1g PLAYWRIGHT_HYPERVISOR_BASE_URL=https://draft.choir-ip.com \
 *     npx playwright test machine-class-stress.spec.ts --project=stress
 *
 * To run all 4 sequentially:
 *   for cls in ch-pmem-2c-1g ch-blk-2c-1g fc-pmem-2c-1g fc-blk-2c-1g; do
 *     MACHINE_CLASS=$cls PLAYWRIGHT_HYPERVISOR_BASE_URL=https://draft.choir-ip.com \
 *       npx playwright test machine-class-stress.spec.ts --project=stress
 *   done
 */

import { test, expect, type BrowserContext, type Page } from "@playwright/test";

// ── Config ───────────────────────────────────────────────────────────────────

const MACHINE_CLASS = process.env.MACHINE_CLASS ?? "";
const BATCH_SIZE = 5;
const MAX_BATCHES = 8; // 40 VMs max
const VM_READY_TIMEOUT_S = 120;
const HEALTH_SWEEP_COUNT = 3; // health checks per VM per sweep

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

function uniqueUsername(batch: number, index: number): string {
  const cls = MACHINE_CLASS.replace(/-/g, "").slice(0, 8);
  return `st_${cls}_b${batch}_i${index}_${Date.now()}@test.choiros.dev`;
}

async function registerUser(
  page: Page,
  batch: number,
  index: number
): Promise<{ username: string; userId: string }> {
  const username = uniqueUsername(batch, index);
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

  const me = (await (
    await page.request.get("/auth/me")
  ).json()) as { authenticated: boolean; user_id?: string };
  expect(me.authenticated).toBe(true);
  return { username, userId: me.user_id! };
}

async function setMachineClass(
  page: Page,
  className: string
): Promise<boolean> {
  if (!className) return true; // no class = use default
  const res = await page.request.put("/profile/machine-class", {
    data: { class_name: className },
    headers: { "Content-Type": "application/json" },
    timeout: 10_000,
  });
  return res.ok();
}

async function waitForSandbox(
  page: Page
): Promise<{ ok: boolean; ms: number }> {
  const t0 = Date.now();
  for (let i = 0; i < VM_READY_TIMEOUT_S; i++) {
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
  } catch { /* best effort */ }
  return null;
}

async function healthLatency(page: Page): Promise<number> {
  const t0 = Date.now();
  try {
    await page.request.get("/health", { timeout: 10_000 });
  } catch { /* count as slow */ }
  return Date.now() - t0;
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
    `[STRESS] ${step.padEnd(12)} ${metric.padEnd(30)} ${value}${unit ? " " + unit : ""}`
  );
}

// ── Session tracking ─────────────────────────────────────────────────────────

interface UserSession {
  batch: number;
  index: number;
  context: BrowserContext;
  page: Page;
  userId: string;
  username: string;
}

// ── Main Test ────────────────────────────────────────────────────────────────

test.describe(`Machine Class Stress: ${MACHINE_CLASS || "default"}`, () => {
  test.setTimeout(1200_000); // 20 minutes

  test("ramp and measure", async ({ browser }) => {
    const className = MACHINE_CLASS || "(default)";
    record("config", "machine-class", className);
    record("config", "batch-size", BATCH_SIZE);
    record("config", "max-batches", MAX_BATCHES);

    // Get baseline stats
    const baseCtx = await browser.newContext();
    const basePage = await baseCtx.newPage();
    await registerUser(basePage, 0, 0);
    const baseStats = await getHostStats(basePage);
    if (baseStats) {
      record("baseline", "memory-total", baseStats.memory_total_mb ?? 0, "MB");
      record("baseline", "memory-available", baseStats.memory_available_mb ?? 0, "MB");
      record("baseline", "vms-running", baseStats.vms_running);
    }
    await baseCtx.close();

    const allSessions: UserSession[] = [];
    let totalBooted = 0;
    let hitCeiling = false;

    for (let batch = 1; batch <= MAX_BATCHES; batch++) {
      const stepName = `batch-${batch}`;
      const targetCount = batch * BATCH_SIZE;
      console.log(
        `\n=== Batch ${batch}: booting VMs ${totalBooted + 1}-${targetCount} (${className}) ===`
      );

      // Register and boot BATCH_SIZE users concurrently
      const batchT0 = Date.now();
      const batchResults = await Promise.allSettled(
        Array.from({ length: BATCH_SIZE }, async (_, i) => {
          const ctx = await browser.newContext();
          const page = await ctx.newPage();

          const { username, userId } = await registerUser(page, batch, i);
          await setMachineClass(page, MACHINE_CLASS);
          const { ok, ms } = await waitForSandbox(page);

          return { batch, index: i, context: ctx, page, userId, username, ok, ms };
        })
      );
      const batchMs = Date.now() - batchT0;

      // Count successes
      const batchSessions: UserSession[] = [];
      let batchOk = 0;
      let batchFail = 0;
      const bootTimes: number[] = [];

      for (const r of batchResults) {
        if (r.status === "fulfilled") {
          if (r.value.ok) {
            batchOk++;
            bootTimes.push(r.value.ms);
            batchSessions.push(r.value);
            allSessions.push(r.value);
          } else {
            batchFail++;
            await r.value.context.close();
          }
        } else {
          batchFail++;
          console.error(`  Boot failed: ${r.reason}`);
        }
      }

      totalBooted += batchOk;
      record(stepName, "booted", `${batchOk}/${BATCH_SIZE}`);
      record(stepName, "total-running", totalBooted);
      record(stepName, "batch-wall-time", batchMs, "ms");

      if (bootTimes.length > 0) {
        bootTimes.sort((a, b) => a - b);
        record(stepName, "boot-min", bootTimes[0], "ms");
        record(stepName, "boot-max", bootTimes[bootTimes.length - 1], "ms");
        record(
          stepName,
          "boot-median",
          bootTimes[Math.floor(bootTimes.length / 2)],
          "ms"
        );
      }

      // Host memory snapshot
      if (allSessions.length > 0) {
        const stats = await getHostStats(allSessions[0].page);
        if (stats) {
          record(stepName, "memory-available", stats.memory_available_mb ?? 0, "MB");
          record(stepName, "vms-running-host", stats.vms_running);

          // Compute per-VM memory delta from baseline
          if (baseStats?.memory_available_mb && stats.memory_available_mb) {
            const used = baseStats.memory_available_mb - stats.memory_available_mb;
            const perVm = Math.round(used / totalBooted);
            record(stepName, "memory-per-vm-avg", perVm, "MB");
          }
        }
      }

      // Health latency sweep: sample 5 random running VMs
      const sampleSize = Math.min(5, allSessions.length);
      const sample = allSessions
        .slice()
        .sort(() => Math.random() - 0.5)
        .slice(0, sampleSize);

      const latencies: number[] = [];
      for (const s of sample) {
        for (let h = 0; h < HEALTH_SWEEP_COUNT; h++) {
          latencies.push(await healthLatency(s.page));
        }
      }
      if (latencies.length > 0) {
        latencies.sort((a, b) => a - b);
        record(stepName, "health-p50", latencies[Math.floor(latencies.length * 0.5)], "ms");
        record(
          stepName,
          "health-p99",
          latencies[Math.floor(latencies.length * 0.99)],
          "ms"
        );
      }

      // Stop if we hit the ceiling
      if (batchFail >= Math.ceil(BATCH_SIZE / 2)) {
        record(stepName, "ceiling-hit", "yes");
        hitCeiling = true;
        break;
      }
    }

    record("summary", "total-booted", totalBooted);
    record("summary", "hit-ceiling", hitCeiling ? "yes" : "no");

    // ── I/O workload phase ────────────────────────────────────────────────
    // Run a conductor prompt on up to 3 VMs to measure workload latency.
    if (allSessions.length >= 1) {
      console.log("\n=== I/O workload phase: conductor prompts ===");
      const ioSample = allSessions.slice(0, Math.min(3, allSessions.length));

      const ioResults = await Promise.allSettled(
        ioSample.map(async (s, i) => {
          const t0 = Date.now();
          const res = await s.page.request.post("/conductor/execute", {
            data: {
              objective: "What is 2+2? Answer with just the number.",
              desktop_id: `stress-io-${Date.now()}-${i}`,
              output_mode: "auto",
            },
            timeout: 120_000,
          });
          const ms = Date.now() - t0;
          return { status: res.status(), ms };
        })
      );

      for (let i = 0; i < ioResults.length; i++) {
        const r = ioResults[i];
        if (r.status === "fulfilled") {
          record("io-workload", `prompt-${i}-status`, r.value.status);
          record("io-workload", `prompt-${i}-time`, r.value.ms, "ms");
        } else {
          record("io-workload", `prompt-${i}-error`, String(r.reason).slice(0, 100));
        }
      }
    }

    // ── Cleanup: stop all VMs ─────────────────────────────────────────────
    console.log("\n=== Cleanup: stopping all VMs ===");
    for (const s of allSessions) {
      try {
        await s.page.request.post(
          `/admin/sandboxes/${s.userId}/live/stop`,
          { timeout: 15_000 }
        );
      } catch { /* best effort */ }
    }
    // Wait for stops to propagate
    await new Promise((r) => setTimeout(r, 5_000));

    // Verify cleanup
    if (allSessions.length > 0) {
      const finalStats = await getHostStats(allSessions[0].page);
      if (finalStats) {
        record("cleanup", "memory-available", finalStats.memory_available_mb ?? 0, "MB");
        record("cleanup", "vms-running", finalStats.vms_running);
      }
    }

    // Close all browser contexts
    await Promise.all(allSessions.map((s) => s.context.close()));

    // At least the first batch should succeed
    expect(totalBooted).toBeGreaterThanOrEqual(BATCH_SIZE);
  });
});

// ── Report ───────────────────────────────────────────────────────────────────

test.afterAll(() => {
  if (metrics.length === 0) return;

  const className = MACHINE_CLASS || "(default)";
  console.log(
    "\n╔════════════════════════════════════════════════════════════════════════════════╗"
  );
  console.log(
    `║  MACHINE CLASS STRESS TEST: ${className.padEnd(48)}║`
  );
  console.log(
    "╠════════════════════════════════════════════════════════════════════════════════╣"
  );

  let lastStep = "";
  for (const m of metrics) {
    if (m.step !== lastStep) {
      if (lastStep !== "") {
        console.log(
          `║  ${"─".repeat(76)}║`
        );
      }
      lastStep = m.step;
    }
    const label = `${m.step.padEnd(12)} ${m.metric}`;
    const val = `${m.value}${m.unit ? " " + m.unit : ""}`;
    console.log(`║  ${label.padEnd(50)} ${val.padStart(24)} ║`);
  }

  console.log(
    "╚════════════════════════════════════════════════════════════════════════════════╝"
  );
});
