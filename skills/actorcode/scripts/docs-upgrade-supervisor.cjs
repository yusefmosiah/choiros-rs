#!/usr/bin/env node
/**
 * Docs Upgrade Runbook Executor
 * Spawns workers to fix the 19 coherence issues identified
 */

const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');

const RUN_ID = `docs-upgrade-${Date.now()}`;
const LOG_DIR = 'logs/actorcode';

// Ensure log directory exists
if (!fs.existsSync(LOG_DIR)) {
    fs.mkdirSync(LOG_DIR, { recursive: true });
}

function log(event, data) {
    const logPath = path.join(LOG_DIR, `${RUN_ID}.jsonl`);
    const entry = JSON.stringify({
        timestamp: new Date().toISOString(),
        event,
        ...data
    }) + '\n';
    fs.appendFileSync(logPath, entry);
    console.log(`[${event}]`, data.workerId || data.phase || '');
}

const WORKERS = [
    {
        id: 'fix-sprites',
        title: 'Remove Sprites.dev references',
        files: ['docs/ARCHITECTURE_SPECIFICATION.md'],
        prompt: `Remove all Sprites.dev references from ARCHITECTURE_SPECIFICATION.md.

Lines to fix: 59, 568-570, 600-605, 972, 1112

Replace with: "**Sandbox:** Local process (port 8080) - containerization planned for future"

Be careful not to break document structure. Update any related sections.`,
        tier: 'nano'
    },
    {
        id: 'fix-actors',
        title: 'Fix actor list',
        files: ['docs/ARCHITECTURE_SPECIFICATION.md'],
        prompt: `Update the actor list in ARCHITECTURE_SPECIFICATION.md lines 200-205.

Remove: WriterActor, BamlActor, ToolExecutor
Add: ChatAgent

Correct list:
- EventStoreActor - libsql event log
- ChatActor - Chat app logic  
- ChatAgent - BAML-powered AI agent
- DesktopActor - Window state management

Note: Tools exist as module, not actor. BAML is direct integration.`,
        tier: 'nano'
    },
    {
        id: 'fix-hypervisor',
        title: 'Mark hypervisor as stub',
        files: ['docs/ARCHITECTURE_SPECIFICATION.md'],
        prompt: `Update hypervisor section in ARCHITECTURE_SPECIFICATION.md lines 138-167, 89-97.

Add status note: "STUB IMPLEMENTATION - Not yet functional"
Current state: hypervisor/src/main.rs is just a 5-line placeholder

Be honest about what's implemented vs planned.`,
        tier: 'nano'
    },
    {
        id: 'fix-test-counts',
        title: 'Fix test counts',
        files: ['docs/TESTING_STRATEGY.md'],
        prompt: `Fix test count claims in TESTING_STRATEGY.md lines 46, 742, 764, 820.

Change "18 tests" to:
- Unit Tests: 48 tests passing
- Integration Tests: 123+ tests (3 known failures in chat_api_test.rs)

Verify by running: cargo test -p sandbox`,
        tier: 'nano'
    },
    {
        id: 'fix-dev-browser',
        title: 'Fix dev-browser references',
        files: ['docs/TESTING_STRATEGY.md'],
        prompt: `Replace dev-browser references with agent-browser in TESTING_STRATEGY.md.

We have agent-browser skill, not dev-browser.
Update all 20+ references to point to correct skill.`,
        tier: 'nano'
    },
    {
        id: 'fix-docker',
        title: 'Mark Docker as pending',
        files: ['docs/ARCHITECTURE_SPECIFICATION.md'],
        prompt: `Update Docker section in ARCHITECTURE_SPECIFICATION.md lines 550-614.

Mark as: "Future - pending NixOS research"

We are researching Nix/NixOS for deployment instead of Docker.
Current deployment is EC2 + systemd.`,
        tier: 'nano'
    },
    {
        id: 'fix-ci-cd',
        title: 'Mark CI/CD as planned',
        files: ['docs/ARCHITECTURE_SPECIFICATION.md'],
        prompt: `Update CI/CD section in ARCHITECTURE_SPECIFICATION.md lines 780-838.

Mark as: "Planned - not yet implemented"

No .github/workflows/ directory exists yet.`,
        tier: 'nano'
    },
    {
        id: 'fix-ports',
        title: 'Fix port numbers',
        files: ['docs/ARCHITECTURE_SPECIFICATION.md', 'docs/AUTOMATED_WORKFLOW.md'],
        prompt: `Fix port number contradictions:

ARCHITECTURE_SPECIFICATION.md:
- Line 90: Change :8001 to note hypervisor not running
- Line 584: Change :5173 to :3000

AUTOMATED_WORKFLOW.md:
- Line 46: Change :5173 to :3000

Actual ports:
- Sandbox API: :8080
- UI: :3000`,
        tier: 'nano'
    },
    {
        id: 'fix-database',
        title: 'Fix database tech',
        files: ['docs/ARCHITECTURE_SPECIFICATION.md'],
        prompt: `Fix database technology in ARCHITECTURE_SPECIFICATION.md line 57.

Change "SQLite" to "libSQL (Turso fork)"

We use libsql crate, not standard SQLite.`,
        tier: 'nano'
    },
    {
        id: 'fix-api-contracts',
        title: 'Fix API contracts',
        files: ['docs/ARCHITECTURE_SPECIFICATION.md'],
        prompt: `Fix API contract mismatches in ARCHITECTURE_SPECIFICATION.md lines 418-437.

Remove /api prefix from paths.
Add missing /desktop/* and /ws/chat/* endpoints.

Verify against: sandbox/src/api/mod.rs:15-36`,
        tier: 'nano'
    },
    {
        id: 'fix-baml-paths',
        title: 'Fix BAML paths',
        files: ['docs/ARCHITECTURE_SPECIFICATION.md'],
        prompt: `Fix BAML file paths in ARCHITECTURE_SPECIFICATION.md lines 1060-1070.

Change: sandbox/baml/ 
To: baml_src/

BAML files are at repo root, not in sandbox/.`,
        tier: 'nano'
    },
    {
        id: 'fix-handoffs-taxonomy',
        title: 'Add handoffs to taxonomy',
        files: ['docs/DOCUMENTATION_UPGRADE_PLAN.md'],
        prompt: `Add handoffs to doc taxonomy in DOCUMENTATION_UPGRADE_PLAN.md lines 22-28.

The taxonomy defines 6 categories but omits handoffs/ which has 14 files.

Add: "Handoffs: session context preservation for multi-session workflows"`,
        tier: 'nano'
    },
    {
        id: 'fix-actorcode-dashboard',
        title: 'Clarify actorcode dashboard',
        files: ['docs/CHOIR_MULTI_AGENT_VISION.md'],
        prompt: `Clarify actorcode dashboard in CHOIR_MULTI_AGENT_VISION.md lines 109, 131-132.

The actorcode dashboard (port 8765) is separate from ChoirOS Rust (port 8080).
They are completely different systems. Make this clear.`,
        tier: 'nano'
    },
    {
        id: 'fix-vision-actors',
        title: 'Mark vision actors as planned',
        files: ['docs/CHOIR_MULTI_AGENT_VISION.md'],
        prompt: `Mark vision actors as planned in CHOIR_MULTI_AGENT_VISION.md lines 75-81.

Only EventStoreActor exists. The other 7 are planned but not implemented:
- BusActor
- NotesActor  
- WatcherActor
- SupervisorActor
- RunActor
- RunRegistryActor
- SummaryActor

Mark them as "Planned - Not Implemented"`,
        tier: 'nano'
    },
    {
        id: 'fix-openprose',
        title: 'Add OpenProse disclaimer',
        files: ['docs/AUTOMATED_WORKFLOW.md'],
        prompt: `Add OpenProse disclaimer in AUTOMATED_WORKFLOW.md lines 91-108.

The prose CLI is not installed. Add note:
"Requires prose CLI (not currently installed)"`,
        tier: 'nano'
    },
    {
        id: 'fix-dependencies',
        title: 'Document dependencies',
        files: ['docs/AGENTS.md', 'docs/AUTOMATED_WORKFLOW.md'],
        prompt: `Add missing dependencies to AGENTS.md and AUTOMATED_WORKFLOW.md lines 47, 193.

cargo-watch and multitail are used but not documented.

Add to AGENTS.md dependencies section.`,
        tier: 'nano'
    },
    {
        id: 'fix-e2e-paths',
        title: 'Fix E2E paths',
        files: ['docs/AUTOMATED_WORKFLOW.md', 'docs/TESTING_STRATEGY.md'],
        prompt: `Fix E2E test directory paths:

AUTOMATED_WORKFLOW.md line 59:
- Change: ./e2e/tests
- To: tests/e2e

TESTING_STRATEGY.md line 485:
- Change: tests/integration/
- To: sandbox/tests/*.rs`,
        tier: 'nano'
    },
    {
        id: 'rewrite-agents-md',
        title: 'Rewrite AGENTS.md',
        files: ['docs/AGENTS.md'],
        prompt: `Add Task Concurrency section to AGENTS.md.

Key rules:
1. Supervisors NEVER spawn blocking task() calls
2. Supervisors coordinate, workers execute
3. Use actorcode runs for parallel work
4. Tool call budgets: Supervisor (50), Worker (200), Async Run (unlimited)

Include examples of correct vs wrong patterns.

This is CRITICAL for the automatic computer architecture.`,
        tier: 'micro'
    }
];

