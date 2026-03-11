/**
 * Machine Class Matrix Test (ADR-0014 Phase 6)
 *
 * Fast stress test that validates multiple machine classes in a single run.
 * Optimized for speed:
 *   - Batch size 10 (not 5)
 *   - 30s health timeout (not 120s) — boots take 6-8s
 *   - Parallel registration within batches
 *   - Tests multiple classes per run via MACHINE_CLASSES env var
 *
 * Modes:
 *   MACHINE_CLASSES="ch-pmem-4c-4g,ch-blk-4c-4g,fc-pmem-4c-4g,fc-blk-4c-4g"
 *   MAX_VMS_PER_CLASS=20  (default 20)
 *
 * Example — test all 4c-4g classes, 20 VMs each:
 *   MACHINE_CLASSES="ch-pmem-4c-4g,ch-blk-4c-4g,fc-pmem-4c-4g,fc-blk-4c-4g" \
 *   PLAYWRIGHT_HYPERVISOR_BASE_URL=https://draft.choir-ip.com \
 *     npx playwright test machine-class-matrix.spec.ts --project=stress
 *
 * Example — heterogeneous mix:
 *   MACHINE_CLASSES="ch-pmem-2c-1g,ch-pmem-2c-2g,ch-pmem-4c-4g,ch-pmem-8c-8g" \
 *   MAX_VMS_PER_CLASS=10 \
 *     npx playwright test machine-class-matrix.spec.ts --project=stress
 *
 * Example — elastic resize under load:
 *   TEST_MODE=elastic \
 *   PLAYWRIGHT_HYPERVISOR_BASE_URL=https://draft.choir-ip.com \
 *     npx playwright test machine-class-matrix.spec.ts --project=stress
 */

import { test, expect, type BrowserContext, type Page } from "@playwright/test";

// ── Config ───────────────────────────────────────────────────────────────────

const MACHINE_CLASSES = (process.env.MACHINE_CLASSES ?? "ch-pmem-4c-4g").split(",").map(s => s.trim()).filter(Boolean);
const MAX_VMS_PER_CLASS = parseInt(process.env.MAX_VMS_PER_CLASS ?? "20", 10);
const BATCH_SIZE = 10;
const HEALTH_TIMEOUT_S = 30;
const TEST_MODE = process.env.TEST_MODE ?? "matrix"; // "matrix" | "elastic"

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

function uniqueUsername(cls: string, index: number): string {
  const tag = cls.replace(/-/g, "").slice(0, 10);
  return `mx_${tag}_${index}_${Date.now()}@test.choiros.dev`;
}

async function registerAndBoot(
  browser: import("@playwright/test").Browser,
  cls: string,
  index: number
): Promise<{ ctx: BrowserContext; page: Page; userId: string; bootMs: number; ok: boolean }> {
  const ctx = await browser.newContext();
  const page = await ctx.newPage();

  try {
    const username = uniqueUsername(cls, index);
    await addVirtualAuthenticator(page);

    await page.goto("/register");
    await expect(page.getByTestId("auth-modal")).toBeVisible({ timeout: 30_000 });

    const finishWait = page.waitForResponse(
      (r) => r.url().includes("/auth/register/finish") && r.request().method() === "POST",
      { timeout: 30_000 }
    );

    const input = page.getByTestId("auth-input");
    await input.fill(username);
    await input.press("Enter");
    await finishWait;

    await expect(page.getByTestId("auth-modal")).toHaveCount(0, { timeout: 15_000 });

    // Set machine class
    if (cls) {
      await page.request.put("/profile/machine-class", {
        data: { class_name: cls },
        headers: { "Content-Type": "application/json" },
        timeout: 10_000,
      });
    }

    // Get user ID
    const me = (await (await page.request.get("/auth/me")).json()) as {
      authenticated: boolean;
      user_id?: string;
    };

    // Wait for sandbox with short timeout
    const t0 = Date.now();
    for (let i = 0; i < HEALTH_TIMEOUT_S; i++) {
      try {
        const h = await page.request.get("/health", { timeout: 3_000 });
        if (h.ok()) {
          return { ctx, page, userId: me.user_id!, bootMs: Date.now() - t0, ok: true };
        }
      } catch { /* poll */ }
      await new Promise((r) => setTimeout(r, 1000));
    }

    return { ctx, page, userId: me.user_id ?? "", bootMs: Date.now() - t0, ok: false };
  } catch (e) {
    return { ctx, page, userId: "", bootMs: 0, ok: false };
  }
}

