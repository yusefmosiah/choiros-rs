Date: 2026-03-16
Kind: Architecture decision
Status: Proposed
Priority: 2
Requires: [adr-0007, adr-0014, adr-0020, adr-0026]

## ADR-0029: cagent vsock Work Broker (Per-User Distributed Work API)

### Context

cagent currently runs as a local CLI with a local SQLite DB. To run inside
ChoirOS VMs, cagent needs a distributed work API that:
- Lets user VMs create and observe work
- Lets worker VMs claim and execute work
- Keeps work graphs per-user (no cross-user leakage)
- Uses vsock (no network stack, same-host only)

### Decision

The cagent command vocabulary becomes a JSON-RPC protocol over virtio-vsock.
The host hypervisor runs a per-user work broker. User VMs and worker VMs are
thin clients that speak the same cagent CLI commands over vsock.

### Architecture

```
Host (Hypervisor)
├── cagent-broker (listens on vsock port 6000)
│   ├── per-user SQLite DBs on btrfs subvolumes (ADR-0014)
│   ├── routes work.create from user VM → user's DB
│   ├── routes work.claim from worker VM → user's DB
│   └── scopes all operations by user identity
│
├── User VM (per-user, ADR-0014)
│   ├── cagent CLI (vsock transport, thin client)
│   ├── sandbox actors: conductor → writer → terminal
│   ├── terminal calls cagent to create/observe work
│   └── mind-graph UI serves from user VM
│
└── Worker VM (shared pool)
    ├── cagent CLI (vsock transport, thin client)
    ├── codex, claude, opencode adapters
    ├── cagent supervisor claims work from any user's graph
    └── worker identity scoped by user assignment
```

### Integration with ChoirOS Actor Model

cagent is NOT a replacement for the conductor/writer/terminal actor hierarchy.
It sits below terminal as a tool:

```
Conductor (global policy, non-blocking orchestration)
  └── Writer (app agent, living document authority)
        └── Terminal (tool execution, bounded)
              └── cagent CLI (work graph + adapter dispatch)
                    └── codex/claude (LLM execution in worker VMs)
```

- Conductor coordinates app agents (writer, researcher, etc.)
- Writer manages the living document and delegates tool work to terminal
- Terminal calls cagent to dispatch coding work to worker VMs
- cagent supervisor on worker VMs picks up work and executes via adapters

### Protocol

JSON-RPC over vsock. Each cagent command maps to a method:

```json
{"method": "work.create", "params": {"title": "...", "kind": "implement"}, "user": "alice"}
{"method": "work.list", "params": {"limit": 50}, "user": "alice"}
{"method": "work.claim_next", "params": {"claimant": "worker-1"}, "user": "alice"}
{"method": "work.complete", "params": {"work_id": "...", "message": "done"}, "user": "alice"}
{"method": "work.attest", "params": {"work_id": "...", "result": "passed"}, "user": "alice"}
```

The `user` field is set by the broker from the vsock connection's CID→user mapping,
not by the client. This prevents impersonation.

### Transport Detection

cagent detects its environment automatically:
- `/dev/vsock` exists + `CAGENT_VSOCK_CID` set → vsock transport (VM guest)
- Otherwise → local SQLite (host or standalone)

Same CLI, same commands, same skill file. The transport is invisible to the
user and to agents.

### Per-User Scoping

Each user's work graph is an independent SQLite DB on the host's btrfs
subvolume (ADR-0014). The broker maps vsock CIDs to users and routes
all operations to the correct DB.

Worker VMs are assigned to users per-job. When a worker claims work,
the broker records which user's graph the work came from and scopes
all subsequent operations (complete, attest, note-add) to that user.

### Why vsock

- No network stack (no IP, no firewall, no TLS)
- Same-host only (security boundary = hypervisor)
- cloud-hypervisor already supports it (ADR-0020)
- Lower latency than HTTP
- User identity derived from VM CID (no auth tokens needed)

### Implementation Phases

Phase 1: cagent broker on host (Go service, vsock listener, per-user DB routing)
Phase 2: cagent vsock client transport (auto-detect in CLI)
Phase 3: Worker VM supervisor with vsock transport
Phase 4: Mind-graph UI in user VM serving from vsock-backed cagent

### References

- ADR-0007: 3-Tier architecture (hypervisor = control plane)
- ADR-0014: Per-user VM lifecycle and storage
- ADR-0020: Security hardening (vsock for secrets, extends to work API)
- ADR-0026: Self-directing agent dispatch
- cagent v0 spec section 18: "In v1, the command vocabulary becomes the remote broker protocol"