function spawnWorker(worker) {
    return new Promise((resolve, reject) => {
        log('WORKER_SPAWN', { workerId: worker.id, tier: worker.tier });
        
        const outputFile = path.join(LOG_DIR, `${RUN_ID}-${worker.id}.log`);
        
        // Create prompt that includes file editing instructions
        const fullPrompt = `${worker.prompt}

IMPORTANT: You must actually edit the files listed above using the Edit tool.
Do not just describe the changes - make them.
Files to edit: ${worker.files.join(', ')}

After editing, verify the changes look correct.
Report what you changed and any issues encountered.

Use [LEARNING] protocol for any insights during the work.`;

        const cmd = 'node';
        const args = [
            'skills/actorcode/scripts/actorcode.js',
            'spawn',
            '--title', worker.title,
            '--tier', worker.tier,
            '--prompt', fullPrompt
        ];
        
        console.log(`\n🚀 Spawning: ${worker.id} (${worker.tier})`);
        console.log(`   Files: ${worker.files.join(', ')}`);
        
        const child = spawn(cmd, args, {
            stdio: ['ignore', 'pipe', 'pipe'],
            env: { ...process.env, SESSION_ID: `${RUN_ID}-${worker.id}` }
        });
        
        let stdout = '';
        let stderr = '';
        
        child.stdout.on('data', (data) => {
            stdout += data;
            fs.appendFileSync(outputFile, data);
        });
        
        child.stderr.on('data', (data) => {
            stderr += data;
        });
        
        child.on('close', (code) => {
            log('WORKER_COMPLETE', { workerId: worker.id, exitCode: code });
            
            if (code === 0) {
                resolve({ workerId: worker.id, success: true });
            } else {
                resolve({ workerId: worker.id, success: false, exitCode: code, stderr });
            }
        });
        
        child.on('error', (err) => {
            log('WORKER_ERROR', { workerId: worker.id, error: err.message });
            reject(err);
        });
    });
}

