/**
 * Multi-provider model comparison test (ADR-0028)
 *
 * Tests different LLM providers through the conductor pipeline with video recording.
 * Captures latency, tracing, and response quality per provider config.
 *
 * Usage:
 *   PLAYWRIGHT_HYPERVISOR_BASE_URL=https://draft.choir-ip.com \
 *     npx playwright test multi-provider-comparison.spec.ts --project=hypervisor --headed
 */
import { test, expect, type BrowserContext, type Page } from "@playwright/test";

const SHOT = "../artifacts/playwright";
const PROMPT = "What are 3 interesting facts about the Mandelbrot set? Keep it brief.";

interface ProviderConfig {
  name: string;
  callsite_models: Record<string, string>;
}

const CONFIGS: ProviderConfig[] = [
  {
    name: "bedrock-haiku",
    callsite_models: {
      conductor: "ClaudeBedrockHaiku45",
      writer: "ClaudeBedrockHaiku45",
      terminal: "ClaudeBedrockHaiku45",
      researcher: "ClaudeBedrockHaiku45",
    },
  },
  {
    name: "inception-mercury",
    callsite_models: {
      conductor: "InceptionMercury2",
      writer: "InceptionMercury2",
      terminal: "InceptionMercury2",
      researcher: "InceptionMercury2",
    },
  },
  {
    name: "openrouter-nemotron",
    callsite_models: {
      conductor: "OpenRouterNemotron",
      writer: "OpenRouterNemotron",
      terminal: "OpenRouterNemotron",
      researcher: "OpenRouterNemotron",
    },
  },
  {
    name: "openrouter-hunter",
    callsite_models: {
      conductor: "OpenRouterHunterAlpha",
      writer: "OpenRouterHunterAlpha",
      terminal: "OpenRouterHunterAlpha",
      researcher: "OpenRouterHunterAlpha",
    },
  },
  {
    name: "openrouter-healer",
    callsite_models: {
      conductor: "OpenRouterHealerAlpha",
      writer: "OpenRouterHealerAlpha",
      terminal: "OpenRouterHealerAlpha",
      researcher: "OpenRouterHealerAlpha",
    },
  },
  {
    name: "mixed-optimal",
    callsite_models: {
      conductor: "InceptionMercury2",
      writer: "OpenRouterNemotron",
      terminal: "OpenRouterHunterAlpha",
      researcher: "OpenRouterHealerAlpha",
    },
  },
];

async function shot(page: Page, name: string) {
  const path = `${SHOT}/provider-${name}.png`;
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

  const username = `provider_test_${Date.now()}@test.choiros.dev`;
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

async function setModelConfig(page: Page, userId: string, config: ProviderConfig) {
  const res = await page.request.patch(`/user/${userId}/model-config`, {
    data: { callsite_models: config.callsite_models },
    timeout: 10_000,
  });
  // API may not be deployed yet — log but don't fail
  if (!res.ok()) {
    console.log(`[WARN] model-config API returned ${res.status()} — may not be deployed yet`);
  }
}

async function runConductorPrompt(page: Page, prompt: string): Promise<{ durationMs: number; status: string; runId?: string }> {
  const t0 = Date.now();
  const res = await page.request.post("/conductor/execute", {
    data: { prompt, output_mode: "full" },
    timeout: 120_000,
  });
  const durationMs = Date.now() - t0;

  if (!res.ok()) {
    return { durationMs, status: `error-${res.status()}` };
  }

  const body = (await res.json()) as any;
  return {
    durationMs,
    status: body.status ?? "unknown",
    runId: body.run_id,
  };
}

test("multi-provider comparison", async ({ browser }) => {
  test.setTimeout(600_000); // 10 min total

  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  const userId = await registerViaWebAuthn(page);
  console.log(`[AUTH] registered as ${userId}`);

  // Wait for sandbox to be ready
  await expect.poll(async () => {
    try {
      const r = await page.request.get("/health", { timeout: 5_000 });
      return r.ok();
    } catch { return false; }
  }, { timeout: 120_000, message: "waiting for sandbox health" }).toBe(true);

  await shot(page, "00-ready");

  const results: Array<{
    config: string;
    durationMs: number;
    status: string;
    runId?: string;
  }> = [];

  for (let i = 0; i < CONFIGS.length; i++) {
    const config = CONFIGS[i];
    console.log(`\n[CONFIG ${i + 1}/${CONFIGS.length}] ${config.name}`);
    console.log(`  Models: ${JSON.stringify(config.callsite_models)}`);

    // Set model config
    await setModelConfig(page, userId, config);

    // Run the prompt
    const result = await runConductorPrompt(page, PROMPT);
    results.push({ config: config.name, ...result });

    console.log(`  Duration: ${result.durationMs}ms`);
    console.log(`  Status: ${result.status}`);
    if (result.runId) console.log(`  Run ID: ${result.runId}`);

    // Navigate to trace view to capture in video
    if (result.runId) {
      await page.goto(`/`, { waitUntil: "domcontentloaded" });
      // Give UI time to render trace
      await page.waitForTimeout(3000);
      await shot(page, `${String(i + 1).padStart(2, "0")}-${config.name}-trace`);
    } else {
      await shot(page, `${String(i + 1).padStart(2, "0")}-${config.name}-result`);
    }

    // Brief pause between configs
    await page.waitForTimeout(1000);
  }

  // Summary
  console.log("\n═══════════════════════════════════════════");
  console.log("Multi-Provider Comparison Results");
  console.log("═══════════════════════════════════════════");
  console.log(`| Config               | Duration (ms) | Status    |`);
  console.log(`|----------------------|---------------|-----------|`);
  for (const r of results) {
    const name = r.config.padEnd(20);
    const dur = String(r.durationMs).padStart(13);
    const st = r.status.padEnd(9);
    console.log(`| ${name} | ${dur} | ${st} |`);
  }
  console.log("═══════════════════════════════════════════\n");

  await ctx.close();
});
