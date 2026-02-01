/**
 * E2E Test: Concurrent Users
 * 
 * Tests conversation isolation between different users/actors:
 * 1. Open 2 browser instances with different actor IDs
 * 2. Send message in Browser A
 * 3. Verify message does NOT appear in Browser B
 * 4. Send different message in Browser B
 * 5. Verify conversations are isolated
 */

import { connect, waitForPageLoad } from "@/client.js";
import * as path from "path";

const SCREENSHOT_DIR = process.env.SCREENSHOT_DIR || "tests/screenshots/phase4";
const TEST_NAME = process.env.TEST_NAME || "test_e2e_concurrent_users";

async function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

async function takeScreenshot(page: any, browser: string, step: number, description: string): Promise<string> {
  const filename = `${TEST_NAME}_${browser}_step${step}_${description}.png`;
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

async function openChat(page: any): Promise<void> {
  const chatIcon = await page.locator('text=💬').first();
  if (await chatIcon.isVisible().catch(() => false)) {
    await chatIcon.click();
    await sleep(1000);
  }
}

async function main(): Promise<void> {
  console.log("🧪 Starting Concurrent Users E2E Test");
  
  const client = await connect();
  
  try {
    // Step 1: Open Browser A
    console.log("Step 1: Opening Browser A (Actor A)");
    const pageA = await client.page("concurrent-A", { 
      viewport: { width: 1280, height: 900 } 
    });
    
    // Navigate with unique actor ID via query parameter if supported
    await pageA.goto("http://localhost:3000?actor=actor-a");
    await waitForPageLoad(pageA);
    await sleep(2000);
    
    await takeScreenshot(pageA, "A", 1, "browser_a_load");
    
    // Step 2: Open Browser B
    console.log("Step 2: Opening Browser B (Actor B)");
    const pageB = await client.page("concurrent-B", { 
      viewport: { width: 1280, height: 900 } 
    });
    
    await pageB.goto("http://localhost:3000?actor=actor-b");
    await waitForPageLoad(pageB);
    await sleep(2000);
    
    await takeScreenshot(pageB, "B", 1, "browser_b_load");
    
    // Step 3: Open chat in both browsers
    console.log("Step 3: Opening chat in both browsers");
    await openChat(pageA);
    await openChat(pageB);
    await sleep(1000);
    
    await takeScreenshot(pageA, "A", 2, "chat_opened");
    await takeScreenshot(pageB, "B", 2, "chat_opened");
    
    // Step 4: Send message in Browser A
    console.log("Step 4: Sending message in Browser A");
    const messageA = "This is a message from User A - should only appear here";
    await sendMessage(pageA, messageA);
    await sleep(3000);
    
    await takeScreenshot(pageA, "A", 3, "message_sent");
    
    // Step 5: Verify message in Browser A
    console.log("Step 5: Verifying message in Browser A");
    const textA = await pageA.textContent('body');
    const hasMessageInA = textA?.includes(messageA);
    
    console.log(`   Message visible in Browser A: ${hasMessageInA}`);
    
    if (!hasMessageInA) {
      throw new Error("Message not found in Browser A after sending");
    }
    
    // Step 6: Verify message does NOT appear in Browser B
    console.log("Step 6: Verifying message isolation in Browser B");
    const textB = await pageB.textContent('body');
    const hasMessageInB = textB?.includes(messageA);
    
    console.log(`   Message visible in Browser B: ${hasMessageInB}`);
    
    if (hasMessageInB) {
      throw new Error("CRITICAL: Message from User A appeared in User B's conversation - no isolation!");
    }
    
    await takeScreenshot(pageB, "B", 3, "message_isolated");
    
    // Step 7: Send different message in Browser B
    console.log("Step 7: Sending message in Browser B");
    const messageB = "This is a message from User B - should only appear here";
    await sendMessage(pageB, messageB);
    await sleep(3000);
    
    await takeScreenshot(pageB, "B", 4, "message_sent");
    
    // Step 8: Verify Browser B's message
    console.log("Step 8: Verifying Browser B's message");
    const textBAfter = await pageB.textContent('body');
    const hasOwnMessageInB = textBAfter?.includes(messageB);
    
    console.log(`   Browser B's own message visible: ${hasOwnMessageInB}`);
    
    if (!hasOwnMessageInB) {
      throw new Error("Browser B's message not found in its own conversation");
    }
    
    // Step 9: Verify Browser B's message doesn't leak to Browser A
    console.log("Step 9: Verifying Browser B's message doesn't leak to Browser A");
    const textAAfter = await pageA.textContent('body');
    const hasMessageBInA = textAAfter?.includes(messageB);
    
    console.log(`   Browser B's message in Browser A: ${hasMessageBInA}`);
    
    if (hasMessageBInA) {
      throw new Error("CRITICAL: Message from User B appeared in User A's conversation - no isolation!");
    }
    
    await takeScreenshot(pageA, "A", 4, "conversation_isolated");
    
    // Step 10: Verify both conversations have correct message counts
    console.log("Step 10: Verifying conversation integrity");
    
    const userMessagesA = await pageA.locator('.message-wrapper.user, .user-bubble').count();
    const userMessagesB = await pageB.locator('.message-wrapper.user, .user-bubble').count();
    
    console.log(`   Browser A user messages: ${userMessagesA}`);
    console.log(`   Browser B user messages: ${userMessagesB}`);
    
    // Each should have only their own message(s)
    if (userMessagesA < 1) {
      throw new Error("Browser A has no user messages");
    }
    if (userMessagesB < 1) {
      throw new Error("Browser B has no user messages");
    }
    
    // Step 11: Additional verification - content isolation
    console.log("Step 11: Additional content isolation checks");
    
    // Count unique messages in each browser
    const allMessagesA = await pageA.locator('.message-wrapper.user, .user-bubble').all();
    const allMessagesB = await pageB.locator('.message-wrapper.user, .user-bubble').all();
    
    let crossContamination = false;
    
    for (const msgA of allMessagesA) {
      const text = await msgA.textContent() || "";
      if (text.includes("User B") || text.includes(messageB)) {
        crossContamination = true;
        console.error(`   CROSS-CONTAMINATION: Found User B message in Browser A: ${text.substring(0, 50)}`);
      }
    }
    
    for (const msgB of allMessagesB) {
      const text = await msgB.textContent() || "";
      if (text.includes("User A") || text.includes(messageA)) {
        crossContamination = true;
        console.error(`   CROSS-CONTAMINATION: Found User A message in Browser B: ${text.substring(0, 50)}`);
      }
    }
    
    if (crossContamination) {
      throw new Error("CRITICAL: Cross-contamination detected between user conversations!");
    }
    
    await takeScreenshot(pageA, "A", 5, "final_state");
    await takeScreenshot(pageB, "B", 5, "final_state");
    
    console.log("✅ Concurrent Users test passed!");
    console.log("   ✓ Conversations are properly isolated");
    console.log("   ✓ No cross-contamination between actors");
    console.log("   ✓ Each user sees only their own messages");
    
  } catch (error) {
    console.error("❌ Test failed:", error);
    try {
      const pageA = await client.page("concurrent-A");
      const pageB = await client.page("concurrent-B");
      await takeScreenshot(pageA, "A", 99, "error_state");
      await takeScreenshot(pageB, "B", 99, "error_state");
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
