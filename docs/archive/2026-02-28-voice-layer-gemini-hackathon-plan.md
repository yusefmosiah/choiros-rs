# VoiceLayer Architecture Plan

**Status:** NEEDS REVISION

**Date:** 2026-02-28
**Context:** Gemini Live Agent Challenge hackathon (March 16 deadline) with $100 Google Cloud credits
**Primary Goal:** Product vision - voice as a general interface into ChoirOS living documents
**Secondary Goal:** Hackathon qualification with Google Cloud deployment

---

## Narrative Summary (1-minute read)

VoiceLayer is a standalone service that makes ChoirOS accessible via voice. It runs on Google Cloud (Cloud Run) during the hackathon, uses Gemini Live API for bidirectional audio, and calls simple HTTP endpoints on the OVH-hosted ChoirOS core. If Google Cloud credits expire, the same container deploys to OVH with minimal changes.

Key insight: VoiceLayer is a **client** of ChoirOS agents (Conductor, Writer), not part of the agentic system. It does language understanding locally (via Gemini), then calls fast structured HTTP APIs - no added LLM roundtrip latency.

---

## What Changed

Previous thinking considered voice as "just another input channel" or "natural language API". This plan clarifies:

1. **VoiceLayer is external** - decoupled from ChoirOS core, deploys separately
2. **LLM lives in VoiceLayer** - Gemini Live API handles speech→intent, not ChoirOS APIs
3. **ChoirOS APIs are fast/structured** - GET semantic-unit, POST execute-task return immediately
4. **Agentic work is async** - Conductor/Writer do LLM work in background, VoiceLayer subscribes for progress

---

## What To Do Next

### Phase 1: VoiceLayer Foundation (Days 1-6)
Create new `voice-layer/` crate:
- `src/main.rs` - Axum server entry
- `src/gemini/client.rs` - Gemini Live API WebSocket client
- `src/webrtc/server.rs` - WebRTC peer connections from browser
- `src/choir_client.rs` - HTTP client to OVH ChoirOS
- `src/intent.rs` - Intent interpretation using Gemini
- `src/document_context.rs` - Living document semantic unit fetch

### Phase 2: ChoirOS API Extensions (Days 7-9)
Add fast structured endpoints to `sandbox/src/api/`:
- `GET /api/documents/:id/semantic-unit?at_position=X` - Returns paragraph/section containing position
- `GET /api/documents/:id/state` - Returns cursor position, active run
- `GET /api/runs/:id/state` - Returns structured run status
- `POST /api/conductor/execute` - Accepts JSON, returns run_id immediately
- Extend WebSocket for external client subscriptions

### Phase 3: Frontend Voice Integration (Days 10-13)
In `dioxus-desktop/src/voice/`:
- `mic_button.rs` - Voice input component
- `webrtc_client.rs` - Browser WebRTC to GCP VoiceLayer
- `transcript.rs` - Live transcript display

Flow: User clicks mic → WebRTC to GCP → Gemini Live API → VoiceLayer → HTTP to OVH → Events back via WebSocket

### Phase 4: Google Cloud Deployment (Days 14-16)
- `voice-layer/Dockerfile` - Container build
- `deploy/gcp/voice-layer.yaml` - Cloud Run service definition
- `scripts/deploy/gcp-voice.sh` - Deployment script

### Phase 5: Polish (Day 17)
- Interruption handling refinement
- Demo video recording
- README with spin-up instructions

---

## Architecture

```
┌───────────────────────────────────────────────────────────────────────┐
│                    VoiceLayer (Google Cloud)                         │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │  VoiceActor                                                      │ │
│  │  ┌─────────────┐  ┌─────────────────┐  ┌─────────────────────┐  │ │
│  │  │ WebRTC      │──│ Gemini Live API │──│ Intent Router       │  │ │
│  │  │ from browser│  │ (bidi audio)    │  │ (language understanding)│ │
│  │  └─────────────┘  └─────────────────┘  └──────────┬────────────┘  │ │
│  │                                                   │               │ │
│  │                          ┌────────────────────────┘               │ │
│  │                          ▼                                        │ │
│  │  ┌──────────────────────────────────────────────────────────┐    │ │
│  │  │  ChoirOS HTTP Client (fast structured APIs)              │    │ │
│  │  │  - GET /api/documents/:id/semantic-unit                 │    │ │
│  │  │  - POST /api/conductor/execute                          │    │ │
│  │  │  - GET /api/runs/:id/state                              │    │ │
│  │  └──────────────────────────────────────────────────────────┘    │ │
│  └─────────────────────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────────────────────┘
                                    │
                                    │ WebSocket (text/events only)
                                    ▼
┌───────────────────────────────────────────────────────────────────────┐
│                    ChoirOS Core (OVH Bare Metal)                      │
│  ┌───────────┐    ┌───────────┐    ┌─────────────────────────────┐   │
│  │ Conductor │◀──▶│ EventBus  │◀──▶│ Terminal/Researcher Workers │   │
│  │           │    │           │    │ (LLM work async)            │   │
│  └─────┬─────┘    └───────────┘    └─────────────────────────────┘   │
│        │                                                             │
│        ▼                                                             │
│  ┌───────────┐    ┌───────────┐    ┌─────────────────────────────┐   │
│  │ Writer    │◀──▶│ EventStore│    │ dioxus-desktop UI           │   │
│  │           │    │           │    └─────────────────────────────┘   │
│  └───────────┘    └───────────┘                                      │
└───────────────────────────────────────────────────────────────────────┘
```

