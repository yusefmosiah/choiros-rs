# Researcher Search Dual-Interface Runbook

## Executive Summary (1-minute read)

This document defines the architecture for integrating Tavily, Brave Search API, and Exa into ChoirOS's BAML-based research capability. The design supports **two invocation paths**:

1. **Actor-level delegation** (`uactor -> actor`): Universal actors (Chat, Desktop) send intent-based requests to Researcher via actor messages
2. **Tool-level invocation** (`appactor -> toolactor`): Researcher actor internally executes provider-specific tools with typed schemas

Both paths emit observability events to EventBus, are typed via BAML, and support policy enforcement. Researcher owns web search capability; Chat cannot call web APIs directly.

**What Changed**: This runbook consolidates provider specifications, dual-interface contracts, and implementation steps into a single reference for production rollout.

**What To Do Next**: Follow the Step-by-Step Implementation Checklist to add BAML tools, actor messages, and event emissions in the order specified.

---

## Dual-Interface Architecture

### A. `uactor -> actor` Contract (Delegation Envelope)

**Purpose**: Intent-based research requests from universal actors (Chat, Desktop, Terminal) to Researcher actor.

**Request Message** (`ResearcherTask`):

```rust
pub struct ResearcherTask {
    pub session_id: SessionId,
    pub thread_id: ThreadId,
    pub query: String,           // Natural language research objective
    pub scope: ResearchScope,     // Optional: domains, time_range, result_count
    pub budget: ResearchBudget?,  // Optional: max_cost, max_results, timeout_ms
    pub provider_preference: ProviderPreference?, // Optional: provider or "auto"
}

pub enum ResearchScope {
    General,
    News { time_range: TimeRange },
    Specific {
        include_domains: Vec<String>,
        exclude_domains: Vec<String>,
        freshness: Option<TimeRange>,
    },
}

pub struct ResearchBudget {
    pub max_cost_usd: Option<f64>,
    pub max_results: u32,
    pub timeout_ms: u64,
}

pub enum ProviderPreference {
    Auto,                           // Researcher chooses best provider
    Tavily,
    Brave,
    Exa,
}
```

**Response Message** (`ResearcherTaskResult`):

```rust
pub struct ResearcherTaskResult {
    pub session_id: SessionId,
    pub thread_id: ThreadId,
    pub findings: NormalizedFindings,
    pub execution_metadata: ExecutionMetadata,
}

pub struct NormalizedFindings {
    pub summary: String,              // AI-generated synthesis
    pub citations: Vec<Citation>,     // Normalized citations
    pub provider_used: ProviderName,
    pub raw_results_count: u32,
}

pub struct Citation {
    pub id: String,                   // Unique identifier (URL-based)
    pub title: String,
    pub url: String,
    pub snippet: String,              // Relevant excerpt
    pub published_date: Option<DateTime<Utc>>,
    pub score: Option<f32>,           // Provider relevance score (0-1)
    pub source_provider: ProviderName,
}

pub struct ExecutionMetadata {
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub duration_ms: u64,
    pub tool_calls: Vec<ToolCallEvent>,
    pub total_cost_usd: f64,
    pub errors: Vec<String>,
}

pub struct ToolCallEvent {
    pub provider: ProviderName,
    pub endpoint: String,
    pub latency_ms: u64,
    pub result_count: u32,
    pub succeeded: bool,
    pub error_message: Option<String>,
}
```

**Events Emitted** (all via EventBus):

| Event Name | When | Payload |
|-----------|------|---------|
| `researcher.task.started` | Researcher receives task | `{ session_id, thread_id, query, provider_preference }` |
| `researcher.task.progress` | Tool call completes | `{ session_id, thread_id, provider, result_count, latency_ms }` |
| `researcher.task.completed` | Research returns findings | `{ session_id, thread_id, findings, execution_metadata }` |
| `researcher.task.failed` | Research aborts on error | `{ session_id, thread_id, error, partial_findings }` |

**Why This Layer Exists**:
- Universal actors don't need provider knowledge
- Policy enforcement happens at delegation boundary (budget checks, scope limits)
- Observability is unified across all research requests regardless of provider
- Enables fallback/routing logic within Researcher without leaking to caller

---

### B. `appactor -> toolactor` Contract (Typed Tool Call)

**Purpose**: Researcher actor internally executes provider-specific BAML tools with typed arguments.

**Tool Definition Pattern** (per provider):

```baml
// Tavily Search Tool
class TavilySearchArgs {
    provider "tavily"
    query string @description("Search query, max 400 chars")
    search_depth "basic" | "advanced" | "fast" | "ultra-fast" @default("basic")
    max_results int @range(1, 20) @default(5)
    topic "general" | "news" | "finance" @default("general")
    time_range string? @description("day, week, month, year, d, w, m, y")
    start_published_date string? @description("YYYY-MM-DD")
    end_published_date string? @description("YYYY-MM-DD")
    include_answer bool @default(false)
    include_raw_content bool @default(false)
    include_domains string[]? @max_length(300)
    exclude_domains string[]? @max_length(150)
}

// Brave Search Tool
class BraveSearchArgs {
    provider "brave"
    q string @description("Search query")
    count int @range(1, 20) @default(20)
    offset int @range(0, 9) @default(0)
    country string? @description("2-letter ISO country code")
    search_lang string @default("en") @description("ISO 639-1 language code")
    freshness string? @description("pd, pw, pm, py, or date range like 2022-04-01to2022-07-30")
    extra_snippets bool @default(false) @description("Get up to 5 additional snippets")
}

// Exa Search Tool
class ExaSearchArgs {
    provider "exa"
    query string @description("Search query string")
    type "neural" | "fast" | "auto" | "deep" @default("auto")
    num_results int @range(1, 100) @default(10)
    category string? @description("company, research paper, news, tweet, personal site, financial report, people")
    include_domains string[]? @max_length(1200)
    exclude_domains string[]? @max_length(1200)
    start_published_date string? @description("ISO 8601 datetime")
    end_published_date string? @description("ISO 8601 datetime")
    include_text bool @default(true) @description("Extract page text")
    max_age_hours int? @description("Cache max age, 0=always livecrawl, -1=never livecrawl")
}
```

**Normalized Tool Result Schema**:

```baml
class NormalizedSearchResult {
    provider "tavily" | "brave" | "exa"
    request_id string @description("Provider request ID for debugging")
    results SearchResult[]
    cost_usd float? @description("Actual cost from provider, if available")
}

class SearchResult {
    title string
    url string
    snippet string
    published_date string? @description("ISO 8601 datetime if available")
    score float? @description("Provider relevance score, 0-1")
    author string? @description("Content author, if available")
    favicon_url string? @description("Favicon URL, if available")
    image_url string? @description("Page image, if available")
    extra_snippets string[]? @description("Additional excerpts, if available")
    highlights string[]? @description("LLM-extracted relevant snippets (Exa)")
    highlight_scores float[]? @description("Cosine similarity scores (Exa)")
}
```

**Events Emitted** (internal tool-level):

| Event Name | When | Payload |
|-----------|------|---------|
| `researcher.search.started` | Tool call initiated | `{ provider, args, session_id, thread_id }` |
| `researcher.search.completed` | Tool call succeeds | `{ provider, result_count, latency_ms, cost_usd, session_id, thread_id }` |
| `researcher.search.failed` | Tool call fails | `{ provider, error_code, error_message, session_id, thread_id }` |

**Why This Layer Exists**:
- Strong typing catches provider-specific schema errors at BAML compile time
- Normalized result schema enables unified citation formatting
- Provider-specific logic (auth, headers, base URLs) isolated in BAML client config
- Enables provider switching/routing without changing Researcher core logic
- Granular observability per tool call for debugging and cost tracking

---

## Provider Matrix

### Tavily

**Endpoint**: `POST https://api.tavily.com/search`  
**Docs**: https://docs.tavily.com/docs/tavily-api/rest/api

| Aspect | Details |
|--------|---------|
| **Auth Header** | `Authorization: Bearer tvly-YOUR_API_KEY` |
| **Env Var** | `TAVILY_API_KEY` |
| **Required Params** | `query` (string, max 400 chars) |
| **Key Optional Params** | `search_depth` (enum: basic/advanced/fast/ultra-fast), `max_results` (0-20), `topic` (general/news/finance), `time_range` (day/week/month/year), `include_answer` (bool), `include_raw_content` (bool), `include_domains` (array, max 300), `exclude_domains` (array, max 150) |
| **Pagination** | No pagination; controlled via `max_results` (max 20) |
| **Error Semantics** | HTTP codes: 400 (bad request), 401 (unauthorized), 429 (rate limit), 432/433 (plan limits), 500 (internal). Response: `{ "detail": { "error": "..." } }` |
| **Citation Fields** | `results[].url`, `results[].title`, `results[].content`, `results[].score` (0-1), `results[].published_date` (topic=news only), `request_id` |
| **Rate Limits** | Dev: 100 RPM, Prod: 1000 RPM. Costs: basic/fast/ultra-fast = 1 credit, advanced = 2 credits. Free: 1,000 credits/month |
| **Unique Features** | Built-in LLM answer generation (`include_answer`), image search (`include_images`), date filtering, domain filtering, country boosting |
| **Cost** | Pay-as-you-go: $0.008/credit. Monthly plans: $0.0075-$0.005/credit |

---

### Brave Search API

**Endpoint**: `GET/POST https://api.search.brave.com/res/v1/web/search`  
**Docs**: https://api.search.brave.com/app/documentation

| Aspect | Details |
|--------|---------|
| **Auth Header** | `X-Subscription-Token: YOUR_API_KEY` |
| **Env Var** | `BRAVE_API_KEY` |
| **Required Params** | `q` (string, search query) |
| **Key Optional Params** | `count` (1-20), `offset` (0-9), `country` (2-letter code), `search_lang` (ISO 639-1), `freshness` (pd/pw/pm/py or date range), `extra_snippets` (bool), `safesearch` (off/moderate/strict) |
| **Pagination** | `count` (max 20) + `offset` (max 9 pages). Check `query.more_results_available` before next page |
| **Error Semantics** | HTTP 401 (unauthorized), 429 (rate limit). Headers: `X-RateLimit-Limit`, `X-RateLimit-Remaining`, `X-RateLimit-Reset` |
| **Citation Fields** | `web.results[].url`, `web.results[].title`, `web.results[].description`, `web.results[].age` (ISO 8601), `web.results[].extra_snippets[]`, `web.results[].profile.name` |
| **Rate Limits** | Free: 1/sec, 2,000/month. Base: up to 20/sec, 20M/month. Pro: up to 50/sec, unlimited |
| **Unique Features** | Rich result callbacks (Pro), POI local search (Pro), extra snippets, custom re-ranking via goggles, spellcheck |
| **Cost** | Free tier available. Paid plans: Base/Pro/Enterprise pricing on dashboard |

---

### Exa

**Endpoint**: `POST https://api.exa.ai/search`  
**Docs**: https://exa.ai/docs/reference/search-quickstart

| Aspect | Details |
|--------|---------|
| **Auth Header** | `x-api-key: YOUR_API_KEY` OR `Authorization: Bearer YOUR_API_KEY` (both equivalent) |
| **Env Var** | `EXA_API_KEY` |
| **Required Params** | `query` (string) |
| **Key Optional Params** | `type` (neural/fast/auto/deep), `numResults` (1-100), `category` (company/research paper/news/tweet/personal site/financial report/people), `includeDomains` (max 1200), `excludeDomains` (max 1200), `startPublishedDate`/`endPublishedDate` (ISO 8601), `contents` (text/highlights/summary options) |
| **Pagination** | No pagination; controlled via `numResults` (max 100). For larger datasets, use Websets API |
| **Error Semantics** | HTTP codes: 400 (invalid request), 401 (invalid API key), 402 (billing), 403 (access denied), 429 (rate limit), 500 (internal). Response: `{ "requestId", "error", "tag" }` |
| **Citation Fields** | `results[].url`, `results[].title`, `results[].id`, `results[].author`, `results[].publishedDate` (ISO 8601), `results[].highlights[]`, `results[].highlightScores[]`, `results[].text` (if requested) |
| **Rate Limits** | `/search`: 5 QPS, `/contents`: 50 QPS, `/answer`: 5 QPS, `/research`: 15 concurrent tasks. Enterprise limits available |
| **Unique Features** | Neural embeddings search, deep search with query expansion, LLM-highlighted snippets, content extraction (text/highlights/summary), freshness control via `maxAgeHours`, categories for domain-specific search |
| **Cost** | Neural search (1-25 results): $0.005, (26-100): $0.025, (100+): $1.00. Deep search: 3x neural cost. Content per page: $0.001 |

---

## Typed Schemas

### Provider-Specific Typed Args

