/**
 * Interactive worker session — run step by step, screenshot at each point.
 * Usage: npx playwright test worker-interactive.ts --project=hypervisor --headed
 */
import { test, expect, type BrowserContext, type Page } from "@playwright/test";

const WORKER_CLASS = process.env.WORKER_CLASS ?? "w-ch-pmem-4c-4g";
const SHOT = "../artifacts/playwright";

async function shot(page: Page, name: string) {
  const path = `${SHOT}/interactive-${name}.png`;
  await page.screenshot({ path, fullPage: true });
  console.log(`[SHOT] ${path}`);
}

async function registerViaWebAuthn(page: Page): Promise<string> {
  const cdp = await (page.context() as BrowserContext).newCDPSession(page);
  await cdp.send("WebAuthn.enable", { enableUI: false });
  await cdp.send("WebAuthn.addVirtualAuthenticator", {
    options: {
      protocol: "ctap2", transport: "internal",
      hasResidentKey: true, hasUserVerification: true, isUserVerified: true,
    },
  });

  const username = `worker_i_${Date.now()}@test.choiros.dev`;
  await page.goto("/register", { waitUntil: "domcontentloaded" });

  const startRes = await page.request.post("/auth/register/start", {
    timeout: 30_000,
    data: { username, display_name: username },
  });
  expect(startRes.ok()).toBeTruthy();
  const startJson = JSON.parse(await startRes.text()) as { publicKey: unknown };

  const credentialJson = await page.evaluate(async (publicKeyOptions) => {
    const b64uToBytes = (b64u: string): Uint8Array => {
      const pad = b64u.length % 4 === 0 ? "" : "=".repeat(4 - (b64u.length % 4));
      const b64 = (b64u + pad).replace(/-/g, "+").replace(/_/g, "/");
      return Uint8Array.from(atob(b64), (ch) => ch.charCodeAt(0));
    };
    const bytesToB64u = (buf: ArrayBuffer): string => {
      let binary = "";
      for (const b of new Uint8Array(buf)) binary += String.fromCharCode(b);
      return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=/g, "");
    };
    const decodeOpts = (opts: any) => {
      const out: any = { ...opts };
      if (out.challenge) out.challenge = b64uToBytes(out.challenge);
      if (out.user?.id) out.user = { ...out.user, id: b64uToBytes(out.user.id) };
      if (Array.isArray(out.excludeCredentials))
        out.excludeCredentials = out.excludeCredentials.map((i: any) => ({ ...i, id: b64uToBytes(i.id) }));
      return out;
    };
    const cred = (await navigator.credentials.create({ publicKey: decodeOpts(publicKeyOptions) })) as PublicKeyCredential | null;
    if (!cred) throw new Error("null");
    const response = cred.response as AuthenticatorAttestationResponse;
    return {
      id: cred.id, rawId: bytesToB64u(cred.rawId), type: cred.type,
      response: { attestationObject: bytesToB64u(response.attestationObject), clientDataJSON: bytesToB64u(response.clientDataJSON) },
      clientExtensionResults: cred.getClientExtensionResults?.() ?? {},
      transports: response.getTransports?.() ?? [],
    };
  }, startJson.publicKey);

  const finishRes = await page.request.post("/auth/register/finish", { timeout: 30_000, data: credentialJson });
  expect(finishRes.ok()).toBeTruthy();
  await expect.poll(async () => {
    const r = await page.request.get("/auth/me");
    return r.ok() ? ((await r.json()) as any).authenticated : false;
  }, { timeout: 60_000 }).toBe(true);

  const me = (await (await page.request.get("/auth/me")).json()) as any;
  return me.user_id!;
}

