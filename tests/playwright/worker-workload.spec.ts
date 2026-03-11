/**
 * Worker Workload Test
 *
 * Boots a worker-class VM (thick guest image with Go, Rust, Node.js, etc.)
 * and sends real development prompts through the prompt bar UI. The conductor
 * → writer → terminal agent pipeline handles the rest. The living document
 * updates in real-time as work progresses.
 *
 * Video recording captures the full flow (configured in playwright.config.ts).
 * These are long-running tests (minutes per prompt) — set generous timeouts.
 *
 * Config via env:
 *   WORKER_CLASS=w-ch-pmem-4c-4g   (default)
 *   PROMPT_TIMEOUT_S=300            (per-prompt timeout, default 5 min)
 *
 * Run:
 *   PLAYWRIGHT_HYPERVISOR_BASE_URL=https://draft.choir-ip.com \
 *     npx playwright test worker-workload.spec.ts --project=hypervisor
 */

import {
  test,
  expect,
  type BrowserContext,
  type Page,
  type Locator,
  type APIRequestContext,
} from "@playwright/test";

// ── Config ───────────────────────────────────────────────────────────────────

const WORKER_CLASS = process.env.WORKER_CLASS ?? "w-ch-pmem-4c-4g";
const PROMPT_TIMEOUT_MS = parseInt(process.env.PROMPT_TIMEOUT_S ?? "300", 10) * 1000;
const DESKTOP_ID = "default-desktop";

// ── Helpers ──────────────────────────────────────────────────────────────────

function log(label: string, key: string, value: string | number, unit = "") {
  const v = typeof value === "number" ? value.toLocaleString() : value;
  console.log(
    `[WORKER] ${label.padEnd(16)} ${key.padEnd(30)} ${v}${unit ? " " + unit : ""}`
  );
}

async function authMe(page: Page): Promise<{ authenticated: boolean; user_id?: string }> {
  const res = await page.request.get("/auth/me");
  if (!res.ok()) return { authenticated: false };
  return (await res.json()) as { authenticated: boolean; user_id?: string };
}

async function registerViaWebAuthn(page: Page): Promise<string> {
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

  const username = `worker_${Date.now()}_${Math.random().toString(36).slice(2, 6)}@test.choiros.dev`;
  await page.goto("/register", { waitUntil: "domcontentloaded" });

  const startRes = await page.request.post("/auth/register/start", {
    timeout: 30_000,
    data: { username, display_name: username },
  });
  expect(startRes.ok(), `register/start failed: ${startRes.status()}`).toBeTruthy();
  const startJson = JSON.parse(await startRes.text()) as { publicKey: unknown };

  const credentialJson = await page.evaluate(async (publicKeyOptions) => {
    const b64uToBytes = (b64u: string): Uint8Array => {
      const pad = b64u.length % 4 === 0 ? "" : "=".repeat(4 - (b64u.length % 4));
      const b64 = (b64u + pad).replace(/-/g, "+").replace(/_/g, "/");
      const binary = atob(b64);
      return Uint8Array.from(binary, (ch) => ch.charCodeAt(0));
    };
    const bytesToB64u = (buf: ArrayBuffer): string => {
      const bytes = new Uint8Array(buf);
      let binary = "";
      for (const b of bytes) binary += String.fromCharCode(b);
      return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=/g, "");
    };
    const decodeOpts = (opts: any): PublicKeyCredentialCreationOptions => {
      const out: any = { ...opts };
      if (out.challenge) out.challenge = b64uToBytes(out.challenge);
      if (out.user?.id) out.user = { ...out.user, id: b64uToBytes(out.user.id) };
      if (Array.isArray(out.excludeCredentials))
        out.excludeCredentials = out.excludeCredentials.map((i: any) => ({
          ...i, id: b64uToBytes(i.id),
        }));
      return out;
    };
    const cred = (await navigator.credentials.create({
      publicKey: decodeOpts(publicKeyOptions),
    })) as PublicKeyCredential | null;
    if (!cred) throw new Error("credentials.create returned null");
    const response = cred.response as AuthenticatorAttestationResponse;
    return {
      id: cred.id,
      rawId: bytesToB64u(cred.rawId),
      type: cred.type,
      response: {
        attestationObject: bytesToB64u(response.attestationObject),
        clientDataJSON: bytesToB64u(response.clientDataJSON),
      },
      clientExtensionResults: cred.getClientExtensionResults?.() ?? {},
      transports: response.getTransports?.() ?? [],
    };
  }, startJson.publicKey);

  const finishRes = await page.request.post("/auth/register/finish", {
    timeout: 30_000,
    data: credentialJson,
  });
  expect(finishRes.ok(), `register/finish failed: ${finishRes.status()}`).toBeTruthy();

  await expect.poll(async () => (await authMe(page)).authenticated, {
    timeout: 60_000,
  }).toBe(true);

  const me = await authMe(page);
  return me.user_id!;
}

