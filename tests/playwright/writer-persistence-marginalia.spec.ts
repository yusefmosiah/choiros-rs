import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";

const BACKEND = "http://127.0.0.1:8080";
const DESKTOP_ID = "default-desktop";
const STATE_FILE = path.resolve(
  __dirname,
  "../artifacts/playwright/writer-persistence-state.json"
);

type WriterPersistenceState = {
  objectiveA: string;
  objectiveB: string;
  runA: string;
  runB: string;
  docPathA: string;
  docPathB: string;
  savedAt: string;
};

type ConductorRun = {
  run_id: string;
  objective: string;
};

async function executeRun(
  request: APIRequestContext,
  objective: string
): Promise<{ runId: string; docPath: string }> {
  const resp = await request.post(`${BACKEND}/conductor/execute`, {
    data: {
      objective,
      desktop_id: DESKTOP_ID,
      output_mode: "auto",
      hints: null,
    },
  });
  expect(resp.ok()).toBeTruthy();
  const json = await resp.json();
  expect(typeof json.run_id).toBe("string");
  expect((json.run_id as string).length).toBeGreaterThan(6);

  const runId = json.run_id as string;
  return {
    runId,
    docPath: `conductor/runs/${runId}/draft.md`,
  };
}

async function listRuns(request: APIRequestContext): Promise<ConductorRun[]> {
  const resp = await request.get(`${BACKEND}/conductor/runs`);
  expect(resp.ok()).toBeTruthy();
  return (await resp.json()) as ConductorRun[];
}

async function waitForRuns(
  request: APIRequestContext,
  runIds: string[],
  timeoutMs = 30_000
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const runs = await listRuns(request);
    const ids = new Set(runs.map((run) => run.run_id));
    if (runIds.every((id) => ids.has(id))) {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 400));
  }
  throw new Error(`Timed out waiting for runs: ${runIds.join(", ")}`);
}

async function loadPreviousState(): Promise<WriterPersistenceState | null> {
  try {
    const raw = await fs.readFile(STATE_FILE, "utf-8");
    const parsed = JSON.parse(raw) as Partial<WriterPersistenceState>;
    if (
      typeof parsed.objectiveA === "string" &&
      typeof parsed.objectiveB === "string" &&
      typeof parsed.runA === "string" &&
      typeof parsed.runB === "string" &&
      typeof parsed.docPathA === "string" &&
      typeof parsed.docPathB === "string"
    ) {
      return {
        objectiveA: parsed.objectiveA,
        objectiveB: parsed.objectiveB,
        runA: parsed.runA,
        runB: parsed.runB,
        docPathA: parsed.docPathA,
        docPathB: parsed.docPathB,
        savedAt: typeof parsed.savedAt === "string" ? parsed.savedAt : "",
      };
    }
    return null;
  } catch {
    return null;
  }
}

async function saveState(state: WriterPersistenceState): Promise<void> {
  await fs.mkdir(path.dirname(STATE_FILE), { recursive: true });
  await fs.writeFile(STATE_FILE, JSON.stringify(state, null, 2), "utf-8");
}

async function closeAllDesktopWindows(request: APIRequestContext): Promise<void> {
  const windowsResp = await request.get(`${BACKEND}/desktop/${DESKTOP_ID}/windows`);
  expect(windowsResp.ok()).toBeTruthy();
  const windowsJson = (await windowsResp.json()) as {
    success: boolean;
    windows: Array<{ id: string }>;
  };
  for (const win of windowsJson.windows) {
    await request.delete(`${BACKEND}/desktop/${DESKTOP_ID}/windows/${win.id}`);
  }
}

async function ensureWriterAppRegistered(request: APIRequestContext): Promise<void> {
  const appsResp = await request.get(`${BACKEND}/desktop/${DESKTOP_ID}/apps`);
  if (appsResp.ok()) {
    const appsJson = (await appsResp.json()) as {
      success: boolean;
      apps: Array<{ id: string }>;
    };
    if (appsJson.apps.some((app) => app.id == "writer")) {
      return;
    }
  }

  const registerResp = await request.post(`${BACKEND}/desktop/${DESKTOP_ID}/apps`, {
    data: {
      id: "writer",
      name: "Writer",
      icon: "📝",
      component_code: "WriterApp",
      default_width: 1100,
      default_height: 720,
    },
  });

  // 200 = registered, 400 = already exists due race; both are acceptable.
  expect([200, 400]).toContain(registerResp.status());
}

