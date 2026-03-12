/**
 * Compute Workload Stress Test
 *
 * Boots multiple worker VMs and runs real compute tasks concurrently:
 *   - Go compilation (clone + build fzf)
 *   - Rust compilation (create + build calculator)
 *   - Node.js script execution
 *   - Playwright browser automation (navigate a website, take screenshot)
 *
 * All work flows through the conductor → writer → terminal agent pipeline
 * via the prompt bar UI. This tests compute-heavy (CPU + disk) workloads,
 * not just LLM throughput.
 *
 * Config via env:
 *   WORKER_CLASS=w-ch-pmem-4c-4g     (default)
 *   NUM_WORKERS=3                     (concurrent workers, default 3)
 *   PROMPT_TIMEOUT_S=300              (per-prompt timeout, default 5 min)
 *
 * Run:
 *   PLAYWRIGHT_HYPERVISOR_BASE_URL=https://draft.choir-ip.com \
 *     npx playwright test compute-workload-stress.spec.ts --project=stress
 */

import { test, expect, type BrowserContext, type Page } from "@playwright/test";

// ── Config ───────────────────────────────────────────────────────────────────

const WORKER_CLASS = process.env.WORKER_CLASS ?? "w-ch-pmem-4c-4g";
const NUM_WORKERS = parseInt(process.env.NUM_WORKERS ?? "3", 10);
const PROMPT_TIMEOUT_MS = parseInt(process.env.PROMPT_TIMEOUT_S ?? "300", 10) * 1000;
const HEALTH_TIMEOUT_S = 60;

// ── Compute workloads — each is a prompt sent through the prompt bar ──

const COMPUTE_WORKLOADS = [
  {
    name: "go-compile-fzf",
    prompt: "In the terminal: cd /opt/choiros/data/sandbox/workspace && git clone --depth 1 https://github.com/junegunn/fzf.git fzf-$RANDOM && cd fzf-* && go build -o fzf-binary . && ls -lh fzf-binary && echo COMPILE_DONE",
    description: "Clone and compile fzf (Go)",
  },
  {
    name: "rust-hello",
    prompt: "In the terminal: cd /opt/choiros/data/sandbox/workspace && cargo init --name hello-stress hello-stress-$RANDOM && cd hello-stress-* && cargo build --release && ./target/release/hello-stress && echo BUILD_DONE",
    description: "Create and build a Rust hello world",
  },
  {
    name: "node-compute",
    prompt: "In the terminal: node -e \"const crypto = require('crypto'); const start = Date.now(); let hash = 'seed'; for (let i = 0; i < 100000; i++) hash = crypto.createHash('sha256').update(hash).digest('hex'); console.log('100K SHA256 hashes in', Date.now() - start, 'ms'); console.log('Final:', hash.slice(0,16)); console.log('NODE_DONE')\"",
    description: "Node.js SHA256 hash chain (CPU-bound)",
  },
  {
    name: "playwright-browse",
    prompt: "In the terminal: cd /opt/choiros/data/sandbox/workspace && npx --yes playwright install chromium 2>&1 | tail -5 && node -e \"const { chromium } = require('playwright'); (async () => { const b = await chromium.launch({args:['--no-sandbox']}); const p = await b.newPage(); await p.goto('https://example.com'); const title = await p.title(); console.log('Page title:', title); const text = await p.locator('h1').innerText(); console.log('H1:', text); await b.close(); console.log('BROWSER_DONE'); })()\"",
    description: "Playwright: open example.com, read title + h1",
  },
  {
    name: "disk-io",
    prompt: "In the terminal: cd /opt/choiros/data/sandbox/workspace && dd if=/dev/urandom of=testfile bs=1M count=50 2>&1 && md5sum testfile && rm testfile && echo DISK_DONE",
    description: "50MB random write + checksum (disk I/O)",
  },
  {
    name: "go-test-suite",
    prompt: "In the terminal: cd /opt/choiros/data/sandbox/workspace && mkdir -p gotest-$RANDOM && cd gotest-* && go mod init gotest && cat > main_test.go << 'GOEOF'\npackage main\nimport (\"testing\"; \"math\")\nfunc TestPrimes(t *testing.T) { count := 0; for i := 2; i < 10000; i++ { prime := true; for j := 2; j <= int(math.Sqrt(float64(i))); j++ { if i%j == 0 { prime = false; break } }; if prime { count++ } }; if count != 1229 { t.Errorf(\"expected 1229 primes, got %d\", count) }; t.Logf(\"Found %d primes under 10000\", count) }\nGOEOF\ngo test -v ./... && echo TEST_DONE",
    description: "Go test suite (prime sieve)",
  },
];

// ── Helpers ──────────────────────────────────────────────────────────────────