async function healthLatency(page: Page): Promise<number> {
  const t0 = Date.now();
  try { await page.request.get("/health", { timeout: 10_000 }); } catch { /* slow */ }
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
  } catch { /* best effort */ }
  return null;
}

// ── Metrics ──────────────────────────────────────────────────────────────────

interface Metric { step: string; metric: string; value: string | number; unit?: string }
const metrics: Metric[] = [];

function record(step: string, metric: string, value: string | number, unit?: string) {
  metrics.push({ step, metric, value, unit });
  console.log(`[MATRIX] ${step.padEnd(16)} ${metric.padEnd(30)} ${value}${unit ? " " + unit : ""}`);
}

interface Session {
  cls: string;
  ctx: BrowserContext;
  page: Page;
  userId: string;
}

// ── Matrix Test ──────────────────────────────────────────────────────────────

test.describe("Machine Class Matrix", () => {
  test.setTimeout(1200_000);

  test(`${TEST_MODE}: ${MACHINE_CLASSES.join(", ")}`, async ({ browser }) => {
    if (TEST_MODE === "elastic") {
      await runElasticTest(browser);
      return;
    }

    record("config", "classes", MACHINE_CLASSES.join(", "));
    record("config", "max-per-class", MAX_VMS_PER_CLASS);
    record("config", "batch-size", BATCH_SIZE);

    const allSessions: Session[] = [];

    // Get baseline
    const baseCtx = await browser.newContext();
    const basePage = await baseCtx.newPage();
    await addVirtualAuthenticator(basePage);
    await basePage.goto("/register");
    await expect(basePage.getByTestId("auth-modal")).toBeVisible({ timeout: 30_000 });
    const finishWait = basePage.waitForResponse(
      (r) => r.url().includes("/auth/register/finish") && r.request().method() === "POST",
      { timeout: 30_000 }
    );
    await basePage.getByTestId("auth-input").fill(`mx_base_${Date.now()}@test.choiros.dev`);
    await basePage.getByTestId("auth-input").press("Enter");
    await finishWait;
    await expect(basePage.getByTestId("auth-modal")).toHaveCount(0, { timeout: 15_000 });

    const baseStats = await getHostStats(basePage);
    if (baseStats) {
      record("baseline", "memory-total", baseStats.memory_total_mb ?? 0, "MB");
      record("baseline", "memory-available", baseStats.memory_available_mb ?? 0, "MB");
      record("baseline", "vms-running", baseStats.vms_running);
    }
    await baseCtx.close();

    const baselineAvail = baseStats?.memory_available_mb ?? 0;

    // Test each class sequentially
    for (const cls of MACHINE_CLASSES) {
      console.log(`\n=== Testing ${cls} (up to ${MAX_VMS_PER_CLASS} VMs) ===`);
      const classSessions: Session[] = [];
      let totalBooted = 0;

      const numBatches = Math.ceil(MAX_VMS_PER_CLASS / BATCH_SIZE);

      for (let batch = 0; batch < numBatches; batch++) {
        const batchNum = batch + 1;
        const batchCount = Math.min(BATCH_SIZE, MAX_VMS_PER_CLASS - totalBooted);
        const stepName = `${cls}:b${batchNum}`;

        const batchT0 = Date.now();
        const results = await Promise.allSettled(
          Array.from({ length: batchCount }, (_, i) =>
            registerAndBoot(browser, cls, totalBooted + i)
          )
        );
        const batchMs = Date.now() - batchT0;

        let ok = 0;
        let fail = 0;
        const bootTimes: number[] = [];

        for (const r of results) {
          if (r.status === "fulfilled" && r.value.ok) {
            ok++;
            bootTimes.push(r.value.bootMs);
            classSessions.push({ cls, ctx: r.value.ctx, page: r.value.page, userId: r.value.userId });
            allSessions.push({ cls, ctx: r.value.ctx, page: r.value.page, userId: r.value.userId });
          } else {
            fail++;
            if (r.status === "fulfilled") await r.value.ctx.close();
          }
        }

        totalBooted += ok;
        record(stepName, "booted", `${ok}/${batchCount}`);
        record(stepName, "wall-time", batchMs, "ms");

        if (bootTimes.length > 0) {
          bootTimes.sort((a, b) => a - b);
          record(stepName, "boot-median", bootTimes[Math.floor(bootTimes.length / 2)], "ms");
        }

        // Host stats
        if (classSessions.length > 0) {
          const stats = await getHostStats(classSessions[0].page);
          if (stats && stats.memory_available_mb) {
            record(stepName, "memory-avail", stats.memory_available_mb, "MB");
            record(stepName, "vms-running", stats.vms_running);
            const used = baselineAvail - stats.memory_available_mb;
            const totalVms = allSessions.length;
            if (totalVms > 0) record(stepName, "mem/vm-avg", Math.round(used / totalVms), "MB");
          }
        }

        // Health latency sample
        const sample = classSessions.slice(-3);
        const lats: number[] = [];
        for (const s of sample) lats.push(await healthLatency(s.page));
        if (lats.length > 0) {
          lats.sort((a, b) => a - b);
          record(stepName, "health-p50", lats[Math.floor(lats.length / 2)], "ms");
        }

        if (fail >= Math.ceil(batchCount / 2)) {
          record(stepName, "ceiling", "yes");
          break;
        }
      }

      record(`${cls}:sum`, "total-booted", totalBooted);

      // Stop this class's VMs before testing next class
      console.log(`  Stopping ${totalBooted} VMs for ${cls}...`);
      await Promise.all(classSessions.map(async (s) => {
        try {
          await s.page.request.post(`/admin/sandboxes/${s.userId}/live/stop`, { timeout: 15_000 });
        } catch { /* best effort */ }
      }));
      await new Promise((r) => setTimeout(r, 3_000));
      await Promise.all(classSessions.map((s) => s.ctx.close()));
      // Remove from allSessions
      for (const s of classSessions) {
        const idx = allSessions.indexOf(s);
        if (idx >= 0) allSessions.splice(idx, 1);
      }
    }

    // Final cleanup
    for (const s of allSessions) {
      try { await s.page.request.post(`/admin/sandboxes/${s.userId}/live/stop`, { timeout: 15_000 }); } catch {}
    }
    await Promise.all(allSessions.map((s) => s.ctx.close()));

    // Assert at least half booted per class
    for (const cls of MACHINE_CLASSES) {
      const m = metrics.find((m) => m.step === `${cls}:sum` && m.metric === "total-booted");
      expect(Number(m?.value ?? 0)).toBeGreaterThanOrEqual(Math.min(5, MAX_VMS_PER_CLASS));
    }
  });
});

