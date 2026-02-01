/**
 * E2E Test: Basic Chat Flow
 * 
 * Tests the fundamental chat interaction:
 * 1. Load the page
 * 2. Open chat window
 * 3. Send a message
 * 4. Verify AI response
 */

import { connect, waitForPageLoad } from "@/client.js";
import * as path from "path";

const SCREENSHOT_DIR = process.env.SCREENSHOT_DIR || "tests/screenshots/phase4";
const TEST_NAME = process.env.TEST_NAME || "test_e2e_basic_chat_flow";

async function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

async function takeScreenshot(page: any, step: number, description: string): Promise<string> {
  const filename = `${TEST_NAME}_step${step}_${description}.png`;
  const filepath = path.join(SCREENSHOT_DIR, filename);
  await page.screenshot({ path: filepath, fullPage: true });
  console.log(`SCREENSHOT: ${filepath}`);
  return filepath;
}

async function main(): Promise<void> {
  console.log("🧪 Starting Basic Chat Flow E2E Test");
  
  const client = await connect();
  
  try {
    // Step 1: Create page and navigate to frontend
    console.log("Step 1: Opening browser to http://localhost:3000");
    const page = await client.page("chat-test", { 
      viewport: { width: 1920, height: 1080 } 
    });
    
    await page.goto("http://localhost:3000");
    await waitForPageLoad(page);
    await sleep(2000); // Wait for desktop to load
    
    await takeScreenshot(page, 1, "initial_load");
    
    // Step 2: Click on chat icon to open chat window
    console.log("Step 2: Opening chat window");
    const snapshot = await client.getAISnapshot("chat-test");
    
    // Find and click chat icon (💬)
    const chatIcon = await page.locator('text=💬').first();
    if (await chatIcon.isVisible().catch(() => false)) {
      await chatIcon.click();
      await sleep(1000);
    } else {
      // Try to find by button with Chat text
      const chatButton = await page.locator('button:has-text("Chat")').first();
      if (await chatButton.isVisible().catch(() => false)) {
        await chatButton.click();
        await sleep(1000);
      }
    }
    
    await takeScreenshot(page, 2, "chat_window_opened");
    
    // Step 3: Type message in the input field
    console.log("Step 3: Typing message");
    const messageText = "Hello, can you hear me?";
    
    // Try to find input field
    const inputField = await page.locator('input[placeholder*="message"], input[placeholder*="Ask"], .message-input').first();
    if (await inputField.isVisible().catch(() => false)) {
      await inputField.fill(messageText);
      await sleep(500);
      await takeScreenshot(page, 3, "message_typed");
      
      // Step 4: Send the message
      console.log("Step 4: Sending message");
      
      // Try to find send button or press Enter
      const sendButton = await page.locator('button:has-text("Send"), .send-button').first();
      if (await sendButton.isVisible().catch(() => false)) {
        await sendButton.click();
      } else {
        await inputField.press("Enter");
      }
      
      await sleep(1000);
      await takeScreenshot(page, 4, "message_sent");
      
      // Step 5: Wait for AI response
      console.log("Step 5: Waiting for AI response (timeout: 30s)");
      let responseReceived = false;
      const startTime = Date.now();
      const timeout = 30000; // 30 seconds
      
      while (Date.now() - startTime < timeout) {
        // Check for assistant message
        const assistantMessages = await page.locator('.message-wrapper.assistant, .assistant-bubble').count();
        if (assistantMessages > 0) {
          responseReceived = true;
          break;
        }
        await sleep(1000);
      }
      
      await takeScreenshot(page, 5, "ai_response");
      
      if (!responseReceived) {
        throw new Error("AI response not received within timeout");
      }
      
      // Step 6: Verify conversation has both messages
      console.log("Step 6: Verifying conversation");
      const userMessages = await page.locator('.message-wrapper.user, .user-bubble').count();
      const aiMessages = await page.locator('.message-wrapper.assistant, .assistant-bubble').count();
      
      console.log(`   User messages: ${userMessages}`);
      console.log(`   AI messages: ${aiMessages}`);
      
      if (userMessages === 0) {
        throw new Error("No user messages found");
      }
      if (aiMessages === 0) {
        throw new Error("No AI messages found");
      }
      
      // Verify the user message text is visible
      const pageText = await page.textContent('body');
      if (!pageText?.includes(messageText)) {
        throw new Error(`User message "${messageText}" not found in conversation`);
      }
      
      await takeScreenshot(page, 6, "conversation_verified");
      
      console.log("✅ Basic Chat Flow test passed!");
    } else {
      throw new Error("Could not find message input field");
    }
    
  } catch (error) {
    console.error("❌ Test failed:", error);
    // Take failure screenshot
    try {
      const page = await client.page("chat-test");
      await takeScreenshot(page, 99, "error_state");
    } catch (e) {
      // Ignore screenshot errors
    }
    process.exit(1);
  } finally {
    await client.disconnect();
  }
}

main().catch(error => {
  console.error("Fatal error:", error);
  process.exit(1);
});
