# ADR-0028: Multi-Provider LLM Scaling

Date: 2026-03-11
Kind: Decision
Status: Draft
Priority: 2
Requires: [ADR-0003, ADR-0022]
Owner: platform/runtime

## Narrative Summary (1-minute read)

Stress testing at 62 concurrent VMs revealed that the LLM throughput bottleneck
is upstream provider throttling, not gateway or host capacity. All LLM calls
currently route through a single AWS Bedrock account/region. At 38+ concurrent
requests, Bedrock latency degrades from 5s to 20s+ p50. Multi-provider routing
across Bedrock, direct Anthropic API, OpenAI, GLM, and Inception spreads
concurrent load across independent rate limit pools, increasing the effective
LLM ceiling without additional hardware. This is a UX improvement -- users
perceive LLM latency as system slowness.

## What Changed

Scale testing (2026-03-11) proved single-provider is a hard ceiling:

- 7 VMs: 5.3s LLM p50 (baseline)
- 25 VMs: 15.3s LLM p50 (gateway saturating)
- 38 VMs: 20.8s LLM p50 (clean ceiling)
- 62 VMs: 32.4s LLM p50 (near-OOM, degraded)

The latency increase is Bedrock concurrency throttling, not gateway overhead
(gateway adds ~1-2ms). Health probes stay at 42-49ms throughout -- the host is
fine, the provider is the bottleneck.

## What To Do Next

Implement in phases below. Phase 1 (multi-region Bedrock) is the lowest-effort
highest-impact change and can ship independently.

---

## Decision

### 1. Provider routing in the gateway

The gateway currently rewrites all requests to Bedrock format. It needs to
support multiple upstream providers simultaneously, selecting based on: model
requested, provider health/latency, rate limit headroom, and cost preference.

### 2. Provider pool concept

Instead of one Bedrock credential, the gateway maintains a pool of provider
backends:

- AWS Bedrock (existing, us-east-1)
- AWS Bedrock (additional regions: us-west-2, eu-west-1)
- Anthropic Messages API (direct)
- OpenAI (for models where applicable, and as overflow)
- GLM (Z.ai, already in BAML config but unused at scale)
- Inception (diffusion-based, new provider, details TBD)

### 3. Routing strategy

Simple round-robin or least-latency across healthy providers. No complex
ML-based routing. If a provider returns 429 or latency exceeds threshold, mark
it degraded and shift load. Provider health is a sliding window, not a circuit
breaker -- degraded providers still get occasional requests to detect recovery.

### 4. BYOK (Bring Your Own Key)

Per-user credential storage enables users to add their own provider keys, which
both increases aggregate throughput (each user's keys are independent rate limit
pools) and enables model access the platform doesn't have. Auth flows by
provider:

- **OpenAI:** OAuth2 PKCE flow (ChatGPT subscription tokens, officially blessed
  for third-party use).
- **Anthropic:** Paste-token flow (no subscription OAuth yet, API keys for
  BYOK).
- Per-user credential store in hypervisor, gateway routes per-user.

Reference: PicoClaw (github.com/sipeed/picoclaw) implemented OAuth2 PKCE +
paste-token + auto-refresh + token storage in ~200 lines of Go. ADR-0003
defines the secrets boundary -- BYOK credentials are user-level secrets stored
in the hypervisor and brokered through the existing capability model.

### 5. Cost awareness

Different providers have different pricing. The routing layer should expose cost
signals (not enforce budgets -- that's ADR-0022's rate limiter territory) so
that model policy can factor in cost. E.g., prefer Bedrock for Haiku (cheapest),
fall back to direct Anthropic for Sonnet when Bedrock is throttled.

### 6. Gateway protocol normalization

The gateway already rewrites Anthropic Messages API -> Bedrock InvokeModel. It
needs the reverse too (Bedrock-format requests -> direct Anthropic API) and new
normalizations for OpenAI and GLM. The sandbox always speaks Anthropic Messages
API format; the gateway handles upstream translation.

---

## Phases

### Phase 1: Multi-region Bedrock (lowest effort, highest immediate impact)

- Add us-west-2 and eu-west-1 Bedrock credentials
- Round-robin across regions
- Effectively 3x the concurrent request ceiling

### Phase 2: Direct Anthropic API as overflow

- When Bedrock is throttled, route to direct Anthropic Messages API
- Requires separate API key (not Bedrock credentials)
- Gateway skips the Bedrock rewrite for these requests

### Phase 3: BYOK credential storage

- Per-user key storage in hypervisor DB (ADR-0003 `user_secrets` table)
- Gateway checks for user-specific credentials before using platform keys
- OpenAI OAuth2 PKCE flow for ChatGPT subscription tokens
- Anthropic paste-token flow for API keys

### Phase 4: Additional providers (GLM, OpenAI, Inception, OpenRouter)

- GLM already has model IDs in the BAML config -- activate in gateway
- OpenAI for overflow/alternative models
- Inception Mercury 2 -- confirmed working via BAML (0.54s response, diffusion LLM)
- OpenRouter -- aggregator providing access to many models via OpenAI-compatible API.
  Confirmed working: NVIDIA Nemotron 3 Super 120B (free, 0.76s), Hunter Alpha (free,
  1T params, reasoning, 3s), Healer Alpha (free, multimodal, 1.6s)

### Phase 5: Runtime model selection

The current model selection is a static config file loaded by the Settings app.
This needs two improvements:

- **GUI selectors:** User-facing model picker in the Settings app, per-role
  (writer model, conductor model, worker model). Changes take effect on next
  agent invocation, no restart needed.
- **Conductor-controlled model policy:** Conductor should be able to set model
  selection dynamically based on task characteristics. A research task might use
  a cheap fast model; a complex rewrite might use Opus. This is metadata on the
  dispatch, not a user setting. Conductor currently only spawns Writer -- it
  needs broader model policy authority across all app agents and their workers.

---

## Provider quirks (discovered during BAML smoke testing)

- **OpenRouter Stealth provider (Hunter/Healer Alpha):** System-only messages
  return 400. Requires at least one `user` role message in the conversation.
  Gateway normalization must ensure a user message exists when routing to these
  models.
- **OpenRouter reasoning models:** Support `reasoning.enabled` parameter in
  request body. Response includes `reasoning_details` array. When continuing
  conversations, `reasoning_details` must be preserved in assistant messages.
- **BAML local testing flow:** Add client to `baml_src/clients.baml`, run
  `npx @boundaryml/baml test`. If it passes in BAML, it works through the
  gateway. No deployment needed for provider validation.

## Non-goals

- Automated cost optimization / cheapest-route selection (premature)
- Provider-specific model fine-tuning
- Client-side provider selection (gateway decides, sandbox doesn't know or care)

---

## Sources

- Capacity stress test: `docs/state/reports/2026-03-11-capacity-stress-test.md`
- ADR-0022 concurrency findings: `docs/theory/decisions/adr-0022-hypervisor-concurrency-and-capacity.md`
- ADR-0003 secrets boundary: `docs/theory/decisions/adr-0003-hypervisor-sandbox-secrets-boundary.md`
- BYOK/provider auth session notes: `docs/theory/notes/2026-03-11-agent-architecture-session-notes.md` (section 3)
- PicoClaw auth implementation: github.com/sipeed/picoclaw (Issue #18, merged 2026-02-12)
