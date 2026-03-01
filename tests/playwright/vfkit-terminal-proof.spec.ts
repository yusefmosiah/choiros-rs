import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { ensureAuthenticated } from "./auth.helpers";

const DESKTOP_ID = "default-desktop";
const PROOF_MARKER = "__choir_vfkit_proof__";

interface MeResponse {
  authenticated: boolean;
  user_id?: string | null;
}

interface SandboxSnapshot {
  user_id: string;
  role?: string | null;
  port: number;
  status: string;
}

async function closeAllDesktopWindows(request: APIRequestContext): Promise<void> {
  const windowsResp = await request.get(`/desktop/${DESKTOP_ID}/windows`);
  if (!windowsResp.ok()) {
    return;
  }

  const windowsJson = (await windowsResp.json()) as {
    windows?: Array<{ id: string }>;
  };

  for (const win of windowsJson.windows ?? []) {
    await request.delete(`/desktop/${DESKTOP_ID}/windows/${win.id}`);
  }
}

async function currentUserId(page: Page): Promise<string> {
  const meRes = await page.request.get("/auth/me");
  expect(meRes.ok()).toBeTruthy();
  const me = (await meRes.json()) as MeResponse;
  expect(me.authenticated).toBe(true);
  expect(me.user_id).toBeTruthy();
  return me.user_id as string;
}

async function setMainPointerToRole(page: Page, userId: string, role: "live" | "dev"): Promise<void> {
  const setPointer = await page.request.post(`/admin/sandboxes/${userId}/pointers/set`, {
    data: {
      pointer_name: "main",
      target_kind: "role",
      target_value: role,
    },
  });
  const body = await setPointer.text();
  expect(
    setPointer.ok(),
    `set pointer to role failed: status=${setPointer.status()} body=${body}`
  ).toBeTruthy();
}

async function ensureLiveSandboxRunning(page: Page, userId: string): Promise<void> {
  const start = await page.request.post(`/admin/sandboxes/${userId}/live/start`);
  const body = await start.text();
  expect(
    start.ok(),
    `start live sandbox failed: status=${start.status()} body=${body}`
  ).toBeTruthy();

  for (let i = 0; i < 45; i++) {
    const snapshotsRes = await page.request.get("/admin/sandboxes");
    if (snapshotsRes.ok()) {
      const snapshots = (await snapshotsRes.json()) as SandboxSnapshot[];
      const liveSnapshot = snapshots.find(
        (s) => s.user_id === userId && s.role === "live" && s.status === "running"
      );
      if (liveSnapshot) {
        return;
      }
    }
    await page.waitForTimeout(1_000);
  }

  throw new Error("live sandbox did not become running within 45s");
}

async function ensureTerminalAppRegistered(request: APIRequestContext): Promise<void> {
  const appsResp = await request.get(`/desktop/${DESKTOP_ID}/apps`);
  if (appsResp.ok()) {
    const appsJson = (await appsResp.json()) as {
      success: boolean;
      apps?: Array<{ id: string }>;
    };
    if ((appsJson.apps ?? []).some((app) => app.id === "terminal")) {
      return;
    }
  }

  const registerResp = await request.post(`/desktop/${DESKTOP_ID}/apps`, {
    data: {
      id: "terminal",
      name: "Terminal",
      icon: "🖥️",
      component_code: "TerminalApp",
      default_width: 700,
      default_height: 450,
    },
  });
  expect([200, 400]).toContain(registerResp.status());
}

async function openTerminalViaApi(request: APIRequestContext): Promise<void> {
  await ensureTerminalAppRegistered(request);

  const openResp = await request.post(`/desktop/${DESKTOP_ID}/windows`, {
    data: {
      app_id: "terminal",
      title: "Terminal",
      props: {},
    },
  });
  const body = await openResp.text();
  expect(
    openResp.ok(),
    `open terminal window failed: status=${openResp.status()} body=${body}`
  ).toBeTruthy();
}

async function ensureDesktopReachable(page: Page): Promise<void> {
  for (let attempt = 1; attempt <= 4; attempt++) {
    await page.goto("/");
    const body = (await page.locator("body").innerText()).toLowerCase();
    if (!body.includes("sandbox unreachable")) {
      return;
    }
    await page.waitForTimeout(2_000 * attempt);
  }

  throw new Error("desktop page remained unreachable after retries");
}

async function openTerminalWindow(page: Page): Promise<void> {
  await ensureDesktopReachable(page);

  const terminalLauncher = page
    .locator("button, [role='button'], .desktop-icon")
    .filter({ hasText: /terminal/i })
    .first();

  if (await terminalLauncher.isVisible({ timeout: 10_000 }).catch(() => false)) {
    await terminalLauncher.click();
  } else {
    await openTerminalViaApi(page.request);
    await page.reload();
  }

  const terminalWindow = page
    .locator(".desktop-window")
    .filter({ has: page.locator(".window-titlebar", { hasText: /terminal/i }) })
    .first();

  await expect(terminalWindow).toBeVisible({ timeout: 60_000 });
}

test.describe.serial("vfkit terminal proof", () => {
  test.skip(
    process.env.CHOIR_E2E_EXPECT_NIXOS !== "1",
    "set CHOIR_E2E_EXPECT_NIXOS=1 when running against vfkit + NixOS runtime"
  );

  test("terminal app shows NixOS guest identity", async ({ page }) => {
    await ensureAuthenticated(page);
    const userId = await currentUserId(page);

    try {
      await setMainPointerToRole(page, userId, "live");
      await ensureLiveSandboxRunning(page, userId);
      await closeAllDesktopWindows(page.request);

      await openTerminalWindow(page);

      const status = page.locator(".terminal-status").first();
      await expect(status).toHaveText(/Connected/i, { timeout: 90_000 });

      const terminalContainer = page.locator(".terminal-container").first();
      await expect(terminalContainer).toBeVisible({ timeout: 90_000 });

      await terminalContainer.click();
      await page.keyboard.type(`cat /etc/os-release && echo ${PROOF_MARKER}`);
      await page.keyboard.press("Enter");

      await expect
        .poll(async () => (await page.locator("body").innerText()).toLowerCase(), {
          timeout: 90_000,
        })
        .toContain(PROOF_MARKER);

      const bodyText = (await page.locator("body").innerText()).toLowerCase();
      expect(bodyText).toContain("nixos");
    } finally {
      await page.request.post(`/admin/sandboxes/${userId}/pointers/set`, {
        data: {
          pointer_name: "main",
          target_kind: "role",
          target_value: "live",
        },
      });
      await page.request.post(`/admin/sandboxes/${userId}/live/stop`);
    }
  });
});
