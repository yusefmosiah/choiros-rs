// ChoirOS Chat Agent Clients
// Define both model providers: ClaudeBedrock (AWS) and GLM47 (Z.ai)

// AWS Bedrock Claude configuration
// Authentication: Automatically reads AWS_BEARER_TOKEN_BEDROCK from environment
// AWS introduced API key auth in July 2025 - BAML's aws-bedrock provider supports this natively
client<llm> ClaudeBedrock {
  provider aws-bedrock
  retry_policy Exponential
  options {
    model "us.anthropic.claude-opus-4-5-20251101-v1:0"
    region "us-east-1"
    // No explicit auth needed - provider auto-detects AWS_BEARER_TOKEN_BEDROCK
  }
}

// Z.ai GLM4.7 configuration using Anthropic-compatible API
client<llm> GLM47 {
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