async function setMachineClass(page: Page, cls: string): Promise<void> {
  await page.request.put("/profile/machine-class", {
    data: { class_name: cls },
    headers: { "Content-Type": "application/json" },
    timeout: 10_000,
  });
}

async function waitForSandboxRunning(page: Page, userId: string, timeoutMs: number): Promise<void> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const res = await page.request.get("/admin/sandboxes", { timeout: 15_000 });
    if (res.ok()) {
      const snapshots = (await res.json()) as Array<{
        user_id: string; role: string; status: string;
      }>;
      if (snapshots.some((s) => s.user_id === userId && s.role === "live" && s.status === "running")) {
        return;
      }
    }
    await page.waitForTimeout(1_000);
  }
  throw new Error(`sandbox did not become running within ${timeoutMs}ms`);
}

async function waitForPromptBar(page: Page, timeoutMs: number): Promise<Locator> {
  const promptInput = page.locator("input.prompt-input[placeholder*='Ask anything']").first();
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    await page.goto("/", { waitUntil: "domcontentloaded" });
    if (await promptInput.isVisible().catch(() => false)) {
      return promptInput;
    }
    const bodyText = (await page.locator("body").innerText().catch(() => "")).toLowerCase();
    if (bodyText.includes("sandbox unreachable")) {
      await page.waitForTimeout(2_000);
      continue;
    }
    await page.waitForTimeout(2_000);
  }
  await expect(promptInput).toBeVisible({ timeout: 10_000 });
  return promptInput;
}

function writerWindow(page: Page): Locator {
  return page.getByRole("dialog", { name: /Writer/i }).last();
}

function writerProseBody(page: Page): Locator {
  return page.locator(".writer-prose-body").first();
}

async function closeAllDesktopWindows(request: APIRequestContext): Promise<void> {
  const resp = await request.get(`/desktop/${DESKTOP_ID}/windows`, { timeout: 15_000 });
  if (!resp.ok()) return;
  const json = (await resp.json()) as { windows?: Array<{ id: string }> };
  for (const win of json.windows ?? []) {
    await request.delete(`/desktop/${DESKTOP_ID}/windows/${win.id}`, { timeout: 15_000 });
  }
}

interface WorkloadResult {
  name: string;
  durationMs: number;
  writerOpened: boolean;
  documentText: string;
  screenshot: string;
}

/**
 * Send a prompt through the prompt bar and wait for the writer to produce
 * output. Returns when the writer's living document stops updating (content
 * stable for 10s) or the timeout fires.
 */
