//! Shared types between frontend and backend
//!
//! These types are used by both:
//! - Actix actors (native Rust)
//! - Dioxus components (WASM)
//!
//! Serializable with serde for JSON over WebSocket/HTTP

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ============================================================================
// Core Types
// ============================================================================

/// Unique identifier for actors
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ActorId(pub String);

impl ActorId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for ActorId {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Event System
// ============================================================================

/// Event - append-only log entry
/// All state changes are logged as events
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct Event {
    /// Global sequence number (strictly increasing)
    pub seq: i64,

    /// Unique event ID (ULID)
    pub event_id: String,

    /// When the event occurred
    pub timestamp: DateTime<Utc>,

    /// Which actor produced this event
    pub actor_id: ActorId,

    /// Event type (e.g., "chat.user_msg", "file.write")
    pub event_type: String,

    /// Event-specific payload
    #[ts(type = "unknown")]
    pub payload: serde_json::Value,

    /// Which user triggered this (for audit)
    pub user_id: String,
}

/// Request to append an event
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct AppendEvent {
    pub event_type: String,
    #[ts(type = "unknown")]
    pub payload: serde_json::Value,
    pub actor_id: ActorId,
    pub user_id: String,
}

/// Query events for an actor
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct QueryEvents {
    pub actor_id: ActorId,
    pub since_seq: i64,
}

// ============================================================================
// Actor Messages
// ============================================================================

/// Messages that can be sent to ChatActor
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum ChatMsg {
    /// User typed a message
    UserTyped { text: String, window_id: String },

    /// Assistant responded
    AssistantReply { text: String, model: String },

    /// Tool was called
    ToolCall {
        tool: String,
        #[ts(type = "unknown")]
        args: serde_json::Value,
        call_id: String,
    },

    /// Tool returned result
    ToolResult {
        call_id: String,
        status: ToolStatus,
        #[ts(type = "unknown")]
        output: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum ToolStatus {
    Success,
    Error(String),
}

/// Messages that can be sent to WriterActor  
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WriterMsg {
    CreateDoc { title: String },
    EditFile { path: String, content: String },
    ReadFile { path: String },
}

// ============================================================================
// UI State
// ============================================================================

/// Desktop state - all windows and their positions
#[derive(Debug, Clone, Serialize, Deserialize, Default, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct DesktopState {
    pub windows: Vec<WindowState>,
    pub active_window: Option<String>,
    pub apps: Vec<AppDefinition>,
}

/// Individual window state
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct WindowState {
    pub id: String,
    pub app_id: String, // "chat", "writer", "mail", etc.
    pub title: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub z_index: u32,
    pub minimized: bool,
    pub maximized: bool,
    #[ts(type = "unknown")]
    pub props: serde_json::Value, // App-specific data
}

/// App definition for dynamic app registration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct AppDefinition {
    pub id: String,
    pub name: String,
    pub icon: String,           // emoji or SVG
    pub component_code: String, // Source code or WASM path
    pub default_width: i32,
    pub default_height: i32,
}

/// Chat message for UI display
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ChatMessage {
    pub id: String,
    pub text: String,
    pub sender: Sender,
    pub timestamp: DateTime<Utc>,
    pub pending: bool, // True if optimistic (not confirmed by actor yet)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum Sender {
    User,
    Assistant,
    System,
}

// ============================================================================
// Viewer Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum ViewerKind {
    Text,
    Image,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ViewerResource {
    pub uri: String,
    pub mime: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ViewerCapabilities {
    pub readonly: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ViewerDescriptor {
    pub kind: ViewerKind,
    pub resource: ViewerResource,
    pub capabilities: ViewerCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ViewerRevision {
    pub rev: i64,
    pub updated_at: String,
}

// ============================================================================
// API Types
// ============================================================================

/// Generic API response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

/// WebSocket message protocol
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum WsMsg {
    /// Client → Server: Subscribe to actor events
    Subscribe { actor_id: ActorId },

    /// Client → Server: Send message to actor
    Send {
        actor_id: ActorId,
        #[ts(type = "unknown")]
        payload: serde_json::Value,
    },

    /// Server → Client: Event occurred
    Event { actor_id: ActorId, event: Event },

    /// Server → Client: Current state snapshot
    State {
        actor_id: ActorId,
        #[ts(type = "unknown")]
        state: serde_json::Value,
    },

    /// Server → Client: Error occurred
    Error { message: String },
}

// ============================================================================
// Tool Definitions
// ============================================================================

/// Tool definition for LLM
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    #[ts(type = "unknown")]
    pub parameters: serde_json::Value, // JSON Schema
}

/// Tool call from LLM
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ToolCall {
    pub id: String,
    pub tool: String,
    #[ts(type = "unknown")]
    pub args: serde_json::Value,
}

// ============================================================================
// Control Plane Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum WorkerTurnStatus {
    Running,
    Completed,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct WorkerFinding {
    pub finding_id: String,
    pub claim: String,
    pub confidence: f64,
    pub evidence_refs: Vec<String>,
    pub novel: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct WorkerLearning {
    pub learning_id: String,
    pub insight: String,
    pub confidence: f64,
    pub supports: Vec<String>,
    pub changes_plan: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum WorkerEscalationKind {
    Blocker,
    Help,
    Approval,
    Conflict,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum WorkerEscalationUrgency {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct WorkerEscalation {
    pub escalation_id: String,
    pub kind: WorkerEscalationKind,
    pub reason: String,
    pub urgency: WorkerEscalationUrgency,
    pub options: Vec<String>,
    pub recommended_option: Option<String>,
    pub requires_human: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct WorkerArtifact {
    pub artifact_id: String,
    pub kind: String,
    pub reference: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct WorkerTurnReport {
    pub turn_id: String,
    pub worker_id: String,
    pub task_id: String,
    pub worker_role: Option<String>,
    pub status: WorkerTurnStatus,
    pub summary: Option<String>,
    pub findings: Vec<WorkerFinding>,
    pub learnings: Vec<WorkerLearning>,
    pub escalations: Vec<WorkerEscalation>,
    pub artifacts: Vec<WorkerArtifact>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum WorkerSignalType {
    Finding,
    Learning,
    Escalation,
    Artifact,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum WorkerSignalRejectReason {
    MaxPerTurnExceeded,
    LowConfidence,
    MissingEvidence,
    DuplicateWithinWindow,
    EscalationCooldown,
    InvalidPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct WorkerSignalRejection {
    pub signal_type: WorkerSignalType,
    pub signal_id: String,
    pub reason: WorkerSignalRejectReason,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct WorkerTurnReportIngestResult {
    pub accepted_findings: usize,
    pub accepted_learnings: usize,
    pub accepted_escalations: usize,
    pub accepted_artifacts: usize,
    pub escalation_notified: bool,
    pub rejections: Vec<WorkerSignalRejection>,
}

/// Status of an objective during agent execution
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum ObjectiveStatus {
    /// Objective complete, final_response required
    Satisfied,
    /// Still working, tool_calls allowed
    InProgress,
    /// Cannot proceed, completion_reason required
    Blocked,
}

/// Planning mode for agent execution control
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum PlanMode {
    /// Execute tool calls
    CallTools,
    /// Synthesize final response
    Finalize,
    /// Escalate to parent/supervisor
    Escalate,
}

/// Classification of failure types for error handling and retry logic
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum FailureKind {
    Timeout,    // Time limit exceeded
    Network,    // Connectivity issues
    Auth,       // Authentication/authorization failed
    RateLimit,  // Rate limit hit
    Validation, // Input validation failed
    Provider,   // Upstream provider error
    Unknown,    // Unclassified failure
}

/// Contract defining an objective for parent-child delegation
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ObjectiveContract {
    pub objective_id: String,                // Unique objective identifier
    pub parent_objective_id: Option<String>, // Hierarchy linkage
    pub primary_objective: String,           // What to accomplish
    pub success_criteria: Vec<String>,       // Measurable completion criteria
    pub constraints: ObjectiveConstraints,   // Budgets, timeouts, policies
    pub attempts_budget: u8,                 // Max retry attempts
    pub evidence_requirements: EvidenceRequirements, // What evidence to collect
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ObjectiveConstraints {
    pub max_tool_calls: u32,
    pub timeout_ms: u64,
    pub max_subframe_depth: u8,
    pub allowed_capabilities: Vec<String>, // Capability whitelist
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct EvidenceRequirements {
    pub requires_citations: bool,
    pub min_confidence: f64,
    pub required_source_types: Vec<String>,
}

/// Payload for child-to-parent completion reporting
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct CompletionPayload {
    pub objective_status: ObjectiveStatus,
    pub objective_fulfilled: bool, // Explicit completion boolean
    pub completion_reason: String, // Why completed/blocked
    pub evidence: Vec<Evidence>,   // Structured evidence
    pub unresolved_items: Vec<UnresolvedItem>, // What remains undone
    pub recommended_next_action: Option<NextAction>, // Suggested continuation
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct Evidence {
    pub evidence_id: String,
    pub evidence_type: EvidenceType,
    pub source: String,
    pub content: String,
    pub confidence: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct UnresolvedItem {
    pub item_id: String,
    pub description: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct NextAction {
    pub action_type: NextActionType, // escalate | continue | complete
    pub recommended_capability: Option<String>,
    pub recommended_objective: Option<String>,
    pub rationale: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum EvidenceType {
    SearchResult,
    CodeSnippet,
    Documentation,
    TerminalOutput,
    FileContent,
    WebPage,
    Other,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum NextActionType {
    Escalate,
    Continue,
    Complete,
}

// ============================================================================
// Conductor Runtime Types (Phase A: Agentic Readiness)
// ============================================================================

/// Event lane metadata for conductor/runtime processing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum EventLane {
    /// Event is part of orchestration control flow.
    Control,
    /// Event is telemetry only.
    Telemetry,
}

/// Importance level for events
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum EventImportance {
    Low,
    Normal,
    High,
}

/// Event metadata for control/telemetry lane separation
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct EventMetadata {
    /// Control/telemetry lane for this event
    pub lane: EventLane,
    /// Importance level
    pub importance: EventImportance,
    /// Run ID for grouping related work
    pub run_id: Option<String>,
    /// Capability call ID
    pub call_id: Option<String>,
    /// Which capability produced this event
    pub capability: Option<String>,
    /// Execution phase
    pub phase: Option<String>,
}

impl Default for EventMetadata {
    fn default() -> Self {
        Self {
            lane: EventLane::Telemetry,
            importance: EventImportance::Normal,
            run_id: None,
            call_id: None,
            capability: None,
            phase: None,
        }
    }
}

impl EventMetadata {
    /// Create a control-lane event with high importance
    pub fn control() -> Self {
        Self {
            lane: EventLane::Control,
            importance: EventImportance::High,
            ..Default::default()
        }
    }

    /// Create a telemetry-lane event with normal importance
    pub fn telemetry() -> Self {
        Self {
            lane: EventLane::Telemetry,
            importance: EventImportance::Normal,
            ..Default::default()
        }
    }

    /// Set the run_id
    pub fn with_run_id(mut self, run_id: impl Into<String>) -> Self {
        self.run_id = Some(run_id.into());
        self
    }

    /// Set the call_id
    pub fn with_call_id(mut self, call_id: impl Into<String>) -> Self {
        self.call_id = Some(call_id.into());
        self
    }

    /// Set the capability
    pub fn with_capability(mut self, capability: impl Into<String>) -> Self {
        self.capability = Some(capability.into());
        self
    }

    /// Set the phase
    pub fn with_phase(mut self, phase: impl Into<String>) -> Self {
        self.phase = Some(phase.into());
        self
    }
}

/// Status of a capability call
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum CapabilityCallStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Blocked,
}

/// A single item in the conductor's agenda
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ConductorAgendaItem {
    pub item_id: String,
    pub capability: String,
    pub objective: String,
    pub priority: u8,            // 0 = highest
    pub depends_on: Vec<String>, // item_ids that must complete first
    pub status: AgendaItemStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Status of an agenda item
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum AgendaItemStatus {
    Pending,
    Ready, // Dependencies satisfied, ready to run
    Running,
    Completed,
    Failed,
    Blocked,
}

/// A tracked capability call in-flight
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ConductorCapabilityCall {
    pub call_id: String,
    pub capability: String,
    pub objective: String,
    pub status: CapabilityCallStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub parent_call_id: Option<String>, // For nested calls
    pub agenda_item_id: Option<String>, // Link back to agenda
    pub artifact_ids: Vec<String>,      // Produced artifacts
    pub error: Option<String>,
}

/// A typed artifact produced during execution
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ConductorArtifact {
    pub artifact_id: String,
    pub kind: ArtifactKind,
    pub reference: String, // Path, URL, or content hash
    pub mime_type: Option<String>,
    pub created_at: DateTime<Utc>,
    pub source_call_id: String,
    #[ts(type = "unknown")]
    pub metadata: Option<serde_json::Value>,
}

/// Kinds of artifacts that can be produced
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum ArtifactKind {
    Report,
    File,
    WebPage,
    SearchResults,
    TerminalOutput,
    CodeSnippet,
    JsonData,
    Other,
}

/// A decision made by the conductor
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ConductorDecision {
    pub decision_id: String,
    pub decision_type: DecisionType,
    pub reason: String,
    pub timestamp: DateTime<Utc>,
    pub affected_agenda_items: Vec<String>,
    pub new_agenda_items: Vec<String>,
}

/// Types of decisions the conductor can make
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum DecisionType {
    /// Dispatch a capability call
    Dispatch,
    /// Retry a failed call
    Retry,
    /// Spawn a follow-up task
    SpawnFollowup,
    /// Mark run as complete
    Complete,
    /// Mark run as blocked
    Block,
    /// Continue to next agenda item
    Continue,
}

/// Full runtime state for a conductor run
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ConductorRunState {
    pub run_id: String,
    pub objective: String,
    pub status: ConductorRunStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    /// Ordered pending work items
    pub agenda: Vec<ConductorAgendaItem>,
    /// In-flight capability calls
    pub active_calls: Vec<ConductorCapabilityCall>,
    /// Typed output references
    pub artifacts: Vec<ConductorArtifact>,
    /// Typed decisions made during orchestration
    pub decision_log: Vec<ConductorDecision>,
    /// Path to the living document (draft.md)
    pub document_path: String,
    /// Output mode for final delivery
    pub output_mode: ConductorOutputMode,
    /// Desktop ID for UI coordination
    pub desktop_id: String,
}

/// Status of a conductor run
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum ConductorRunStatus {
    Initializing,
    Running,
    WaitingForCalls,
    Completing,
    Completed,
    Failed,
    Blocked,
}

// ============================================================================
// Conductor Types (Legacy - keep for compatibility)
// ============================================================================

/// Output mode for Conductor task execution
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum ConductorOutputMode {
    Auto,
    MarkdownReportToWriter,
    ToastWithReportLink,
}

/// Visual tone for prompt-bar toast output.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum ConductorToastTone {
    Info,
    Success,
    Warning,
    Error,
}

/// Typed prompt-bar toast payload for Conductor completion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ConductorToastPayload {
    pub title: String,
    pub message: String,
    pub tone: ConductorToastTone,
    pub report_path: Option<String>,
}

/// Request to execute a Conductor run.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ConductorExecuteRequest {
    pub objective: String,
    pub desktop_id: String,
    pub output_mode: ConductorOutputMode,
    #[ts(type = "unknown")]
    pub hints: Option<serde_json::Value>,
}

/// Typed error for Conductor task failures
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ConductorError {
    pub code: String,
    pub message: String,
    pub failure_kind: Option<FailureKind>,
}

/// Response from Conductor task execution
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ConductorExecuteResponse {
    pub run_id: String,
    pub status: ConductorRunStatus,
    pub document_path: Option<String>,
    pub writer_window_props: Option<WriterWindowProps>,
    pub toast: Option<ConductorToastPayload>,
    pub error: Option<ConductorError>,
}

/// Typed window props for Writer integration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct WriterWindowProps {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub path: String,
    pub preview_mode: bool,
    pub run_id: Option<String>,
}

// ============================================================================
// Writer Run Event Types (Phase A: Writer-First Cutover)
// ============================================================================

/// Single patch operation for document editing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(tag = "op", rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum PatchOp {
    Insert { pos: u64, text: String },
    Delete { pos: u64, len: u64 },
    Replace { pos: u64, len: u64, text: String },
    Retain { len: u64 },
}

/// Status for writer run events
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum WriterRunStatusKind {
    Initializing,
    Running,
    WaitingForWorker,
    Completing,
    Completed,
    Failed,
    Blocked,
}

/// Source of a patch operation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum PatchSource {
    Agent,
    User,
    System,
}

/// Base fields required on every writer run event
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct WriterRunEventBase {
    pub desktop_id: String,
    pub session_id: String,
    pub thread_id: String,
    pub run_id: String,
    pub document_path: String,
    pub revision: u64,
    pub timestamp: DateTime<Utc>,
}

/// Payload for writer.run.patch events
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct WriterRunPatchPayload {
    pub patch_id: String,
    pub source: PatchSource,
    pub section_id: Option<String>,
    pub ops: Vec<PatchOp>,
    pub proposal: Option<String>,
    pub base_version_id: Option<u64>,
    pub target_version_id: Option<u64>,
    pub overlay_id: Option<String>,
}

/// Impact level for writer.run.changeset events (mirrors BAML ImpactLevel)
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum ChangesetImpact {
    Low,
    Medium,
    High,
}

/// Payload for writer.run.changeset events (semantic summary of a document patch)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct WriterRunChangesetPayload {
    /// Correlates to the patch_id from the preceding writer.run.patch event
    pub patch_id: String,
    /// Correlates to the writer run loop id when available
    pub loop_id: Option<String>,
    /// Human-readable 1–2 sentence summary of what changed
    pub summary: String,
    /// Estimated scope of the change
    pub impact: ChangesetImpact,
    /// List of change categories present (e.g. "insert", "structural_rewrite")
    pub op_taxonomy: Vec<String>,
}

/// Full writer run event with base fields and typed payload
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "event_type", rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum WriterRunEvent {
    #[serde(rename = "writer.run.started")]
    Started {
        #[serde(flatten)]
        base: WriterRunEventBase,
        objective: String,
    },
    #[serde(rename = "writer.run.progress")]
    Progress {
        #[serde(flatten)]
        base: WriterRunEventBase,
        phase: String,
        message: String,
        progress_pct: Option<u8>,
    },
    #[serde(rename = "writer.run.patch")]
    Patch {
        #[serde(flatten)]
        base: WriterRunEventBase,
        #[serde(flatten)]
        payload: WriterRunPatchPayload,
    },
    #[serde(rename = "writer.run.changeset")]
    Changeset {
        #[serde(flatten)]
        base: WriterRunEventBase,
        #[serde(flatten)]
        payload: WriterRunChangesetPayload,
    },
    #[serde(rename = "writer.run.status")]
    Status {
        #[serde(flatten)]
        base: WriterRunEventBase,
        status: WriterRunStatusKind,
        message: Option<String>,
    },
    #[serde(rename = "writer.run.failed")]
    Failed {
        #[serde(flatten)]
        base: WriterRunEventBase,
        error_code: String,
        error_message: String,
        failure_kind: Option<FailureKind>,
    },
}

/// State tracking for a Conductor run via API
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ConductorRunStatusResponse {
    pub run_id: String,
    pub status: ConductorRunStatus,
    pub objective: String,
    pub desktop_id: String,
    pub output_mode: ConductorOutputMode,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub document_path: String,
    pub report_path: Option<String>,
    pub toast: Option<ConductorToastPayload>,
    pub error: Option<ConductorError>,
}

// ============================================================================
// Constants
// ============================================================================

/// Event types
pub const EVENT_CHAT_USER_MSG: &str = "chat.user_msg";
pub const EVENT_CHAT_ASSISTANT_MSG: &str = "chat.assistant_msg";
pub const EVENT_CHAT_TOOL_CALL: &str = "chat.tool_call";
pub const EVENT_CHAT_TOOL_RESULT: &str = "chat.tool_result";
pub const EVENT_MODEL_SELECTION: &str = "model.selection";
pub const EVENT_MODEL_CHANGED: &str = "model.changed";
pub const EVENT_MODEL_CONTEXT_TRACE: &str = "model.context.trace";

/// Build a chat user-message payload with optional session/thread scope metadata.
pub fn chat_user_payload(
    text: impl Into<String>,
    session_id: Option<String>,
    thread_id: Option<String>,
) -> serde_json::Value {
    let text = text.into();
    let mut scope = serde_json::Map::new();
    if let Some(session_id) = session_id {
        scope.insert(
            "session_id".to_string(),
            serde_json::Value::String(session_id),
        );
    }
    if let Some(thread_id) = thread_id {
        scope.insert(
            "thread_id".to_string(),
            serde_json::Value::String(thread_id),
        );
    }

    if scope.is_empty() {
        return serde_json::Value::String(text);
    }

    serde_json::json!({
        "text": text,
        "scope": scope,
    })
}

/// Parse chat user-message text from either legacy string payloads or scoped object payloads.
pub fn parse_chat_user_text(payload: &serde_json::Value) -> Option<String> {
    payload.as_str().map(ToString::to_string).or_else(|| {
        payload
            .get("text")
            .and_then(|v| v.as_str())
            .map(ToString::to_string)
    })
}

/// Attach optional scope metadata to any payload.
///
/// If no scope is provided, returns payload unchanged.
/// If payload is an object, inserts `scope`.
/// Otherwise wraps as `{ "value": <payload>, "scope": {...} }`.
pub fn with_scope(
    payload: serde_json::Value,
    session_id: Option<String>,
    thread_id: Option<String>,
) -> serde_json::Value {
    let mut scope = serde_json::Map::new();
    if let Some(session_id) = session_id {
        scope.insert(
            "session_id".to_string(),
            serde_json::Value::String(session_id),
        );
    }
    if let Some(thread_id) = thread_id {
        scope.insert(
            "thread_id".to_string(),
            serde_json::Value::String(thread_id),
        );
    }
    if scope.is_empty() {
        return payload;
    }

    match payload {
        serde_json::Value::Object(mut obj) => {
            obj.insert("scope".to_string(), serde_json::Value::Object(scope));
            serde_json::Value::Object(obj)
        }
        other => serde_json::json!({
            "value": other,
            "scope": scope,
        }),
    }
}
pub const EVENT_USER_THEME_PREFERENCE: &str = "user.theme_preference";
pub const EVENT_FILE_WRITE: &str = "file.write";
pub const EVENT_FILE_EDIT: &str = "file.edit";
pub const EVENT_ACTOR_SPAWNED: &str = "actor.spawned";
pub const EVENT_VIEWER_CONTENT_SAVED: &str = "viewer.content_saved";
pub const EVENT_VIEWER_CONTENT_CONFLICT: &str = "viewer.content_conflict";
pub const EVENT_TOPIC_WORKER_TASK_STARTED: &str = "worker.task.started";
pub const EVENT_TOPIC_WORKER_TASK_PROGRESS: &str = "worker.task.progress";
pub const EVENT_TOPIC_WORKER_TASK_COMPLETED: &str = "worker.task.completed";
pub const EVENT_TOPIC_WORKER_TASK_FAILED: &str = "worker.task.failed";
pub const EVENT_TOPIC_WORKER_REPORT_RECEIVED: &str = "worker.report.received";
pub const EVENT_TOPIC_WORKER_SIGNAL_REJECTED: &str = "worker.signal.rejected";
pub const EVENT_TOPIC_WORKER_SIGNAL_ESCALATION_REQUESTED: &str =
    "worker.signal.escalation_requested";
pub const EVENT_TOPIC_WORKER_FINDING_CREATED: &str = "worker.finding.created";
pub const EVENT_TOPIC_WORKER_LEARNING_CREATED: &str = "worker.learning.created";
pub const EVENT_TOPIC_RESEARCH_FINDING_CREATED: &str = "research.finding.created";
pub const EVENT_TOPIC_RESEARCH_LEARNING_CREATED: &str = "research.learning.created";
pub const EVENT_TOPIC_RESEARCH_TASK_STARTED: &str = "research.task.started";
pub const EVENT_TOPIC_RESEARCH_TASK_PROGRESS: &str = "research.task.progress";
pub const EVENT_TOPIC_RESEARCH_TASK_COMPLETED: &str = "research.task.completed";
pub const EVENT_TOPIC_RESEARCH_TASK_FAILED: &str = "research.task.failed";
pub const EVENT_TOPIC_RESEARCH_PROVIDER_CALL: &str = "research.provider.call";
pub const EVENT_TOPIC_RESEARCH_PROVIDER_RESULT: &str = "research.provider.result";
pub const EVENT_TOPIC_RESEARCH_PROVIDER_ERROR: &str = "research.provider.error";
pub const EVENT_TOPIC_ARTIFACT_CREATED: &str = "artifact.created";

pub const EVENT_TOPIC_CONDUCTOR_TASK_STARTED: &str = "conductor.task.started";
pub const EVENT_TOPIC_CONDUCTOR_TASK_PROGRESS: &str = "conductor.task.progress";
pub const EVENT_TOPIC_CONDUCTOR_WORKER_CALL: &str = "conductor.worker.call";
pub const EVENT_TOPIC_CONDUCTOR_WORKER_RESULT: &str = "conductor.worker.result";
pub const EVENT_TOPIC_CONDUCTOR_TASK_COMPLETED: &str = "conductor.task.completed";
pub const EVENT_TOPIC_CONDUCTOR_TASK_FAILED: &str = "conductor.task.failed";

pub const EVENT_TOPIC_WRITER_RUN_STARTED: &str = "writer.run.started";
pub const EVENT_TOPIC_WRITER_RUN_PROGRESS: &str = "writer.run.progress";
pub const EVENT_TOPIC_WRITER_RUN_PATCH: &str = "writer.run.patch";
pub const EVENT_TOPIC_WRITER_RUN_CHANGESET: &str = "writer.run.changeset";
pub const EVENT_TOPIC_WRITER_RUN_STATUS: &str = "writer.run.status";
pub const EVENT_TOPIC_WRITER_RUN_FAILED: &str = "writer.run.failed";

pub const EVENT_TOPIC_TRACE_PROMPT_RECEIVED: &str = "trace.prompt.received";
pub const EVENT_TOPIC_LLM_CALL_STARTED: &str = "llm.call.started";
pub const EVENT_TOPIC_LLM_CALL_COMPLETED: &str = "llm.call.completed";
pub const EVENT_TOPIC_LLM_CALL_FAILED: &str = "llm.call.failed";
pub const EVENT_TOPIC_WORKER_TOOL_CALL: &str = "worker.tool.call";
pub const EVENT_TOPIC_WORKER_TOOL_RESULT: &str = "worker.tool.result";

pub const INTERFACE_KIND_UACTOR_ACTOR: &str = "uactor_actor";
pub const INTERFACE_KIND_APPACTOR_TOOLACTOR: &str = "appactor_toolactor";

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use ts_rs::Config;

    #[test]
    fn test_actor_id_generation() {
        let id1 = ActorId::new();
        let id2 = ActorId::new();
        assert_ne!(id1, id2);
        assert_eq!(id1.0.len(), 36); // UUID length
    }

    #[test]
    fn test_event_serialization() {
        let event = Event {
            seq: 1,
            event_id: "evt_123".to_string(),
            timestamp: Utc::now(),
            actor_id: ActorId::new(),
            event_type: EVENT_CHAT_USER_MSG.to_string(),
            payload: serde_json::json!({"text": "Hello"}),
            user_id: "user_1".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: Event = serde_json::from_str(&json).unwrap();

        assert_eq!(event.seq, deserialized.seq);
        assert_eq!(event.event_type, deserialized.event_type);
    }

    #[test]
    fn test_ws_msg_protocol() {
        let msg = WsMsg::Subscribe {
            actor_id: ActorId::new(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("Subscribe"));
    }

    #[test]
    fn test_viewer_kind_serialization() {
        let kind = ViewerKind::Text;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, "\"text\"");
    }

    #[test]
    fn export_types() {
        // Export all types to TypeScript
        // The export_to attribute in each type's #[ts] macro specifies the output file
        let config = Config::default();
        ActorId::export(&config).unwrap();
        Event::export(&config).unwrap();
        AppendEvent::export(&config).unwrap();
        QueryEvents::export(&config).unwrap();
        ChatMsg::export(&config).unwrap();
        ToolStatus::export(&config).unwrap();
        DesktopState::export(&config).unwrap();
        WindowState::export(&config).unwrap();
        AppDefinition::export(&config).unwrap();
        ChatMessage::export(&config).unwrap();
        Sender::export(&config).unwrap();
        ViewerKind::export(&config).unwrap();
        ViewerResource::export(&config).unwrap();
        ViewerCapabilities::export(&config).unwrap();
        ViewerDescriptor::export(&config).unwrap();
        ViewerRevision::export(&config).unwrap();
        WsMsg::export(&config).unwrap();
        ToolDef::export(&config).unwrap();
        ToolCall::export(&config).unwrap();
        WorkerTurnStatus::export(&config).unwrap();
        WorkerFinding::export(&config).unwrap();
        WorkerLearning::export(&config).unwrap();
        WorkerEscalationKind::export(&config).unwrap();
        WorkerEscalationUrgency::export(&config).unwrap();
        WorkerEscalation::export(&config).unwrap();
        WorkerArtifact::export(&config).unwrap();
        WorkerTurnReport::export(&config).unwrap();
        WorkerSignalType::export(&config).unwrap();
        WorkerSignalRejectReason::export(&config).unwrap();
        WorkerSignalRejection::export(&config).unwrap();
        WorkerTurnReportIngestResult::export(&config).unwrap();
        ConductorOutputMode::export(&config).unwrap();
        ConductorToastTone::export(&config).unwrap();
        ConductorToastPayload::export(&config).unwrap();
        ConductorExecuteRequest::export(&config).unwrap();
        ConductorExecuteResponse::export(&config).unwrap();
        ConductorError::export(&config).unwrap();
        ConductorRunStatusResponse::export(&config).unwrap();
        // New runtime types
        EventLane::export(&config).unwrap();
        EventImportance::export(&config).unwrap();
        EventMetadata::export(&config).unwrap();
        CapabilityCallStatus::export(&config).unwrap();
        ConductorAgendaItem::export(&config).unwrap();
        AgendaItemStatus::export(&config).unwrap();
        ConductorCapabilityCall::export(&config).unwrap();
        ConductorArtifact::export(&config).unwrap();
        ArtifactKind::export(&config).unwrap();
        ConductorDecision::export(&config).unwrap();
        DecisionType::export(&config).unwrap();
        ConductorRunState::export(&config).unwrap();
        ConductorRunStatus::export(&config).unwrap();
    }
}
