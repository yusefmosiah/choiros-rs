# Self-Hosting Introspection: Choir Modifying Choir

**Date:** 2026-02-08
**Status:** Design Document
**Depends On:** Capability Actor Architecture

---

## Core Vision

> ChoirOS can see, understand, and modify its own code.
> Changes are tested in isolated sandboxes before live deployment.
> Nix provides reproducible builds and rollback capability.

---

## The Introspection Stack

### Layer 1: Prompt Visibility

All system prompts are first-class artifacts:

```rust
pub struct SystemPrompt {
    pub prompt_id: PromptId,
    pub name: String,
    pub description: String,
    pub template: String,                    // Tera/Jinja template
    pub variables: Vec<PromptVariable>,      // Expected inputs
    pub version: u32,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub safety_classification: SafetyLevel,  // critical, standard, experimental
}
```

**Storage:**
- Source of truth: `sandbox/src/prompts/*.md` (in repo)
- Runtime cache: SQLite table `system_prompts`
- Editable via: Choir UI → PromptEditorApp

**Prompt Safety:**
```rust
pub struct PromptSafetyGuardrail {
    pub check_type: PromptSafetyCheck,
    pub enforcement: EnforcementLevel,  // block, warn, log
}

pub enum PromptSafetyCheck {
    NoSystemInstructionLeakage,     // Don't reveal system internals
    NoCapabilityEscalation,         // Don't add capabilities without approval
    NoOutputFormatViolation,        // Must maintain parseable output
    SchemaCompatibility,            // Changes must maintain backward compat
}
```

---

### Layer 2: Code Introspection

ChoirOS can read and understand its own codebase:

```rust
pub struct CodeIntrospectionActor {
    repo_path: PathBuf,           // /Users/wiz/choiros-rs (local) or /app (deployed)
    ast_index: RustAstIndex,      // Parsed Rust modules
    capability_registry: CapabilityRegistry,
}

pub enum CodeIntrospectionMsg {
    // Query capabilities
    ListCapabilities {
        reply: RpcReplyPort<Vec<CapabilityDefinition>>,
    },

    // Find where a capability is implemented
    FindCapabilityImplementation {
        capability_id: CapabilityId,
        reply: RpcReplyPort<Vec<CodeLocation>>,
    },

    // Get system prompt source
    GetPromptSource {
        prompt_id: PromptId,
        reply: RpcReplyPort<String>,
    },

    // Analyze dependencies between actors
    GetActorDependencyGraph {
        reply: RpcReplyPort<DependencyGraph>,
    },

    // Read any source file
    ReadSourceFile {
        path: RelativePathBuf,    // Relative to repo root
        reply: RpcReplyPort<Result<String, FileError>>,
    },
}
```

**UI: System Browser App**
- Tree view of all actors
- Click to see source code (with syntax highlighting)
- See which prompts each actor uses
- See event types emitted/consumed

---

### Layer 3: Safe Self-Modification

Changes go through a safety pipeline:

```
User/Agent requests change
        │
        ▼
┌─────────────────────┐
│   Change Analyzer   │  ← Parse what will be modified
│  (static analysis)  │
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│  Safety Guardrails  │  ← LLM + prompt-based checks
│   (PromptSafety)    │
└──────────┬──────────┘
           │
      ┌────┴────┐
      ▼         ▼
  [APPROVED]  [REJECTED]
      │         │
      ▼         ▼
┌──────────┐  ┌──────────┐
│  Spawn   │  │  HITL    │  ← Human approval for rejected changes
│ Headless │  │Escalation│
│ Sandbox  │  │          │
└────┬─────┘  └────┬─────┘
     │             │
     ▼             ▼
┌──────────────────────────┐
│    Build + Test Suite    │  ← Nix reproducible build
│   (isolated sandbox)     │
└──────────┬───────────────┘
           │
      ┌────┴────┐
      ▼         ▼
  [PASS]    [FAIL]
      │         │
      ▼         ▼
┌──────────┐  ┌──────────┐
│ Browser  │  │ Report   │
│ Routing  │  │ Failure  │
│   +      │  │          │
│ Hydration│  │          │
└──────────┘  └──────────┘
```

