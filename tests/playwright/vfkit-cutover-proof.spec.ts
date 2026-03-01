import {
  expect,
  test,
  type APIRequestContext,
  type BrowserContext,
  type Page,
} from "@playwright/test";

const DESKTOP_ID = "default-desktop";
const PROOF_MARKER = "__choir_vfkit_proof__";

interface MeResponse {
  authenticated: boolean;
  user_id?: string | null;
  username?: string | null;
}

interface SandboxSnapshot {
  user_id: string;
  role?: string | null;
  branch?: string | null;
  status: string;
}

async function showProofStep(page: Page, step: string): Promise<void> {
  await page
    .evaluate((label) => {
      const id = "__vfkit_proof_step__";
      let el = document.getElementById(id);
      if (!el) {
        el = document.createElement("div");
        el.id = id;
        el.setAttribute(
          "style",
          [
            "position:fixed",
            "top:8px",
            "right:8px",
            "z-index:2147483647",
            "padding:6px 10px",
            "border-radius:8px",
            "background:rgba(0,0,0,0.7)",
            "color:#e6f4ff",
            "font:12px/1.3 monospace",
            "pointer-events:none",
          ].join(";")
        );
        document.body.appendChild(el);
      }
      el.textContent = `vfkit proof: ${label}`;
    }, step)
    .catch(() => undefined);
}

function uniqueBranchName(): string {
  return `feature_${Date.now()}_${Math.random().toString(36).slice(2, 6)}`;
}

function uniqueUsername(): string {
  return `vfkit_e2e_${Date.now()}_${Math.random().toString(36).slice(2, 8)}@example.com`;
}

async function addVirtualAuthenticator(page: Page): Promise<string> {
  const cdp = await (page.context() as BrowserContext).newCDPSession(page);
  await cdp.send("WebAuthn.enable", { enableUI: false });
  const { authenticatorId } = await cdp.send("WebAuthn.addVirtualAuthenticator", {
    options: {
      protocol: "ctap2",
      transport: "internal",
      hasResidentKey: true,
      hasUserVerification: true,
      isUserVerified: true,
    },
  });
  return authenticatorId;
}

async function authMe(page: Page): Promise<MeResponse> {
  const res = await page.request.get("/auth/me", { timeout: 30_000 });
  if (!res.ok()) {
    return { authenticated: false };
  }
  return (await res.json()) as MeResponse;
}

async function ensureAuthenticatedForProof(page: Page): Promise<void> {
  const me = await authMe(page);
  if (me.authenticated) {
    return;
  }

  await addVirtualAuthenticator(page);
  await page.goto("/register");
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

async function setMainPointer(
  page: Page,
  userId: string,
  targetKind: "role" | "branch",
  targetValue: string
): Promise<void> {
  const res = await page.request.post(`/admin/sandboxes/${userId}/pointers/set`, {
    timeout: 30_000,
    data: {
      pointer_name: "main",
      target_kind: targetKind,
      target_value: targetValue,
    },
  });
  const body = await res.text();
  expect(
    res.ok(),
    `set pointer failed: status=${res.status()} target=${targetKind}:${targetValue} body=${body}`
  ).toBeTruthy();
}

async function startLiveRuntime(page: Page, userId: string): Promise<void> {
  const start = await page.request.post(`/admin/sandboxes/${userId}/live/start`, {
    timeout: 420_000,
  });
  const body = await start.text();
  expect(
    start.ok(),
    `start live sandbox failed: status=${start.status()} body=${body}`
  ).toBeTruthy();
}

async function startBranchRuntime(page: Page, userId: string, branch: string): Promise<void> {
  const start = await page.request.post(`/admin/sandboxes/${userId}/branches/${branch}/start`, {
    timeout: 420_000,
  });
  const body = await start.text();
  expect(
    start.ok(),
    `start branch sandbox failed: status=${start.status()} body=${body}`
  ).toBeTruthy();
}

async function waitForSnapshotTarget(
  page: Page,
  matcher: (snapshot: SandboxSnapshot) => boolean,
  timeoutMs: number
): Promise<void> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const res = await page.request.get("/admin/sandboxes", { timeout: 15_000 });
    if (res.ok()) {
      const snapshots = (await res.json()) as SandboxSnapshot[];
      if (snapshots.some(matcher)) {
        return;
      }
    }
    await page.waitForTimeout(1_000);
  }
  throw new Error(`sandbox target did not appear within ${timeoutMs}ms`);
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

