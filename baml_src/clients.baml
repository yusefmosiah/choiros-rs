// ChoirOS Chat Agent Clients
// Semantic client names for role-based model routing.
// Actual providers are configured at runtime via ClientRegistry.

// Orchestrator: High-quality model for complex reasoning and orchestration
client<llm> Orchestrator {
  provider aws-bedrock
  retry_policy Exponential
  options {
    model "us.anthropic.claude-opus-4-5-20251101-v1:0"
    region "us-east-1"
    // No explicit auth needed - provider auto-detects AWS_BEARER_TOKEN_BEDROCK
  }
}

// FastResponse: Fast/cheap model for quick responses and high-volume tasks
client<llm> FastResponse {
  provider anthropic
  retry_policy Exponential
  options {
    api_key env.ZAI_API_KEY
    base_url "https://api.z.ai/api/anthropic"
    model "glm-4.7"
  }
}

// Retry policies
retry_policy Exponential {
  max_retries 2
  strategy {
    type exponential_backoff
    delay_ms 300
    multiplier 1.5
    max_delay_ms 10000
  }
}
