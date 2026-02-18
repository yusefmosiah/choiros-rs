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
  use: {
    baseURL: "http://127.0.0.1:3000",
    trace: "on",
    video: "on",
    screenshot: "only-on-failure",
    viewport: {
      width: 1720,
      height: 980,
    },
  },
});