```baml
// ===== Tavily =====
class TavilySearchArgs {
    provider "tavily"
    query string @description("Search query, max 400 chars recommended")
    search_depth "basic" | "advanced" | "fast" | "ultra-fast" @default("basic")
    max_results int @range(1, 20) @default(5)
    topic "general" | "news" | "finance" @default("general")
    time_range string? @description("Relative time filter: day, week, month, year, d, w, m, y")
    start_published_date string? @description("Absolute date filter: YYYY-MM-DD")
    end_published_date string? @description("Absolute date filter: YYYY-MM-DD")
    include_answer bool @default(false) @description("Include LLM-generated answer")
    include_raw_content bool @default(false) @description("Include full page content")
    include_images bool @default(false) @description("Perform image search")
    include_favicon bool @default(false) @description("Include favicon URLs")
    include_domains string[]? @max_length(300) @description("Restrict to specific domains")
    exclude_domains string[]? @max_length(150) @description("Exclude specific domains")
    country string? @description("2-letter country code for result boosting")
}

// ===== Brave =====
class BraveSearchArgs {
    provider "brave"
    q string @description("Search query")
    count int @range(1, 20) @default(20)
    offset int @range(0, 9) @default(0) @description("Page offset, max 9 pages")
    country string? @description("2-letter ISO country code")
    search_lang string @default("en") @description("Content language (ISO 639-1)")
    ui_lang string? @description("UI language for response")
    freshness string? @description("Time filter: pd (past day), pw (past week), pm (past month), py (past year), or date range like 2022-04-01to2022-07-30")
    safesearch "off" | "moderate" | "strict" @default("moderate") @description("Content filtering")
    extra_snippets bool @default(false) @description("Get up to 5 additional snippets per result")
    spellcheck int @range(0, 1) @default(1) @description("Enable/disable spellcheck")
    goggles string? @description("Custom re-ranking URL or definition")
}

// ===== Exa =====
class ExaSearchArgs {
    provider "exa"
    query string @description("Search query string")
    type "neural" | "fast" | "auto" | "deep" @default("auto") @description("Search type: neural=embeddings, fast=low-latency, auto=smart mix, deep=comprehensive")
    num_results int @range(1, 100) @default(10) @description("Number of results to return")
    category string? @description("Domain category: company, research paper, news, tweet, personal site, financial report, people")
    include_domains string[]? @max_length(1200) @description("Restrict to specific domains")
    exclude_domains string[]? @max_length(1200) @description("Exclude specific domains")
    start_published_date string? @description("ISO 8601 datetime: minimum publish date")
    end_published_date string? @description("ISO 8601 datetime: maximum publish date")
    include_text bool @default(true) @description("Extract full page text")
    include_highlights bool @default(false) @description("Extract LLM-highlighted relevant snippets")
    highlights_max_characters int @range(100, 5000) @default(2000) @description("Max characters per highlight")
    include_summary bool @default(false) @description("Extract LLM-generated summary")
    summary_query string? @description("Custom query for summary generation")
    max_age_hours int? @description("Cache max age in hours: 0=always livecrawl, -1=never livecrawl, omit=fallback to livecrawl")
    livecrawl_timeout_ms int @range(1000, 60000) @default(10000) @description("Livecrawl timeout")
    moderation bool @default(false) @description("Filter unsafe content")
    user_location string? @description("2-letter ISO country code for localization")
}
```

### Normalized Result Schema

```baml
// ===== Normalized Output =====
class NormalizedSearchResult {
    provider "tavily" | "brave" | "exa"
    request_id string @description("Provider request ID for debugging and support")
    results SearchResult[]
    total_results int @description("Total number of results returned")
    cost_usd float? @description("Actual cost from provider, if available in response")
    metadata ProviderMetadata?
}

class SearchResult {
    id string @description("Unique identifier (URL or provider-specific ID)")
    title string @description("Page title")
    url string @description("Full URL")
    snippet string @description("Primary content excerpt")
    published_date string? @description("ISO 8601 datetime if available")
    score float? @description("Provider relevance score, 0-1 (higher is better)")
    author string? @description("Content author, if available")
    favicon_url string? @description("Favicon URL, if available")
    image_url string? @description("Page image/thumbnail, if available")
    extra_snippets string[]? @description("Additional content excerpts, if available")
    highlights string[]? @description("LLM-extracted relevant snippets (Exa only)")
    highlight_scores float[]? @description("Cosine similarity scores for highlights (Exa only)")
}

class ProviderMetadata {
    // Tavily-specific
    answer string? @description("LLM-generated answer (Tavily only)")
    images string[]? @description("Image URLs (Tavily only)")
    response_time_ms float? @description("Provider response time (Tavily only)")
    search_type string? @description("Search type used (Exa only: neural/fast/auto/deep)")
    more_results_available bool? @description("Whether more results exist (Brave only)")
    usage_credits int? @description("Credits used (Tavily only)")
}
```

### Mapping Rules (Provider Payload -> Normalized Schema)

| Normalized Field | Tavily Source | Brave Source | Exa Source |
|------------------|---------------|---------------|------------|
| `id` | `url` | `url` | `id` (or `url` if id=URL) |
| `title` | `results[].title` | `web.results[].title` | `results[].title` |
| `url` | `results[].url` | `web.results[].url` | `results[].url` |
| `snippet` | `results[].content` | `web.results[].description` | `results[].text` (or first highlight if text empty) |
| `published_date` | `results[].published_date` | `web.results[].age` | `results[].publishedDate` |
| `score` | `results[].score` | Not available | `results[].highlightScores` (use max if available) |
| `author` | Not available | Not available | `results[].author` |
| `favicon_url` | `results[].favicon` | `web.results[].profile.img` | `results[].favicon` |
| `image_url` | `images[].url` | Not available | `results[].image` |
| `extra_snippets` | Not available | `web.results[].extra_snippets` | Not available |
| `highlights` | Not available | Not available | `results[].highlights` |
| `highlight_scores` | Not available | Not available | `results[].highlightScores` |
| `request_id` | `request_id` | Not in response | `requestId` |
| `cost_usd` | Not in response | Not in response | `costDollars.total` |
| `total_results` | `results.length` | `web.results.length` | `results.length` |

**Implementation Notes**:
- Always validate URL format before assigning to `url` field
- Convert `published_date` to ISO 8601 if provider returns alternate format
- Score normalization: Tavily (0-1), Exa (0-1), Brave (none)
- Handle missing fields with `None`/null, don't infer

### Citation Object Shape

```baml
class Citation {
    id string @description("URL-based unique identifier")
    title string
    url string
    snippet string
    published_date string? @description("ISO 8601 datetime")
    score float? @description("Relevance score 0-1")
    source_provider "tavily" | "brave" | "exa"
}
```

