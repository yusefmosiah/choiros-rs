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
// Phase 2 — Type Definitions
// ============================================================================

// ============================================================================
// Phase 2.1 — .qwy Core Types
// ============================================================================

/// Stable identifier for a block in a `.qwy` document.
/// Newtype over a ULID string — never reassigned, never reused.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct BlockId(pub String);

impl BlockId {
    pub fn new() -> Self {
        Self(ulid::Ulid::new().to_string())
    }
}

impl Default for BlockId {
    fn default() -> Self {
        Self::new()
    }
}

/// Block type variants for a `.qwy` document node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum BlockType {
    Paragraph,
    Heading,
    Code,
    Embed,
    CitationAnchor,
}

/// SHA-256 content hash — embedding cache key for selective re-embedding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ChunkHash(pub [u8; 32]);

impl ChunkHash {
    /// Compute a SHA-256 hash of the given text.
    pub fn from_text(text: &str) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        // NOTE: replace with sha2::Sha256 when sha2 is available in workspace.
        // For now, use a deterministic placeholder to keep shared-types zero-dependency.
        let mut h = DefaultHasher::new();
        text.hash(&mut h);
        let v = h.finish();
        let mut out = [0u8; 32];
        out[..8].copy_from_slice(&v.to_le_bytes());
        Self(out)
    }
}

/// W3C PROV-O style provenance envelope attached to every `.qwy` block.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ProvenanceEnvelope {
    /// Activity that produced this block (loop_id or run_id).
    pub was_generated_by: Option<String>,
    /// Agent that produced this block (actor id or role string).
    pub was_attributed_to: Option<String>,
    /// Previous block_id this block revises, if any.
    pub was_revision_of: Option<BlockId>,
    /// Source reference (URL, document_id, etc.) if sourced externally.
    pub had_primary_source: Option<String>,
    /// ChoirOS conductor run ID that triggered this block's creation.
    pub conductor_run_id: Option<String>,
    /// Loop ID within the conductor run.
    pub loop_id: Option<String>,
}

/// An inline annotation on a block (citation anchor, highlight, comment).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct BlockAnnotation {
    /// Annotation category: "citation_anchor" | "highlight" | "comment"
    pub annotation_type: String,
    /// Byte offset start within block content
    pub start: u64,
    /// Byte offset end within block content
    pub end: u64,
    #[ts(type = "unknown")]
    pub attrs: serde_json::Value,
}

/// A single node in the `.qwy` block tree.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct BlockNode {
    /// Stable ULID — never reassigned.
    pub block_id: BlockId,
    pub block_type: BlockType,
    /// Parent block, or `None` for root-level blocks.
    pub parent_id: Option<BlockId>,
    /// Ordered child block IDs.
    pub children: Vec<BlockId>,
    /// Plain text content (atjson style — no embedded markup).
    pub content: String,
    /// SHA-256 of rendered content — embedding cache key.
    #[ts(type = "string | null")]
    pub chunk_hash: Option<String>,
    pub provenance: ProvenanceEnvelope,
    pub annotations: Vec<BlockAnnotation>,
}

/// A single operation in the `.qwy` append-only patch log.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "action", rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum QwyPatchOp {
    /// Insert a new block into the tree.
    Insert {
        path: Vec<BlockId>,
        value: BlockNode,
    },
    /// Remove a block from the tree.
    Remove { path: Vec<BlockId> },
    /// Replace block content in place.
    Replace {
        path: Vec<BlockId>,
        value: BlockNode,
    },
    /// Reorder children of a parent block.
    Reorder {
        path: Vec<BlockId>,
        new_order: Vec<BlockId>,
    },
}

/// A timestamped entry in the `.qwy` patch log.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct QwyPatchEntry {
    pub patch_id: String,
    /// Transaction grouping ID — atomic across multiple ops.
    pub tx_id: String,
    pub timestamp: DateTime<Utc>,
    /// Actor role or id ("writer" | "user" | "researcher" | "terminal")
    pub author: String,
    pub run_id: Option<String>,
    pub loop_id: Option<String>,
    pub ops: Vec<QwyPatchOp>,
}

/// Version index entry within a `.qwy` document.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct QwyVersionIndexEntry {
    /// SHA-256 of the full document state at this version.
    pub snapshot_hash: String,
    /// The transaction that produced this version.
    pub tx_id: String,
    pub timestamp: DateTime<Utc>,
    pub author: String,
}