function log(label: string, key: string, value: string | number, unit = "") {
  const v = typeof value === "number" ? value.toLocaleString() : value;
  console.log(
    `[COMPUTE] ${label.padEnd(16)} ${key.padEnd(35)} ${v}${unit ? " " + unit : ""}`
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

interface WorkerSession {
  ctx: BrowserContext;
  page: Page;
  userId: string;
  bootMs: number;
  ok: boolean;
  index: number;
}

async function registerAndBoot(
  browser: import("@playwright/test").Browser,
  cls: string,
  index: number
): Promise<WorkerSession> {
  const ctx = await browser.newContext();
  const page = await ctx.newPage();

  try {
    const username = `compute_w${index}_${Date.now()}@test.choiros.dev`;
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
        if (h.ok()) return { ctx, page, userId: me.user_id!, bootMs: Date.now() - t0, ok: true, index };
      } catch { /* poll */ }
      await new Promise((r) => setTimeout(r, 1000));
    }
    return { ctx, page, userId: me.user_id ?? "", bootMs: Date.now() - t0, ok: false, index };
  } catch {
    return { ctx, page, userId: "", bootMs: 0, ok: false, index };
  }
}

interface ComputeResult {
  workerIndex: number;
  workloadName: string;
  status: number;
  durationMs: number;
  ok: boolean;
}

async function sendComputePrompt(
  session: WorkerSession,
  workload: typeof COMPUTE_WORKLOADS[number]
): Promise<ComputeResult> {
  const t0 = Date.now();
  try {
    const r = await session.page.request.post("/conductor/execute", {
      data: {
        objective: workload.prompt,
        desktop_id: `compute-w${session.index}-${Date.now()}`,
        output_mode: "auto",
      },
      timeout: PROMPT_TIMEOUT_MS,
    });
    return {
      workerIndex: session.index,
      workloadName: workload.name,
      status: r.status(),
      durationMs: Date.now() - t0,
      ok: r.status() >= 200 && r.status() < 300,
    };
  } catch {
    return {
      workerIndex: session.index,
      workloadName: workload.name,
      status: 0,
      durationMs: Date.now() - t0,
      ok: false,
    };
  }
}

async function getHostStats(page: Page): Promise<{
  memory_available_mb: number | null;
  vms_running: number;
} | null> {
  try {
    const r = await page.request.get("/admin/stats", { timeout: 15_000 });
    if (!r.ok()) return null;
    const data = (await r.json()) as {
      memory_available_mb?: number;
      vms_running?: number;
    };
    return {
      memory_available_mb: data.memory_available_mb ?? null,
      vms_running: data.vms_running ?? 0,
    };
  } catch {
    return null;
  }
}

async function healthLatency(page: Page): Promise<number> {
  const t0 = Date.now();
  try {
    await page.request.get("/health", { timeout: 10_000 });
  } catch { /* slow */ }
  return Date.now() - t0;
}

// ── Test ─────────────────────────────────────────────────────────────────────