**Deterministic Properties**:
- `id` always equals `url` (canonical identifier)
- `snippet` is the primary excerpt for citation
- `score` omitted if provider doesn't provide one
- `published_date` omitted if not available
- `source_provider` always set

---

## BAML Config Strategy

### Client Definitions (One Per Provider)

**Why Separate Clients**: Each provider has unique auth, base URL, and header requirements. Separation enables provider-specific retry policies and fallback chains.

```baml
// ===== Tavily Client =====
client<llm> TavilyClient {
    provider openai-generic
    retry_policy SearchRetryPolicy
    options {
        base_url "https://api.tavily.com/search"
        api_key env.TAVILY_API_KEY
        headers {
            "Content-Type" "application/json"
            "Accept" "application/json"
        }
        http {
            connect_timeout_ms 5000
            request_timeout_ms 30000
        }
    }
}

// ===== Brave Client =====
client<llm> BraveClient {
    provider openai-generic
    retry_policy SearchRetryPolicy
    options {
        base_url "https://api.search.brave.com/res/v1/web/search"
        api_key ""  // Empty to skip Authorization header
        headers {
            "X-Subscription-Token" env.BRAVE_API_KEY
            "Accept" "application/json"
        }
        http {
            connect_timeout_ms 5000
            request_timeout_ms 30000
        }
    }
}

// ===== Exa Client =====
client<llm> ExaClient {
    provider openai-generic
    retry_policy SearchRetryPolicy
    options {
        base_url "https://api.exa.ai/search"
        api_key env.EXA_API_KEY  // Uses Authorization: Bearer header
        headers {
            "Content-Type" "application/json"
            "Accept" "application/json"
        }
        http {
            connect_timeout_ms 5000
            request_timeout_ms 30000
        }
    }
}

// ===== Retry Policy =====
retry_policy SearchRetryPolicy {
    max_retries 2
    strategy {
        type exponential_backoff
        delay_ms 200
        multiplier 2
        max_delay_ms 10000
    }
}
```

### Tool Definitions (Separate Tools)

**Why Separate Tools vs Routed Tool**:

**Separate Tools (RECOMMENDED)**:
- Clear separation enables provider-specific validation
- Type-safe args prevent accidental cross-provider calls
- Each tool has dedicated error handling
- BAML validates provider-specific fields at compile time

```baml
// Separate tool functions
function ExecuteTavilySearch(args: TavilySearchArgs) -> TavilyResponse {
    client TavilyClient
    prompt #"
        Execute Tavily search with the following parameters.
        Return the exact JSON response from Tavily API.
        {{ ctx.output_format }}
        Args: {{ args }}
    "#
}

function ExecuteBraveSearch(args: BraveSearchArgs) -> BraveResponse {
    client BraveClient
    prompt #"
        Execute Brave search with the following parameters.
        Return the exact JSON response from Brave API.
        {{ ctx.output_format }}
        Args: {{ args }}
    "#
}

function ExecuteExaSearch(args: ExaSearchArgs) -> ExaResponse {
    client ExaClient
    prompt #"
        Execute Exa search with the following parameters.
        Return the exact JSON response from Exa API.
        {{ ctx.output_format }}
        Args: {{ args }}
    "#
}
```

**Alternative: Routed Tool (NOT RECOMMENDED for MVP)**:
```baml
// Single union tool with provider routing
class SearchToolRequest {
    provider "tavily" | "brave" | "exa"
    tavily_args TavilySearchArgs?
    brave_args BraveSearchArgs?
    exa_args ExaSearchArgs?
}

function ExecuteSearch(request: SearchToolRequest) -> NormalizedSearchResult {
    // Provider-specific logic in Rust/Python, not BAML
    client GenericClient
    prompt #"
        Execute search for provider: {{ request.provider }}
        {{ ctx.output_format }}
    "#
}
```

**Why Not Routed Tool for MVP**:
- Requires runtime dispatch logic outside BAML
- Loses provider-specific compile-time validation
- More complex error handling
- Separation of concerns is cleaner in actor layer

### Auth and Headers Configuration

**Auth Strategies**:

| Provider | Auth Method | BAML Config |
|----------|-------------|-------------|
| **Tavily** | Bearer token | `api_key env.TAVILY_API_KEY` (sends `Authorization: Bearer`) |
| **Brave** | Custom header | Empty `api_key` + `"X-Subscription-Token" env.BRAVE_API_KEY` in headers |
| **Exa** | Bearer token OR x-api-key | `api_key env.EXA_API_KEY` (sends `Authorization: Bearer`) OR add `"x-api-key" env.EXA_API_KEY` in headers |

**Example: Exa with x-api-key header** (alternative to Bearer):
```baml
client<llm> ExaClientHeaderAuth {
    provider openai-generic
    retry_policy SearchRetryPolicy
    options {
        base_url "https://api.exa.ai/search"
        api_key ""  // Skip Authorization header
        headers {
            "x-api-key" env.EXA_API_KEY
            "Content-Type" "application/json"
        }
    }
}
```

**Environment Variables**:
```bash
# Required (set in production)
TAVILY_API_KEY=tvly-...
BRAVE_API_KEY=BSA...
EXA_API_KEY=...

# Optional (for testing/local dev)
# Can be set in .env file
```

**Safety Notes**:
- Never commit API keys to repo
- Use `.env` files locally, environment variables in production
- Validate keys exist at application startup
- Rotate keys regularly (document in ops runbook)

### Default Routing and Fallback Order

**Recommended Default Order** (for `provider_preference: Auto`):

1. **Tavily** (first) - Fast, reliable, good for general queries, built-in answer generation
2. **Brave** (second) - Cost-effective, good for web results, rich snippets
3. **Exa** (third) - Neural search for complex/research queries, higher cost

**Fallback Policy**:
```rust
enum FallbackStrategy {
    FailFast,           // Fail immediately on first error
    Sequential,         // Try next provider in order (default)
    Parallel,           // Try all in parallel, use best/fastest result
}

struct RoutingConfig {
    default_strategy: FallbackStrategy,
    provider_timeout_ms: u64,  // Timeout per provider
    max_total_cost_usd: f64,   // Budget cap for fallback chain
}
```

**Routing Logic** (in Researcher actor):

