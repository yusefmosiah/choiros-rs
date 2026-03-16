# Implementing ADR-0028: Multi-Provider LLM Scaling

Date: 2026-03-15
Kind: Guide
Status: Active
Priority: 2
Requires: [ADR-0028, ADR-0003, ADR-0022]

## Narrative Summary (1-minute read)

ChoirOS already has two important pieces of ADR-0028 in place:

1. The sandbox runtime can resolve models from a catalog with multiple provider
   types (`aws-bedrock`, Anthropic-compatible, OpenAI-compatible).
2. The hypervisor provider gateway can already proxy Bedrock, Z.ai, Kimi,
   OpenAI-compatible providers, and search providers.

The missing capability is not basic provider support. The missing capability is
dynamic upstream selection under load. Bedrock is still treated as a single
credential/region path, generic provider auth is inferred from upstream URL
heuristics, and routing decisions are static at model-selection time. This
guide turns the existing gateway into a real routing layer: a backend pool with
health, latency, credential precedence, and rollout discipline.

## What Changed

- 2026-03-15: Initial implementation guide grounded against the current
  `sandbox/src/actors/model_config.rs` and `hypervisor/src/provider_gateway.rs`
  architecture.

## What To Do Next

Start with Phase 0 and Phase 1. They are the minimum needed to turn the current
single-Bedrock bottleneck into a multi-backend gateway without breaking the
managed-runtime secret boundary.

---

## Current State Snapshot

### What already exists

1. `sandbox/src/actors/model_config.rs`
   - Managed sandboxes already route model calls through the hypervisor gateway.
   - Bedrock models are rewritten to an Anthropic-compatible gateway path.
   - Non-Bedrock models are already represented in the model catalog.

2. `hypervisor/src/provider_gateway.rs`
   - The gateway authenticates sandbox requests with a shared gateway token.
   - It enforces per-sandbox request-rate limits.
   - It forwards generic upstream requests and has a Bedrock-specific rewrite
     path from Anthropic Messages API to Bedrock InvokeModel.

3. Validation assets already exist:
   - `sandbox/tests/model_provider_live_test.rs`
   - `scripts/ops/validate-local-provider-matrix.sh`
   - `docs/practice/guides/local-provider-matrix-validation.md`

### What is missing

1. No backend pool abstraction in the gateway.
2. No health-aware routing or degraded-provider recovery.
3. Bedrock is effectively one platform credential and one region per model.
4. Generic provider credentials are selected by matching `base_url` substrings,
   which does not scale to multiple accounts, per-user keys, or explicit
   provider policy.
5. No persistent provider-health metrics or load-test verifier for routing.

## Design Constraints

1. Preserve ADR-0003:
   sandboxes stay keyless; hypervisor owns platform and user provider secrets.
2. Preserve the sandbox wire contract:
   the sandbox should keep speaking Anthropic/OpenAI-compatible request formats;
   upstream translation belongs in the gateway.
3. Prefer additive rollout:
   Phase 1 must ship independently and improve throughput before BYOK or broad
   provider expansion lands.
4. Reuse the existing model catalog:
   do not introduce a second, competing model-selection system in the sandbox.

## Target Architecture

### 1. Split logical model selection from backend selection

The sandbox should keep choosing a logical model from the existing catalog.
The hypervisor gateway should then choose a backend for that request from an
allowed backend pool.

Example:

- Logical model: `ClaudeBedrockHaiku45`
- Allowed backends:
  - `bedrock-us-east-1-platform`
  - `bedrock-us-west-2-platform`
  - `bedrock-eu-west-1-platform`
  - `anthropic-direct-platform`

The sandbox picks the logical model. The gateway picks the backend instance.

### 2. Introduce explicit gateway backend config

Replace URL-substring auth selection with an explicit gateway backend registry.
Each backend should declare:

- backend id
- provider kind (`bedrock`, `anthropic`, `openai`, `openrouter`, `search`)
- base URL or region
- credential source
- supported logical models or model-family mapping
- enabled/disabled state
- optional cost/priority weight

### 3. Add backend health state

The gateway should track a sliding window per backend:

- recent latency
- HTTP 429 count
- 5xx/error count
- consecutive failures
- last success timestamp
- degraded-until timestamp

Routing should use simple weighted round-robin across healthy backends, with
occasional probes to degraded backends to detect recovery.

## Phase 0: Observability and Contract Lock-In (1-2 hours)

Before changing routing behavior, capture the current contract and add enough
instrumentation to tell whether routing improves throughput.

### 0a. Add backend-aware metrics/logging

**Files:**
- `hypervisor/src/provider_gateway.rs`
- `hypervisor/src/state.rs`

Add structured fields to provider-gateway logs:

- `logical_model`
- `backend_id`
- `backend_kind`
- `backend_region`
- `route_reason` (`healthy`, `degraded_probe`, `fallback_429`, `fallback_latency`)
- `upstream_status`
- `latency_ms`

