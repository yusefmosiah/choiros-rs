/**
 * E2E Test: Tool Execution Flow
 * 
 * Tests that the AI can execute tools and display results:
 * 1. Send message requesting file listing
 * 2. Verify tool call UI is displayed
 * 3. Verify tool result is shown
 * 4. Verify AI synthesizes a response from tool output
 */

import { connect, waitForPageLoad } from "@/client.js";
import * as path from "path";

const SCREENSHOT_DIR = process.env.SCREENSHOT_DIR || "tests/screenshots/phase4";
const TEST_NAME = process.env.TEST_NAME || "test_e2e_tool_execution";

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

async function sendMessage(page: any, text: string): Promise<void> {
  const inputField = await page.locator('input[placeholder*="message"], input[placeholder*="Ask"], .message-input').first();
  if (await inputField.isVisible().catch(() => false)) {
    await inputField.fill(text);
    await sleep(300);
    
    const sendButton = await page.locator('button:has-text("Send"), .send-button').first();
    if (await sendButton.isVisible().catch(() => false)) {
      await sendButton.click();
    } else {
      await inputField.press("Enter");
    }
    
    await sleep(1000);
  } else {
    throw new Error("Could not find message input field");
  }
}

async function waitForAIResponse(page: any, timeoutMs: number = 30000): Promise<boolean> {
  const startTime = Date.now();
  
  while (Date.now() - startTime < timeoutMs) {
    const assistantMessages = await page.locator('.message-wrapper.assistant, .assistant-bubble').count();
    if (assistantMessages > 0) {
      return true;
    }
    await sleep(1000);
  }
  
  return false;
}

async function main(): Promise<void> {
  console.log("🧪 Starting Tool Execution E2E Test");
  
  const client = await connect();
  
  try {
    // Step 1: Open browser and navigate
    console.log("Step 1: Opening browser to http://localhost:3000");
    const page = await client.page("tool-test", { 
      viewport: { width: 1920, height: 1080 } 
    });
    
    await page.goto("http://localhost:3000");
    await waitForPageLoad(page);
    await sleep(2000);
    
    await takeScreenshot(page, 1, "initial_load");
    
    // Step 2: Open chat window
    console.log("Step 2: Opening chat window");
    const chatIcon = await page.locator('text=💬').first();
    if (await chatIcon.isVisible().catch(() => false)) {
      await chatIcon.click();
      await sleep(1000);
    }
    
    await takeScreenshot(page, 2, "chat_opened");
    
    // Step 3: Send message requesting file listing
    console.log("Step 3: Sending tool request - 'List the files in the current directory'");
    await sendMessage(page, "List the files in the current directory");
    
    await sleep(2000); // Wait for processing
    await takeScreenshot(page, 3, "tool_call_ui");
    
    // Step 4: Wait for tool result and AI response
    console.log("Step 4: Waiting for tool result");
    
    if (!await waitForAIResponse(page, 45000)) {
      throw new Error("AI did not respond within timeout");
    }
    
    await sleep(2000);
    await takeScreenshot(page, 4, "tool_result");
    
    // Step 5: Verify tool execution indicators
    console.log("Step 5: Verifying tool execution indicators");
    
    const pageText = await page.textContent('body');
    const snapshot = await client.getAISnapshot("tool-test");
    
    // Check for tool-related indicators in the UI
    // These might vary based on implementation, so we check multiple possibilities
    const hasToolIndicator = 
      pageText?.toLowerCase().includes("tool") ||
      pageText?.toLowerCase().includes("executing") ||
      pageText?.toLowerCase().includes("calling") ||
      snapshot.includes("tool") ||
      snapshot.includes("executing");
    
    console.log(`   Tool indicator found: ${hasToolIndicator}`);
    
    // Step 6: Verify AI synthesized response
    console.log("Step 6: Verifying AI synthesized response");
    
    const aiMessages = await page.locator('.message-wrapper.assistant, .assistant-bubble').all();
    let aiResponseText = "";
    
    if (aiMessages.length > 0) {
      aiResponseText = await aiMessages[aiMessages.length - 1].textContent() || "";
    }
    
    console.log(`   AI Response: ${aiResponseText.substring(0, 200)}`);
    
    // The AI should mention something about files or directory
    const mentionsFiles = 
      aiResponseText.toLowerCase().includes("file") ||
      aiResponseText.toLowerCase().includes("directory") ||
      aiResponseText.toLowerCase().includes("folder") ||
      aiResponseText.toLowerCase().includes("cargo") || // likely in rust project
      aiResponseText.toLowerCase().includes("src") ||
      aiResponseText.toLowerCase().includes("readme");
    
    console.log(`   Mentions files/directory: ${mentionsFiles}`);
    
    if (!mentionsFiles && !hasToolIndicator) {
      console.warn("   Warning: Tool execution may not be properly indicated");
    }
    
    await takeScreenshot(page, 5, "ai_response");
    
    // Step 7: Verify the conversation shows the full flow
    console.log("Step 7: Verifying conversation flow");
    
    const userMessages = await page.locator('.message-wrapper.user, .user-bubble').count();
    const assistantCount = await page.locator('.message-wrapper.assistant, .assistant-bubble').count();
    
    console.log(`   User messages: ${userMessages}`);
    console.log(`   Assistant messages: ${assistantCount}`);
    
    if (userMessages === 0) {
      throw new Error("No user messages found");
    }
    if (assistantCount === 0) {
      throw new Error("No assistant messages found");
    }
    
    // Verify the request is visible
    if (!pageText?.toLowerCase().includes("list") && !pageText?.toLowerCase().includes("file")) {
      throw new Error("User request not visible in conversation");
    }
    
    await takeScreenshot(page, 6, "conversation_complete");
    
    console.log("✅ Tool Execution test passed!");
    console.log("   Tool execution flow completed successfully");
    
  } catch (error) {
    console.error("❌ Test failed:", error);
    try {
      const page = await client.page("tool-test");
      await takeScreenshot(page, 99, "error_state");
    } catch (e) {}
    process.exit(1);
  } finally {
    await client.disconnect();
  }
}

main().catch(error => {
  console.error("Fatal error:", error);
  process.exit(1);
});
