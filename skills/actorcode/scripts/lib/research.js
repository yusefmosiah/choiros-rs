import { createClient } from "./client.js";
import { logSupervisor } from "./logs.js";

const DIRECTORY = process.cwd();

export class ResearchTask {
  constructor({
    title,
    agent = "explore",
    tier = "pico",
    prompt,
    onLearning,
    onComplete,
    onError,
    supervisorSessionId = null
  }) {
    this.title = title;
    this.agent = agent;
    this.tier = tier;
    this.prompt = prompt;
    this.onLearning = onLearning;
    this.onComplete = onComplete;
    this.onError = onError;
    this.supervisorSessionId = supervisorSessionId;
    this.sessionId = null;
    this.client = createClient();
    this.lastMessageId = null;
    this.isRunning = false;
  }

  async spawn() {
    const sessionResponse = await this.client.session.create({
      query: { directory: DIRECTORY },
      body: { title: this.title }
    });
    this.sessionId = sessionResponse.data?.id;
    
    if (!this.sessionId) {
      throw new Error("Failed to create session");
    }

    await logSupervisor(`research spawn session=${this.sessionId} title=${this.title}`);

    const fullPrompt = this.buildPromptWithReporting();
    
    await this.client.session.promptAsync({
      path: { id: this.sessionId },
      query: { directory: DIRECTORY },
      body: {
        parts: [{ type: "text", text: fullPrompt }],
        agent: this.agent
      }
    });

    this.isRunning = true;
    this.startMonitoring();
    
    return this.sessionId;
  }

  buildPromptWithReporting() {
    return [
      "You are a research subagent working incrementally.",
      "",
      "CRITICAL: Report learnings as you discover them.",
      "",
      "Reporting protocol:",
      "1. When you find something important, IMMEDIATELY send a message to your supervisor",
      this.supervisorSessionId ? `   Supervisor session: ${this.supervisorSessionId}` : "   (supervisor will monitor your session)",
      "2. Format: [LEARNING] <category>: <brief description>",
      "3. Categories: BUG, SECURITY, DOCS, REFACTOR, PERFORMANCE, ARCHITECTURE",
      "4. Continue working after reporting—don't wait for response",
      "",
      "Example:",
      "[LEARNING] SECURITY: Hardcoded API key found in src/config.rs line 45",
      "[LEARNING] BUG: Race condition in actor initialization, details in logs",
      "",
      "Task:",
      this.prompt,
      "",
      "Remember: Report early, report often. Don't hoard findings until the end."
    ].join("\n");
  }

  async startMonitoring() {
    while (this.isRunning) {
      try {
        await this.checkForLearnings();
        await this.sleep(2000);
      } catch (error) {
        if (this.onError) {
          this.onError(error);
        }
        await this.sleep(5000);
      }
    }
  }

  async checkForLearnings() {
    const messagesResponse = await this.client.session.messages({
      path: { id: this.sessionId },
      query: { directory: DIRECTORY, limit: 10 }
    });

    const messages = messagesResponse.data || [];
    
    for (const message of messages) {
      if (message.info?.role !== "assistant") continue;
      if (message.info?.id === this.lastMessageId) continue;
      
      this.lastMessageId = message.info.id;
      
      const text = this.extractText(message);
      if (!text) continue;

      const learnings = this.parseLearnings(text);
      
      for (const learning of learnings) {
        if (this.onLearning) {
          this.onLearning({
            sessionId: this.sessionId,
            category: learning.category,
            description: learning.description,
            fullText: text,
            timestamp: new Date().toISOString()
          });
        }
      }

      if (text.includes("[COMPLETE]") || text.includes("Task completed")) {
        this.isRunning = false;
        if (this.onComplete) {
          this.onComplete({
            sessionId: this.sessionId,
            finalReport: text
          });
        }
      }
    }
  }

  extractText(message) {
    const parts = message?.parts || [];
    return parts
      .filter((part) => part?.type === "text" && part?.text)
      .map((part) => part.text)
      .join("\n");
  }

  parseLearnings(text) {
    const learnings = [];
    const regex = /\[LEARNING\]\s*(\w+):\s*(.+?)(?=\[LEARNING\]|\[COMPLETE\]|$)/gs;
    let match;
    
    while ((match = regex.exec(text)) !== null) {
      learnings.push({
        category: match[1].toUpperCase(),
        description: match[2].trim()
      });
    }
    
    return learnings;
  }

  sleep(ms) {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  async abort() {
    this.isRunning = false;
    if (this.sessionId) {
      await this.client.session.abort({
        path: { id: this.sessionId },
        query: { directory: DIRECTORY }
      });
    }
  }
}

export async function runResearchTasks(tasks, options = {}) {
  const results = {
    learnings: [],
    completed: [],
    errors: []
  };

  const onLearning = (learning) => {
    results.learnings.push(learning);
    if (options.onLearning) {
      options.onLearning(learning);
    }
  };

  const onComplete = (completion) => {
    results.completed.push(completion);
    if (options.onComplete) {
      options.onComplete(completion);
    }
  };

  const onError = (error) => {
    results.errors.push(error);
    if (options.onError) {
      options.onError(error);
    }
  };

  const researchTasks = tasks.map((task) => {
    return new ResearchTask({
      ...task,
      onLearning,
      onComplete,
      onError,
      supervisorSessionId: options.supervisorSessionId
    });
  });

  await Promise.all(researchTasks.map((task) => task.spawn()));

  return results;
}