**Headless Sandbox:**
```rust
pub struct HeadlessTestSandbox {
    sandbox_id: SandboxId,
    modified_code: CodeSnapshot,
    nix_derivation: Derivation,
    test_results: TestResults,
}

pub enum HeadlessSandboxMsg {
    Spawn {
        base_code: CodeSnapshot,
        modifications: Vec<CodeModification>,
        reply: RpcReplyPort<SandboxHandle>,
    },

    Build {
        handle: SandboxHandle,
        reply: RpcReplyPort<BuildResult>,
    },

    RunTests {
        handle: SandboxHandle,
        test_suite: TestSuite,
        reply: RpcReplyPort<TestResults>,
    },

    CompareBehavior {
        handle: SandboxHandle,
        baseline: BehaviorSnapshot,
        scenarios: Vec<TestScenario>,
        reply: RpcReplyPort<BehaviorComparison>,
    },
}
```

---

### Layer 4: Live Deployment with Rollback

On test success:

```rust
pub struct DeploymentCoordinator {
    current_version: Version,
    new_version: Version,
    rollback_snapshot: SystemSnapshot,
}

pub enum DeploymentMsg {
    // Gradual rollout
    CanaryDeploy {
        new_code: CodeSnapshot,
        traffic_percentage: f32,  // Start at 0%, go to 100%
        reply: RpcReplyPort<DeploymentStatus>,
    },

    // Full cutover with hydration
    Cutover {
        preserve_sessions: bool,  // Hydrate new sandbox with browser state
        reply: RpcReplyPort<CutoverResult>,
    },

    // Instant rollback
    Rollback {
        to_version: Version,
        reason: RollbackReason,
        reply: RpcReplyPort<RollbackResult>,
    },
}
```

**Browser State Hydration:**
```rust
pub struct BrowserState {
    pub session_id: SessionId,
    pub open_apps: Vec<AppState>,
    pub event_seq: i64,           // Last processed event
    pub user_preferences: Preferences,
    pub pending_confirmations: Vec<ConfirmationId>,  // Active HITL
}

// On cutover: serialize from old sandbox, deserialize to new
pub async fn hydrate_new_sandbox(
    old_state: Vec<BrowserState>,
    new_sandbox: &SandboxHandle,
) -> Result<(), HydrationError> {
    // Replay events to reconstruct state
    // Re-subscribe to confirmations
    // Resume interrupted workflows
}
```

---

## Safety Guardrails for Self-Modification

### Prompt Safety (LLM-Based)

Before applying any prompt change:

```rust
pub struct PromptSafetyAgent;

impl PromptSafetyAgent {
    async fn evaluate_prompt_change(
        &self,
        old_prompt: &SystemPrompt,
        new_prompt: &SystemPrompt,
    ) -> SafetyEvaluation {
        let checks = vec![
            self.check_no_capability_escalation(&old_prompt, &new_prompt).await,
            self.check_output_schema_preserved(&old_prompt, &new_prompt).await,
            self.check_no_prompt_injection_vectors(&new_prompt).await,
            self.check_no_system_instruction_leakage(&new_prompt).await,
        ];

        SafetyEvaluation::from_checks(checks)
    }
}
```

### Code Safety (Static + LLM)

```rust
pub struct CodeSafetyAgent;

impl CodeSafetyAgent {
    async fn evaluate_code_change(
        &self,
        diff: &CodeDiff,
    ) -> SafetyEvaluation {
        let static_checks = vec![
            self.check_compiles(diff),
            self.check_no_unsafe_increase(diff),
            self.check_test_coverage(diff),
            self.check_no_secrets(diff),
        ];

        let llm_checks = vec![
            self.check_behavioral_compatibility(diff).await,
            self.check_no_capability_escalation(diff).await,
            self.check_security_implications(diff).await,
        ];

        SafetyEvaluation::from_checks([static_checks, llm_checks].concat())
    }
}
```

---

## Nix Integration (Future)

