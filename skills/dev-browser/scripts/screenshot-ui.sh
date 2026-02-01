#!/usr/bin/env bash
# Screenshot the running UI at localhost:3000

cd /Users/wiz/choiros-rs/skills/dev-browser

npx tsx <<'EOF'
import { connect, waitForPageLoad } from "@/client.js";

const client = await connect();
const page = await client.page("ui", { viewport: { width: 1280, height: 800 } });

console.log("Navigating to http://localhost:3000...");
await page.goto("http://localhost:3000");
await waitForPageLoad(page);

console.log("Taking screenshot...");
await page.screenshot({ path: "tmp/choiros-ui.png", fullPage: true });

console.log("Screenshot saved to: skills/dev-browser/tmp/choiros-ui.png");
console.log({ title: await page.title(), url: page.url() });

await client.disconnect();
EOF