```rust
match provider_preference {
    ProviderPreference::Tavily => try_tavily(),
    ProviderPreference::Brave => try_brave(),
    ProviderPreference::Exa => try_exa(),
    ProviderPreference::Auto => {
        // Try Tavily first
        match try_tavily() {
            Ok(result) => Ok(result),
            Err(_) => {
                // Fall back to Brave
                match try_brave() {
                    Ok(result) => Ok(result),
                    Err(_) => {
                        // Fall back to Exa for research queries
                        match try_exa() {
                            Ok(result) => Ok(result),
                            Err(e) => Err(format!("All providers failed: {}", e)),
                        }
                    }
                }
            }
        }
    }
}
```

**Why This Order**:
- **Tavily first**: Fastest, cheapest, good general purpose, built-in LLM answer
- **Brave second**: Free tier available, reliable web results, good cost/value
- **Exa third**: Premium neural search, better for complex/research queries, higher cost

**Exceptions to Override**:
- `scope::News`: Use Tavily (`topic: news`) or Brave with `freshness: pd`
- `scope::Specific { category: "research paper" }`: Use Exa with `category: "research paper"`
- `budget::max_cost_usd < 0.01`: Use Tavily basic or Brave (avoid Exa deep)

---

## Step-by-Step Implementation Checklist

### Phase 1: BAML Schema and Clients

**Files to Create**:

1. `sandbox/baml_client/providers/tavily.baml`
   - Define `TavilySearchArgs` class
   - Define `TavilyClient` (openai-generic with auth)
   - Define `ExecuteTavilySearch` function

2. `sandbox/baml_client/providers/brave.baml`
   - Define `BraveSearchArgs` class
   - Define `BraveClient` (custom header auth)
   - Define `ExecuteBraveSearch` function

3. `sandbox/baml_client/providers/exa.baml`
   - Define `ExaSearchArgs` class
   - Define `ExaClient` (Bearer auth)
   - Define `ExecuteExaSearch` function

4. `sandbox/baml_client/normalized.baml`
   - Define `NormalizedSearchResult` class
   - Define `SearchResult` class
   - Define `Citation` class

5. `sandbox/baml_client/retry_policy.baml`
   - Define `SearchRetryPolicy` (exponential backoff)

**Environment Setup**:

6. Add to `.env.example`:
   ```
   TAVILY_API_KEY=tvly-your-key-here
   BRAVE_API_KEY=your-brave-key-here
   EXA_API_KEY=your-exa-key-here
   ```

7. Update `README.md` with environment variable documentation

**Validation**:

8. Run `baml test` to validate BAML schemas
9. Test each tool individually with mock data:
   ```bash
   baml test ExecuteTavilySearch
   baml test ExecuteBraveSearch
   baml test ExecuteExaSearch
   ```

---

### Phase 2: Rust Actor Messages

**Files to Modify**:

10. `sandbox/src/actors/messages.rs` (or create new file)
    - Add `ResearcherTask` message struct
    - Add `ResearcherTaskResult` message struct
    - Add `ResearchScope` enum
    - Add `ResearchBudget` struct
    - Add `ProviderPreference` enum
    - Add `Citation` struct
    - Add `ExecutionMetadata` struct
    - Add `ToolCallEvent` struct

11. `sandbox/src/actors/researcher.rs` (create if not exists)
    - Define `ResearcherActor` (implements `ractor::Actor`)
    - Implement `handle_message` for `ResearcherTask`
    - Implement routing logic (Tavily -> Brave -> Exa)
    - Implement fallback strategy
    - Add BAML client invocation

**Event Types**:

12. `sandbox/src/events/mod.rs`
    - Add `ResearcherTaskStarted` event
    - Add `ResearcherTaskProgress` event
    - Add `ResearcherTaskCompleted` event
    - Add `ResearcherTaskFailed` event
    - Add `ResearcherSearchStarted` event
    - Add `ResearcherSearchCompleted` event
    - Add `ResearcherSearchFailed` event

---

### Phase 3: BAML Tool Invocation

**Files to Create**:

13. `sandbox/src/baml_provider/mod.rs`
    - Create `BamlSearchProvider` trait
    - Implement `execute_tavily_search()` function
    - Implement `execute_brave_search()` function
    - Implement `execute_exa_search()` function
    - Implement `normalize_response()` function (provider -> normalized)

14. `sandbox/src/baml_provider/mapping.rs`
    - Implement Tavily response mapping
    - Implement Brave response mapping
    - Implement Exa response mapping
    - Handle missing fields gracefully

**Error Handling**:

15. `sandbox/src/baml_provider/error.rs`
    - Define `SearchProviderError` enum
    - Variants: `AuthError`, `RateLimitError`, `InvalidResponse`, `Timeout`
    - Implement `From<baml_py::BamlError>` for `SearchProviderError`

---

### Phase 4: Event Emission

**Event Bus Integration**:

16. `sandbox/src/actors/researcher.rs`
    - Inject `EventBus` into `ResearcherActor`
    - Emit `ResearcherTaskStarted` on task receipt
    - Emit `ResearcherTaskProgress` after each tool call
    - Emit `ResearcherTaskCompleted` on success
    - Emit `ResearcherTaskFailed` on error

17. `sandbox/src/baml_provider/mod.rs`
    - Emit `ResearcherSearchStarted` before tool call
    - Emit `ResearcherSearchCompleted` after success
    - Emit `ResearcherSearchFailed` on error

**Event Payloads**:

18. Ensure all events include:
    - `session_id` and `thread_id` for scope isolation
    - `provider` name for tool-level events
    - `latency_ms` for performance tracking
    - `result_count` for capacity planning
    - `error_code` and `error_message` for debugging

---

### Phase 5: Supervisor Integration

**Files to Modify**:

19. `sandbox/src/actors/supervisor.rs` (or session_supervisor)
    - Add `ResearcherActor` to supervision tree
    - Configure restart strategy (one-for-one)
    - Pass `EventBus` reference to Researcher

20. `sandbox/src/actors/chat.rs`
    - Add message forwarding to Researcher
    - Convert user query to `ResearcherTask`
    - Handle `ResearcherTaskResult` response
    - Format citations for chat display

21. `sandbox/src/actors/desktop.rs`
    - Add message forwarding to Researcher
    - Convert desktop search request to `ResearcherTask`
    - Handle `ResearcherTaskResult` response

---

### Phase 6: Configuration

**Config Files**:

22. `sandbox/config/researcher.toml` (create)
    - Default provider order
    - Fallback strategy
    - Timeout per provider
    - Max cost budget
    - Rate limit per provider

23. `sandbox/src/config/mod.rs`
    - Load researcher config from file
    - Validate config values at startup

**Environment Variables**:

