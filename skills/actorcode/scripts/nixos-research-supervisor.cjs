#!/usr/bin/env node
/**
 * NixOS Research Supervisor
 * Spawns workers to research Nix/NixOS for Rust development and EC2 container management
 */

const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');

const RESEARCH_DIR = 'docs/research/nixos-research-2026-02-01';
const RUN_ID = `nixos-research-${Date.now()}`;

// Ensure directory exists
if (!fs.existsSync(RESEARCH_DIR)) {
    fs.mkdirSync(RESEARCH_DIR, { recursive: true });
}

// Log supervisor start
logSupervisor('SUPERVISOR_START', { runId: RUN_ID, timestamp: new Date().toISOString() });

const WORKERS = [
    {
        id: 'nix-basics',
        title: 'Nix Basics for Rust Dev',
        prompt: `Research Nix package manager basics for Rust development environments.
        
Focus areas:
1. What is Nix and why use it for Rust?
2. Nix flakes for reproducible Rust builds
3. nix-shell vs nix develop
4. Cross-compilation with Nix
5. IDE integration (rust-analyzer, etc.)

Output: Write a comprehensive doc to ${RESEARCH_DIR}/01-nix-basics.md
Include code examples, configuration snippets, and practical workflows.
Use the [LEARNING] protocol for incremental findings.`,
        tier: 'nano'
    },
    {
        id: 'nixos-server',
        title: 'NixOS on EC2',
        prompt: `Research running NixOS on AWS EC2 instances.
        
Focus areas:
1. NixOS AMI options and setup
2. Infrastructure as Code with NixOS
3. NixOps or alternatives for EC2 deployment
4. Secrets management (agenix, sops-nix)
5. System updates and rollbacks
6. Monitoring/observability on NixOS

Output: Write a comprehensive doc to ${RESEARCH_DIR}/02-nixos-ec2.md
Include configuration examples, deployment patterns, and operational concerns.
Use the [LEARNING] protocol for incremental findings.`,
        tier: 'nano'
    },
    {
        id: 'containers-nix',
        title: 'Containers with Nix',
        prompt: `Research container management using Nix instead of Docker.
        
Focus areas:
1. nixpkgs.dockerTools vs Dockerfiles
2. Building minimal container images with Nix
3. OCI-compliant images from Nix
4. Container runtime integration (podman, containerd)
5. Multi-arch builds (ARM64, AMD64)
6. Caching and layer optimization

Output: Write a comprehensive doc to ${RESEARCH_DIR}/03-nix-containers.md
Include build examples, image comparisons, and migration path from Docker.
Use the [LEARNING] protocol for incremental findings.`,
        tier: 'nano'
    },
    {
        id: 'rust-toolchain',
        title: 'Rust Toolchain in Nix',
        prompt: `Research managing Rust toolchains with Nix.
        
Focus areas:
1. fenix, rust-overlay, or rustup in Nix
2. Multiple Rust versions side-by-side
3. Cargo workspace support
4. Build dependencies and native libraries
5. CI/CD integration (GitHub Actions with Nix)
6. Developer experience comparison

Output: Write a comprehensive doc to ${RESEARCH_DIR}/04-rust-toolchain.md
Include flake.nix examples, shell.nix patterns, and workflow comparisons.
Use the [LEARNING] protocol for incremental findings.`,
        tier: 'nano'
    },
    {
        id: 'ec2-patterns',
        title: 'EC2 Deployment Patterns',
        prompt: `Research EC2 deployment patterns for containerized Rust apps.
        
Focus areas:
1. systemd vs containers on EC2
2. Blue-green deployments
3. Health checks and auto-recovery
4. Log aggregation (without CloudWatch $$$)
5. Cost optimization for dev/staging
6. Security hardening (IAM, security groups)

Output: Write a comprehensive doc to ${RESEARCH_DIR}/05-ec2-patterns.md
Include architecture diagrams, cost estimates, and security checklist.
Use the [LEARNING] protocol for incremental findings.`,
        tier: 'nano'
    }
];

