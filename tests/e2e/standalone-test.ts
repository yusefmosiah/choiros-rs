/**
 * Standalone E2E Test Runner for ChoirOS Phase 4
 * 
 * This script can be run directly without the Rust orchestrator:
 *   cd /Users/wiz/choiros-rs && bash -c "cd skills/dev-browser && npx tsx ../../tests/e2e/standalone-test.ts"
 * 
 * Prerequisites:
 *   - Backend server running on :8080
 *   - Frontend server running on :3000
 *   - Browser automation server running
 */

import { connect, waitForPageLoad } from "@/client.js";
import * as path from "path";
import * as fs from "fs";

const SCREENSHOT_DIR = process.env.SCREENSHOT_DIR || "tests/screenshots/phase4";

// Ensure screenshot directory exists
const fullScreenshotDir = path.join("/Users/wiz/choiros-rs", SCREENSHOT_DIR);
if (!fs.existsSync(fullScreenshotDir)) {
  fs.mkdirSync(fullScreenshotDir, { recursive: true });
}

async function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

async function takeScreenshot(page: any, testName: string, step: number, description: string): Promise<string> {
  const filename = `${testName}_step${step}_${description}.png`;
  const filepath = path.join(fullScreenshotDir, filename);
  await page.screenshot({ path: filepath, fullPage: true });
  console.log(`✅ Screenshot: ${filepath}`);
  return filepath;
}

async function sendMessage(page: any, text: string): Promise<void> {
  // Try multiple selector strategies
  const inputSelectors = [
    'input[placeholder*="message" i]',
    'input[placeholder*="Ask" i]', 
    'input[placeholder*="Type" i]',
    '.message-input',
    'input[type="text"]'
  ];
  
  let inputField = null;
  for (const selector of inputSelectors) {
    const field = await page.locator(selector).first();
    if (await field.isVisible().catch(() => false)) {
      inputField = field;
      break;
    }
  }
  
  if (!inputField) {
    throw new Error("Could not find message input field");
  }
  
  await inputField.fill(text);
  await sleep(300);
  
  // Try send button or Enter key
  const sendButton = await page.locator('button:has-text("Send"), .send-button').first();
  if (await sendButton.isVisible().catch(() => false)) {
    await sendButton.click();
  } else {
    await inputField.press("Enter");
  }
  
  await sleep(1000);
}

async function openChat(page: any): Promise<boolean> {
  // Try to find and click chat icon
  const chatSelectors = [
    'text=💬',
    'button:has-text("Chat")',
    '[data-app-id="chat"]',
    '.desktop-icon:has-text("Chat")'
  ];
  
  for (const selector of chatSelectors) {
    const element = await page.locator(selector).first();
    if (await element.isVisible().catch(() => false)) {
      await element.click();
      await sleep(1000);
      return true;
    }
  }
  
  return false;
}

async function runQuickTest(): Promise<boolean> {
  console.log("🚀 Starting quick E2E smoke test...\n");
  
  const client = await connect();
  const testName = "quick_smoke_test";
  
  try {
    // Step 1: Navigate to app
    console.log("Step 1: Navigating to http://localhost:3000");
    const page = await client.page("quick-test", { 
      viewport: { width: 1920, height: 1080 } 
    });
    
    await page.goto("http://localhost:3000");
    await waitForPageLoad(page);
    await sleep(3000);
    
    await takeScreenshot(page, testName, 1, "app_loaded");
    console.log("   ✅ App loaded successfully");
    
    // Step 2: Open chat
    console.log("\nStep 2: Opening chat window");
    const chatOpened = await openChat(page);
    if (!chatOpened) {
      console.log("   ⚠️  Could not find chat icon - chat may already be open or UI different");
    } else {
      console.log("   ✅ Chat window opened");
    }
    await sleep(1000);
    await takeScreenshot(page, testName, 2, "chat_opened");
    
    // Step 3: Send test message
    console.log("\nStep 3: Sending test message");
    const testMessage = "Hello from E2E test!";
    await sendMessage(page, testMessage);
    console.log(`   ✅ Message sent: "${testMessage}"`);
    await takeScreenshot(page, testName, 3, "message_sent");
    
    // Step 4: Wait for response
    console.log("\nStep 4: Waiting for AI response (max 30s)...");
    const startTime = Date.now();
    let responseFound = false;
    
    while (Date.now() - startTime < 30000) {
      // Check for assistant message
      const assistantMessages = await page.locator('.message-wrapper.assistant, .assistant-bubble').count();
      if (assistantMessages > 0) {
        responseFound = true;
        break;
      }
      
      // Also check for any AI response indicators
      const pageText = await page.textContent('body');
      if (pageText && !pageText.includes("Sending...")) {
        // Message was sent and is no longer pending
        const aiElements = await page.locator('.assistant, .ai, .bot').count();
        if (aiElements > 0) {
          responseFound = true;
          break;
        }
      }
      
      await sleep(1000);
    }
    
    await sleep(2000);
    await takeScreenshot(page, testName, 4, "response_received");
    
    if (responseFound) {
      console.log("   ✅ AI response received");
    } else {
      console.log("   ⚠️  No AI response detected (may be slow or different UI)");
    }
    
    // Step 5: Verify basic functionality
    console.log("\nStep 5: Verifying basic functionality");
    const pageText = await page.textContent('body');
    const hasUserMessage = pageText?.includes(testMessage);
    
    console.log(`   User message visible: ${hasUserMessage ? '✅' : '❌'}`);
    console.log(`   AI responded: ${responseFound ? '✅' : '⚠️'}`);
    
    if (!hasUserMessage) {
      throw new Error("User message not found in conversation");
    }
    
    await takeScreenshot(page, testName, 5, "test_complete");
    
    console.log("\n✅ Quick smoke test completed successfully!");
    console.log(`\n📸 Screenshots saved to: ${fullScreenshotDir}`);
    return true;
    
  } catch (error) {
    console.error("\n❌ Test failed:", error);
    return false;
  } finally {
    await client.disconnect();
  }
}

// Run the test
runQuickTest().then(success => {
  process.exit(success ? 0 : 1);
}).catch(error => {
  console.error("Fatal error:", error);
  process.exit(1);
});