/// Header block for a `.qwy` document.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct QwyDocumentHeader {
    /// Stable document ULID — never changes after creation.
    pub document_id: String,
    /// Schema version — additive only, never remove or reorder fields.
    pub schema_version: u32,
    pub created_at: DateTime<Utc>,
    /// Agent or user who created the document.
    pub created_by: String,
    /// Conductor run that kicked off this document, if any.
    pub conductor_run_id: Option<String>,
}

/// A complete `.qwy` document in memory.
///
/// Canonical format is CBOR; this struct is the typed Rust projection.
/// JSON is a derived human-readable encoding. Markdown is a render artifact.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct QwyDocument {
    pub header: QwyDocumentHeader,
    /// Ordered root-level block IDs (all blocks stored flat by block_id).
    pub root_block_ids: Vec<BlockId>,
    /// All blocks keyed by block_id.
    #[ts(type = "Record<string, BlockNode>")]
    pub blocks: std::collections::HashMap<String, BlockNode>,
    /// Append-only patch log.
    pub patch_log: Vec<QwyPatchEntry>,
    /// Citation registry keyed by citation_id.
    #[ts(type = "Record<string, CitationRecord>")]
    pub citation_registry: std::collections::HashMap<String, CitationRecord>,
    /// Version history.
    pub version_index: Vec<QwyVersionIndexEntry>,
}

// ============================================================================
// Phase 2.2 — Citation Types
// ============================================================================

/// Why a resource was cited. Maps to the BAML `CitationKind` enum.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum CitationKind {
    /// Researcher retrieved and pulled it into context.
    RetrievedContext,
    /// Appears as a link or reference in the document text.
    InlineReference,
    /// This run extends or revises the cited artifact.
    BuildsOn,
    /// Explicitly disputes a prior artifact.
    Contradicts,
    /// Restates a prior objective or directive.
    Reissues,
}

/// Lifecycle state of a citation record.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum CitationStatus {
    /// Proposed by researcher or writer — not yet confirmed.
    Proposed,
    /// Confirmed by writer or user — counts toward quality signal.
    Confirmed,
    /// Rejected — not counted; kept for training signal.
    Rejected,
    /// Superseded by a later citation to the same resource.
    Superseded,
}

/// A single citation record — persisted to event store and `.qwy` registry.
///
/// Citation lifecycle:
/// 1. Researcher proposes → `status: Proposed`, `confirmed_by: None`
/// 2. Writer confirms → `status: Confirmed`, `confirmed_by: "writer"`, `confirmed_at: <ts>`
/// 3. Writer rejects → `status: Rejected`
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct CitationRecord {
    pub citation_id: String,
    /// Artifact path, version_id, input_id, URL, or block_id.
    pub cited_id: String,
    /// "version_snapshot" | "user_input" | "external_content" | "qwy_block" | "external_url"
    pub cited_kind: String,
    pub citing_run_id: String,
    pub citing_loop_id: String,
    /// "researcher" | "writer" | "terminal" | "user"
    pub citing_actor: String,
    pub cite_kind: CitationKind,
    /// Model confidence in this citation [0.0, 1.0].
    pub confidence: f64,
    /// Specific text span that triggered this citation, if extractable.
    pub excerpt: Option<String>,
    /// Why this citation was relevant.
    pub rationale: String,
    pub status: CitationStatus,
    /// Who proposed: "researcher" | "writer" | "user"
    pub proposed_by: String,
    /// Who confirmed: "writer" | "user" | None
    pub confirmed_by: Option<String>,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

// ============================================================================
// Phase 2.3 — Embedding Collection Record Types
// ============================================================================

/// Record type for the `user_inputs` embedding collection.
///
/// One record per `EventType::UserInput` on any surface.
/// All surfaces share one collection — cross-app correlations are the point.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct UserInputRecord {
    pub input_id: String,
    /// Plain text of the user directive.
    pub content: String,
    /// Surface that received the input: "conductor" | "writer" | "prompt_bar"
    pub surface: String,
    pub desktop_id: String,
    pub session_id: String,
    pub thread_id: String,
    pub run_id: Option<String>,
    pub document_path: Option<String>,
    pub base_version_id: Option<u64>,
    pub created_at: DateTime<Utc>,
}

