import { expect, test, type Page } from "@playwright/test";
import { ensureAuthenticated } from "./auth.helpers";

interface MeResponse {
  authenticated: boolean;
  user_id?: string | null;
}

interface SandboxSnapshot {
  user_id: string;
  role?: string | null;
  branch?: string | null;
  port: number;
  status: string;
  idle_secs: number;
}

function uniqueBranchName(): string {
  return `feature_${Date.now()}_${Math.random().toString(36).slice(2, 6)}`;
}

async function currentUserId(page: Page): Promise<string> {
  const meRes = await page.request.get("/auth/me");
  expect(meRes.ok()).toBeTruthy();
  const me = (await meRes.json()) as MeResponse;
  expect(me.authenticated).toBe(true);
  expect(me.user_id).toBeTruthy();
  return me.user_id as string;
}

async function startBranchRuntime(page: Page, userId: string, branch: string): Promise<void> {
  for (let attempt = 1; attempt <= 3; attempt++) {
    const start = await page.request.post(
      `/admin/sandboxes/${userId}/branches/${branch}/start`
    );
    if (start.ok()) {
      return;
    }

    const body = await start.text();
    if (attempt === 3) {
      throw new Error(
        `start branch failed after ${attempt} attempts: status=${start.status()} body=${body}`
      );
    }
    await page.waitForTimeout(2_000 * attempt);
  }
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

async function stopBranchBestEffort(page: Page, userId: string, branch: string): Promise<void> {
  await page.request.post(`/admin/sandboxes/${userId}/branches/${branch}/stop`);
}

async function stopLiveBestEffort(page: Page, userId: string): Promise<void> {
  await page.request.post(`/admin/sandboxes/${userId}/live/stop`);
}

async function setMainPointerToLiveBestEffort(page: Page, userId: string): Promise<void> {
  await page.request.post(`/admin/sandboxes/${userId}/pointers/set`, {
    data: {
      pointer_name: "main",
      target_kind: "role",
      target_value: "live",
    },
  });
}

test.describe.serial("branch proxy integration", () => {
  test("branch runtime is startable and routable via /branch/<name>/...", async ({ page }) => {
    await ensureAuthenticated(page);
    const userId = await currentUserId(page);
    const branch = uniqueBranchName();

    try {
      await setMainPointerToRole(page, userId, "live");
      await startBranchRuntime(page, userId, branch);

      const snapshotsRes = await page.request.get("/admin/sandboxes");
      expect(snapshotsRes.ok()).toBeTruthy();
      const snapshots = (await snapshotsRes.json()) as SandboxSnapshot[];

      const branchSnapshot = snapshots.find(
        (s) =>
          s.user_id === userId &&
          s.branch === branch &&
          s.status === "running" &&
          typeof s.port === "number"
      );
      expect(branchSnapshot, "branch snapshot should exist and be running").toBeTruthy();

      // Use a known sandbox route through the branch prefix. We accept sandbox-origin
      // statuses because behavior may vary by auth/session/data state.
      const proxied = await page.request.get(`/branch/${branch}/logs/events`, {
        timeout: 15_000,
      });
      expect(
        [200, 204, 400, 401, 404].includes(proxied.status()),
        `expected sandbox-origin status via branch proxy, got ${proxied.status()}`
      ).toBe(true);
    } finally {
      await setMainPointerToLiveBestEffort(page, userId);
      await stopBranchBestEffort(page, userId, branch);
      await stopLiveBestEffort(page, userId);
    }
  });

  test("main pointer can be switched to branch target for default routing", async ({ page }) => {
    await ensureAuthenticated(page);
    const userId = await currentUserId(page);
    const branch = uniqueBranchName();

    try {
      await setMainPointerToRole(page, userId, "live");
      await startBranchRuntime(page, userId, branch);

      const setPointer = await page.request.post(
        `/admin/sandboxes/${userId}/pointers/set`,
        {
          data: {
            pointer_name: "main",
            target_kind: "branch",
            target_value: branch,
          },
        }
      );
      const setPointerBody = await setPointer.text();
      expect(
        setPointer.ok(),
        `set pointer failed: status=${setPointer.status()} body=${setPointerBody}`
      ).toBeTruthy();

      const pointers = await page.request.get(`/admin/sandboxes/${userId}/pointers`);
      expect(pointers.ok()).toBeTruthy();
      const pointerRows = (await pointers.json()) as Array<{
        pointer_name: string;
        target_kind: string;
        target_value: string;
      }>;

      const main = pointerRows.find((p) => p.pointer_name === "main");
      expect(main).toBeTruthy();
      expect(main?.target_kind).toBe("branch");
      expect(main?.target_value).toBe(branch);

      const proxied = await page.request.get("/logs/events", { timeout: 15_000 });
      expect(
        [200, 204, 400, 401, 404].includes(proxied.status()),
        `expected sandbox-origin status via main pointer branch route, got ${proxied.status()}`
      ).toBe(true);
    } finally {
      await setMainPointerToLiveBestEffort(page, userId);
      await stopBranchBestEffort(page, userId, branch);
      await stopLiveBestEffort(page, userId);
    }
  });
});