test.describe("Compute Workload Stress", () => {
  test.setTimeout(900_000); // 15 min

  test("concurrent compute tasks across worker VMs", async ({ browser }) => {
    const sessions: WorkerSession[] = [];

    log("config", "worker-class", WORKER_CLASS);
    log("config", "num-workers", NUM_WORKERS);
    log("config", "prompt-timeout", PROMPT_TIMEOUT_MS / 1000, "s");
    log("config", "workloads-available", COMPUTE_WORKLOADS.length);

    // ════════════════════════════════════════════════════════════════════
    // Phase 1: Boot worker VMs
    // ════════════════════════════════════════════════════════════════════

    console.log(`\n=== Phase 1: Boot ${NUM_WORKERS} worker VMs ===`);

    const t0Boot = Date.now();
    const bootResults = await Promise.all(
      Array.from({ length: NUM_WORKERS }, (_, i) =>
        registerAndBoot(browser, WORKER_CLASS, i)
      )
    );
    const bootMs = Date.now() - t0Boot;

    for (const s of bootResults) {
      if (s.ok) sessions.push(s);
      log("boot", `worker-${s.index}`, s.ok ? `${s.bootMs}ms` : "FAILED");
    }

    log("phase-1", "workers-booted", `${sessions.length}/${NUM_WORKERS}`);
    log("phase-1", "boot-wall-time", bootMs, "ms");
    log("phase-1", "boot-median", median(sessions.map((s) => s.bootMs)), "ms");

    const stats1 = sessions.length > 0 ? await getHostStats(sessions[0].page) : null;
    if (stats1) {
      log("phase-1", "memory-available", stats1.memory_available_mb ?? 0, "MB");
      log("phase-1", "vms-running", stats1.vms_running);
    }

    expect(sessions.length).toBeGreaterThan(0);

    // ════════════════════════════════════════════════════════════════════
    // Phase 2: Concurrent compute — different workload per worker
    // ════════════════════════════════════════════════════════════════════

    console.log(`\n=== Phase 2: Concurrent compute (${sessions.length} workers) ===`);

    // Pre-compute health baseline
    const preHealth: number[] = [];
    for (const s of sessions.slice(0, 3)) {
      preHealth.push(await healthLatency(s.page));
    }
    log("phase-2", "pre-health-p50", median(preHealth), "ms");

    // Assign different workloads to each worker (round-robin)
    const computePromises: Promise<ComputeResult>[] = [];
    for (let i = 0; i < sessions.length; i++) {
      const workload = COMPUTE_WORKLOADS[i % COMPUTE_WORKLOADS.length];
      log("phase-2", `worker-${i}-task`, workload.name);
      computePromises.push(sendComputePrompt(sessions[i], workload));
    }

    const t0Compute = Date.now();
    const computeResults = await Promise.all(computePromises);
    const computeMs = Date.now() - t0Compute;

    for (const r of computeResults) {
      const status = r.ok ? "OK" : `FAIL(${r.status})`;
      log("phase-2", `worker-${r.workerIndex}-${r.workloadName}`, `${r.durationMs}ms ${status}`);
    }

    log("phase-2", "wall-time", computeMs, "ms");
    log("phase-2", "tasks-ok", `${computeResults.filter((r) => r.ok).length}/${computeResults.length}`);
    log("phase-2", "task-p50", median(computeResults.map((r) => r.durationMs)), "ms");
    log("phase-2", "task-p99", percentile(computeResults.map((r) => r.durationMs), 99), "ms");

    // Post-compute health
    const postHealth: number[] = [];
    for (const s of sessions.slice(0, 3)) {
      postHealth.push(await healthLatency(s.page));
    }
    log("phase-2", "post-health-p50", median(postHealth), "ms");

    const stats2 = sessions.length > 0 ? await getHostStats(sessions[0].page) : null;
    if (stats2) {
      log("phase-2", "memory-available", stats2.memory_available_mb ?? 0, "MB");
    }

    // ════════════════════════════════════════════════════════════════════
    // Phase 3: Sequential compute — same workload across all workers
    // ════════════════════════════════════════════════════════════════════

    console.log(`\n=== Phase 3: All workers compile Go (concurrent) ===`);

    const goWorkload = COMPUTE_WORKLOADS[0]; // go-compile-fzf
    const goPromises: Promise<ComputeResult>[] = [];
    for (const s of sessions) {
      goPromises.push(sendComputePrompt(s, goWorkload));
    }

    const t0Go = Date.now();
    const goResults = await Promise.all(goPromises);
    const goMs = Date.now() - t0Go;

    for (const r of goResults) {
      const status = r.ok ? "OK" : `FAIL(${r.status})`;
      log("phase-3", `worker-${r.workerIndex}`, `${r.durationMs}ms ${status}`);
    }

    log("phase-3", "wall-time", goMs, "ms");
    log("phase-3", "tasks-ok", `${goResults.filter((r) => r.ok).length}/${goResults.length}`);
    log("phase-3", "compile-p50", median(goResults.map((r) => r.durationMs)), "ms");

    const healthDuring: number[] = [];
    for (const s of sessions.slice(0, 3)) {
      healthDuring.push(await healthLatency(s.page));
    }
    log("phase-3", "health-p50", median(healthDuring), "ms");

    const stats3 = sessions.length > 0 ? await getHostStats(sessions[0].page) : null;
    if (stats3) {
      log("phase-3", "memory-available", stats3.memory_available_mb ?? 0, "MB");
    }

    // ════════════════════════════════════════════════════════════════════
    // Summary
    // ════════════════════════════════════════════════════════════════════

    console.log("\n=== Summary ===");

    console.log("\n  Phase 2 — Heterogeneous compute (different task per worker):");
    console.log("  Worker | Task               | Duration  | Status");
    console.log("  -------|--------------------|-----------|-------");
    for (const r of computeResults) {
      const status = r.ok ? "OK" : `FAIL`;
      console.log(
        `  ${String(r.workerIndex).padStart(6)} | ${r.workloadName.padEnd(18)} | ${String(r.durationMs).padStart(7)}ms | ${status}`
      );
    }

    console.log("\n  Phase 3 — Homogeneous compute (all workers compile Go):");
    console.log("  Worker | Duration  | Status");
    console.log("  -------|-----------|-------");
    for (const r of goResults) {
      const status = r.ok ? "OK" : `FAIL`;
      console.log(
        `  ${String(r.workerIndex).padStart(6)} | ${String(r.durationMs).padStart(7)}ms | ${status}`
      );
    }

    // ════════════════════════════════════════════════════════════════════
    // Cleanup
    // ════════════════════════════════════════════════════════════════════

    console.log("\n=== Cleanup ===");
    for (const s of sessions) {
      try {
        await s.page.request.post(`/admin/sandboxes/${s.userId}/live/stop`, { timeout: 5_000 });
      } catch { /* best-effort */ }
    }
    await new Promise((r) => setTimeout(r, 5_000));

    const finalStats = sessions.length > 0 ? await getHostStats(sessions[0].page) : null;
    if (finalStats) {
      log("cleanup", "memory-available", finalStats.memory_available_mb ?? 0, "MB");
      log("cleanup", "vms-running", finalStats.vms_running);
    }

    for (const s of sessions) {
      await s.ctx.close().catch(() => {});
    }

    expect(sessions.length).toBeGreaterThan(0);
  });
});