function logSupervisor(event, data) {
    const logPath = path.join(RESEARCH_DIR, 'supervisor.log.jsonl');
    const entry = JSON.stringify({
        timestamp: new Date().toISOString(),
        event,
        ...data
    }) + '\n';
    fs.appendFileSync(logPath, entry);
    console.log(`[SUPERVISOR] ${event}:`, data.runId || data.workerId || '');
}

function spawnWorker(worker) {
    return new Promise((resolve, reject) => {
        logSupervisor('WORKER_SPAWN', { workerId: worker.id, tier: worker.tier });
        
        const outputFile = path.join(RESEARCH_DIR, `${worker.id}.log`);
        const artifactFile = path.join(RESEARCH_DIR, `${worker.id}.jsonl`);
        
        // Create artifact file header
        fs.writeFileSync(artifactFile, JSON.stringify({
            type: 'worker_start',
            workerId: worker.id,
            timestamp: new Date().toISOString(),
            tier: worker.tier
        }) + '\n');
        
        const cmd = `node`;
        const args = [
            'skills/actorcode/scripts/actorcode.js',
            'spawn',
            '--title', worker.title,
            '--tier', worker.tier,
            '--prompt', worker.prompt
        ];
        
        console.log(`\n🚀 Spawning worker: ${worker.id} (${worker.tier})`);
        console.log(`   Output: ${outputFile}`);
        
        const child = spawn(cmd, args, {
            stdio: ['ignore', 'pipe', 'pipe'],
            env: {
                ...process.env,
                SESSION_ID: `${RUN_ID}-${worker.id}`,
                ARTIFACT_FILE: artifactFile
            }
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
            logSupervisor('WORKER_COMPLETE', { 
                workerId: worker.id, 
                exitCode: code,
                hasOutput: stdout.length > 0
            });
            
            // Append completion to artifact
            fs.appendFileSync(artifactFile, JSON.stringify({
                type: 'worker_complete',
                workerId: worker.id,
                timestamp: new Date().toISOString(),
                exitCode: code
            }) + '\n');
            
            if (code === 0) {
                resolve({ workerId: worker.id, success: true, outputFile });
            } else {
                resolve({ workerId: worker.id, success: false, exitCode: code, stderr });
            }
        });
        
        child.on('error', (err) => {
            logSupervisor('WORKER_ERROR', { workerId: worker.id, error: err.message });
            reject(err);
        });
    });
}

async function spawnMergeWorker() {
    logSupervisor('MERGE_WORKER_START', {});
    
    const prompt = `You are a merge worker. Your job is to combine the research docs from previous workers into a unified document.

Input files (read all of these):
${WORKERS.map(w => `- ${RESEARCH_DIR}/0${WORKERS.indexOf(w) + 1}-${w.id.replace(/-/g, '-')}.md`).join('\n')}

Task:
1. Read all 5 research docs above
2. Identify overlaps and gaps
3. Create a unified document at ${RESEARCH_DIR}/06-merged-research.md
4. Structure it as:
   - Executive Summary
   - Nix Fundamentals (merged from 01, 04)
   - NixOS on EC2 (merged from 02, 05)
   - Container Strategy (from 03)
   - Decision Matrix (when to use what)
   - Implementation Roadmap
   - Open Questions

Use the [LEARNING] protocol for any new insights during merge.
Output the merged doc and a summary of what was consolidated.`;

    return spawnWorker({
        id: 'merge',
        title: 'Merge Research Docs',
        prompt,
        tier: 'micro'
    });
}

async function spawnCritiqueWorker() {
    logSupervisor('CRITIQUE_WORKER_START', {});
    
    const prompt = `You are a critique worker with web search capability. Your job is to validate and critique the merged research.

Input file: ${RESEARCH_DIR}/06-merged-research.md

Task:
1. Read the merged research doc
2. Use web search to verify key claims:
   - Current NixOS AMI availability
   - Latest Nix flake features
   - Container image size comparisons
   - EC2 pricing for different approaches
3. Identify outdated information
4. Find missing alternatives not covered
5. Check for Nix community best practices

Output: Write critique to ${RESEARCH_DIR}/07-web-critique.md
Include:
- Verified claims (with sources)
- Corrections needed
- Missing alternatives
- Community recommendations
- Risk assessment

Use the [LEARNING] protocol for findings.`;

    return spawnWorker({
        id: 'web-critique',
        title: 'Web Source Critique',
        prompt,
        tier: 'milli'
    });
}

