/**
 * Heterogeneous Workload Stress Test
 *
 * Tests real concurrent workloads on the heterogeneous topology:
 *   - User tier:   ch-blk-2c-2g     (light prompts — conductor/toast)
 *   - Worker tier:  w-ch-pmem-4c-4g  (heavy prompts — writer + terminal agent)
 *
 * Unlike the capacity test (idle VMs), this measures performance under
 * actual LLM + terminal agent load. The bottleneck shifts from memory
 * to provider gateway throughput.
 *
 * Phases:
 *   1. Ramp:     Boot users + workers in batches
 *   2. Workload: Fire concurrent prompts at all VMs
 *   3. Sustain:  Repeated prompt waves, measure gateway saturation
 *   4. Boot-under-load: Add more VMs while existing ones work
 *
 * Config via env:
 *   USER_CLASS=ch-blk-2c-2g         (default)
 *   WORKER_CLASS=w-ch-pmem-4c-4g    (default)
 *   INITIAL_USERS=5                  (users in first batch)
 *   INITIAL_WORKERS=2                (workers in first batch)
 *   PROMPT_WAVES=3                   (sustained load waves)
 *   EXTRA_BATCH=3                    (VMs to boot under load)
 *
 * Run:
 *   PLAYWRIGHT_HYPERVISOR_BASE_URL=https://draft.choir-ip.com \
 *     npx playwright test heterogeneous-workload-stress.spec.ts --project=stress
 */

import { test, expect, type BrowserContext, type Page } from "@playwright/test";

// ── Config ───────────────────────────────────────────────────────────────────

const USER_CLASS = process.env.USER_CLASS ?? "ch-blk-2c-2g";
const WORKER_CLASS = process.env.WORKER_CLASS ?? "w-ch-pmem-4c-4g";
const INITIAL_USERS = parseInt(process.env.INITIAL_USERS ?? "5", 10);
const INITIAL_WORKERS = parseInt(process.env.INITIAL_WORKERS ?? "2", 10);
const PROMPT_WAVES = parseInt(process.env.PROMPT_WAVES ?? "3", 10);
const EXTRA_BATCH = parseInt(process.env.EXTRA_BATCH ?? "3", 10);
const BOOT_BATCH_SIZE = parseInt(process.env.BOOT_BATCH_SIZE ?? "10", 10);
const HEALTH_TIMEOUT_S = parseInt(process.env.HEALTH_TIMEOUT_S ?? "60", 10);
const PROMPT_TIMEOUT_MS = 120_000;

// ── Light prompts (user tier — conductor/toast, no terminal agent) ──
const LIGHT_PROMPTS = [
  "What is 7 * 8? Just the number.",
  "What is 123 + 456? Just the number.",
  "What is the capital of France? One word.",
  "What is 2^10? Just the number.",
  "What color is the sky? One word.",
];

// ── Heavy prompts (worker tier — writer + terminal agent delegation) ──
const HEAVY_PROMPTS = [
  "In the terminal, run: go version && rustc --version && node --version && free -m && nproc. Show me the output.",
  "In the terminal, check disk usage: df -h /opt/choiros/data/sandbox && ls -la /opt/choiros/data/sandbox/workspace/. Show the output.",
  "In the terminal, create a small Go program: mkdir -p /opt/choiros/data/sandbox/workspace/hello && echo 'package main; import \"fmt\"; func main() { fmt.Println(\"hello from worker\") }' > /opt/choiros/data/sandbox/workspace/hello/main.go && cd /opt/choiros/data/sandbox/workspace/hello && go run main.go",
];

// ── Helpers ──────────────────────────────────────────────────────────────────

function log(label: string, key: string, value: string | number, unit = "") {
  const v = typeof value === "number" ? value.toLocaleString() : value;
  console.log(
    `[WSTRESS] ${label.padEnd(16)} ${key.padEnd(35)} ${v}${unit ? " " + unit : ""}`
  );
}

function percentile(arr: number[], p: number): number {
  if (arr.length === 0) return 0;
  const sorted = [...arr].sort((a, b) => a - b);
  const idx = Math.ceil((p / 100) * sorted.length) - 1;
  return sorted[Math.max(0, idx)];
}

function median(arr: number[]): number {
  return percentile(arr, 50);
}

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

interface VMSession {
  ctx: BrowserContext;
  page: Page;
  userId: string;
  tier: "user" | "worker";
  bootMs: number;
  ok: boolean;
}

