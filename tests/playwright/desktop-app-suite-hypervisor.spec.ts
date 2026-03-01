import {
  expect,
  test,
  type APIRequestContext,
  type BrowserContext,
  type Locator,
  type Page,
} from "@playwright/test";

const DESKTOP_ID = "default-desktop";

interface MeResponse {
  authenticated: boolean;
  user_id?: string | null;
}

interface SandboxSnapshot {
  user_id: string;
  role?: string | null;
  status: string;
}

type CoreApp = {
  id: string;
  name: string;
  icon: string;
  component_code: string;
  default_width: number;
  default_height: number;
};

const CORE_APPS: CoreApp[] = [
  {
    id: "writer",
    name: "Writer",
    icon: "📝",
    component_code: "WriterApp",
    default_width: 1100,
    default_height: 720,
  },
  {
    id: "terminal",
    name: "Terminal",
    icon: "🖥️",
    component_code: "TerminalApp",
    default_width: 700,
    default_height: 450,
  },
  {
    id: "trace",
    name: "Trace",
    icon: "🔍",
    component_code: "TraceApp",
    default_width: 900,
    default_height: 600,
  },
];

async function authMe(page: Page): Promise<MeResponse> {
  const res = await page.request.get("/auth/me", { timeout: 30_000 });
  if (!res.ok()) {
    return { authenticated: false };
  }
  return (await res.json()) as MeResponse;
}

function uniqueUsername(): string {
  return `desktop_suite_${Date.now()}_${Math.random().toString(36).slice(2, 8)}@example.com`;
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

async function ensureAuthenticatedForSuite(page: Page): Promise<void> {
  const me = await authMe(page);
  if (me.authenticated) {
    return;
  }

  await addVirtualAuthenticator(page);
  await page.goto("/register", { waitUntil: "domcontentloaded" });
  const username = uniqueUsername();

  const startRes = await page.request.post("/auth/register/start", {
    timeout: 30_000,
    data: {
      username,
      display_name: username,
    },
  });
  const startBody = await startRes.text();
  expect(
    startRes.ok(),
    `register/start failed: status=${startRes.status()} body=${startBody}`
  ).toBeTruthy();
  const startJson = JSON.parse(startBody) as { publicKey: unknown };

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
      for (const b of bytes) {
        binary += String.fromCharCode(b);
      }
      return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=/g, "");
    };

    const decodePublicKeyOptions = (opts: any): PublicKeyCredentialCreationOptions => {
      const out: any = { ...opts };
      if (out.challenge) {
        out.challenge = b64uToBytes(out.challenge);
      }
      if (out.user?.id) {
        out.user = { ...out.user, id: b64uToBytes(out.user.id) };
      }
      if (Array.isArray(out.excludeCredentials)) {
        out.excludeCredentials = out.excludeCredentials.map((item: any) => ({
          ...item,
          id: b64uToBytes(item.id),
        }));
      }
      return out;
    };

    const publicKey = decodePublicKeyOptions(publicKeyOptions);
    const cred = (await navigator.credentials.create({
      publicKey,
    })) as PublicKeyCredential | null;
    if (!cred) {
      throw new Error("navigator.credentials.create returned null");
    }

    const response = cred.response as AuthenticatorAttestationResponse;
    return {
      id: cred.id,
      rawId: bytesToB64u(cred.rawId),
      type: cred.type,
      response: {
        attestationObject: bytesToB64u(response.attestationObject),
        clientDataJSON: bytesToB64u(response.clientDataJSON),
      },
      clientExtensionResults: cred.getClientExtensionResults
        ? cred.getClientExtensionResults()
        : {},
      transports: response.getTransports ? response.getTransports() : [],
    };
  }, startJson.publicKey);

  const finishRes = await page.request.post("/auth/register/finish", {
    timeout: 30_000,
    data: credentialJson,
  });
  const finishBody = await finishRes.text();
  expect(
    finishRes.ok(),
    `register/finish failed: status=${finishRes.status()} body=${finishBody}`
  ).toBeTruthy();

  await expect.poll(async () => (await authMe(page)).authenticated, { timeout: 180_000 }).toBe(
    true
  );
}

async function currentUserId(page: Page): Promise<string> {
  const me = await authMe(page);
  expect(me.authenticated).toBe(true);
  expect(me.user_id).toBeTruthy();
  return me.user_id as string;
}

async function startLiveRuntime(page: Page, userId: string): Promise<void> {
  const start = await page.request.post(`/admin/sandboxes/${userId}/live/start`, {
    timeout: 420_000,
  });
  const body = await start.text();
  expect(start.ok(), `start live sandbox failed: status=${start.status()} body=${body}`).toBeTruthy();
}

async function setMainPointerToLive(page: Page, userId: string): Promise<void> {
  const res = await page.request.post(`/admin/sandboxes/${userId}/pointers/set`, {
    timeout: 30_000,
    data: {
      pointer_name: "main",
      target_kind: "role",
      target_value: "live",
    },
  });
  const body = await res.text();
  expect(res.ok(), `set pointer failed: status=${res.status()} body=${body}`).toBeTruthy();
}

async function waitForLiveRunning(page: Page, userId: string, timeoutMs: number): Promise<void> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const res = await page.request.get("/admin/sandboxes", { timeout: 15_000 });
    if (res.ok()) {
      const snapshots = (await res.json()) as SandboxSnapshot[];
      if (snapshots.some((s) => s.user_id === userId && s.role === "live" && s.status === "running")) {
        return;
      }
    }
    await page.waitForTimeout(1_000);
  }
  throw new Error(`live sandbox did not become running within ${timeoutMs}ms`);
}