24. Update startup validation to check:
    - `TAVILY_API_KEY` exists
    - `BRAVE_API_KEY` exists
    - `EXA_API_KEY` exists
    - Emit warning if any missing (don't fail startup)

---

### Phase 7: Testing (See Test Matrix Below)

**Unit Tests**:

25. `sandbox/src/baml_provider/tests/mapping_test.rs`
    - Test Tavily response mapping
    - Test Brave response mapping
    - Test Exa response mapping
    - Test missing field handling

**Integration Tests**:

26. `sandbox/tests/researcher_integration_test.rs`
    - Test `uactor -> actor` path (task -> result)
    - Test `appactor -> toolactor` path (tool call -> normalized)
    - Test fallback routing (Tavily -> Brave -> Exa)
    - Test event emission

**Live Tests**:

27. `sandbox/tests/researcher_live_test.rs`
    - Test Tavily (gated by `TAVILY_API_KEY`)
    - Test Brave (gated by `BRAVE_API_KEY`)
    - Test Exa (gated by `EXA_API_KEY`)
    - Mark with `#[ignore]` unless env vars set

---

### Phase 8: Backward Compatibility

**Migration Notes**:

28. If existing research capability exists:
    - Add deprecation warning to old API
    - Provide migration path (old args -> new `ResearcherTask`)
    - Support dual-mode for 1 release cycle
    - Remove old API in next release

**Event Schema Compatibility**:

29. Ensure new events are additive:
    - Don't modify existing event fields
    - Add new events with clear naming
    - Update event store schema with migration
    - Document event evolution in `EVENTS.md`

---

### Phase 9: Documentation

**Files to Create**:

30. `docs/architecture/researcher.md`
    - Actor architecture diagram
    - Message flow diagram
    - Routing logic explanation
    - Event catalog

31. `docs/operators/researcher-ops.md`
    - Environment setup guide
    - API key management
    - Cost monitoring
    - Rate limit handling
    - Troubleshooting

32. `AGENTS.md` (update)
    - Add Researcher to supervision tree
    - Document researcher capabilities
    - Update quick commands

---

## Test Matrix

### Unit Tests

| Test File | Test Name | Purpose | Mocking Strategy |
|-----------|-----------|---------|------------------|
| `baml_provider/tests/mapping_test.rs` | `test_tavily_response_mapping` | Verify Tavily JSON -> Normalized schema | Mock Tavily JSON response |
| `baml_provider/tests/mapping_test.rs` | `test_brave_response_mapping` | Verify Brave JSON -> Normalized schema | Mock Brave JSON response |
| `baml_provider/tests/mapping_test.rs` | `test_exa_response_mapping` | Verify Exa JSON -> Normalized schema | Mock Exa JSON response |
| `baml_provider/tests/mapping_test.rs` | `test_tavily_missing_fields` | Verify graceful handling of missing fields | Incomplete Tavily JSON |
| `baml_provider/tests/mapping_test.rs` | `test_citation_validation` | Verify citation object shape | Valid/invalid citation data |
| `actors/researcher/tests/message_test.rs` | `test_researcher_task_validation` | Verify `ResearcherTask` validation | Valid/invalid task messages |
| `actors/researcher/tests/routing_test.rs` | `test_routing_fallback_order` | Verify Tavily -> Brave -> Exa routing | Mock tool results |

**Mock Data Storage**: `sandbox/tests/mocks/tavily_response.json`, `sandbox/tests/mocks/brave_response.json`, `sandbox/tests/mocks/exa_response.json`

---

### Integration Tests

| Test File | Test Name | Purpose | Setup |
|-----------|-----------|---------|-------|
| `tests/researcher_integration_test.rs` | `test_actor_delegation_path` | Verify `uactor -> actor` path | Spawn ResearcherActor, send task, await result |
| `tests/researcher_integration_test.rs` | `test_tool_invocation_path` | Verify `appactor -> toolactor` path | Mock BAML client, call tool directly |
| `tests/researcher_integration_test.rs` | `test_event_emission` | Verify all events emitted | Mock EventBus, assert event calls |
| `tests/researcher_integration_test.rs` | `test_fallback_on_error` | Verify routing with provider failure | Mock Tavily failure, Brave success |
| `tests/researcher_integration_test.rs` | `test_budget_enforcement` | Verify cost budget is respected | Set low budget, assert failover |
| `tests/researcher_integration_test.rs` | `test_scope_isolation` | Verify session/thread scoping | Send tasks with different IDs, assert no cross-contamination |

**Test Helpers**:
- `setup_researcher_actor()`: Spawns ResearcherActor with mock EventBus
- `create_mock_task()`: Creates `ResearcherTask` with test data
- `await_task_result()`: Waits for `ResearcherTaskResult` with timeout

---

### Live Smoke Tests

| Test File | Test Name | Purpose | Gate Condition |
|-----------|-----------|---------|----------------|
| `tests/researcher_live_test.rs` | `test_tavily_live_search` | Verify Tavily API integration | `TAVILY_API_KEY` env var set |
| `tests/researcher_live_test.rs` | `test_brave_live_search` | Verify Brave API integration | `BRAVE_API_KEY` env var set |
| `tests/researcher_live_test.rs` | `test_exa_live_search` | Verify Exa API integration | `EXA_API_KEY` env var set |
| `tests/researcher_live_test.rs` | `test_all_providers_live` | Verify all providers in one test | All three API keys set |

**Concurrency Guidance**:
- Mark all live tests with `#[tokio::test]` and `#[ignore]`
- Run with: `cargo test -p sandbox --test researcher_live_test -- --ignored`
- Set `--test-threads=1` for live tests to avoid rate limit conflicts
- Add 1-second sleep between provider calls in test
- Use simple queries to minimize cost (e.g., "test query", not complex research)

**Cost Estimation**:
- Tavily: ~1 credit = $0.008
- Brave: Free tier up to 2,000/month
- Exa: ~$0.005 per request
- Total per full test run: ~$0.02

---

### Mixed-Path Tests

| Test File | Test Name | Purpose | Coverage |
|-----------|-----------|---------|----------|
| `tests/researcher_mixed_test.rs` | `test_actor_delegation_then_tool_call` | Verify both paths in sequence | Delegation -> Tool invocation |
| `tests/researcher_mixed_test.rs` | `test_concurrent_delegations` | Verify multiple actor delegations don't interfere | Parallel tasks |
| `tests/researcher_mixed_test.rs` | `test_tool_events_propagate_to_actor_events` | Verify tool-level events contribute to actor-level metadata | Event aggregation |

---

### Failure-Path Tests

| Test File | Test Name | Failure Scenario | Expected Behavior |
|-----------|-----------|------------------|-------------------|
| `tests/researcher_failure_test.rs` | `test_provider_timeout` | Provider exceeds timeout | Fallback to next provider |
| `tests/researcher_failure_test.rs` | `test_auth_error` | Invalid API key | Emit `ResearcherSearchFailed`, fall back |
| `tests/researcher_failure_test.rs` | `test_malformed_response` | Invalid JSON from provider | Emit `ResearcherSearchFailed`, return partial results |
| `tests/researcher_failure_test.rs` | `test_all_providers_fail` | All providers return errors | Emit `ResearcherTaskFailed` with error details |
| `tests/researcher_failure_test.rs` | `test_empty_results` | Valid response but 0 results | Return empty `NormalizedFindings` with metadata |
| `tests/researcher_failure_test.rs` | `test_rate_limit_hit` | Provider returns 429 | Emit `ResearcherSearchFailed`, retry after backoff |
| `tests/researcher_failure_test.rs` | `test_budget_exceeded` | Max cost reached | Abort task, emit `ResearcherTaskFailed` |

**Mock Strategy**:
- Use `httpmock` or `wiremock` for HTTP-level failures
- Inject errors via BAML mock client
- Verify error propagation through all layers

---

### Concurrency Tests

| Test File | Test Name | Purpose | Configuration |
|-----------|-----------|---------|----------------|
| `tests/researcher_concurrency_test.rs` | `test_concurrent_researcher_tasks` | Verify ResearcherActor handles concurrent tasks | 10 concurrent tasks |
| `tests/researcher_concurrency_test.rs` | `test_provider_rate_limiting` | Verify concurrent tool calls respect provider rate limits | 20 rapid calls |
| `tests/researcher_concurrency_test.rs` | `test_event_bus_concurrency` | Verify EventBus handles high event volume | 100 events/sec |

**Timeout Configuration**:
- Unit tests: 5 second timeout
- Integration tests: 30 second timeout
- Live tests: 60 second timeout
- Concurrency tests: 120 second timeout

---

## Risks and Mitigations

### Provider Drift

**Risk**: Provider APIs change (new fields, deprecations, response format changes).

**Mitigations**:
1. **Schema Versioning**: Store provider response schemas in `sandbox/baml_client/schemas/` with version numbers (e.g., `tavily_v1.json`, `brave_v1.json`). Compare live responses against schema on integration.
2. **Automated Integration Tests**: Run daily CI job that calls each provider and compares response structure against expected schema. Fail if schema drift detected.
3. **Mapping Layer Isolation**: All provider-to-normalized mapping logic in `sandbox/src/baml_provider/mapping.rs`. Provider changes only affect mapping code, not actor logic.
4. **Graceful Degradation**: If unexpected field encountered, log warning and continue (don't crash). Add field to mapping in next release.
5. **Provider Changelog Monitoring**: Subscribe to provider changelogs (Tavily, Brave, Exa) and schedule quarterly reviews for breaking changes.

**Monitoring**:
- Alert on schema validation failures
- Track unmapped field warnings
- Dashboard shows provider version compatibility

---

### Tool-Call Schema Mismatch

**Risk**: BAML tool args don't match provider API expectations, causing silent failures or malformed requests.

**Mitigations**:
1. **Compile-Time Validation**: BAML validates types and constraints at compile time. Enforce `cargo check` in CI before PR merge.
2. **Provider Spec Sync**: Store provider API specs (OpenAPI/Swagger if available) in `sandbox/docs/provider-specs/`. Run diff against BAML schemas in CI.
3. **Request Validation**: Add client-side validation before tool call (check required fields, enum values, ranges). Log warnings if validation fails.
4. **Error Mapping**: Map provider error responses to `SearchProviderError` variants. Include raw error in `ExecutionMetadata` for debugging.
5. **Contract Testing**: Use Pact or similar to mock provider responses and validate tool requests.

**Validation Points**:
- BAML compile time
- Rust runtime (before tool call)
- Provider API runtime (returns 400 on invalid params)

---

### Citation Quality Regressions

**Risk**: Normalized citations lose information (e.g., missing published dates, scores degrade over time).

**Mitigations**:
1. **Field Coverage Tracking**: Track which fields each provider provides. Alert if coverage drops (e.g., Tavily stops returning `published_date`).
2. **Citation Validation Tests**: Unit tests verify citation fields are preserved from provider to normalized schema. Run on every PR.
3. **Quality Metrics Dashboard**: Track citation completeness (% with URL, title, snippet), accuracy (URL validation), and consistency (field format).
4. **Manual Review**: Spot-check citations weekly during initial rollout. Reduce to monthly after stability.
5. **Fallback for Missing Data**: If provider stops returning a field, attempt to fetch from alternative provider or metadata service.

**Metrics**:
- Citation completeness: Target >95% for required fields (url, title, snippet)
- URL validity: 100% (all URLs must be valid HTTP/HTTPS)
- Published date accuracy: Target >80% (when provider provides date)

---

### Cost/Rate-Limit Spikes

**Risk**: Unbounded research tasks cause unexpected cost spikes or provider rate-limit bans.

**Mitigations**:
1. **Budget Enforcement**: `ResearchBudget` enforces `max_cost_usd` and `max_results`. Abort task if budget exceeded.
2. **Per-Provider Rate Limiting**: Track API calls per provider in-memory with sliding window. Throttle if approaching limit.
3. **Cost Tracking**: Log actual cost per request (from provider response). Aggregate to dashboard. Alert on anomaly (e.g., >10% increase day-over-day).
4. **Circuit Breaker**: If provider returns 429 (rate limit), stop calling for provider-specific cooldown (e.g., 60 seconds).
5. **Fallback Optimization**: On rate limit, skip expensive provider (Exa deep) and use cheaper alternative (Tavily basic).

**Monitoring**:
- Real-time cost dashboard
- Rate limit hit rate (alert if >5%)
- Per-provider call volume

**Ops Actions**:
- Emergency kill switch: Disable provider via config without code deployment
- Quota enforcement: Set daily/monthly caps in provider dashboard

---

### Observability Gaps

**Risk**: Actor-level events and tool-level events don't correlate, making debugging difficult.

**Mitigations**:
1. **Correlation IDs**: All events include `session_id`, `thread_id`, and `task_id` (or `request_id` from provider). Use these to link actor events to tool events.
2. **Event Hierarchy**: Actor events (`researcher.task.*`) aggregate tool events (`researcher.search.*`). Include tool event counts in actor event payload.
3. **Event Ordering**: Guarantee event order via EventBus (or EventStore replay). Emit `started` → `progress` → `completed` in sequence.
4. **Structured Logging**: Log full tool request/response on error (with sanitization for auth keys). Store in EventStore for replay.
5. **Observability Dashboard**: Grafana dashboard shows:
   - Task throughput (tasks/sec)
   - Tool call latency (p50/p95/p99)
   - Provider success rate
   - Cost per task

**Gap Detection**:
- Assert in tests that for every `ResearcherTaskStarted`, there is a matching `ResearcherTaskCompleted` or `ResearcherTaskFailed`
- Alert on orphaned events (no matching start/complete)

---

## Final Recommendation

### MVP Provider Order

**Primary Choice**: **Tavily**  
- Fast (ultra-fast mode: <1s)
- Reliable (99.9% uptime in production)
- Cheap ($0.008/request, 1 credit)
- Good for general queries (news, finance, general)
- Built-in LLM answer generation reduces post-processing

**Fallback 1**: **Brave**  
- Free tier available (2,000/month)
- Reliable web results
- Rich snippets (extra_snippets option)
- Good for citation-heavy queries

**Fallback 2**: **Exa**  
- Neural search for complex queries
- Better for research papers, technical content
- LLM highlights provide superior snippets
- Higher cost ($0.005-$0.025/request)

**Recommendation**: Start with Tavily-only MVP, add Brave as fallback in v2, add Exa in v3.

---

### Fallback Policy

**Strategy**: **Sequential Fallback** (not parallel)

```rust
pub enum FallbackPolicy {
    FailFast,           // Abort on first error (for low-latency use cases)
    Sequential,         // Try next provider in order (default)
    Parallel,          // Try all providers, use best result (deferred)
}
```

**MVP Config**:
```toml
[researcher.fallback]
strategy = "Sequential"
timeout_per_provider_ms = 30000
max_total_cost_usd = 0.10
providers = ["Tavily", "Brave"]  # Exa deferred to v3
```

**When to Override**:
- `fail_fast: true` for real-time queries (e.g., user-facing chat)
- `parallel: true` for offline batch research (deferred feature)

---

### What to Defer

**Deferred to v2**:
- Brave integration (add after Tavily proves stable)
- Parallel fallback (requires cost aggregation logic)
- Provider-specific result ranking (use provider score as-is)

**Deferred to v3**:
- Exa integration (after cost monitoring in place)
- Query expansion (use Exa deep search)
- Custom re-ranking (use Exa highlights)

**Deferred to Future**:
- Websets API for large-scale research
- Async `/research/v1` endpoint for long-running tasks
- Custom goggles for Brave (requires Pro plan)
- Advanced content extraction (Exa summaries, structured data)

---

### Definition of "Done" for Production Rollout

**Must-Have Criteria**:

1. **All Unit Tests Pass**:
   - BAML schema validation
   - Provider response mapping
   - Actor message validation
   - Citation validation

2. **All Integration Tests Pass**:
   - `uactor -> actor` delegation path
   - `appactor -> toolactor` invocation path
   - Event emission (all 7 events)
   - Fallback routing (Tavily -> Brave)

3. **Live Smoke Tests Pass** (for deployed providers):
   - Tavily: Successful search with citations
   - (Optional) Brave: Successful search with citations

4. **Observability in Place**:
   - EventStore collecting all 7 event types
   - Grafana dashboard showing task throughput, latency, cost
   - Alerting configured for rate limits, schema drift, cost anomalies

5. **Documentation Complete**:
   - `docs/architecture/researcher.md` (architecture)
   - `docs/operators/researcher-ops.md` (ops guide)
   - API key setup in `.env.example`
   - Quick command in `AGENTS.md`

6. **Backward Compatibility**:
   - If existing research capability exists: deprecation warning added
   - Migration path documented
   - Dual-mode supported for 1 release

7. **Ops Readiness**:
   - API keys stored in production secrets manager
   - Cost budget set in provider dashboards
   - Rate limit alerts configured
   - Emergency kill switch documented

8. **Security Review**:
   - Auth keys never logged (use `***` redaction)
   - User input sanitized before passing to providers
   - Content from providers validated for XSS (if displayed in UI)

**Success Metrics** (after 2 weeks in production):

- P95 task latency: <5 seconds
- Success rate: >99%
- Cost per task: < $0.05
- Citation completeness: >95%
- No critical bugs (data loss, security issues)

**Rollback Plan**:
- Feature flag: Disable ResearcherActor and route to old research capability
- Config rollback: Revert to previous provider configuration
- Database rollback: EventStore continues logging, no data loss

---

## Appendix

### Provider Documentation Links

- **Tavily API Docs**: https://docs.tavily.com/docs/tavily-api/rest/api
- **Tavily Dashboard**: https://app.tavily.com
- **Brave Search API Docs**: https://api.search.brave.com/app/documentation
- **Brave Dashboard**: https://api.search.brave.com/app/dashboard
- **Exa API Docs**: https://exa.ai/docs
- **Exa Dashboard**: https://dashboard.exa.ai
- **BAML Docs**: https://docs.boundaryml.com

### Related ChoirOS Documentation

- `docs/architecture/NARRATIVE_INDEX.md` - Architecture index
- `AGENTS.md` - Agent development guide
- `docs/architecture/supervision-tree.md` - Supervision tree design
- `docs/operators/eventbus-guide.md` - EventBus usage

### Implementation Timeline Estimate

| Phase | Time Estimate | Dependencies |
|-------|---------------|--------------|
| Phase 1: BAML Schema and Clients | 1 day | None |
| Phase 2: Rust Actor Messages | 1 day | Phase 1 |
| Phase 3: BAML Tool Invocation | 2 days | Phase 1, 2 |
| Phase 4: Event Emission | 1 day | Phase 3 |
| Phase 5: Supervisor Integration | 1 day | Phase 4 |
| Phase 6: Configuration | 0.5 days | Phase 5 |
| Phase 7: Testing | 3 days | Phase 6 |
| Phase 8: Backward Compatibility | 0.5 days | Phase 7 |
| Phase 9: Documentation | 1 day | Phase 8 |
| **Total** | **11 days** | Parallelizable: Phases 4-6 can overlap |

### Contact

For questions or clarifications, refer to:
- Architecture: `docs/architecture/` directory
- Implementation: `sandbox/src/` codebase
- Operations: `docs/operators/` directory
- Issues: Create GitHub issue with `researcher` label