/// Record type for the `version_snapshots` embedding collection.
///
/// One record per `VersionSource::Writer` harness loop completion.
/// Intermediate loop versions are NOT embedded — only final loop outputs.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct VersionSnapshotRecord {
    pub version_id: String,
    pub document_path: String,
    /// Full document text at this version.
    pub content: String,
    /// The objective the writer was given for this loop.
    pub objective: String,
    pub loop_id: String,
    pub run_id: String,
    /// SHA-256 of content — deduplication and selective re-embedding key.
    pub chunk_hash: String,
    pub created_at: DateTime<Utc>,
}

/// Record type for the `run_trajectories` embedding collection.
///
/// One record per completed `AgentResult` from any harness.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct RunTrajectoryRecord {
    pub loop_id: String,
    pub run_id: String,
    /// "researcher" | "writer" | "terminal" | "conductor" | "subharness"
    pub worker_type: String,
    pub objective: String,
    /// Human-readable summary of what happened in this run.
    pub summary: String,
    pub steps_taken: u32,
    pub success: bool,
    pub created_at: DateTime<Utc>,
}

/// Record type for the `doc_trajectories` embedding collection.
///
/// One record per document path, updated each time a new `VersionSnapshotRecord`
/// is added for that path. Captures the strategic arc of a document over time.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct DocTrajectoryRecord {
    pub document_path: String,
    pub version_count: u32,
    pub run_count: u32,
    pub last_loop_id: String,
    /// Rolled-up narrative summary across all runs on this document.
    pub cumulative_summary: String,
    pub last_updated_at: DateTime<Utc>,
}

/// Local private record for externally fetched content.
///
/// Never published directly — stripped of private fields before entering
/// `GlobalExternalContentRecord`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ExternalContentRecord {
    pub content_id: String,
    pub url: String,
    /// SHA-256 of the fetched content text — deduplication key.
    pub content_hash: String,
    pub fetched_at: DateTime<Utc>,
    /// Loop ID of the research loop that fetched this.
    pub fetched_by: String,
    pub run_id: Option<String>,
    pub title: Option<String>,
    /// Embeddable cleaned text extracted from the page.
    pub content_text: String,
    /// "full" | "sections" | "paragraphs"
    pub chunk_strategy: String,
    /// Local filesystem path to the raw snapshot, if stored.
    pub snapshot_ref: Option<String>,
    pub domain: Option<String>,
    /// CSL-JSON bibliographic metadata if extractable.
    #[ts(type = "unknown")]
    pub csl_metadata: Option<serde_json::Value>,
}

/// Public record in the global hypervisor external content store.
///
/// Private fields (`fetched_by`, `run_id`, `snapshot_ref`) are stripped at
/// the publish boundary. `content_id` is the `content_hash` for natural
/// deduplication.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct GlobalExternalContentRecord {
    /// Natural dedup key — content_hash from the local record.
    pub content_id: String,
    pub url: String,
    pub title: Option<String>,
    pub content_text: String,
    pub chunk_strategy: String,
    #[ts(type = "unknown")]
    pub csl_metadata: Option<serde_json::Value>,
    pub first_cited_at: DateTime<Utc>,
    /// Confirmed citation count — used as retrieval quality weight.
    pub citation_count: u32,
    pub domain: Option<String>,
    /// Always "external_content" — enables unified collection filtering.
    pub record_kind: String,
}

// ============================================================================
// Phase 4.5 — ContextSnapshot (Memory service stub types)
// ============================================================================

/// A single item in a ContextSnapshot.
///
/// Each item is a piece of text (e.g., a document excerpt, a prior
/// run summary, or an external URL snippet) with provenance metadata.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ContextItem {
    /// Unique item identifier (ULID).
    pub item_id: String,
    /// Kind of context: "version_snapshot" | "run_trajectory" | "external_content" | "user_input"
    pub kind: String,
    /// Source identifier (document path, URL, loop_id, etc.)
    pub source_ref: String,
    /// Plain-text content for this item (may be truncated).
    pub content: String,
    /// Relevance score assigned by the retrieval model [0.0, 1.0].
    pub relevance: f64,
    pub created_at: DateTime<Utc>,
}

/// Citation reference included in a ContextSnapshot.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct CitationRef {
    pub cited_id: String,
    pub cite_kind: CitationKind,
    pub confidence: f64,
    pub rationale: String,
}

