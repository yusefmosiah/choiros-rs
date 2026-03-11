/**
 * Capacity Stress Test v2
 *
 * Ramps VMs with proportional wave sizing, background activity from
 * existing users, and mixed boot types (cold boot + snapshot restore).
 *
 * Key differences from v1:
 *   - Wave size grows proportionally: max(10, 15% of running VMs)
 *   - Background activity runs DURING new wave boots (desynchronizes load)
 *   - Existing VMs get hibernated and restored (mixed boot types)
 *   - Registration is staggered (random 0-2s jitter per user)
 *   - Activity profiles: dormant, polling, active, bursty
 *
 * Usage:
 *   PLAYWRIGHT_HYPERVISOR_BASE_URL=https://draft.choir-ip.com \
 *     npx playwright test capacity-stress-test.spec.ts --project=stress
 *
 * Config env vars:
 *   STRESS_INITIAL_WAVE    — first wave size (default: 10)
 *   STRESS_GROWTH_RATE     — wave size as fraction of running VMs (default: 0.15)
 *   STRESS_MAX_WAVE_SIZE   — cap on wave size (default: 50)
 *   STRESS_MAX_WAVES       — max waves before stopping (default: 30)
 *   STRESS_BOOT_TIMEOUT    — per-VM boot timeout in seconds (default: 120)
 *   STRESS_FAIL_THRESHOLD  — fraction of failed boots to stop (default: 0.3)
 *   STRESS_HIBERNATE_RATE  — fraction of existing VMs to hibernate/restore per wave (default: 0.05)
 */

import { test, expect, type BrowserContext, type Page } from "@playwright/test";

// ── Config ───────────────────────────────────────────────────────────────────

const INITIAL_WAVE = parseInt(process.env.STRESS_INITIAL_WAVE ?? "10", 10);
const GROWTH_RATE = parseFloat(process.env.STRESS_GROWTH_RATE ?? "0.15");
const MAX_WAVE_SIZE = parseInt(process.env.STRESS_MAX_WAVE_SIZE ?? "50", 10);
const MAX_WAVES = parseInt(process.env.STRESS_MAX_WAVES ?? "30", 10);
const BOOT_TIMEOUT = parseInt(process.env.STRESS_BOOT_TIMEOUT ?? "120", 10);
const FAIL_THRESHOLD = parseFloat(process.env.STRESS_FAIL_THRESHOLD ?? "0.3");
const HIBERNATE_RATE = parseFloat(process.env.STRESS_HIBERNATE_RATE ?? "0.05");
const DEGRADE_BOOT_MS = 60_000;

// ── Activity profiles ────────────────────────────────────────────────────────
//
// Each profile defines what a user does in the background while new waves boot.
// Profiles are assigned randomly with weighted distribution to simulate
// realistic usage: most users are idle, some poll, few are active.

type ActivityProfile = "dormant" | "polling" | "active" | "bursty";

const PROFILE_WEIGHTS: Record<ActivityProfile, number> = {
  dormant: 50,  // 50% — logged in, browser tab in background
  polling: 25,  // 25% — tab visible, periodic heartbeat/health
  active: 15,   // 15% — actively using conductor
  bursty: 10,   // 10% — quiet then sudden burst
};

function pickProfile(): ActivityProfile {
  const total = Object.values(PROFILE_WEIGHTS).reduce((a, b) => a + b, 0);
  let r = Math.random() * total;
  for (const [profile, weight] of Object.entries(PROFILE_WEIGHTS)) {
    r -= weight;
    if (r <= 0) return profile as ActivityProfile;
  }
  return "dormant";
}

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

function uniqueUsername(wave: number, index: number): string {
  return `stress_w${wave}_u${index}_${Date.now()}_${Math.random().toString(36).slice(2, 6)}@example.com`;
}

interface MeResponse {
  authenticated: boolean;
  user_id?: string | null;
}

async function registerAndLogin(
  page: Page,
  wave: number,
  index: number
): Promise<{ username: string; userId: string; ms: number }> {
  const username = uniqueUsername(wave, index);
  // Stagger: random 0-2s delay to avoid thundering herd
  await new Promise((r) => setTimeout(r, Math.random() * 2000));
  const t0 = Date.now();
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
  return { username, userId: me.user_id!, ms: Date.now() - t0 };
}

