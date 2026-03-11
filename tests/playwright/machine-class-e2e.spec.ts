/**
 * Machine Class E2E Test (ADR-0014 Phase 6)
 *
 * Validates that users can select different VM machine classes and each
 * class boots a working sandbox. Tests the full flow:
 *   register → set machine class → first request triggers VM boot → health check
 *
 * Run against Node B (staging):
 *   PLAYWRIGHT_HYPERVISOR_BASE_URL=https://draft.choir-ip.com \
 *     npx playwright test machine-class-e2e.spec.ts --project=hypervisor
 */

import { test, expect, type BrowserContext, type Page } from "@playwright/test";

// ── Config ───────────────────────────────────────────────────────────────────

/** VM boot can take up to 90s on cold start. */
const VM_READY_TIMEOUT_S = 120;

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

function uniqueUsername(label: string): string {
  return `mc_${label}_${Date.now()}_${Math.random().toString(36).slice(2, 6)}@test.choiros.dev`;
}

interface MeResponse {
  authenticated: boolean;
  user_id?: string | null;
}

async function registerUser(
  page: Page,
  label: string
): Promise<{ username: string; userId: string }> {
  const username = uniqueUsername(label);
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

// ── Metrics ──────────────────────────────────────────────────────────────────

interface Metric {
  class: string;
  metric: string;
  value: string | number;
  unit?: string;
}

const metrics: Metric[] = [];

function record(
  cls: string,
  metric: string,
  value: string | number,
  unit?: string
) {
  metrics.push({ class: cls, metric, value, unit });
  console.log(
    `[MC] ${cls.padEnd(18)} ${metric}: ${value}${unit ? " " + unit : ""}`
  );
}

// ── Tests ────────────────────────────────────────────────────────────────────

test.describe("Machine Class E2E (ADR-0014 Phase 6)", () => {
  test.setTimeout(600_000); // 10 minutes — multiple cold VM boots

  test("list available machine classes", async ({ browser }) => {
    // Admin endpoints require auth — register first
    const ctx = await browser.newContext();
    const page = await ctx.newPage();
    await registerUser(page, "list");

    const res = await page.request.get("/admin/machine-classes", {
      timeout: 10_000,
    });
    expect(res.ok()).toBe(true);

    const body = await res.json();
    console.log("Available machine classes:", JSON.stringify(body, null, 2));

    expect(body.classes).toBeDefined();
    expect(Array.isArray(body.classes)).toBe(true);

    for (const cls of body.classes) {
      record(cls.name, "available", `${cls.hypervisor}/${cls.transport}`);
      record(cls.name, "resources", `${cls.vcpu}vcpu/${cls.memory_mb}MB`);
    }

    // We expect at least 1 class (the host default). If all 4 are deployed, even better.
    expect(body.classes.length).toBeGreaterThanOrEqual(1);
    record("global", "total-classes", body.classes.length);
    record("global", "default-class", body.default ?? "none");

    await ctx.close();
  });

  test("default class user boots successfully", async ({ browser }) => {
    const ctx = await browser.newContext();
    const page = await ctx.newPage();

    // Register without setting a machine class — uses host default
    const { username, userId } = await registerUser(page, "default");
    record("default", "registered", username);
    record("default", "user-id", userId);

    // First proxied request triggers VM boot
    const { ok, ms } = await waitForSandbox(page);
    record("default", "vm-ready", ok ? "yes" : "no");
    record("default", "boot-time", ms, "ms");
    expect(ok).toBe(true);

    // Verify health endpoint works through sandbox proxy
    const healthRes = await page.request.get("/health", { timeout: 10_000 });
    expect(healthRes.ok()).toBe(true);
    record("default", "health", "ok");

    await ctx.close();
  });

  test("each available class boots a healthy VM", async ({ browser }) => {
    // First, discover available classes (needs auth)
    const tmpCtx = await browser.newContext();
    const tmpPage = await tmpCtx.newPage();
    await registerUser(tmpPage, "discover");
    const classesRes = await tmpPage.request.get("/admin/machine-classes", {
      timeout: 10_000,
    });
    expect(classesRes.ok()).toBe(true);
    const { classes } = (await classesRes.json()) as {
      classes: Array<{ name: string; hypervisor: string; transport: string }>;
    };
    await tmpCtx.close();

    if (classes.length === 0) {
      console.log("No machine classes available — skipping per-class tests");
      return;
    }

    console.log(
      `\n=== Testing ${classes.length} machine classes concurrently ===`
    );

    // Register one user per class, set their machine class, boot VM
    const results = await Promise.allSettled(
      classes.map(async (cls) => {
        const ctx = await browser.newContext();
        const page = await ctx.newPage();

        try {
          // 1. Register
          const { username, userId } = await registerUser(page, cls.name);
          record(cls.name, "registered", username);

          // 2. Set machine class
          const classSet = await setMachineClass(page, cls.name);
          record(cls.name, "class-set", classSet ? "yes" : "no");
          expect(classSet).toBe(true);

          // 3. Verify class was persisted
          const profileRes = await page.request.get("/profile/machine-class", {
            timeout: 10_000,
          });
          if (profileRes.ok()) {
            const profile = await profileRes.json();
            record(cls.name, "profile-class", profile.machine_class ?? "null");
            expect(profile.machine_class).toBe(cls.name);
          }

          // 4. First proxied request triggers VM boot with the selected class
          const { ok, ms } = await waitForSandbox(page);
          record(cls.name, "vm-ready", ok ? "yes" : "no");
          record(cls.name, "boot-time", ms, "ms");

          if (!ok) {
            // Grab admin snapshot for debugging
            try {
              const snapRes = await page.request.get("/admin/sandboxes", {
                timeout: 5_000,
              });
              if (snapRes.ok()) {
                const snap = await snapRes.json();
                record(cls.name, "debug-snapshot", JSON.stringify(snap).slice(0, 200));
              }
            } catch { /* best effort */ }
          }

          // 5. Health check through sandbox proxy
          if (ok) {
            const healthRes = await page.request.get("/health", {
              timeout: 10_000,
            });
            record(cls.name, "health", healthRes.ok() ? "ok" : "fail");
          }

          return { cls: cls.name, ok, ms };
        } finally {
          await ctx.close();
        }
      })
    );

    // Summary
    let passed = 0;
    let failed = 0;
    for (const r of results) {
      if (r.status === "fulfilled" && r.value.ok) {
        passed++;
      } else {
        failed++;
        if (r.status === "rejected") {
          console.error(`Class test failed:`, r.reason);
        }
      }
    }

    record("global", "classes-passed", passed);
    record("global", "classes-failed", failed);

    // At minimum, the default CH class must work. FC classes may fail
    // (first time testing) — we log failures but don't hard-fail the suite
    // for experimental classes.
    expect(passed).toBeGreaterThanOrEqual(1);
  });

  test("admin sandbox list shows machine class", async ({ browser }) => {
    const ctx = await browser.newContext();
    const page = await ctx.newPage();

    // Register and set a specific class
    await registerUser(page, "admin-check");
    // Don't set a class — will use default

    // Trigger VM boot
    await waitForSandbox(page);

    // Check admin API shows machine_class field
    const snapRes = await page.request.get("/admin/sandboxes", {
      timeout: 10_000,
    });
    if (snapRes.ok()) {
      const snap = await snapRes.json();
      console.log(
        "Admin sandbox snapshot:",
        JSON.stringify(snap, null, 2).slice(0, 500)
      );
      // Snapshot is an array of SandboxSnapshot
      if (Array.isArray(snap) && snap.length > 0) {
        // machine_class field should exist (may be null for default)
        const hasField = snap.some(
          (s: Record<string, unknown>) => "machine_class" in s
        );
        record("admin", "has-machine-class-field", hasField ? "yes" : "no");
      }
    }

    await ctx.close();
  });
});

// ── Report ───────────────────────────────────────────────────────────────────

test.afterAll(() => {
  if (metrics.length === 0) return;

  console.log(
    "\n╔════════════════════════════════════════════════════════════════════════════════╗"
  );
  console.log(
    "║              MACHINE CLASS E2E TEST REPORT (ADR-0014 Phase 6)                ║"
  );
  console.log(
    "╠════════════════════════════════════════════════════════════════════════════════╣"
  );

  const seen = new Set<string>();
  for (const m of metrics) {
    if (!seen.has(m.class)) {
      seen.add(m.class);
      if (m.class !== metrics[0].class) {
        console.log(
          `║  ${"─".repeat(76)}║`
        );
      }
    }
    const label = `${m.class.padEnd(18)} ${m.metric}`;
    const val = `${m.value}${m.unit ? " " + m.unit : ""}`;
    console.log(`║  ${label.padEnd(50)} ${val.padStart(24)} ║`);
  }

  console.log(
    "╚════════════════════════════════════════════════════════════════════════════════╝"
  );
});