async function registerAndBoot(
  browser: import("@playwright/test").Browser,
  cls: string,
  tier: "user" | "worker",
  index: number
): Promise<VMSession> {
  const ctx = await browser.newContext();
  const page = await ctx.newPage();

  try {
    const username = `ws_${tier}_${index}_${Date.now()}@test.choiros.dev`;
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

    await page.request.put("/profile/machine-class", {
      data: { class_name: cls },
      headers: { "Content-Type": "application/json" },
      timeout: 10_000,
    });

    const me = (await (await page.request.get("/auth/me")).json()) as {
      authenticated: boolean;
      user_id?: string;
    };

    const t0 = Date.now();
    for (let i = 0; i < HEALTH_TIMEOUT_S; i++) {
      try {
        const h = await page.request.get("/health", { timeout: 3_000 });
        if (h.ok()) return { ctx, page, userId: me.user_id!, bootMs: Date.now() - t0, ok: true, tier };
      } catch { /* poll */ }
      await new Promise((r) => setTimeout(r, 1000));
    }
    return { ctx, page, userId: me.user_id ?? "", bootMs: Date.now() - t0, ok: false, tier };
  } catch {
    return { ctx, page, userId: "", bootMs: 0, ok: false, tier };
  }
}

interface PromptResult {
  tier: "user" | "worker";
  status: number;
  durationMs: number;
  ok: boolean;
  userId: string;
}

async function sendPrompt(
  session: VMSession,
  objective: string
): Promise<PromptResult> {
  const t0 = Date.now();
  try {
    const r = await session.page.request.post("/conductor/execute", {
      data: {
        objective,
        desktop_id: `stress-${session.tier}-${Date.now()}`,
        output_mode: "auto",
      },
      timeout: PROMPT_TIMEOUT_MS,
    });
    return {
      tier: session.tier,
      status: r.status(),
      durationMs: Date.now() - t0,
      ok: r.status() >= 200 && r.status() < 300,
      userId: session.userId,
    };
  } catch {
    return {
      tier: session.tier,
      status: 0,
      durationMs: Date.now() - t0,
      ok: false,
      userId: session.userId,
    };
  }
}

async function healthLatency(page: Page): Promise<number> {
  const t0 = Date.now();
  try {
    await page.request.get("/health", { timeout: 10_000 });
  } catch { /* slow */ }
  return Date.now() - t0;
}

interface HostStats {
  memory_total_mb: number | null;
  memory_available_mb: number | null;
  vms_running: number;
}

async function getHostStats(page: Page): Promise<HostStats | null> {
  try {
    const r = await page.request.get("/admin/stats", { timeout: 15_000 });
    if (!r.ok()) return null;
    const data = (await r.json()) as {
      memory_total_mb?: number;
      memory_available_mb?: number;
      vms_running?: number;
    };
    return {
      memory_total_mb: data.memory_total_mb ?? null,
      memory_available_mb: data.memory_available_mb ?? null,
      vms_running: data.vms_running ?? 0,
    };
  } catch {
    return null;
  }
}

// ── Test ─────────────────────────────────────────────────────────────────────