### 0b. Add routing-focused tests before refactor

**Files:**
- `hypervisor/src/provider_gateway.rs`
- `sandbox/tests/model_provider_live_test.rs`

Add tests for:

1. Bedrock Anthropic-message rewrite still works.
2. Gateway preserves upstream headers/body for generic providers.
3. Routing metadata is attached without exposing secrets.

### Verify

```bash
cargo test -p hypervisor provider_gateway
cargo test --test model_provider_live_test -- --nocapture
```

## Phase 1: Bedrock Backend Pool (lowest effort, highest impact)

This phase should produce the first real throughput win and can ship without
BYOK or non-Bedrock fallback.

### 1a. Introduce `GatewayBackendConfig`

**Files:**
- `hypervisor/src/provider_gateway.rs`
- `hypervisor/src/config.rs`
- `hypervisor/src/state.rs`

Add explicit config types for provider backends and load them from env/credential
inputs. Start with Bedrock only:

- `bedrock-us-east-1-platform`
- `bedrock-us-west-2-platform`
- `bedrock-eu-west-1-platform`

Each backend should carry its own region and credential reference. Do not infer
auth from URL matching.

### 1b. Replace direct Bedrock forwarding with backend selection

**Files:**
- `hypervisor/src/provider_gateway.rs`
- `sandbox/src/actors/model_config.rs`

Keep the sandbox request contract stable. The minimal change is:

1. Sandbox continues sending a logical model id in headers.
2. Bedrock gateway path becomes a logical Bedrock route, not a literal
   single-region credential path.
3. Gateway resolves the logical model to an eligible Bedrock backend and then
   rewrites to the selected Bedrock region endpoint.

The region should be selected inside the gateway, not encoded as the final
routing decision in the sandbox.

### 1c. Add backend health and weighted round-robin

**Files:**
- `hypervisor/src/provider_gateway.rs`

Routing policy for Phase 1:

1. Choose among healthy Bedrock backends with weighted round-robin.
2. If a backend returns 429, mark it degraded for a short cooldown.
3. If p50 latency over the sliding window crosses the configured threshold,
   temporarily downgrade its selection weight.
4. Continue sending occasional probe traffic to degraded backends.

Keep the policy simple and deterministic. No cost-aware routing yet.

### 1d. Add a load-focused verifier

**Files:**
- `scripts/ops/validate-local-provider-matrix.sh`
- new or existing stress-test harness under `tests/` or `scripts/ops/`

Add a verification mode that sends concurrent requests through the gateway and
asserts that:

1. Requests distribute across multiple Bedrock backend ids.
2. 429s on one backend do not stall all requests.
3. Latency stays materially below the single-region baseline at the same load.

### Verify

```bash
cargo test -p hypervisor provider_gateway
./scripts/ops/validate-local-provider-matrix.sh --skip-model-tests
# Plus a targeted concurrency smoke for multi-region routing
```

## Phase 2: Direct Anthropic Overflow

Once the Bedrock backend pool exists, add direct Anthropic as an overflow path
for Claude-family logical models.

### 2a. Add Anthropic backend kind

**Files:**
- `hypervisor/src/provider_gateway.rs`
- `hypervisor/src/config.rs`

Add explicit Anthropic backend config with:

- backend id
- base URL
- credential reference
- supported logical models or model-family map

### 2b. Route Claude-family logical models to Bedrock or Anthropic

**Files:**
- `hypervisor/src/provider_gateway.rs`
- `sandbox/config/model-catalog.example.toml`

The logical model should describe a model family, not one hardwired provider
account. The gateway backend map should define which upstreams can serve that
family and how to translate model ids if upstreams differ.

Example:

- logical family: `claude-haiku-4-5`
- Bedrock upstream model id: `us.anthropic.claude-haiku-4-5-20251001-v1:0`
- Anthropic upstream model id: provider-specific direct API name

### 2c. Add fallback policy

Fallback order for Claude-family requests:

1. healthy Bedrock backends
2. direct Anthropic backend when Bedrock is throttled or degraded

Treat 429 and sustained latency as routing signals. Do not retry indefinitely
inside one request; choose a fallback once, then fail clearly.

### Verify

```bash
cargo test -p hypervisor provider_gateway
./scripts/ops/validate-local-provider-matrix.sh --models "ClaudeBedrockHaiku45,KimiK25,ZaiGLM5"
```

## Phase 3: Generalize the Backend Registry for OpenAI, GLM, Inception, and OpenRouter

This phase unifies the provider story. The goal is one routing system, not one
code path per provider.

### 3a. Move generic providers off URL-substring credential lookup

**Files:**
- `hypervisor/src/provider_gateway.rs`
- `sandbox/src/actors/model_config.rs`