/// A snapshot of context retrieved by the MemoryActor for a conductor turn.
///
/// Passed as the `context` bundle when spawning a `SubharnessActor`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ContextSnapshot {
    /// Unique snapshot identifier (ULID).
    pub snapshot_id: String,
    /// Run this snapshot was generated for.
    pub run_id: String,
    /// Objective query used to retrieve context.
    pub query: String,
    /// Retrieved context items, ranked by relevance.
    pub items: Vec<ContextItem>,
    /// Citations underlying the retrieved items.
    pub provenance: Vec<CitationRef>,
    pub created_at: DateTime<Utc>,
}

// ============================================================================
// Constants — Phase 2 event topics
// ============================================================================

pub const EVENT_TOPIC_CITATION_PROPOSED: &str = "citation.proposed";
pub const EVENT_TOPIC_CITATION_CONFIRMED: &str = "citation.confirmed";
pub const EVENT_TOPIC_CITATION_REJECTED: &str = "citation.rejected";
pub const EVENT_TOPIC_USER_INPUT: &str = "user_input";
pub const EVENT_TOPIC_GLOBAL_EXTERNAL_CONTENT_UPSERT: &str = "global_external_content.upsert";
pub const EVENT_TOPIC_QWY_CITATION_REGISTRY: &str = "qwy.citation_registry";

// Phase 4 event topics
pub const EVENT_TOPIC_SUBHARNESS_EXECUTE: &str = "subharness.execute";
pub const EVENT_TOPIC_SUBHARNESS_RESULT: &str = "subharness.result";
pub const EVENT_TOPIC_HARNESS_CHECKPOINT: &str = "harness.checkpoint";
pub const EVENT_TOPIC_TOOL_RESULT: &str = "tool.result";

// ============================================================================
// Phase 4.5 — Harness durability types
// ============================================================================

/// A single outstanding actor message the harness fired and hasn't heard back
/// from yet. Written as part of `HarnessCheckpoint` so recovery can reconstruct
/// what to wait for.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct PendingReply {
    /// Correlation ID assigned when the message was sent.
    pub corr_id: String,
    /// What kind of actor we messaged ("terminal", "researcher", "subharness").
    pub actor_kind: String,
    /// Short description of what was requested (for observability).
    pub objective_summary: String,
    /// When the message was sent.
    pub sent_at: DateTime<Utc>,
    /// Hard deadline — if no reply by this time the harness treats it as failed.
    pub timeout_at: Option<DateTime<Utc>>,
}

/// Durable state written to EventStore at every turn boundary where the harness
/// fires outbound messages and suspends.
///
/// On crash+restart the supervisor reads the latest `harness.checkpoint` event
/// for the run_id, reconstructs this struct, checks EventStore for already-received
/// `tool.result` events matching each pending corr_id, and resumes.
///
/// The invariant: if a `harness.checkpoint` event exists, the run is live.
/// If a `subharness.result` / `tool.result` event exists for a pending corr_id,
/// that reply is already in and should be loaded from EventStore rather than
/// waited on as a message.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct HarnessCheckpoint {
    /// Stable identifier for this execution run.
    pub run_id: String,
    /// Actor/session identifier.
    pub actor_id: String,
    /// Turn number that just completed.
    pub turn_number: usize,
    /// Model's articulated reasoning state at this checkpoint.
    pub working_memory: String,
    /// Top-level objective the harness is working toward.
    pub objective: String,
    /// Messages fired this turn that we haven't received replies for yet.
    /// Empty → harness is not waiting on anything (should not happen at a
    /// checkpoint; a checkpoint is only written when replies are pending).
    pub pending_replies: Vec<PendingReply>,
    /// Compact log of all turns so far (for context reassembly on recovery).
    pub turn_summaries: Vec<TurnSummary>,
    pub checkpointed_at: DateTime<Utc>,
}

/// Compact record of a single completed turn, stored inside `HarnessCheckpoint`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct TurnSummary {
    pub turn_number: usize,
    pub action_kind: String,
    pub working_memory_excerpt: String,
    pub corr_ids_fired: Vec<String>,
    pub elapsed_ms: u64,
}