test.describe("Heterogeneous Workload Stress", () => {
  test.setTimeout(900_000); // 15 min

  test("concurrent prompts on user + worker VMs", async ({ browser }) => {
    const allSessions: VMSession[] = [];

    log("config", "user-class", USER_CLASS);
    log("config", "worker-class", WORKER_CLASS);
    log("config", "initial-users", INITIAL_USERS);
    log("config", "initial-workers", INITIAL_WORKERS);
    log("config", "prompt-waves", PROMPT_WAVES);
    log("config", "extra-batch", EXTRA_BATCH);
    log("config", "boot-batch-size", BOOT_BATCH_SIZE);
    log("config", "health-timeout", HEALTH_TIMEOUT_S, "s");

    // Get baseline
    const probePage = await (await browser.newContext()).newPage();
    await addVirtualAuthenticator(probePage);
    await probePage.goto("/register");
    // Just use first session for host stats after boot

    // ════════════════════════════════════════════════════════════════════
    // Phase 1: Ramp — boot initial users + workers (batched)
    // ════════════════════════════════════════════════════════════════════

    const totalBoot = INITIAL_USERS + INITIAL_WORKERS;
    console.log(`\n=== Phase 1: Boot ${INITIAL_USERS} users + ${INITIAL_WORKERS} workers (batch size ${BOOT_BATCH_SIZE}) ===`);

    // Build the full boot queue: interleave users and workers
    const bootQueue: Array<{ cls: string; tier: "user" | "worker"; idx: number }> = [];
    for (let i = 0; i < INITIAL_USERS; i++) {
      bootQueue.push({ cls: USER_CLASS, tier: "user", idx: i });
    }
    for (let i = 0; i < INITIAL_WORKERS; i++) {
      bootQueue.push({ cls: WORKER_CLASS, tier: "worker", idx: i });
    }

    const t0Phase1 = Date.now();
    const bootResults: VMSession[] = [];

    // Boot in batches to avoid overwhelming concurrent boot contention
    for (let batchStart = 0; batchStart < bootQueue.length; batchStart += BOOT_BATCH_SIZE) {
      const batch = bootQueue.slice(batchStart, batchStart + BOOT_BATCH_SIZE);
      const batchNum = Math.floor(batchStart / BOOT_BATCH_SIZE) + 1;
      console.log(`  Batch ${batchNum}: booting ${batch.length} VMs (${batchStart + batch.length}/${bootQueue.length})...`);

      const batchResults = await Promise.all(
        batch.map((b) => registerAndBoot(browser, b.cls, b.tier, b.idx))
      );
      bootResults.push(...batchResults);

      const batchOk = batchResults.filter((s) => s.ok).length;
      log(`batch-${batchNum}`, "booted", `${batchOk}/${batch.length}`);
    }

    const phase1Ms = Date.now() - t0Phase1;

    for (const s of bootResults) {
      if (s.ok) allSessions.push(s);
    }

    const userSessions = allSessions.filter((s) => s.tier === "user");
    const workerSessions = allSessions.filter((s) => s.tier === "worker");
    const userBoots = bootResults.filter((s) => s.tier === "user");
    const workerBoots = bootResults.filter((s) => s.tier === "worker");

    log("phase-1", "users-booted", `${userSessions.length}/${INITIAL_USERS}`);
    log("phase-1", "workers-booted", `${workerSessions.length}/${INITIAL_WORKERS}`);
    log("phase-1", "wall-time", phase1Ms, "ms");
    log("phase-1", "user-boot-median", median(userBoots.filter((s) => s.ok).map((s) => s.bootMs)), "ms");
    log("phase-1", "worker-boot-median", median(workerBoots.filter((s) => s.ok).map((s) => s.bootMs)), "ms");

    // Host stats
    if (allSessions.length > 0) {
      const stats = await getHostStats(allSessions[0].page);
      if (stats) {
        log("phase-1", "memory-available", stats.memory_available_mb ?? 0, "MB");
        log("phase-1", "vms-running", stats.vms_running);
      }
    }

    // ════════════════════════════════════════════════════════════════════
    // Phase 2: Concurrent workload — fire prompts at all VMs
    // ════════════════════════════════════════════════════════════════════

    console.log(`\n=== Phase 2: Concurrent prompts (${allSessions.length} VMs) ===`);

    // Health baseline before workload
    const preHealthLatencies: number[] = [];
    for (const s of allSessions.slice(0, 5)) {
      preHealthLatencies.push(await healthLatency(s.page));
    }
    log("phase-2", "pre-health-p50", median(preHealthLatencies), "ms");
    log("phase-2", "pre-health-p99", percentile(preHealthLatencies, 99), "ms");

    // Fire prompts at all VMs simultaneously
    const promptPromises: Promise<PromptResult>[] = [];
    for (const s of userSessions) {
      const prompt = LIGHT_PROMPTS[Math.floor(Math.random() * LIGHT_PROMPTS.length)];
      promptPromises.push(sendPrompt(s, prompt));
    }
    for (const s of workerSessions) {
      const prompt = HEAVY_PROMPTS[Math.floor(Math.random() * HEAVY_PROMPTS.length)];
      promptPromises.push(sendPrompt(s, prompt));
    }

    const t0Phase2 = Date.now();
    const promptResults = await Promise.all(promptPromises);
    const phase2Ms = Date.now() - t0Phase2;

    const userPrompts = promptResults.filter((r) => r.tier === "user");
    const workerPrompts = promptResults.filter((r) => r.tier === "worker");

    log("phase-2", "wall-time", phase2Ms, "ms");
    log("phase-2", "user-prompts-ok", `${userPrompts.filter((r) => r.ok).length}/${userPrompts.length}`);
    log("phase-2", "worker-prompts-ok", `${workerPrompts.filter((r) => r.ok).length}/${workerPrompts.length}`);
    log("phase-2", "user-prompt-p50", median(userPrompts.map((r) => r.durationMs)), "ms");
    log("phase-2", "user-prompt-p99", percentile(userPrompts.map((r) => r.durationMs), 99), "ms");
    log("phase-2", "worker-prompt-p50", median(workerPrompts.map((r) => r.durationMs)), "ms");
    log("phase-2", "worker-prompt-p99", percentile(workerPrompts.map((r) => r.durationMs), 99), "ms");

    // Health during/after workload
    const postHealthLatencies: number[] = [];
    for (const s of allSessions.slice(0, 5)) {
      postHealthLatencies.push(await healthLatency(s.page));
    }
    log("phase-2", "post-health-p50", median(postHealthLatencies), "ms");
    log("phase-2", "post-health-p99", percentile(postHealthLatencies, 99), "ms");

    // Memory after workload
    if (allSessions.length > 0) {
      const stats = await getHostStats(allSessions[0].page);
      if (stats) {
        log("phase-2", "memory-available", stats.memory_available_mb ?? 0, "MB");
      }
    }

    // ════════════════════════════════════════════════════════════════════
    // Phase 3: Sustained load — repeated prompt waves
    // ════════════════════════════════════════════════════════════════════

    console.log(`\n=== Phase 3: Sustained load (${PROMPT_WAVES} waves) ===`);

    const waveResults: Array<{
      wave: number;
      userP50: number;
      userP99: number;
      workerP50: number;
      workerP99: number;
      healthP50: number;
      healthP99: number;
      wallMs: number;
      memAvailMb: number;
    }> = [];

    for (let wave = 1; wave <= PROMPT_WAVES; wave++) {
      const wavePromises: Promise<PromptResult>[] = [];

      for (const s of userSessions) {
        const prompt = LIGHT_PROMPTS[(wave - 1 + userSessions.indexOf(s)) % LIGHT_PROMPTS.length];
        wavePromises.push(sendPrompt(s, prompt));
      }
      for (const s of workerSessions) {
        const prompt = HEAVY_PROMPTS[(wave - 1 + workerSessions.indexOf(s)) % HEAVY_PROMPTS.length];
        wavePromises.push(sendPrompt(s, prompt));
      }

      const t0Wave = Date.now();
      const waveResults_ = await Promise.all(wavePromises);
      const waveMs = Date.now() - t0Wave;

      const waveUser = waveResults_.filter((r) => r.tier === "user");
      const waveWorker = waveResults_.filter((r) => r.tier === "worker");

      // Health check between waves
      const waveHealth: number[] = [];
      for (const s of allSessions.slice(0, Math.min(5, allSessions.length))) {
        waveHealth.push(await healthLatency(s.page));
      }

      const stats = allSessions.length > 0 ? await getHostStats(allSessions[0].page) : null;
      const memAvail = stats?.memory_available_mb ?? 0;

      const waveData = {
        wave,
        userP50: median(waveUser.map((r) => r.durationMs)),
        userP99: percentile(waveUser.map((r) => r.durationMs), 99),
        workerP50: median(waveWorker.map((r) => r.durationMs)),
        workerP99: percentile(waveWorker.map((r) => r.durationMs), 99),
        healthP50: median(waveHealth),
        healthP99: percentile(waveHealth, 99),
        wallMs: waveMs,
        memAvailMb: memAvail,
      };
      waveResults.push(waveData);

      log(`wave-${wave}`, "wall-time", waveMs, "ms");
      log(`wave-${wave}`, "user-prompt-p50", waveData.userP50, "ms");
      log(`wave-${wave}`, "worker-prompt-p50", waveData.workerP50, "ms");
      log(`wave-${wave}`, "health-p50", waveData.healthP50, "ms");
      log(`wave-${wave}`, "health-p99", waveData.healthP99, "ms");
      log(`wave-${wave}`, "memory-available", memAvail, "MB");
    }

    // ════════════════════════════════════════════════════════════════════
    // Phase 4: Boot under load — add VMs while existing ones work
    // ════════════════════════════════════════════════════════════════════

    console.log(`\n=== Phase 4: Boot ${EXTRA_BATCH} VMs under active load ===`);

    // Start a background workload on existing VMs
    const bgPromises: Promise<PromptResult>[] = [];
    for (const s of workerSessions) {
      bgPromises.push(
        sendPrompt(s, "In the terminal, run: seq 1 1000000 | wc -l && echo background_done")
      );
    }
    for (const s of userSessions.slice(0, 3)) {
      bgPromises.push(sendPrompt(s, "What is 42 * 42? Just the number."));
    }

    // Boot new VMs while background work runs
    const newBootPromises: Promise<VMSession>[] = [];
    for (let i = 0; i < EXTRA_BATCH; i++) {
      const tier = i < Math.ceil(EXTRA_BATCH / 2) ? "user" as const : "worker" as const;
      const cls = tier === "user" ? USER_CLASS : WORKER_CLASS;
      newBootPromises.push(registerAndBoot(browser, cls, tier, 100 + i));
    }

    const t0Phase4 = Date.now();
    const [bgResults, newBoots] = await Promise.all([
      Promise.all(bgPromises),
      Promise.all(newBootPromises),
    ]);
    const phase4Ms = Date.now() - t0Phase4;

    const newBootOk = newBoots.filter((s) => s.ok);
    for (const s of newBootOk) allSessions.push(s);

    log("phase-4", "wall-time", phase4Ms, "ms");
    log("phase-4", "new-vms-booted", `${newBootOk.length}/${EXTRA_BATCH}`);
    log("phase-4", "new-boot-median", median(newBootOk.map((s) => s.bootMs)), "ms");
    log("phase-4", "bg-prompts-ok", `${bgResults.filter((r) => r.ok).length}/${bgResults.length}`);
    log("phase-4", "bg-worker-p50", median(bgResults.filter((r) => r.tier === "worker").map((r) => r.durationMs)), "ms");

    const statsPhase4 = allSessions.length > 0 ? await getHostStats(allSessions[0].page) : null;
    if (statsPhase4) {
      log("phase-4", "memory-available", statsPhase4.memory_available_mb ?? 0, "MB");
      log("phase-4", "vms-running", statsPhase4.vms_running);
    }

    // ════════════════════════════════════════════════════════════════════
    // Summary
    // ════════════════════════════════════════════════════════════════════

    console.log("\n=== Summary ===");
    log("summary", "total-users", userSessions.length + newBootOk.filter((s) => s.tier === "user").length);
    log("summary", "total-workers", workerSessions.length + newBootOk.filter((s) => s.tier === "worker").length);
    log("summary", "total-vms", allSessions.length);
    log("summary", "phase1-boot-wall", phase1Ms, "ms");
    log("summary", "phase2-concurrent-wall", phase2Ms, "ms");
    log("summary", "phase4-boot-under-load-wall", phase4Ms, "ms");

    // Wave summary table
    if (waveResults.length > 0) {
      console.log("\n  Wave | User p50 | Worker p50 | Health p50 | Health p99 | Mem Avail");
      console.log("  -----|----------|------------|------------|------------|----------");
      for (const w of waveResults) {
        console.log(
          `  ${String(w.wave).padStart(4)} | ${String(w.userP50).padStart(7)}ms | ${String(w.workerP50).padStart(9)}ms | ${String(w.healthP50).padStart(9)}ms | ${String(w.healthP99).padStart(9)}ms | ${String(w.memAvailMb).padStart(6)} MB`
        );
      }
    }

    // ════════════════════════════════════════════════════════════════════
    // Cleanup
    // ════════════════════════════════════════════════════════════════════

    console.log("\n=== Cleanup ===");
    for (const s of allSessions) {
      try {
        await s.page.request.post(`/admin/sandboxes/${s.userId}/live/stop`, {
          timeout: 5_000,
        });
      } catch { /* best-effort */ }
    }
    // Wait for cleanup
    await new Promise((r) => setTimeout(r, 5_000));

    if (allSessions.length > 0) {
      const finalStats = await getHostStats(allSessions[0].page);
      if (finalStats) {
        log("cleanup", "memory-available", finalStats.memory_available_mb ?? 0, "MB");
        log("cleanup", "vms-running", finalStats.vms_running);
      }
    }

    for (const s of allSessions) {
      await s.ctx.close().catch(() => {});
    }
    await probePage.context().close().catch(() => {});

    // At least initial boot should succeed
    expect(allSessions.length).toBeGreaterThanOrEqual(
      Math.floor((INITIAL_USERS + INITIAL_WORKERS) * 0.7)
    );
  });
});