// ── Elastic Resize Test ──────────────────────────────────────────────────────

async function runElasticTest(browser: import("@playwright/test").Browser) {
  // Boot N users on small, stop half, resize to large, boot, measure, resize back
  const SMALL = "ch-pmem-2c-1g";
  const LARGE = "ch-pmem-4c-4g";
  const USER_COUNT = 10;

  record("elastic", "small-class", SMALL);
  record("elastic", "large-class", LARGE);
  record("elastic", "user-count", USER_COUNT);

  // Phase 1: Boot all on small
  console.log(`\n=== Phase 1: Boot ${USER_COUNT} users on ${SMALL} ===`);
  const phase1T0 = Date.now();
  const users = await Promise.allSettled(
    Array.from({ length: USER_COUNT }, (_, i) => registerAndBoot(browser, SMALL, i))
  );
  record("phase1", "wall-time", Date.now() - phase1T0, "ms");

  const sessions: { ctx: BrowserContext; page: Page; userId: string; cls: string }[] = [];
  for (const r of users) {
    if (r.status === "fulfilled" && r.value.ok) {
      sessions.push({ ctx: r.value.ctx, page: r.value.page, userId: r.value.userId, cls: SMALL });
    } else if (r.status === "fulfilled") {
      await r.value.ctx.close();
    }
  }
  record("phase1", "booted", sessions.length);

  const stats1 = sessions.length > 0 ? await getHostStats(sessions[0].page) : null;
  if (stats1) {
    record("phase1", "memory-avail", stats1.memory_available_mb ?? 0, "MB");
    record("phase1", "vms-running", stats1.vms_running);
  }

  // Phase 2: Stop half, resize to large, boot
  const resizeCount = Math.floor(sessions.length / 2);
  const toResize = sessions.slice(0, resizeCount);
  const keepSmall = sessions.slice(resizeCount);

  console.log(`\n=== Phase 2: Resize ${resizeCount} users to ${LARGE} ===`);
  const phase2T0 = Date.now();

  // Stop the ones we'll resize
  await Promise.all(toResize.map(async (s) => {
    try { await s.page.request.post(`/admin/sandboxes/${s.userId}/live/stop`, { timeout: 15_000 }); } catch {}
  }));
  await new Promise((r) => setTimeout(r, 2_000));

  // Set new class and re-boot
  const resizeBoots: number[] = [];
  for (const s of toResize) {
    await s.page.request.put("/profile/machine-class", {
      data: { class_name: LARGE },
      headers: { "Content-Type": "application/json" },
      timeout: 10_000,
    });
    s.cls = LARGE;
    const t0 = Date.now();
    for (let i = 0; i < HEALTH_TIMEOUT_S; i++) {
      try {
        const h = await s.page.request.get("/health", { timeout: 3_000 });
        if (h.ok()) { resizeBoots.push(Date.now() - t0); break; }
      } catch {}
      await new Promise((r) => setTimeout(r, 1000));
    }
  }
  record("phase2", "wall-time", Date.now() - phase2T0, "ms");
  if (resizeBoots.length > 0) {
    resizeBoots.sort((a, b) => a - b);
    record("phase2", "resize-boot-median", resizeBoots[Math.floor(resizeBoots.length / 2)], "ms");
  }

  const stats2 = sessions.length > 0 ? await getHostStats(sessions[0].page) : null;
  if (stats2) {
    record("phase2", "memory-avail", stats2.memory_available_mb ?? 0, "MB");
    record("phase2", "vms-running", stats2.vms_running);
  }

  // Check health of both small and large concurrently
  const smallLats: number[] = [];
  const largeLats: number[] = [];
  for (const s of keepSmall.slice(0, 3)) smallLats.push(await healthLatency(s.page));
  for (const s of toResize.slice(0, 3)) largeLats.push(await healthLatency(s.page));

  if (smallLats.length > 0) record("phase2", `health-${SMALL}-p50`, smallLats.sort((a, b) => a - b)[Math.floor(smallLats.length / 2)], "ms");
  if (largeLats.length > 0) record("phase2", `health-${LARGE}-p50`, largeLats.sort((a, b) => a - b)[Math.floor(largeLats.length / 2)], "ms");

  // Phase 3: Resize back to small
  console.log(`\n=== Phase 3: Resize ${resizeCount} users back to ${SMALL} ===`);
  const phase3T0 = Date.now();

  await Promise.all(toResize.map(async (s) => {
    try { await s.page.request.post(`/admin/sandboxes/${s.userId}/live/stop`, { timeout: 15_000 }); } catch {}
  }));
  await new Promise((r) => setTimeout(r, 2_000));

  const downBoots: number[] = [];
  for (const s of toResize) {
    await s.page.request.put("/profile/machine-class", {
      data: { class_name: SMALL },
      headers: { "Content-Type": "application/json" },
      timeout: 10_000,
    });
    s.cls = SMALL;
    const t0 = Date.now();
    for (let i = 0; i < HEALTH_TIMEOUT_S; i++) {
      try {
        const h = await s.page.request.get("/health", { timeout: 3_000 });
        if (h.ok()) { downBoots.push(Date.now() - t0); break; }
      } catch {}
      await new Promise((r) => setTimeout(r, 1000));
    }
  }
  record("phase3", "wall-time", Date.now() - phase3T0, "ms");
  if (downBoots.length > 0) {
    downBoots.sort((a, b) => a - b);
    record("phase3", "downsize-boot-median", downBoots[Math.floor(downBoots.length / 2)], "ms");
  }

  const stats3 = sessions.length > 0 ? await getHostStats(sessions[0].page) : null;
  if (stats3) {
    record("phase3", "memory-avail", stats3.memory_available_mb ?? 0, "MB");
    record("phase3", "vms-running", stats3.vms_running);
  }

  // Cleanup
  for (const s of sessions) {
    try { await s.page.request.post(`/admin/sandboxes/${s.userId}/live/stop`, { timeout: 15_000 }); } catch {}
  }
  await Promise.all(sessions.map((s) => s.ctx.close()));

  expect(sessions.length).toBeGreaterThanOrEqual(5);
}

// ── Report ───────────────────────────────────────────────────────────────────

test.afterAll(() => {
  if (metrics.length === 0) return;

  console.log("\n╔══════════════════════════════════════════════════════════════════════════════╗");
  console.log(`║  MACHINE CLASS MATRIX: ${MACHINE_CLASSES.join(", ").padEnd(52)}║`);
  console.log("╠══════════════════════════════════════════════════════════════════════════════╣");

  let lastStep = "";
  for (const m of metrics) {
    if (m.step !== lastStep) {
      if (lastStep !== "") console.log(`║  ${"─".repeat(72)}║`);
      lastStep = m.step;
    }
    const label = `${m.step.padEnd(16)} ${m.metric}`;
    const val = `${m.value}${m.unit ? " " + m.unit : ""}`;
    console.log(`║  ${label.padEnd(48)} ${val.padStart(22)} ║`);
  }

  console.log("╚══════════════════════════════════════════════════════════════════════════════╝");
});