async function main() {
    console.log('\n╔════════════════════════════════════════════════════════════╗');
    console.log('║     Docs Upgrade Runbook Executor                          ║');
    console.log('║     Run ID:', RUN_ID, '                    ║');
    console.log('╚════════════════════════════════════════════════════════════╝\n');
    
    console.log('⚠️  Note: docs/research/ directory may be updated during this run.');
    console.log('   This is expected - NixOS research is running in parallel.\n');
    
    log('SUPERVISOR_START', { runId: RUN_ID, workerCount: WORKERS.length });
    
    // Spawn all workers in parallel
    console.log('\n📚 Spawning', WORKERS.length, 'workers in parallel...\n');
    const results = await Promise.all(WORKERS.map(spawnWorker));
    
    const successful = results.filter(r => r.success);
    const failed = results.filter(r => !r.success);
    
    console.log(`\n✅ Complete: ${successful.length}/${WORKERS.length} workers succeeded`);
    
    if (failed.length > 0) {
        console.log('\n❌ Failed:', failed.map(f => f.workerId).join(', '));
    }
    
    log('SUPERVISOR_COMPLETE', { 
        runId: RUN_ID,
        succeeded: successful.length,
        failed: failed.length
    });
    
    console.log('\n📄 Log file:', path.join(LOG_DIR, `${RUN_ID}.jsonl`));
}

main().catch(err => {
    console.error('Supervisor error:', err);
    log('SUPERVISOR_ERROR', { error: err.message });
    process.exit(1);
});
