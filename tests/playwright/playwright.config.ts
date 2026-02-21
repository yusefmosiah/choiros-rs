import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: ".",
  testMatch: ["*.spec.ts"],
  timeout: 180_000,
  expect: {
    timeout: 120_000,
  },
  reporter: [
    ["list"],
    ["html", { outputFolder: "../artifacts/playwright/html-report", open: "never" }],
  ],
  outputDir: "../artifacts/playwright/test-results",
  projects: [
    {
      name: "hypervisor",
      testMatch: ["bios-auth.spec.ts", "proxy-integration.spec.ts"],
      use: {
        baseURL: "http://localhost:9090",
        trace: "on",
        video: "on",
        screenshot: "only-on-failure",
        viewport: { width: 1280, height: 800 },
      },
    },
    {
      name: "sandbox",
      testMatch: [
        "conductor-writer.e2e.spec.ts",
        "phase1-marginalia.spec.ts",
        "phase3-citations.spec.ts",
        "phase4-subharness.spec.ts",
        "trace-viewer-phase1.spec.ts",
        "trace-viewer-phase2.spec.ts",
        "trace-viewer-phase3.spec.ts",
        "trace-viewer-phase4.spec.ts",
        "writer-persistence-marginalia.spec.ts",
        "weather-delegation.e2e.spec.ts",
      ],
      use: {
        baseURL: "http://127.0.0.1:3000",
        trace: "on",
        video: "on",
        screenshot: "only-on-failure",
        viewport: { width: 1720, height: 980 },
      },
    },
    {
      name: "trace-eval",
      testMatch: ["trace-viewer-eval.spec.ts"],
      use: {
        baseURL: "http://127.0.0.1:3000",
        trace: "on",
        video: "on",
        screenshot: "on",
        viewport: { width: 1720, height: 980 },
      },
    },
  ],
});