async function closeAllDesktopWindows(request: APIRequestContext): Promise<void> {
  const windowsResp = await request.get(`/desktop/${DESKTOP_ID}/windows`, { timeout: 15_000 });
  if (!windowsResp.ok()) {
    return;
  }

  const windowsJson = (await windowsResp.json()) as {
    windows?: Array<{ id: string }>;
  };

  for (const win of windowsJson.windows ?? []) {
    await request.delete(`/desktop/${DESKTOP_ID}/windows/${win.id}`, { timeout: 15_000 });
  }
}

async function ensureAppRegistered(request: APIRequestContext, app: CoreApp): Promise<void> {
  const appsResp = await request.get(`/desktop/${DESKTOP_ID}/apps`, { timeout: 15_000 });
  if (appsResp.ok()) {
    const appsJson = (await appsResp.json()) as {
      apps?: Array<{ id: string }>;
    };
    if ((appsJson.apps ?? []).some((existing) => existing.id === app.id)) {
      return;
    }
  }

  const registerResp = await request.post(`/desktop/${DESKTOP_ID}/apps`, {
    timeout: 15_000,
    data: app,
  });
  expect([200, 400]).toContain(registerResp.status());
}

function windowLocator(page: Page, title: string): Locator {
  return page.getByRole("dialog", { name: new RegExp(title, "i") }).last();
}

async function openAppWindowViaApi(page: Page, app: CoreApp): Promise<void> {
  const openResp = await page.request.post(`/desktop/${DESKTOP_ID}/windows`, {
    timeout: 15_000,
    data: {
      app_id: app.id,
      title: app.name,
      props: {},
    },
  });
  const body = await openResp.text();
  expect(openResp.ok(), `open ${app.name} window failed: status=${openResp.status()} body=${body}`).toBeTruthy();
  await page.reload({ waitUntil: "domcontentloaded" });
}

async function openAppWindow(page: Page, app: CoreApp): Promise<void> {
  const appIcon = page
    .locator("button, [role='button'], .desktop-icon")
    .filter({ hasText: new RegExp(app.name, "i") })
    .first();

  if (await appIcon.isVisible({ timeout: 10_000 }).catch(() => false)) {
    try {
      await appIcon.click({ timeout: 15_000 });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (!message.includes("intercepts pointer events")) {
        throw error;
      }
      await openAppWindowViaApi(page, app);
    }
  } else {
    await openAppWindowViaApi(page, app);
  }

  await expect(windowLocator(page, app.name)).toBeVisible({ timeout: 90_000 });
}

async function promptBarInteraction(page: Page): Promise<void> {
  const waitStart = Date.now();
  const promptInput = page
    .locator("input.prompt-input[placeholder*='Ask anything']")
    .first();
  while (Date.now() - waitStart < 180_000) {
    await page.goto("/", { waitUntil: "domcontentloaded" });
    if (await promptInput.isVisible().catch(() => false)) {
      break;
    }

    const bodyText = (await page.locator("body").innerText().catch(() => "")).toLowerCase();
    if (bodyText.includes("sandbox unreachable")) {
      await page.waitForTimeout(2_000);
      continue;
    }

    await page.waitForTimeout(2_000);
  }
  await expect(promptInput).toBeVisible({ timeout: 30_000 });
  await promptInput.fill("hi from desktop app suite");
  await promptInput.press("Enter");

  const toastIndicator = page.locator(".conductor-toast-indicator").first();
  const writerWindow = windowLocator(page, "Writer");
  const errorIndicator = page.locator(".conductor-error-indicator").first();

  await expect
    .poll(
      async () => {
        if (await errorIndicator.isVisible().catch(() => false)) {
          return "error";
        }
        if (await toastIndicator.isVisible().catch(() => false)) {
          return "toast";
        }
        if (await writerWindow.isVisible().catch(() => false)) {
          return "writer";
        }
        return "pending";
      },
      { timeout: 120_000, intervals: [500, 1000, 1500, 2000] }
    )
    .not.toBe("pending");
}

test.describe.serial("desktop app suite (hypervisor)", () => {
  test("prompt bar, writer, terminal, and trace app are usable", async ({ page }) => {
    test.setTimeout(900_000);

    await ensureAuthenticatedForSuite(page);
    const userId = await currentUserId(page);

    try {
      await setMainPointerToLive(page, userId);
      await startLiveRuntime(page, userId);
      await waitForLiveRunning(page, userId, 120_000);

      await page.goto("/", { waitUntil: "domcontentloaded" });
      await closeAllDesktopWindows(page.request);
      for (const app of CORE_APPS) {
        await ensureAppRegistered(page.request, app);
      }

      await promptBarInteraction(page);

      await openAppWindow(page, CORE_APPS[0]); // Writer
      await openAppWindow(page, CORE_APPS[1]); // Terminal
      await openAppWindow(page, CORE_APPS[2]); // Trace

      const terminalWindow = windowLocator(page, "Terminal");
      await expect(terminalWindow).toBeVisible({ timeout: 120_000 });
      await expect(
        terminalWindow
          .locator(".terminal-container, .terminal-status, .xterm, [class*='terminal']")
          .first()
      ).toBeVisible({ timeout: 120_000 });
      await expect(page.getByText(/Connected|Connecting/i).first()).toBeVisible({ timeout: 120_000 });

      const traceWindow = windowLocator(page, "Trace");
      await expect(traceWindow).toBeVisible({ timeout: 120_000 });
    } finally {
      await page.request
        .post(`/admin/sandboxes/${userId}/pointers/set`, {
          timeout: 30_000,
          data: {
            pointer_name: "main",
            target_kind: "role",
            target_value: "live",
          },
        })
        .catch(() => undefined);
      await page.request
        .post(`/admin/sandboxes/${userId}/live/stop`, {
          timeout: 30_000,
        })
        .catch(() => undefined);
    }
  });
});