async function waitForSandbox(
  page: Page
): Promise<{ ok: boolean; ms: number }> {
  const t0 = Date.now();
  for (let i = 0; i < BOOT_TIMEOUT; i++) {
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

function percentile(sorted: number[], p: number): number {
  if (sorted.length === 0) return -1;
  const idx = Math.ceil(sorted.length * p) - 1;
  return sorted[Math.max(0, idx)];
}

function avg(arr: number[]): number {
  if (arr.length === 0) return -1;
  return Math.round(arr.reduce((a, b) => a + b, 0) / arr.length);
}

// ── Background activity functions ────────────────────────────────────────────
//
// These run concurrently with wave boot phases to create realistic background
// load. Each returns metrics about what it did.

interface ActivityResult {
  profile: ActivityProfile;
  requests: number;
  ok: number;
  errors: number;
  durationMs: number;
}

async function activityDormant(
  _page: Page,
  durationMs: number
): Promise<ActivityResult> {
  // Do nothing — simulate an idle browser tab
  await new Promise((r) => setTimeout(r, durationMs));
  return { profile: "dormant", requests: 0, ok: 0, errors: 0, durationMs };
}

async function activityPolling(
  page: Page,
  durationMs: number
): Promise<ActivityResult> {
  // Periodic health/heartbeat at ~5s intervals (like a browser keepalive)
  const t0 = Date.now();
  let requests = 0;
  let ok = 0;
  let errors = 0;
  while (Date.now() - t0 < durationMs) {
    try {
      requests++;
      const r = await page.request.get("/health", { timeout: 10_000 });
      if (r.ok()) ok++;
      else errors++;
    } catch {
      errors++;
    }
    // 3-7s random interval (avoids synchronized polling)
    await new Promise((r) => setTimeout(r, 3000 + Math.random() * 4000));
  }
  return { profile: "polling", requests, ok, errors, durationMs: Date.now() - t0 };
}

async function activityActive(
  page: Page,
  durationMs: number
): Promise<ActivityResult> {
  // One conductor prompt + health checks
  const t0 = Date.now();
  let requests = 0;
  let ok = 0;
  let errors = 0;

  // Random delay before starting (user doesn't type instantly)
  await new Promise((r) => setTimeout(r, Math.random() * 3000));

  // Conductor prompt
  try {
    requests++;
    const r = await page.request.post("/conductor/execute", {
      data: {
        objective: "What is 7 * 8? Just the number.",
        desktop_id: `stress-active-${Date.now()}`,
        output_mode: "auto",
      },
      timeout: 120_000,
    });
    if (r.ok()) ok++;
    else errors++;
  } catch {
    errors++;
  }

  // Fill remaining time with health checks
  while (Date.now() - t0 < durationMs) {
    try {
      requests++;
      const r = await page.request.get("/health", { timeout: 10_000 });
      if (r.ok()) ok++;
      else errors++;
    } catch {
      errors++;
    }
    await new Promise((r) => setTimeout(r, 2000 + Math.random() * 3000));
  }
  return { profile: "active", requests, ok, errors, durationMs: Date.now() - t0 };
}

async function activityBursty(
  page: Page,
  durationMs: number
): Promise<ActivityResult> {
  // Quiet for 60-80% of duration, then burst of concurrent requests
  const t0 = Date.now();
  let requests = 0;
  let ok = 0;
  let errors = 0;

  const quietFraction = 0.6 + Math.random() * 0.2;
  await new Promise((r) => setTimeout(r, durationMs * quietFraction));

  // Burst: 5-10 concurrent requests
  const burstSize = 5 + Math.floor(Math.random() * 6);
  const burstResults = await Promise.allSettled(
    Array.from({ length: burstSize }, (_, i) => {
      requests++;
      if (i === 0) {
        // One conductor prompt in the burst
        return page.request.post("/conductor/execute", {
          data: {
            objective: "Say hello.",
            desktop_id: `stress-burst-${Date.now()}-${i}`,
            output_mode: "auto",
          },
          timeout: 60_000,
        });
      }
      // Rest are health/heartbeat
      return i % 2 === 0
        ? page.request.get("/health", { timeout: 10_000 })
        : page.request.post("/heartbeat", { timeout: 10_000 });
    })
  );

  for (const r of burstResults) {
    if (r.status === "fulfilled" && r.value.ok()) ok++;
    else errors++;
  }

  // Fill remaining time idle
  const remaining = durationMs - (Date.now() - t0);
  if (remaining > 0) await new Promise((r) => setTimeout(r, remaining));

  return { profile: "bursty", requests, ok, errors, durationMs: Date.now() - t0 };
}

const ACTIVITY_FNS: Record<
  ActivityProfile,
  (page: Page, durationMs: number) => Promise<ActivityResult>
> = {
  dormant: activityDormant,
  polling: activityPolling,
  active: activityActive,
  bursty: activityBursty,
};

// ── Session tracking ─────────────────────────────────────────────────────────

interface UserSession {
  wave: number;
  index: number;
  context: BrowserContext;
  page: Page;
  userId: string;
  profile: ActivityProfile;
  hibernated: boolean;
}

// ── Wave result ──────────────────────────────────────────────────────────────

interface WaveResult {
  wave: number;
  waveSize: number;
  totalVmsBefore: number;
  totalVmsAfter: number;
  registered: number;
  coldBooted: number;
  snapshotRestored: number;
  bootFailed: number;
  bootTimes: number[];
  restoreTimes: number[];
  bootP50: number;
  bootP95: number;
  bootMax: number;
  restoreP50: number;
  healthOkRate: number;
  healthAvgMs: number;
  bgActivity: Record<ActivityProfile, { count: number; okRate: number }>;
  existingHealthAvgMs: number;
  status: "ok" | "degraded" | "failing" | "crashed";
}

// ── Main Test ────────────────────────────────────────────────────────────────

test.describe("Capacity stress test", () => {
  test.setTimeout(7_200_000); // 2 hours

  test("ramp VMs until degradation", async ({ browser }) => {
    const allSessions: UserSession[] = [];
    const waveResults: WaveResult[] = [];
    let stopReason = "";

    // Get baseline VM count
    let baselineVms = 0;
    try {
      const ctx = await browser.newContext();
      const page = await ctx.newPage();
      await page.goto("/");
      const adminRes = await page.request.get("/admin/sandboxes", {
        timeout: 10_000,
      });
      if (adminRes.ok()) {
        const snap = await adminRes.json();
        for (const u of Object.values(snap) as Array<{ roles?: object }>) {
          baselineVms += Object.keys(u.roles ?? {}).length;
        }
      }
      await ctx.close();
    } catch {
      /* can't get baseline */
    }

    console.log(`\n${"═".repeat(80)}`);
    console.log("  CAPACITY STRESS TEST v2");
    console.log(`  Initial wave: ${INITIAL_WAVE} | Growth: ${(GROWTH_RATE * 100).toFixed(0)}% | Max wave: ${MAX_WAVE_SIZE} | Max waves: ${MAX_WAVES}`);
    console.log(`  Boot timeout: ${BOOT_TIMEOUT}s | Fail threshold: ${(FAIL_THRESHOLD * 100).toFixed(0)}% | Hibernate rate: ${(HIBERNATE_RATE * 100).toFixed(0)}%`);
    console.log(`  Baseline VMs: ${baselineVms}`);
    console.log(`  Profiles: dormant=${PROFILE_WEIGHTS.dormant}% polling=${PROFILE_WEIGHTS.polling}% active=${PROFILE_WEIGHTS.active}% bursty=${PROFILE_WEIGHTS.bursty}%`);
    console.log(`${"═".repeat(80)}\n`);

    for (let wave = 1; wave <= MAX_WAVES; wave++) {
      const runningVms = baselineVms + allSessions.filter((s) => !s.hibernated).length;
      const waveSize = Math.min(
        MAX_WAVE_SIZE,
        Math.max(INITIAL_WAVE, Math.floor(runningVms * GROWTH_RATE))
      );

      console.log(`\n── Wave ${wave}: +${waveSize} new users (${runningVms} running) ──`);

      // ── Phase 0: Hibernate some existing VMs for snapshot-restore testing ──
      const hibernateCount = Math.min(
        Math.floor(allSessions.filter((s) => !s.hibernated).length * HIBERNATE_RATE),
        Math.floor(waveSize * 0.3) // at most 30% of wave is restores
      );
      const toHibernate: UserSession[] = [];
      if (hibernateCount > 0 && wave > 2) {
        // Pick random non-hibernated sessions from older waves
        const candidates = allSessions.filter(
          (s) => !s.hibernated && s.wave < wave - 1
        );
        for (let i = 0; i < hibernateCount && candidates.length > 0; i++) {
          const idx = Math.floor(Math.random() * candidates.length);
          toHibernate.push(candidates.splice(idx, 1)[0]);
        }

        if (toHibernate.length > 0) {
          console.log(`  Hibernating ${toHibernate.length} VMs for snapshot-restore...`);
          await Promise.allSettled(
            toHibernate.map(async (s) => {
              try {
                await s.page.request.post(
                  `/admin/sandboxes/${s.userId}/live/hibernate`,
                  { timeout: 30_000 }
                );
                s.hibernated = true;
              } catch (e) {
                console.log(`  Hibernate failed for ${s.userId}: ${e}`);
              }
            })
          );
        }
      }

      // ── Phase 1: Start background activity on existing sessions ──
      // This runs concurrently with registration+boot phases below.
      // Duration is estimated: registration (~7s) + boot (~15-90s) = ~30-100s.
      // We use 90s as the activity window — activities that finish early just idle.
      const activityDuration = 90_000;
      const activeSessions = allSessions.filter((s) => !s.hibernated);
      const bgActivityPromises = activeSessions.map((s) => {
        const fn = ACTIVITY_FNS[s.profile];
        return fn(s.page, activityDuration).catch(
          (): ActivityResult => ({
            profile: s.profile,
            requests: 0,
            ok: 0,
            errors: 1,
            durationMs: 0,
          })
        );
      });
      // Don't await — runs in parallel with phases 2-4

      // ── Phase 2: Register new users (staggered) ──
      const regT0 = Date.now();
      const regResults = await Promise.allSettled(
        Array.from({ length: waveSize }, (_, i) =>
          (async () => {
            const context = await browser.newContext();
            const page = await context.newPage();
            const { userId, ms } = await registerAndLogin(page, wave, i);
            return { context, page, userId, wave, index: i, regMs: ms };
          })()
        )
      );
      const regMs = Date.now() - regT0;

      const newSessions: UserSession[] = [];
      for (const r of regResults) {
        if (r.status === "fulfilled") {
          newSessions.push({
            wave: r.value.wave,
            index: r.value.index,
            context: r.value.context,
            page: r.value.page,
            userId: r.value.userId,
            profile: pickProfile(),
            hibernated: false,
          });
        } else {
          console.log(`  Registration failed: ${r.reason}`);
        }
      }
      console.log(`  Registered: ${newSessions.length}/${waveSize} in ${regMs}ms`);

      if (newSessions.length === 0) {
        stopReason = `Wave ${wave}: all registrations failed`;
        console.log(`  STOP: ${stopReason}`);
        // Wait for background activity to settle
        await Promise.allSettled(bgActivityPromises);
        break;
      }

      // ── Phase 3: Boot new VMs + restore hibernated VMs ──
      console.log(`  Booting ${newSessions.length} cold + ${toHibernate.filter((s) => s.hibernated).length} snapshot-restore...`);

      // Cold boots
      const coldBootPromises = newSessions.map(async (s) => {
        const { ok, ms } = await waitForSandbox(s.page);
        return { type: "cold" as const, ok, ms, session: s };
      });

      // Snapshot restores (re-start hibernated VMs)
      const restorePromises = toHibernate
        .filter((s) => s.hibernated)
        .map(async (s) => {
          const t0 = Date.now();
          try {
            const res = await s.page.request.post(
              `/admin/sandboxes/${s.userId}/live/start`,
              { timeout: 30_000 }
            );
            if (res.ok()) {
              // Wait for health
              const { ok, ms } = await waitForSandbox(s.page);
              s.hibernated = !ok;
              return {
                type: "restore" as const,
                ok,
                ms: Date.now() - t0,
                session: s,
              };
            }
          } catch {
            /* fall through */
          }
          return {
            type: "restore" as const,
            ok: false,
            ms: Date.now() - t0,
            session: s,
          };
        });

      const allBootResults = await Promise.allSettled([
        ...coldBootPromises,
        ...restorePromises,
      ]);

      const coldBootTimes: number[] = [];
      const restoreTimes: number[] = [];
      let coldBooted = 0;
      let snapshotRestored = 0;
      let bootFailed = 0;

      for (const r of allBootResults) {
        if (r.status !== "fulfilled") {
          bootFailed++;
          continue;
        }
        const { type, ok, ms } = r.value;
        if (ok) {
          if (type === "cold") {
            coldBooted++;
            coldBootTimes.push(ms);
          } else {
            snapshotRestored++;
            restoreTimes.push(ms);
          }
        } else {
          bootFailed++;
          if (type === "cold") {
            console.log(`  Cold boot timeout: w${wave} u${r.value.session.index} (${ms}ms)`);
          } else {
            console.log(`  Restore failed: ${r.value.session.userId} (${ms}ms)`);
          }
        }
      }
      coldBootTimes.sort((a, b) => a - b);
      restoreTimes.sort((a, b) => a - b);

      const totalAttempted = newSessions.length + toHibernate.filter((s) => s.hibernated || snapshotRestored > 0).length;
      console.log(
        `  Cold: ${coldBooted}/${newSessions.length} | Restore: ${snapshotRestored}/${toHibernate.length}` +
          ` | Boot p50: ${percentile(coldBootTimes, 0.5)}ms p95: ${percentile(coldBootTimes, 0.95)}ms` +
          (restoreTimes.length > 0 ? ` | Restore p50: ${percentile(restoreTimes, 0.5)}ms` : "")
      );

      // ── Phase 4: Quick health probe on random existing VMs ──
      let existingHealthAvgMs = -1;
      const healthSample = activeSessions
        .sort(() => Math.random() - 0.5)
        .slice(0, Math.min(10, activeSessions.length));
      if (healthSample.length > 0) {
        const existingTimes: number[] = [];
        await Promise.allSettled(
          healthSample.map(async (s) => {
            const t0 = Date.now();
            try {
              const r = await s.page.request.get("/health", { timeout: 10_000 });
              if (r.ok()) existingTimes.push(Date.now() - t0);
            } catch {
              /* skip */
            }
          })
        );
        existingHealthAvgMs = avg(existingTimes);
      }

      // ── Phase 5: Quick health probe on new VMs ──
      let newHealthOk = 0;
      let newHealthTotal = 0;
      const newHealthTimes: number[] = [];
      await Promise.allSettled(
        newSessions.map(async (s) => {
          for (let i = 0; i < 3; i++) {
            newHealthTotal++;
            const t0 = Date.now();
            try {
              const r = await s.page.request.get("/health", { timeout: 10_000 });
              if (r.ok()) {
                newHealthOk++;
                newHealthTimes.push(Date.now() - t0);
              }
            } catch {
              /* count as failure */
            }
          }
        })
      );
      const healthOkRate = newHealthTotal > 0 ? newHealthOk / newHealthTotal : 0;

      // ── Collect background activity results ──
      const bgResults = await Promise.allSettled(bgActivityPromises);
      const bgByProfile: Record<ActivityProfile, { count: number; ok: number; total: number }> = {
        dormant: { count: 0, ok: 0, total: 0 },
        polling: { count: 0, ok: 0, total: 0 },
        active: { count: 0, ok: 0, total: 0 },
        bursty: { count: 0, ok: 0, total: 0 },
      };
      for (const r of bgResults) {
        if (r.status === "fulfilled") {
          const a = r.value;
          bgByProfile[a.profile].count++;
          bgByProfile[a.profile].ok += a.ok;
          bgByProfile[a.profile].total += a.requests;
        }
      }

      const bgSummary: Record<ActivityProfile, { count: number; okRate: number }> = {
        dormant: { count: bgByProfile.dormant.count, okRate: 1 },
        polling: {
          count: bgByProfile.polling.count,
          okRate: bgByProfile.polling.total > 0 ? bgByProfile.polling.ok / bgByProfile.polling.total : 1,
        },
        active: {
          count: bgByProfile.active.count,
          okRate: bgByProfile.active.total > 0 ? bgByProfile.active.ok / bgByProfile.active.total : 1,
        },
        bursty: {
          count: bgByProfile.bursty.count,
          okRate: bgByProfile.bursty.total > 0 ? bgByProfile.bursty.ok / bgByProfile.bursty.total : 1,
        },
      };

      console.log(
        `  Background: D=${bgSummary.dormant.count} P=${bgSummary.polling.count}(${(bgSummary.polling.okRate * 100).toFixed(0)}%)` +
          ` A=${bgSummary.active.count}(${(bgSummary.active.okRate * 100).toFixed(0)}%) B=${bgSummary.bursty.count}(${(bgSummary.bursty.okRate * 100).toFixed(0)}%)`
      );

      // ── Add new sessions and compute status ──
      allSessions.push(...newSessions);
      const totalAfter = baselineVms + allSessions.filter((s) => !s.hibernated).length;

      const failRate = bootFailed / Math.max(1, totalAttempted);
      let status: WaveResult["status"] = "ok";
      if (failRate >= FAIL_THRESHOLD) {
        status = "crashed";
      } else if (failRate > 0 || percentile(coldBootTimes, 0.95) > DEGRADE_BOOT_MS) {
        status = "failing";
      } else if (
        percentile(coldBootTimes, 0.5) > DEGRADE_BOOT_MS / 2 ||
        healthOkRate < 0.9
      ) {
        status = "degraded";
      }

      const result: WaveResult = {
        wave,
        waveSize,
        totalVmsBefore: runningVms,
        totalVmsAfter: totalAfter,
        registered: newSessions.length,
        coldBooted,
        snapshotRestored,
        bootFailed,
        bootTimes: coldBootTimes,
        restoreTimes,
        bootP50: percentile(coldBootTimes, 0.5),
        bootP95: percentile(coldBootTimes, 0.95),
        bootMax: percentile(coldBootTimes, 1.0),
        restoreP50: percentile(restoreTimes, 0.5),
        healthOkRate,
        healthAvgMs: avg(newHealthTimes),
        bgActivity: bgSummary,
        existingHealthAvgMs,
        status,
      };
      waveResults.push(result);

      console.log(
        `  Health: new ${(healthOkRate * 100).toFixed(0)}% avg ${avg(newHealthTimes)}ms` +
          ` | existing avg ${existingHealthAvgMs}ms` +
          ` | Status: ${status.toUpperCase()} | Total: ${totalAfter}`
      );

      if (status === "crashed") {
        stopReason = `Wave ${wave}: ${(failRate * 100).toFixed(0)}% boot failures (${bootFailed}/${totalAttempted})`;
        console.log(`  STOP: ${stopReason}`);
        break;
      }

      // Brief pause between waves
      await new Promise((r) => setTimeout(r, 2000));
    }

    if (!stopReason) {
      stopReason = `Completed ${MAX_WAVES} waves without crash`;
    }

    // ── Final Report ──────────────────────────────────────────────────────
    console.log(`\n${"═".repeat(100)}`);
    console.log("  CAPACITY STRESS TEST v2 — RESULTS");
    console.log(`${"═".repeat(100)}`);
    console.log(`  Stop reason: ${stopReason}`);
    console.log(`  Total sessions: ${allSessions.length} (+ ${baselineVms} pre-existing)`);
    console.log(`  Peak running VMs: ${Math.max(...waveResults.map((w) => w.totalVmsAfter))}`);
    const profileDist = allSessions.reduce((m, s) => { m[s.profile] = (m[s.profile] || 0) + 1; return m; }, {} as Record<string, number>);
    console.log(`  Profile distribution: ${JSON.stringify(profileDist)}`);
    console.log();

    console.log(
      "  Wave | +New | Total | Cold  | Restore | Boot p50  | Boot p95  | Rst p50   | Hlth  | Exist   | BG ok | Status"
    );
    console.log("  " + "─".repeat(115));

    for (const w of waveResults) {
      const bgOkAvg = (["polling", "active", "bursty"] as const)
        .filter((p) => w.bgActivity[p].count > 0)
        .map((p) => w.bgActivity[p].okRate);
      const bgOkStr = bgOkAvg.length > 0
        ? `${(bgOkAvg.reduce((a, b) => a + b, 0) / bgOkAvg.length * 100).toFixed(0)}%`
        : "  —";

      const line = [
        `  ${String(w.wave).padStart(4)}`,
        `${String(w.waveSize).padStart(4)}`,
        `${String(w.totalVmsAfter).padStart(5)}`,
        `${w.coldBooted}/${w.registered}`.padStart(5),
        `${w.snapshotRestored}`.padStart(7),
        `${String(w.bootP50).padStart(7)}ms`,
        `${String(w.bootP95).padStart(7)}ms`,
        w.restoreP50 >= 0 ? `${String(w.restoreP50).padStart(7)}ms` : "      —  ",
        `${(w.healthOkRate * 100).toFixed(0).padStart(4)}%`,
        `${String(w.existingHealthAvgMs).padStart(5)}ms`,
        bgOkStr.padStart(5),
        w.status.toUpperCase().padStart(8),
      ].join(" | ");
      console.log(line);
    }

    console.log(`${"═".repeat(100)}\n`);

    // Cleanup (ignore artifact errors from 100s of contexts)
    await Promise.allSettled(allSessions.map((s) => s.context.close()));

    const lastWave = waveResults[waveResults.length - 1];
    if (lastWave) {
      console.log(
        `Peak capacity: ${lastWave.totalVmsAfter} VMs (${lastWave.status})`
      );
    }
  });
});