async function spawnFinalReportWorker() {
    logSupervisor('FINAL_REPORT_WORKER_START', {});
    
    const prompt = `You are the final report worker. Create the definitive guide for Nix/NixOS + Rust + EC2.

Input files:
- ${RESEARCH_DIR}/06-merged-research.md (consolidated research)
- ${RESEARCH_DIR}/07-web-critique.md (critique and corrections)

Task:
1. Read both input docs
2. Incorporate critique corrections
3. Create final report at ${RESEARCH_DIR}/08-final-report.md
4. Structure:
   - Decision Summary (1-page executive brief)
   - Technical Deep Dive
   - Implementation Playbook
   - Cost Analysis
   - Risk Mitigation
   - Next Steps

This should be the single source of truth for the team.
Make it actionable and specific to ChoirOS needs.

Use the [LEARNING] protocol for final insights.
Output the report and a 3-slide summary for presentation.`;

    return spawnWorker({
        id: 'final-report',
        title: 'Final Report',
        prompt,
        tier: 'milli'
    });
}

async function main() {
    console.log('\n╔════════════════════════════════════════════════════════════╗');
    console.log('║     NixOS Research Supervisor                              ║');
    console.log('║     Run ID:', RUN_ID, '                    ║');
    console.log('╚════════════════════════════════════════════════════════════╝\n');
    
    // Phase 1: Spawn all research workers in parallel
    console.log('\n📚 Phase 1: Spawning research workers...\n');
    const workerPromises = WORKERS.map(spawnWorker);
    const workerResults = await Promise.all(workerPromises);
    
    const successful = workerResults.filter(r => r.success);
    const failed = workerResults.filter(r => !r.success);
    
    console.log(`\n✅ Phase 1 complete: ${successful.length}/${WORKERS.length} workers succeeded`);
    
    if (failed.length > 0) {
        console.log('\n❌ Failed workers:', failed.map(f => f.workerId).join(', '));
    }
    
    // Phase 2: Merge worker
    console.log('\n🔄 Phase 2: Spawning merge worker...\n');
    const mergeResult = await spawnMergeWorker();
    
    if (!mergeResult.success) {
        console.error('❌ Merge worker failed:', mergeResult.stderr);
        process.exit(1);
    }
    
    // Phase 3: Web critique worker
    console.log('\n🔍 Phase 3: Spawning web critique worker...\n');
    const critiqueResult = await spawnCritiqueWorker();
    
    if (!critiqueResult.success) {
        console.error('❌ Critique worker failed:', critiqueResult.stderr);
        process.exit(1);
    }
    
    // Phase 4: Final report worker
    console.log('\n📊 Phase 4: Spawning final report worker...\n');
    const finalResult = await spawnFinalReportWorker();
    
    if (!finalResult.success) {
        console.error('❌ Final report worker failed:', finalResult.stderr);
        process.exit(1);
    }
    
    // Summary
    console.log('\n╔════════════════════════════════════════════════════════════╗');
    console.log('║     Research Complete!                                     ║');
    console.log('╚════════════════════════════════════════════════════════════╝\n');
    
    console.log('Output files:');
    console.log(`  ${RESEARCH_DIR}/`);
    WORKERS.forEach((w, i) => {
        console.log(`    0${i+1}-${w.id}.md`);
    });
    console.log(`    06-merged-research.md`);
    console.log(`    07-web-critique.md`);
    console.log(`    08-final-report.md`);
    console.log(`\nArtifacts: ${RESEARCH_DIR}/supervisor.log.jsonl`);
    
    logSupervisor('SUPERVISOR_COMPLETE', { 
        runId: RUN_ID,
        workersSucceeded: successful.length,
        workersFailed: failed.length
    });
}

main().catch(err => {
    console.error('Supervisor error:', err);
    logSupervisor('SUPERVISOR_ERROR', { error: err.message });
    process.exit(1);
});
