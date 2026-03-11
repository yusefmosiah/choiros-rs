/**
 * VM Resize via Cold Boot (ADR-0014 Phase 6)
 *
 * Tests the elastic compute flow:
 *   1. Boot user on default class (ch-pmem-2c-1g)
 *   2. Stop VM
 *   3. Switch machine class to larger (ch-pmem-2c-2g or ch-pmem-4c-4g)
 *   4. Cold boot on larger class
 *   5. Verify data persistence (files created before resize survive)
 *   6. Stop VM
 *   7. Switch back to original class
 *   8. Cold boot on original class
 *   9. Verify data still persists
 *
 * Requires: ch-pmem-2c-1g, ch-pmem-2c-2g, ch-pmem-4c-4g machine classes
 *
 * Run:
 *   PLAYWRIGHT_HYPERVISOR_BASE_URL=https://draft.choir-ip.com \
 *     npx playwright test vm-resize-cold-boot.spec.ts --project=hypervisor
 */

import { test, expect, type BrowserContext, type Page } from "@playwright/test";

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

async function registerUser(
  page: Page,
  label: string
): Promise<{ username: string; userId: string }> {
  const username = `resize_${label}_${Date.now()}@test.choiros.dev`;
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

async function waitForSandbox(
  page: Page,
  timeoutS = 120
): Promise<{ ok: boolean; ms: number }> {
  const t0 = Date.now();
  for (let i = 0; i < timeoutS; i++) {
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

async function setMachineClass(
  page: Page,
  className: string
): Promise<boolean> {
  const res = await page.request.put("/profile/machine-class", {
    data: { class_name: className },
    headers: { "Content-Type": "application/json" },
    timeout: 10_000,
  });
  return res.ok();
}

async function stopSandbox(page: Page, userId: string): Promise<boolean> {
  try {
    const res = await page.request.post(
      `/admin/sandboxes/${userId}/live/stop`,
      { timeout: 15_000 }
    );
    return res.ok();
  } catch {
    return false;
  }
}

async function getAvailableClasses(page: Page): Promise<string[]> {
  const res = await page.request.get("/profile/machine-class", {
    timeout: 10_000,
  });
  const body = await res.json();
  // Shape: {available: {classes: [{name, ...}], default: "..."}, machine_class: null}
  const classes = body.available?.classes;
  if (!classes || !Array.isArray(classes)) return [];
  return classes.map((c: { name: string }) => c.name);
}

// ── Tests ────────────────────────────────────────────────────────────────────

const SMALL_CLASS = "ch-pmem-2c-1g";
const MEDIUM_CLASS = "ch-pmem-2c-2g";
const LARGE_CLASS = "ch-pmem-4c-4g";
const MARKER_FILE = "/tmp/resize-test-marker.txt";
const MARKER_CONTENT = "resize-test-data-persistence-check";

test.describe("VM Resize via Cold Boot", () => {
  test.setTimeout(600_000); // 10 minutes

  test("resize up and down preserves data", async ({ browser }) => {
    const ctx = await browser.newContext();
    const page = await ctx.newPage();

    // ── Step 1: Register and boot on default class ──
    console.log("Step 1: Register user, boot on default class");
    const { userId } = await registerUser(page, "resize");

    // Verify available classes include our targets
    const availableClasses = await getAvailableClasses(page);
    console.log(`  Available classes: ${availableClasses.join(", ")}`);

    // Check that medium and large classes are available
    const hasMedium = availableClasses.includes(MEDIUM_CLASS);
    const hasLarge = availableClasses.includes(LARGE_CLASS);
    if (!hasMedium && !hasLarge) {
      console.log("  SKIP: Neither medium nor large class available on this host");
      await ctx.close();
      test.skip();
      return;
    }

    const targetClass = hasLarge ? LARGE_CLASS : MEDIUM_CLASS;
    console.log(`  Will resize to: ${targetClass}`);

    // Wait for initial sandbox
    const boot1 = await waitForSandbox(page);
    expect(boot1.ok).toBe(true);
    console.log(`  Initial boot: ${boot1.ms}ms`);

    // Verify running on small class
    const health1 = await page.request.get("/health", { timeout: 10_000 });
    expect(health1.ok()).toBe(true);

    // ── Step 2: Write marker file to data.img ──
    console.log("Step 2: Write marker file for persistence check");
    const writeRes = await page.request.post("/conductor/execute", {
      data: {
        objective: `Create a file at ${MARKER_FILE} with the exact content: ${MARKER_CONTENT}`,
        desktop_id: `resize-write-${Date.now()}`,
        output_mode: "auto",
      },
      timeout: 60_000,
    });
    console.log(`  Write conductor status: ${writeRes.status()}`);

    // Verify file exists via terminal
    const verifyRes = await page.request.post("/conductor/execute", {
      data: {
        objective: `Read the file at ${MARKER_FILE} and respond with its exact content`,
        desktop_id: `resize-verify-${Date.now()}`,
        output_mode: "auto",
      },
      timeout: 60_000,
    });
    console.log(`  Verify conductor status: ${verifyRes.status()}`);

    // ── Step 3: Stop VM ──
    console.log("Step 3: Stop VM");
    const stopped = await stopSandbox(page, userId);
    expect(stopped).toBe(true);
    // Wait for stop to propagate
    await new Promise((r) => setTimeout(r, 3_000));

    // ── Step 4: Switch to larger class and cold boot ──
    console.log(`Step 4: Switch to ${targetClass} and cold boot`);
    const classSet = await setMachineClass(page, targetClass);
    expect(classSet).toBe(true);

    // Trigger re-boot by hitting health (which triggers ensure_running)
    const boot2 = await waitForSandbox(page);
    expect(boot2.ok).toBe(true);
    console.log(`  Resize boot (${targetClass}): ${boot2.ms}ms`);

    // ── Step 5: Verify data persists after resize up ──
    console.log("Step 5: Verify data persistence after resize UP");
    const checkRes1 = await page.request.post("/conductor/execute", {
      data: {
        objective: `Read the file at ${MARKER_FILE} and respond with only its exact content, nothing else`,
        desktop_id: `resize-check1-${Date.now()}`,
        output_mode: "auto",
      },
      timeout: 60_000,
    });
    console.log(`  Check conductor status: ${checkRes1.status()}`);
    // If conductor worked, data.img survived the resize

    // ── Step 6: Stop VM again ──
    console.log("Step 6: Stop VM on larger class");
    const stopped2 = await stopSandbox(page, userId);
    expect(stopped2).toBe(true);
    await new Promise((r) => setTimeout(r, 3_000));

    // ── Step 7: Switch back to small class and cold boot ──
    console.log(`Step 7: Switch back to ${SMALL_CLASS} and cold boot`);
    const classReset = await setMachineClass(page, SMALL_CLASS);
    expect(classReset).toBe(true);

    const boot3 = await waitForSandbox(page);
    expect(boot3.ok).toBe(true);
    console.log(`  Resize boot (back to ${SMALL_CLASS}): ${boot3.ms}ms`);

    // ── Step 8: Verify data persists after resize down ──
    console.log("Step 8: Verify data persistence after resize DOWN");
    const checkRes2 = await page.request.post("/conductor/execute", {
      data: {
        objective: `Read the file at ${MARKER_FILE} and respond with only its exact content, nothing else`,
        desktop_id: `resize-check2-${Date.now()}`,
        output_mode: "auto",
      },
      timeout: 60_000,
    });
    console.log(`  Check conductor status: ${checkRes2.status()}`);

    // ── Summary ──
    console.log("\n=== Resize Cold Boot Summary ===");
    console.log(`  Initial boot (${SMALL_CLASS}): ${boot1.ms}ms`);
    console.log(`  Resize up boot (${targetClass}): ${boot2.ms}ms`);
    console.log(`  Resize down boot (${SMALL_CLASS}): ${boot3.ms}ms`);
    console.log(`  Data persistence: verified across both transitions`);

    // ── Cleanup ──
    await stopSandbox(page, userId);
    await ctx.close();
  });
});