---

## VoiceLayer Responsibilities

1. **Audio Interface**: WebRTC from browser, bidirectional PCM streaming with Gemini Live API
2. **Language Understanding**: Gemini Live API converts speech → structured intent
3. **Fast API Calls**: Calls ChoirOS HTTP endpoints (no LLM in request path)
4. **Living Document Context**: Fetches semantic unit at cursor position (not whole document)
5. **Response Formatting**: Converts structured API responses → natural speech via Gemini
6. **Latency Compensation**: Live audio feels responsive even when MAS takes time

---

## VoiceLayer is NOT

- **NOT an orchestrator**: Doesn't spawn workers, doesn't know about supervision tree
- **NOT a transport**: Does more than STT/TTS - interprets intent, provides context
- **NOT voice-aware to core agents**: Conductor/Writer behave identically whether input came from voice or text UI
- **NOT calling natural language APIs**: Uses fast structured HTTP endpoints

---

## Living Document Integration

### The Problem

Voice conversations about documents get unwieldy if you feed the entire document into context every turn. A 1000-line document overwhelms the conversation.

### The Solution: Semantic Unit at First Change

```rust
// VoiceLayer asks Writer for minimal context
pub struct SemanticUnitRequest {
    pub document_id: String,
    pub position: Option<usize>,  // If known, else latest change
}

pub struct SemanticUnitResponse {
    pub unit_text: String,        // The paragraph/section with the change
    pub unit_type: SemanticUnitType,  // Paragraph, Section, CodeBlock, etc.
    pub surrounding_context: String,  // 1-2 sentences before/after
}

// Example: User says "change that to use serde"
// VoiceLayer detects reference to "that" → queries Writer
// Writer returns: the code block where cursor is, not whole file
```

### Example Flow

```
User: "Update the authentication logic"
  ▼
VoiceLayer: Does this refer to a document?
  - Check active_document_id from ChoirOS
  - Fetch semantic unit at cursor position
  ▼
VoiceLayer → Gemini Live API:
  "User wants to update authentication logic.
   Current code at cursor:
   ```rust
   fn login() { ... }
   ```
   How should I proceed?"
  ▼
Gemini: "I'll help update that. Should I add JWT support or session-based?"
  ▼
VoiceLayer: TTS response to user
User: "JWT"
  ▼
VoiceLayer → ChoirOS: POST /api/conductor/execute
  { "objective": "Update authentication logic to use JWT" }
  ▼
ChoirOS returns: { "run_id": "run_123" } immediately
  ▼
VoiceLayer: "Started. I'll let you know when it's done."
  ▼
VoiceLayer subscribes to WebSocket for progress events
```

---

## Intent Router

```rust
enum VoiceIntent {
    // Execute a task - calls POST /api/conductor/execute with JSON
    ExecuteTask { objective: String, context: TaskContext },

    // Get document context - calls GET /api/documents/:id/semantic-unit
    GetDocumentContext { document_id: String, position: usize },

    // Check run status - calls GET /api/runs/:id/state
    CheckStatus { run_id: String },

    // Conversation not requiring action
    Conversation { response: String },

    // Interruption
    BargeIn { action: BargeInAction },
}

impl VoiceLayer {
    async fn route_intent(
        &self,
        transcript: &str,
        context: &VoiceContext,
    ) -> Result<()> {
        // Language understanding happens in VoiceLayer (via Gemini Live API)
        let intent = self.interpret_with_gemini(transcript, context).await?;

        match intent {
            ExecuteTask { objective, context } => {
                // Fast POST, returns run_id immediately, no LLM roundtrip
                let run_id = self.choir_client.post_json(
                    "/api/conductor/execute",
                    json!({ "objective": objective, "context": context })
                ).await?;

                // Subscribe to WebSocket for async progress
                self.subscribe_to_run(run_id).await?;
                self.speak("Started. I'll let you know when it's done.").await;
            }
            GetDocumentContext { document_id, position } => {
                // Fast GET, returns structured data immediately
                let unit: SemanticUnit = self.choir_client.get(
                    &format!("/api/documents/{}/semantic-unit?at_position={}",
                             document_id, position)
                ).await?;

                // Format with Gemini for speech
                let summary = self.gemini.summarize_for_voice(unit.text).await?;
                self.speak(summary).await;
            }
            // ... etc
        }
        Ok(())
    }
}
```