async function runPrompt(
  page: Page,
  promptInput: Locator,
  objective: string,
  name: string,
  timeoutMs: number
): Promise<WorkloadResult> {
  const t0 = Date.now();
  const screenshotPath = `../artifacts/playwright/worker-${name}-${Date.now()}.png`;

  // Close existing windows so we get a clean writer
  await closeAllDesktopWindows(page.request);
  await page.waitForTimeout(500);

  // Type the prompt
  await promptInput.fill(objective);
  await promptInput.press("Enter");

  // Wait for writer window OR toast/error
  const writer = writerWindow(page);
  const toastIndicator = page.locator(".conductor-toast-indicator").first();
  const errorIndicator = page.locator(".conductor-error-indicator").first();

  let writerOpened = false;
  await expect.poll(
    async () => {
      if (await errorIndicator.isVisible().catch(() => false)) return "error";
      if (await writer.isVisible().catch(() => false)) { writerOpened = true; return "writer"; }
      if (await toastIndicator.isVisible().catch(() => false)) return "toast";
      return "pending";
    },
    { timeout: Math.min(timeoutMs, 120_000), intervals: [500, 1000, 2000] }
  ).not.toBe("pending");

  if (!writerOpened) {
    // Toast response (simple answer, no terminal work needed)
    await page.screenshot({ path: screenshotPath, fullPage: true });
    return {
      name,
      durationMs: Date.now() - t0,
      writerOpened: false,
      documentText: "",
      screenshot: screenshotPath,
    };
  }

  // Writer opened — wait for the living document to have real content,
  // then wait for it to stabilize. The writer flow is:
  //   1. Blank/perfunctory update (rewriting to reiterate topic + plan)
  //   2. Rewrite with initial results from terminal agent
  //   3. Keep rewriting as more results come in
  //   4. Yield with stable version
  // Known bug: content can go blank mid-flow. We wait for substantial
  // content (>50 chars) then stability (no change for 15s).
  const prose = writerProseBody(page);
  let lastText = "";
  let stableCount = 0;
  const stabilityThreshold = 5; // 5 × 3s = 15s of no changes
  const minContentLen = 50; // don't consider doc "done" if nearly empty
  let peakLen = 0;

  // Take periodic screenshots to capture the living document in motion
  let screenshotIndex = 0;
  const progressScreenshotInterval = 30_000; // every 30s
  let lastScreenshot = Date.now();

  while (Date.now() - t0 < timeoutMs) {
    const currentText = await prose.innerText().catch(() => "");
    peakLen = Math.max(peakLen, currentText.length);

    // Progress screenshot
    if (Date.now() - lastScreenshot > progressScreenshotInterval) {
      screenshotIndex++;
      const progressPath = screenshotPath.replace(".png", `-progress-${screenshotIndex}.png`);
      await page.screenshot({ path: progressPath, fullPage: true }).catch(() => {});
      lastScreenshot = Date.now();
      log(name, `progress-${screenshotIndex}`, currentText.length, "chars");
    }

    // Only start stability counting once we have substantial content
    if (currentText.length >= minContentLen && currentText === lastText) {
      stableCount++;
      if (stableCount >= stabilityThreshold) break;
    } else {
      stableCount = 0;
    }
    lastText = currentText;

    // Also check the writer status badge — if it says "Done" and we have
    // content, we can stop waiting
    const statusBadge = page.locator(".writer-status-badge, [class*='Done']").first();
    const badgeText = await statusBadge.innerText().catch(() => "");
    if (badgeText.includes("Done") && currentText.length >= minContentLen) {
      log(name, "early-exit", "status=Done with content");
      break;
    }

    await page.waitForTimeout(3_000);
  }

  await page.screenshot({ path: screenshotPath, fullPage: true });
  log(name, "final-doc-length", lastText.length, "chars");
  log(name, "peak-doc-length", peakLen, "chars");

  return {
    name,
    durationMs: Date.now() - t0,
    writerOpened: true,
    documentText: lastText.slice(0, 2000),
    screenshot: screenshotPath,
  };
}

// ── Test ─────────────────────────────────────────────────────────────────────