```nix
# flake.nix for ChoirOS
{
  description = "ChoirOS - Self-Hosting Agent Operating System";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, nixpkgs, crane }: {
    # Reproducible builds
    packages.choir-sandbox = crane.lib.buildPackage {
      src = ./sandbox;
      buildInputs = [ /* Rust deps */ ];
    };

    # Headless test sandbox
    packages.choir-test = crane.lib.buildPackage {
      src = ./sandbox;
      buildType = "test";
      doCheck = true;
    };

    # Rollback generations
    packages.choir-v1 = self.packages.choir-sandbox.override { src = inputs.choir-v1; };
    packages.choir-v2 = self.packages.choir-sandbox.override { src = inputs.choir-v2; };

    # Deployment
    nixosModules.choir = import ./nix/module.nix;
  };
}
```

**Benefits:**
- Reproducible builds (same code = same binary)
- Rollback to any previous generation
- Declarative system configuration
- Binary cache for fast deployments

---

## Deferred Integrations (Duly Noted)

HITL can work with many channels:
- Email (Resend + mymx) ← First
- Slack
- Discord
- SMS (Twilio)
- Push notifications
- Calendar invites
- Video calls (Daily.co)

All implement the same `HumanInTheLoop` trait.

---

## UI: The System Prompts App

```
┌─────────────────────────────────────────────────────────────┐
│  ChoirOS System Prompts                                      │
├─────────────────────────────────────────────────────────────┤
│  Filter: [All] [Critical] [Standard] [Experimental]         │
├─────────────────────────────────────────────────────────────┤
│  Prompt Name          │ Used By      │ Version │ Safety    │
├───────────────────────┼──────────────┼─────────┼───────────┤
│  chat_agent_system    │ ChatAgent    │ v12     │ 🟡 Std    │
│  git_commit_review    │ GitActor     │ v3      │ 🔴 Crit   │
│  verifier_critic      │ Verifier     │ v7      │ 🟡 Std    │
│  hitl_email_template  │ HITLActor    │ v1      │ 🔴 Crit   │
├─────────────────────────────────────────────────────────────┤
│  [View] [Edit] [Test] [History] [Rollback]                  │
└─────────────────────────────────────────────────────────────┘

Edit Mode:
┌─────────────────────────────────────────────────────────────┐
│  Editing: chat_agent_system (v12 → v13)                     │
├─────────────────────────────────────────────────────────────┤
│  Template:                                                  │
│  ┌───────────────────────────────────────────────────────┐  │
│  │ You are a helpful assistant...                        │  │
│  │ [editable markdown]                                   │  │
│  └───────────────────────────────────────────────────────┘  │
├─────────────────────────────────────────────────────────────┤
│  Variables: {{user_name}}, {{context}}, {{capabilities}}    │
├─────────────────────────────────────────────────────────────┤
│  Safety Check: [Run]                                        │
│  ├─ ✓ No capability escalation                              │
│  ├─ ✓ Output schema preserved                               │
│  ├─ ⚠ Consider adding retry guidance                        │
│  └─ ✓ No instruction leakage                                │
├─────────────────────────────────────────────────────────────┤
│  [Test in Sandbox] [Stage for Deploy] [Cancel]              │
└─────────────────────────────────────────────────────────────┘
```

---

## Implementation Order

**Phase C (Current):**
1. Capability Actor Architecture (done)
2. StateIndexActor (compressed state)
3. PromptRegistry (read-only for now)
4. PromptEditorApp (UI for viewing prompts)

**Phase D (After Core Apps):**
1. CodeIntrospectionActor
2. SystemBrowserApp (view source)
3. HeadlessTestSandbox
4. Prompt modification pipeline

**Phase E (Nix DevOps):**
1. Nix flake for reproducible builds
2. Deployment coordinator
3. Browser state hydration
4. Full self-modification loop

---

## Open Questions

1. **Prompt versioning:** How many versions to keep? Pruning strategy?
2. **Concurrent edits:** Locking for prompt editing?
3. **Test coverage:** What constitutes sufficient testing for prompt changes?
4. **Hydration fidelity:** Can we perfectly reconstruct state, or are there edge cases?
5. **Nix + Rust:** How to handle Cargo.lock in Nix builds?

---

## Summary

| Feature | Status | Phase |
|---------|--------|-------|
| Prompt visibility | Design | C |
| Prompt editing | Design | D |
| Prompt safety guardrails | Design | D |
| Code introspection | Design | D |
| Headless test sandbox | Design | D |
| Nix builds | Future | E |
| Live deployment | Future | E |
| Self-modification loop | Future | E |
