# Prompt for choirOS BAML/Authentication Analysis

Search the ~/choirOS codebase and create a comprehensive document about how AWS Bedrock authentication is configured. Specifically:

## Files to examine:

1. **baml_src/*.baml** - All BAML configuration files
   - Look for client definitions (client<llm>)
   - Note the provider type (aws-bedrock vs anthropic vs openai-generic)
   - Note any authentication options (api_key, access_key_id, etc.)
   - Note the exact model IDs used

2. **supervisor/provider_factory.py** 
   - How does it configure Bedrock authentication?
   - What environment variables does it use?
   - How does it switch between providers?

3. **supervisor/baml_client/globals.py or async_client.py**
   - How is the BAML client initialized?
   - What environment variables are passed?

4. **supervisor/agent/harness.py**
   - How does it call BAML functions?
   - Does it use with_options to override clients?

5. **supervisor/main.py or any initialization code**
   - Where are environment variables loaded?
   - Any special AWS configuration?

## Questions to answer:

1. What provider type is used for the ClaudeBedrock client in BAML? (aws-bedrock, anthropic, or openai-generic?)

2. What is the exact model ID string used? (e.g., "us.anthropic.claude-opus-4-5-20251101-v1:0")

3. How is AWS authentication configured? Look for:
   - AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY
   - AWS_BEARER_TOKEN_BEDROCK
   - AWS_PROFILE
   - Any other AWS credential method

4. If using aws-bedrock provider, how does it authenticate with just a bearer token? (This is unusual - aws-bedrock typically needs standard AWS credentials)

5. Are they using a proxy service, custom endpoint, or native AWS Bedrock?

6. Show the complete BAML client configuration for both:
   - ClaudeBedrock (AWS)
   - ClaudeZAI or GLM (Z.ai)

7. Show any relevant Python code that configures or overrides BAML clients at runtime

## Format your response as:

```markdown
# choirOS BAML/Authentication Configuration

## BAML Client Definitions
```baml
[paste the exact client definitions from baml_src/ files]
```

## Provider Factory Configuration
```python
[paste relevant sections from provider_factory.py]
```

## BAML Client Initialization
```python
[paste relevant sections from baml_client/ or initialization code]
```

## Environment Variables Used
- AWS_BEARER_TOKEN_BEDROCK: [how it's used]
- AWS_ACCESS_KEY_ID: [if used]
- AWS_SECRET_ACCESS_KEY: [if used]
- Other relevant env vars...

## Model IDs
- Bedrock: [exact model ID]
- Z.ai: [exact model ID]

## Key Findings
[Explain how authentication actually works - is it standard AWS credentials, bearer token via some proxy, or something else?]
```

Be thorough - the goal is to understand exactly how to replicate this authentication pattern in the Rust/Dioxus version (choiros-rs).