async function ensureTerminalAppRegistered(request: APIRequestContext): Promise<void> {
  const appsResp = await request.get(`/desktop/${DESKTOP_ID}/apps`, { timeout: 15_000 });
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
    timeout: 15_000,
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

async function proveNixosViaTerminalWebsocket(page: Page, userId: string): Promise<void> {
  const terminalId = `vfkit-proof-${Date.now()}`;
  const origin = new URL(page.url());
  const wsProtocol = origin.protocol === "https:" ? "wss:" : "ws:";
  const wsUrl = `${wsProtocol}//${origin.host}/ws/terminal/${terminalId}?user_id=${encodeURIComponent(userId)}`;

  try {
    const output = await page.evaluate(
      async ({ wsUrl, marker }) => {
        const messageToText = (data: unknown): string => {
          if (typeof data === "string") {
            return data;
          }
          if (data instanceof Blob) {
            throw new Error("blob websocket payload unsupported");
          }
          if (data instanceof ArrayBuffer) {
            return new TextDecoder().decode(new Uint8Array(data));
          }
          return String(data ?? "");
        };

        return await new Promise<string>((resolve, reject) => {
          const ws = new WebSocket(wsUrl);
          let output = "";
          let settled = false;

          const done = (err?: Error) => {
            if (settled) {
              return;
            }
            settled = true;
            clearTimeout(timeout);
            try {
              ws.close();
            } catch {
              // no-op
            }
            if (err) {
              reject(err);
            } else {
              resolve(output);
            }
          };

          const sendProof = () => {
            ws.send(
              JSON.stringify({
                type: "input",
                data: `cat /etc/os-release && echo ${marker}\n`,
              })
            );
          };

          const timeout = setTimeout(() => {
            done(new Error(`terminal proof timeout; output=${output.slice(-2000)}`));
          }, 180_000);

          ws.addEventListener("open", () => {
            sendProof();
          });

          ws.addEventListener("message", (event) => {
            const payload = messageToText(event.data);
            try {
              const msg = JSON.parse(payload) as {
                type?: string;
                data?: string;
                is_running?: boolean;
                message?: string;
              };
              if (msg.type === "info" && msg.is_running) {
                sendProof();
              }
              if (typeof msg.data === "string") {
                output += msg.data;
              }
              if (msg.type === "error") {
                done(new Error(`terminal websocket error: ${msg.message ?? "unknown"}`));
                return;
              }
            } catch {
              output += payload;
            }

            const lower = output.toLowerCase();
            if (lower.includes(marker.toLowerCase()) && lower.includes("nixos")) {
              done();
            }
          });

          ws.addEventListener("close", (event) => {
            if (settled) {
              return;
            }
            done(
              new Error(
                `terminal websocket closed (code=${event.code}, reason=${event.reason}); output=${output.slice(
                  -2000
                )}`
              )
            );
          });

          ws.addEventListener("error", () => {
            done(new Error(`terminal websocket error event; output=${output.slice(-2000)}`));
          });
        });
      },
      { wsUrl, marker: PROOF_MARKER }
    );

    const lower = output.toLowerCase();
    expect(
      lower.includes(PROOF_MARKER.toLowerCase()) && lower.includes("nixos"),
      `terminal proof missing marker/nixos; output=${output.slice(-2000)}`
    ).toBe(true);
  } finally {
    await page.request
      .get(`/api/terminals/${terminalId}/stop`, { timeout: 30_000 })
      .catch(() => undefined);
  }
}

async function stopBestEffort(
  request: APIRequestContext,
  path: string,
  body?: Record<string, unknown>
): Promise<void> {
  await request
    .post(path, {
      timeout: 10_000,
      data: body,
    })
    .catch(() => undefined);
}

test.describe.serial("vfkit cutover proof", () => {
  test.skip(
    process.env.CHOIR_E2E_EXPECT_NIXOS !== "1",
    "set CHOIR_E2E_EXPECT_NIXOS=1 when running against vfkit + NixOS runtime"
  );

  test("single-user live + branch runtime proof with NixOS terminal evidence", async ({
    page,
  }) => {
    test.setTimeout(900_000);

    await showProofStep(page, "auth: start");
    await ensureAuthenticatedForProof(page);
    await showProofStep(page, "auth: done");
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await showProofStep(page, "desktop: loaded");

    const userId = await currentUserId(page);
    const branch = uniqueBranchName();

    try {
      await showProofStep(page, "live: pointer set");
      await setMainPointer(page, userId, "role", "live");
      await showProofStep(page, "live: starting runtime");
      await startLiveRuntime(page, userId);
      await waitForSnapshotTarget(
        page,
        (s) => s.user_id === userId && s.role === "live" && s.status === "running",
        120_000
      );
      await showProofStep(page, "live: running");

      await closeAllDesktopWindows(page.request);
      await ensureTerminalAppRegistered(page.request);
      await showProofStep(page, "terminal: websocket proof");
      await proveNixosViaTerminalWebsocket(page, userId);
      await showProofStep(page, "terminal: nixos verified");

      await showProofStep(page, "branch: starting runtime");
      await startBranchRuntime(page, userId, branch);
      await waitForSnapshotTarget(
        page,
        (s) => s.user_id === userId && s.branch === branch && s.status === "running",
        120_000
      );
      await showProofStep(page, "branch: running");

      const branchRouteResp = await page.request.get(`/branch/${branch}/logs/events`, {
        timeout: 30_000,
      });
      expect(
        [200, 204, 400, 401, 404].includes(branchRouteResp.status()),
        `expected sandbox-origin status via branch proxy, got ${branchRouteResp.status()}`
      ).toBe(true);

      await setMainPointer(page, userId, "branch", branch);
      const pointerDefaultResp = await page.request.get("/logs/events", {
        timeout: 30_000,
      });
      expect(
        [200, 204, 400, 401, 404].includes(pointerDefaultResp.status()),
        `expected sandbox-origin status via main pointer branch route, got ${pointerDefaultResp.status()}`
      ).toBe(true);
      await showProofStep(page, "done");
    } finally {
      await showProofStep(page, "cleanup");
      await stopBestEffort(page.request, `/admin/sandboxes/${userId}/pointers/set`, {
        pointer_name: "main",
        target_kind: "role",
        target_value: "live",
      });
      await stopBestEffort(page.request, `/admin/sandboxes/${userId}/branches/${branch}/stop`);
      await stopBestEffort(page.request, `/admin/sandboxes/${userId}/live/stop`);
    }
  });
});