test("interactive worker session", async ({ browser }) => {
  test.setTimeout(900_000);
  const ctx = await browser.newContext();
  const page = await ctx.newPage();

  // ── Step 1: Register ──
  console.log("[STEP 1] Registering...");
  const userId = await registerViaWebAuthn(page);
  console.log(`[STEP 1] user_id=${userId}`);

  // ── Step 2: Set worker class + boot ──
  console.log(`[STEP 2] Setting class=${WORKER_CLASS}, booting...`);
  await page.request.put("/profile/machine-class", {
    data: { class_name: WORKER_CLASS },
    headers: { "Content-Type": "application/json" },
  });
  const startRes = await page.request.post(`/admin/sandboxes/${userId}/live/start`, { timeout: 120_000 });
  expect(startRes.ok()).toBeTruthy();
  await page.request.post(`/admin/sandboxes/${userId}/pointers/set`, {
    timeout: 30_000,
    data: { pointer_name: "main", target_kind: "role", target_value: "live" },
  });

  // Wait for health
  for (let i = 0; i < 60; i++) {
    try {
      const h = await page.request.get("/health", { timeout: 3_000 });
      if (h.ok()) { console.log(`[STEP 2] Healthy after ${i}s`); break; }
    } catch { /* poll */ }
    await page.waitForTimeout(1_000);
  }

  // ── Step 3: Navigate to app ──
  console.log("[STEP 3] Loading app...");
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.waitForTimeout(3_000);
  await shot(page, "01-app-loaded");

  // ── Step 4: Wait for prompt bar ──
  const promptInput = page.locator("input.prompt-input[placeholder*='Ask anything']").first();
  for (let i = 0; i < 60; i++) {
    if (await promptInput.isVisible().catch(() => false)) break;
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await page.waitForTimeout(2_000);
  }
  await shot(page, "02-prompt-bar-ready");

  // ── Step 5: Send prompt — check toolchains ──
  console.log("[STEP 5] Sending env check prompt...");
  await promptInput.fill("Check what dev tools we have: run go version, rustc --version, node --version, gcc --version, free -m, nproc in the terminal. Show me the output.");
  await promptInput.press("Enter");
  console.log("[STEP 5] Prompt sent, waiting...");

  // Wait and screenshot every 10s for 3 minutes
  for (let i = 1; i <= 18; i++) {
    await page.waitForTimeout(10_000);
    await shot(page, `03-env-${String(i).padStart(2, "0")}-${i * 10}s`);
    // Log what we see
    const proseText = await page.locator(".writer-prose-body").first().innerText().catch(() => "(not visible)");
    console.log(`[STEP 5] t+${i * 10}s prose=${proseText.length} chars: ${proseText.slice(0, 120).replace(/\n/g, " ")}`);
  }

  // ── Step 6: Open trace app ──
  console.log("[STEP 6] Opening trace app...");
  const traceLauncher = page.locator("button, [role='button'], .desktop-icon").filter({ hasText: "Trace" }).first();
  if (await traceLauncher.isVisible().catch(() => false)) {
    await traceLauncher.click();
    await page.waitForTimeout(3_000);
    await shot(page, "04-trace-opened");
  } else {
    console.log("[STEP 6] Trace launcher not visible, trying API...");
    await page.request.post("/desktop/default-desktop/apps", {
      data: { id: "trace", name: "Trace", icon: "🔍", component_code: "TraceApp", default_width: 700, default_height: 450 },
    });
    await page.request.post("/desktop/default-desktop/windows", {
      data: { app_id: "trace", title: "Trace", props: {} },
    });
    await page.reload({ waitUntil: "domcontentloaded" });
    await page.waitForTimeout(3_000);
    await shot(page, "04-trace-opened");
  }

  // ── Step 7: Send Go compile prompt ──
  console.log("[STEP 7] Sending Go compile prompt...");
  // Re-find prompt input after page changes
  const prompt2 = page.locator("input.prompt-input[placeholder*='Ask anything']").first();
  await prompt2.fill("In the terminal, clone fzf (git clone --depth 1 https://github.com/junegunn/fzf.git into /opt/choiros/data/sandbox/workspace/) and build it with 'cd fzf && go build -o fzf-binary .' — show me the output and binary size.");
  await prompt2.press("Enter");

  // Wait and screenshot every 15s for 5 minutes
  for (let i = 1; i <= 20; i++) {
    await page.waitForTimeout(15_000);
    await shot(page, `05-go-${String(i).padStart(2, "0")}-${i * 15}s`);
    const proseText = await page.locator(".writer-prose-body").first().innerText().catch(() => "(not visible)");
    console.log(`[STEP 7] t+${i * 15}s prose=${proseText.length} chars: ${proseText.slice(0, 120).replace(/\n/g, " ")}`);
    // If we see substantial content that's stable, we can move on
    if (proseText.length > 200) {
      console.log("[STEP 7] Got substantial content, taking final screenshot...");
      await page.waitForTimeout(10_000);
      await shot(page, "05-go-final");
      break;
    }
  }

  // Final screenshot
  await shot(page, "06-final");

  // Cleanup
  try {
    await page.request.post(`/admin/sandboxes/${userId}/live/stop`, { timeout: 10_000 });
  } catch { /* best-effort */ }

  await ctx.close();
});
