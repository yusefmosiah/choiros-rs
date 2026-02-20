import { expect, Page, APIRequestContext } from "@playwright/test";

export const BACKEND = "http://127.0.0.1:8080";

export async function fetchEvents(
  request: APIRequestContext,
  prefix: string,
  limit = 300
): Promise<any[]> {
  const resp = await request.get(
    `${BACKEND}/logs/events?event_type_prefix=${encodeURIComponent(prefix)}&limit=${limit}`
  );
  expect(resp.ok()).toBeTruthy();
  const body = await resp.json();
  return body.events ?? [];
}

export async function waitForEvent(
  request: APIRequestContext,
  prefix: string,
  predicate: (event: any) => boolean,
  timeoutMs = 90_000
): Promise<any> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const events = await fetchEvents(request, prefix, 500);
    const match = events.find(predicate);
    if (match) return match;
    await new Promise((resolve) => setTimeout(resolve, 1_500));
  }
  throw new Error(`Timeout waiting for event prefix "${prefix}"`);
}

export async function triggerRun(
  request: APIRequestContext,
  objective: string,
  desktopId: string
): Promise<string | null> {
  let resp = await request.post(`${BACKEND}/conductor/execute`, {
    data: {
      objective,
      desktop_id: desktopId,
      output_mode: "markdown_report_to_writer",
    },
  });
  for (let attempt = 0; attempt < 4 && resp.status() >= 500; attempt += 1) {
    await new Promise((resolve) => setTimeout(resolve, 1_250));
    resp = await request.post(`${BACKEND}/conductor/execute`, {
      data: {
        objective,
        desktop_id: `${desktopId}-retry-${attempt}`,
        output_mode: "markdown_report_to_writer",
      },
    });
  }
  expect(resp.status()).toBeLessThan(500);
  if (!resp.ok()) return null;
  const body = await resp.json();
  return body.run_id ?? body.data?.run_id ?? null;
}

export async function openTraceWindow(page: Page): Promise<void> {
  await page.goto("/");
  const traceLauncher = page
    .locator("button, [role='button'], .desktop-icon")
    .filter({ hasText: "Trace" })
    .first();
  await expect(traceLauncher).toBeVisible({ timeout: 60_000 });
  await traceLauncher.click({ force: true });

  const traceWindowTitle = page
    .locator(".floating-window .window-titlebar span")
    .filter({ hasText: "Trace" })
    .first();
  await expect(traceWindowTitle).toBeVisible({ timeout: 60_000 });
}

export async function openFirstRunInTrace(page: Page): Promise<void> {
  await expect(page.locator(".trace-run-toggle").first()).toBeVisible({ timeout: 90_000 });
  await page.locator(".trace-run-toggle").first().click();
}
