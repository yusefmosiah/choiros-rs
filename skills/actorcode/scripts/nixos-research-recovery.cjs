#!/usr/bin/env node
/**
 * NixOS Research Recovery - Complete the failed workers
 */

const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');

const RESEARCH_DIR = 'docs/research/nixos-research-2026-02-01';

const WORKERS = [
    {
        id: '01-nix-basics',
        title: 'Nix Basics for Rust Dev (RECOVERY)',
        prompt: `Write comprehensive documentation about Nix for Rust development.

Your task: Create ${RESEARCH_DIR}/01-nix-basics.md

Content to cover:
1. What is Nix and why use it for Rust projects
2. Basic flake.nix structure for Rust
3. nix-shell vs nix develop for Rust dev
4. Using rust-overlay for latest toolchain
5. Cross-compilation setup
6. IDE integration (rust-analyzer)

Write this as a practical guide with working code examples.
Do not return until you've written the complete file.
Use [LEARNING] protocol for key insights.

The file MUST be created at: ${RESEARCH_DIR}/01-nix-basics.md`,
        tier: 'micro'
    },
    {
        id: '04-rust-toolchain',
        title: 'Rust Toolchain in Nix (RECOVERY)',
        prompt: `Write comprehensive documentation about managing Rust toolchains with Nix.

Your task: Create ${RESEARCH_DIR}/04-rust-toolchain.md

Content to cover:
1. Comparing fenix vs rust-overlay vs rustup in Nix
2. Setting up multiple Rust versions
3. Cargo workspace configuration
4. Build dependencies and native libraries
5. GitHub Actions CI with Nix
6. Developer experience tips

Write this as a practical guide with working flake.nix examples.
Do not return until you've written the complete file.
Use [LEARNING] protocol for key insights.

The file MUST be created at: ${RESEARCH_DIR}/04-rust-toolchain.md`,
        tier: 'micro'
    }
];

function spawnWorker(worker) {
    return new Promise((resolve, reject) => {
        console.log(`\n🚀 Recovery worker: ${worker.id}`);
        
        const cmd = 'node';
        const args = [
            'skills/actorcode/scripts/actorcode.js',
            'spawn',
            '--title', worker.title,
            '--tier', worker.tier,
            '--prompt', worker.prompt
        ];
        
        const child = spawn(cmd, args, {
            stdio: 'inherit',
            env: { ...process.env }
        });
        
        child.on('close', (code) => {
            if (code === 0) {
                console.log(`✅ ${worker.id} completed`);
                resolve({ success: true });
            } else {
                console.log(`❌ ${worker.id} failed with code ${code}`);
                resolve({ success: false });
            }
        });
    });
}

async function main() {
    console.log('╔════════════════════════════════════════════════════════════╗');
    console.log('║     NixOS Research Recovery                                ║');
    console.log('║     Completing failed workers: nix-basics, rust-toolchain  ║');
    console.log('╚════════════════════════════════════════════════════════════╝\n');
    
    for (const worker of WORKERS) {
        const result = await spawnWorker(worker);
        if (!result.success) {
            console.log(`\n⚠️  ${worker.id} failed, continuing...`);
        }
    }
    
    console.log('\n✅ Recovery complete');
    console.log('Check output:');
    console.log(`  ${RESEARCH_DIR}/01-nix-basics.md`);
    console.log(`  ${RESEARCH_DIR}/04-rust-toolchain.md`);
}

main();