test.describe("Worker Workload", () => {
  // These are long-running: compilation, cloning, etc.
  test.setTimeout(900_000); // 15 min total

  test("real dev prompts on worker VM via prompt bar", async ({ browser }) => {
    const ctx = await browser.newContext();
    const page = await ctx.newPage();

    log("config", "worker-class", WORKER_CLASS);
    log("config", "prompt-timeout", PROMPT_TIMEOUT_MS / 1000, "s");

    // ── Register + set worker class + boot ──

    const userId = await registerViaWebAuthn(page);
    log("setup", "user-id", userId);

    await setMachineClass(page, WORKER_CLASS);
    log("setup", "machine-class-set", WORKER_CLASS);

    // Start the sandbox and wait for it to be running
    const bootStart = Date.now();
    const startRes = await page.request.post(`/admin/sandboxes/${userId}/live/start`, {
      timeout: 120_000,
    });
    expect(startRes.ok(), `start failed: ${startRes.status()}`).toBeTruthy();

    await waitForSandboxRunning(page, userId, 60_000);
    const bootMs = Date.now() - bootStart;
    log("setup", "boot-time", bootMs, "ms");

    // Set pointer so proxy routes to the live sandbox
    await page.request.post(`/admin/sandboxes/${userId}/pointers/set`, {
      timeout: 30_000,
      data: { pointer_name: "main", target_kind: "role", target_value: "live" },
    });

    // Wait for prompt bar to appear
    const promptInput = await waitForPromptBar(page, 180_000);
    log("setup", "prompt-bar-visible", "yes");

    // ── Workload 1: Check environment ──

    log("workload-1", "description", "check available toolchains");
    const envResult = await runPrompt(
      page, promptInput,
      "In the terminal, check what dev tools are installed: run go version, rustc --version, cargo --version, gcc --version, node --version, git --version, which htop btop, free -m, nproc. Show me all the output.",
      "env-check",
      PROMPT_TIMEOUT_MS
    );
    log("workload-1", "duration", envResult.durationMs, "ms");
    log("workload-1", "writer-opened", envResult.writerOpened ? "yes" : "no");
    log("workload-1", "doc-length", envResult.documentText.length, "chars");

    // ── Workload 2: Clone + compile Go project ──

    log("workload-2", "description", "clone and compile fzf (Go)");
    const goResult = await runPrompt(
      page, promptInput,
      "In the terminal: cd /opt/choiros/data/sandbox/workspace && git clone --depth 1 https://github.com/junegunn/fzf.git && cd fzf && go build -o fzf-binary . && ls -lh fzf-binary. Show me the output — I want to see the binary size.",
      "go-compile",
      PROMPT_TIMEOUT_MS
    );
    log("workload-2", "duration", goResult.durationMs, "ms");
    log("workload-2", "writer-opened", goResult.writerOpened ? "yes" : "no");
    log("workload-2", "doc-length", goResult.documentText.length, "chars");

    // ── Workload 3: Write + compile Rust project ──

    log("workload-3", "description", "create and compile Rust calculator");
    const rustResult = await runPrompt(
      page, promptInput,
      "Create a Rust project at /opt/choiros/data/sandbox/workspace/rust-calc: cargo init rust-calc, then write a calculator in src/main.rs that takes 3 args (num1 op num2) and supports add/subtract/multiply/divide. Build it with cargo build --release, then run ./target/release/rust-calc 6 multiply 7 and show the output.",
      "rust-compile",
      PROMPT_TIMEOUT_MS
    );
    log("workload-3", "duration", rustResult.durationMs, "ms");
    log("workload-3", "writer-opened", rustResult.writerOpened ? "yes" : "no");
    log("workload-3", "doc-length", rustResult.documentText.length, "chars");

    // ── Summary ──

    const results = [envResult, goResult, rustResult];
    const totalMs = results.reduce((s, r) => s + r.durationMs, 0);

    console.log("");
    console.log("╔══════════════════════════════════════════════════════════════════════════════╗");
    console.log(`║  WORKER WORKLOAD: ${WORKER_CLASS.padEnd(56)} ║`);
    console.log("╠══════════════════════════════════════════════════════════════════════════════╣");
    for (const r of results) {
      const dur = `${(r.durationMs / 1000).toFixed(1)}s`;
      const writer = r.writerOpened ? "writer" : "toast";
      console.log(`║  ${r.name.padEnd(20)} ${dur.padStart(8)}   ${writer.padEnd(8)} ${(r.documentText.length + " chars").padStart(12)}   ║`);
    }
    console.log("╠══════════════════════════════════════════════════════════════════════════════╣");
    console.log(`║  boot-time         ${(bootMs / 1000).toFixed(1).padStart(8)}s                                                ║`);
    console.log(`║  total-prompt-time ${(totalMs / 1000).toFixed(1).padStart(8)}s                                                ║`);
    console.log("╚══════════════════════════════════════════════════════════════════════════════╝");

    // ── Cleanup ──

    try {
      await page.request.post(`/admin/sandboxes/${userId}/live/stop`, { timeout: 10_000 });
    } catch { /* best-effort */ }

    await ctx.close();

    // At least one prompt should have opened the writer
    expect(results.some((r) => r.writerOpened)).toBe(true);
  });
});