Replace `provider_key_for_upstream(...)` heuristics with backend-id-based lookup.
The sandbox should identify the logical provider/model. The gateway should
resolve that to an explicit backend config and its credential source.

### 3b. Add provider quirks as backend policy

**Files:**
- `hypervisor/src/provider_gateway.rs`
- `sandbox/tests/model_provider_live_test.rs`

Encode known quirks in one place:

- OpenRouter Hunter/Healer require at least one `user` message.
- Reasoning-model response metadata must be preserved when the upstream needs it.
- OpenAI-compatible and Anthropic-compatible providers may require different
  header or body normalization.

### 3c. Keep model catalog logical

**Files:**
- `sandbox/config/model-catalog.example.toml`
- `docs/practice/guides/model-provider-agnostic-runbook.md`

The sandbox catalog should remain user-facing and logical:

- model identity
- default callsite mapping
- optional allowed backend families

Do not force the sandbox catalog to name every platform account or region.

### Verify

```bash
./scripts/ops/validate-local-provider-matrix.sh \
  --models "ZaiGLM5,ZaiGLM47Flash,KimiK25,InceptionMercury2,OpenRouterHunterAlpha"
```

## Phase 4: BYOK Credential Storage and Per-User Routing

Do this after platform-key routing is stable. Otherwise debugging throughput,
auth, and routing simultaneously will be unnecessarily expensive.

### 4a. Extend the secrets model for provider credentials

**Files:**
- hypervisor DB migrations
- `hypervisor/src/db/`
- `hypervisor/src/provider_gateway.rs`
- auth/API surfaces as needed

Use ADR-0003 boundaries:

- provider credentials live in hypervisor storage
- sandbox never sees raw keys
- gateway selects user credentials first, then platform fallback

Credential precedence:

1. user BYOK credential for backend family
2. platform backend credential

### 4b. Provider-specific auth flows

Start with:

- Anthropic: paste-token flow
- OpenAI: OAuth2 PKCE flow

Defer provider-specific UX polish until the storage and routing precedence are
working end to end.

### 4c. Scope routing by user

Health remains backend-wide, but credential selection becomes user-aware.
Two users can route to the same backend family with different credentials and
independent upstream rate-limit pools.

### Verify

1. User with BYOK routes through user credential.
2. User without BYOK falls back to platform credential.
3. Missing or revoked BYOK key degrades cleanly without leaking secret state.

## Phase 5: Cost Signals and Runtime Policy

This phase is optional for throughput but necessary for a complete ADR-0028
implementation.

### 5a. Expose backend cost metadata

**Files:**
- `hypervisor/src/provider_gateway.rs`
- `sandbox/src/actors/model_config.rs`
- conductor model-policy surfaces

Expose cost class and provider-family hints, but keep enforcement out of the
gateway. The gateway should report signals; higher-level policy chooses how to
use them.

### 5b. Let runtime policy choose logical models, not raw backend ids

The Settings app and conductor should continue selecting logical models or
model families. Backend routing stays a gateway concern.

This keeps user experience stable while operational routing changes underneath.

## Recommended File-Level Work Order

1. `hypervisor/src/provider_gateway.rs`
2. `hypervisor/src/config.rs`
3. `hypervisor/src/state.rs`
4. `sandbox/src/actors/model_config.rs`
5. `sandbox/config/model-catalog.example.toml`
6. `sandbox/tests/model_provider_live_test.rs`
7. `scripts/ops/validate-local-provider-matrix.sh`
8. docs/runbooks updated after code paths stabilize

## Risks and Guardrails

1. Do not let the sandbox choose provider account or region directly.
   That hard-codes operational routing into the runtime and makes BYOK harder.
2. Do not keep expanding URL-substring auth inference.
   It will become unmaintainable once multiple accounts exist per provider.
3. Do not mix BYOK rollout into Phase 1.
   The first deployable goal is lower latency under concurrent load.
4. Do not add complex circuit-breaker logic yet.
   Sliding-window health plus weighted round-robin is enough for the first ship.

## Verification Matrix

Minimum evidence before calling ADR-0028 implemented:

1. Unit/integration tests cover backend selection, degrade-on-429, and recovery.
2. `sandbox/tests/model_provider_live_test.rs` passes for the enabled provider set.
3. `scripts/ops/validate-local-provider-matrix.sh` passes for model and gateway lanes.
4. A concurrency verifier demonstrates lower p50/p95 latency with multi-region
   Bedrock than the single-region baseline under comparable load.
5. Logs or metrics show requests distributed across backend ids.

## Suggested Rollout

1. Ship Phase 1 with platform Bedrock multi-region only.
2. Re-run the 38+ concurrent request scenario from ADR-0028.
3. If latency remains dominated by one provider family, add Phase 2 direct
   Anthropic overflow.
4. Only then expand BYOK and broader provider families.