async function openWriterWindowForPath(
  request: APIRequestContext,
  docPath: string
): Promise<void> {
  await ensureWriterAppRegistered(request);

  let openResp = await request.post(`${BACKEND}/desktop/${DESKTOP_ID}/windows`, {
    data: {
      app_id: "writer",
      title: "Writer",
      props: { path: docPath },
    },
  });
  if (!openResp.ok()) {
    await ensureWriterAppRegistered(request);
    openResp = await request.post(`${BACKEND}/desktop/${DESKTOP_ID}/windows`, {
      data: {
        app_id: "writer",
        title: "Writer",
        props: { path: docPath },
      },
    });
  }
  expect(openResp.ok()).toBeTruthy();
}

async function verifyWriterDocumentView(page: Page, docPath: string): Promise<void> {
  const targetDialog = page
    .getByRole("dialog", { name: "Writer" })
    .filter({ has: page.getByText(docPath, { exact: false }) })
    .first();

  await expect(targetDialog).toBeVisible({ timeout: 60_000 });
  await expect(targetDialog.getByText(docPath, { exact: false }).first()).toBeVisible({
    timeout: 20_000,
  });
  await expect(targetDialog.locator(".writer-layout")).toBeVisible({ timeout: 20_000 });
  await expect(targetDialog.locator(".writer-margin-left")).toBeVisible();
  await expect(targetDialog.locator(".writer-prose-body[contenteditable='true']")).toBeVisible();
  await expect(targetDialog.locator("textarea")).toHaveCount(0);
  await expect(
    targetDialog.getByText("Live patch stream lost continuity", { exact: false })
  ).toHaveCount(0);
}

async function openWriterForPath(
  page: Page,
  request: APIRequestContext,
  docPath: string
): Promise<void> {
  await closeAllDesktopWindows(request);
  await openWriterWindowForPath(request, docPath);
  await page.goto("/");
  await verifyWriterDocumentView(page, docPath);
}

async function assertObjectivesPresent(
  request: APIRequestContext,
  objectives: string[]
): Promise<void> {
  const runs = await listRuns(request);
  const objectiveSet = new Set(runs.map((run) => run.objective));
  for (const objective of objectives) {
    expect(objectiveSet.has(objective)).toBeTruthy();
  }
}

test("writer run docs persist across reload and across repeated playwright runs", async ({
  page,
  request,
}) => {
  const previousState = await loadPreviousState();

  const marker = Date.now();
  const objectiveA = `Persistence run A ${marker}`;
  const objectiveB = `Persistence run B ${marker}`;

  const runA = await executeRun(request, objectiveA);
  const runB = await executeRun(request, objectiveB);
  await waitForRuns(request, [runA.runId, runB.runId]);

  await assertObjectivesPresent(request, [objectiveA, objectiveB]);

  let previousStillAvailable = false;
  if (previousState) {
    const runs = await listRuns(request);
    const ids = new Set(runs.map((run) => run.run_id));
    previousStillAvailable = ids.has(previousState.runA) && ids.has(previousState.runB);
  }

  await openWriterForPath(page, request, runA.docPath);
  await page.reload();
  await verifyWriterDocumentView(page, runA.docPath);

  await openWriterForPath(page, request, runB.docPath);

  if (previousState && previousStillAvailable) {
    await assertObjectivesPresent(request, [previousState.objectiveA, previousState.objectiveB]);
    await openWriterForPath(page, request, previousState.docPathA);
  }

  await saveState({
    objectiveA,
    objectiveB,
    runA: runA.runId,
    runB: runB.runId,
    docPathA: runA.docPath,
    docPathB: runB.docPath,
    savedAt: new Date().toISOString(),
  });
});