---

## Google Cloud Integration

### What Deploys Where

**OVH Bare Metal (unchanged):**
- ChoirOS core: Conductor, Writer, Terminal, EventBus, EventStore
- All supervision trees, all workers
- dioxus-desktop UI served from here

**Google Cloud (new):**
- VoiceLayer service (Cloud Run)
- Gemini Live API client
- Optional: GCS bucket for audio recordings, document exports

### Connection: OVH ↔ GCP

VoiceLayer connects to OVH as a **client**:
```
VoiceActor (GCP) ──WebSocket──▶ OVH ChoirOS API
  - Calls POST /api/conductor/execute
  - Calls GET /api/documents/:id/semantic-unit
  - Subscribes to EventBus via WebSocket for progress
```

### Lock-in Mitigation

| Component | Google Choice | Migration Path |
|-----------|--------------|----------------|
| Live audio | Gemini Live API (ADK) | Swap for OpenAI Realtime, Whisper, or local |
| Object Storage | GCS (optional) | MinIO (S3-compatible) on OVH |
| VoiceLayer compute | Cloud Run | Same container on OVH Docker |

### $100 Budget Allocation

| Service | Estimated Monthly Cost | Purpose |
|---------|----------------------|---------|
| Cloud Run (1 vCPU, 512MB) | ~$15-30 | VoiceLayer container |
| Cloud Storage (optional, 5GB) | ~$0.10 | Audio recordings, exports |
| Gemini Live API | Free tier + credits | Bidirectional audio |
| **Total** | **< $50/mo** | Well within $100 credit |

---

## Migration Path (if dropping GCP)

1. Deploy `voice-layer` container on OVH alongside sandbox
2. Update dioxus-desktop WebRTC config to point to OVH instead of GCP
3. Swap `GeminiLiveClient` for `WhisperLiveClient` or similar
4. Zero changes to ChoirOS core

---

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| WebRTC complexity | Use `webrtc-rs` crate, start with data channel, add audio |
| Gemini Live API latency | Live audio covers it; use text for heavy operations |
| OVH ↔ GCP connection | WebSocket with auto-reconnect, queue during reconnect |
| 17-day timeline | Scope to: voice commands, document queries, interruptions |
| GCP costs | Cloud Run scales to zero, monitor daily |

---

## Key Files

### VoiceLayer (new crate)
- `voice-layer/src/main.rs` - Axum server, WebRTC setup
- `voice-layer/src/actor.rs` - VoiceActor (internal ractor state machine)
- `voice-layer/src/gemini/client.rs` - Gemini Live API WebSocket
- `voice-layer/src/choir_client.rs` - HTTP client to OVH ChoirOS
- `voice-layer/src/intent.rs` - Intent interpretation
- `voice-layer/src/document_context.rs` - Living document integration

### ChoirOS Modifications (minimal)
- `sandbox/src/api/writer.rs` - Add semantic unit endpoint
- `sandbox/src/actors/writer/mod.rs` - Add GetSemanticUnit message
- `sandbox/src/api/event_bus_ws.rs` - Support external client subscriptions

### Frontend
- `dioxus-desktop/src/voice/` - Voice UI components
- `dioxus-desktop/src/voice/webrtc_client.rs` - Browser WebRTC to GCP

---

## Related Plans

See companion plans for broader context:

- `2026-02-28-wave-plan-local-to-ovh-bootstrap.md` - Deployment sequence
- `2026-02-28-3-tier-gap-closure-plan.md` - Tier structure alignment
- `2026-02-28-cutover-stocktake-and-pending-work.md` - Cutover status
- `2026-02-28-local-cutover-status-and-next-steps.md` - Local cutover next steps

---

*Plan created for Gemini Live Agent Challenge. Deadline: March 16, 2026.*