/// Written to EventStore as `tool.result` by whichever actor produced the
/// result (TerminalActor, ResearcherActor, SubharnessActor).
///
/// The harness reads this by `corr_id` on recovery rather than waiting for
/// the message if the actor already completed.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct ToolResult {
    /// Matches the `corr_id` in the original `PendingReply`.
    pub corr_id: String,
    /// Actor kind that produced this result.
    pub actor_kind: String,
    /// Whether the execution succeeded.
    pub success: bool,
    /// The output (stdout, research result, subharness working memory, etc.)
    pub output: String,
    /// Error details if `success` is false.
    pub error: Option<String>,
    pub elapsed_ms: u64,
    pub completed_at: DateTime<Utc>,
}

// ============================================================================
// WorkerMsg Lateral Protocol
// ============================================================================
//
// Workers (Terminal, Researcher) can message each other directly without
// routing through Conductor. This is the lateral mesh — each agent's context
// is updated by the other's discoveries in real time, enabling cultural learning
// across runs when persisted to EventStore.
//
// Protocol rules:
// - Request → Response is a corr_id-keyed round trip (both async sends, no blocking)
// - Signal is fire-and-forget; receiver may ignore it
// - Neither side blocks waiting for the other — fire and continue
// - Conductor is never in the path; these messages are worker-to-worker only
// - Every message is persisted to EventStore by the sender for tracing

/// The kind of work being requested between workers.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum WorkerRequestKind {
    /// Ask the Terminal worker to execute a command and return the result.
    RunCommand {
        command: String,
        timeout_ms: Option<u64>,
    },
    /// Ask the Researcher worker to look something up and return a summary.
    Research {
        query: String,
        max_results: Option<u32>,
    },
    /// Ask for a specific file's content (routed to whoever owns the filesystem).
    ReadFile { path: String },
}

/// A lateral request from one worker to another.
///
/// The sender fires this and continues — it does not await a reply inline.
/// The reply arrives later as a `WorkerMsg::Response` keyed by `corr_id`,
/// which the sender's harness reads via `resolve_source(ToolOutput, corr_id)`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct WorkerRequest {
    /// Sender-assigned correlation ID. Receiver echoes this in the Response.
    pub corr_id: String,
    /// Which worker sent this (for routing the response back).
    pub from_actor_id: String,
    /// What kind of work is being requested.
    pub kind: WorkerRequestKind,
    /// Optional natural-language context to help the receiver understand why.
    pub context: Option<String>,
    pub sent_at: DateTime<Utc>,
}

/// Response to a lateral worker request.
///
/// The responding worker sends this as a fire-and-forget message back to the
/// requester actor, which reads it via `resolve_source(ToolOutput, corr_id)`.
/// The result is also written to EventStore so it survives crash/recovery.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct WorkerResponse {
    /// Echoed from the original `WorkerRequest`.
    pub corr_id: String,
    /// Whether the request was successfully handled.
    pub success: bool,
    /// The result payload (command output, research summary, file content, etc.)
    pub output: String,
    /// Error details if `success` is false.
    pub error: Option<String>,
    pub elapsed_ms: u64,
    pub completed_at: DateTime<Utc>,
}

/// One-way signal from one worker to another — no reply expected.
///
/// Used for observations ("I found something relevant to your objective"),
/// intent announcements ("I'm about to modify this file"), or advisory notes
/// ("this directory is locked by another process").
///
/// Receivers may ignore signals. Signals are persisted to EventStore for
/// post-hoc analysis and cultural learning.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum WorkerSignalKind {
    /// "I found something that may be relevant to your current objective."
    RelevantFinding,
    /// "I'm about to take an action that affects shared state."
    IntentAnnouncement,
    /// "I've completed work that you may want to build on."
    WorkComplete,
    /// "I encountered a condition you should know about."
    Advisory,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct WorkerSignal {
    pub from_actor_id: String,
    pub to_actor_id: String,
    pub kind: WorkerSignalKind,
    /// Human-readable content of the signal.
    pub content: String,
    /// Optional structured data (JSON string).
    pub metadata: Option<String>,
    pub sent_at: DateTime<Utc>,
}

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
        // WorkerMsg lateral protocol
        WorkerRequestKind::export(&config).unwrap();
        WorkerRequest::export(&config).unwrap();
        WorkerResponse::export(&config).unwrap();
        WorkerSignalKind::export(&config).unwrap();
        WorkerSignal::export(&config).unwrap();
    }
}
