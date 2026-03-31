# nix-bench

## Narrative Summary (1-minute read)

Most model evaluation for infrastructure will converge on Kubernetes-style RL
environments because they are easy to host, easy to instrument, and reward
imperative repair loops. That leaves a major blind spot: declarative systems
engineering.

`nix-bench` is a benchmark idea for evaluating whether a model can take a
natural-language system brief and produce working Nix or NixOS code that
actually deploys, boots, and satisfies black-box end-to-end tests.

The point is not "can the model write a flake that parses." The point is:
can the model reason about closure boundaries, module composition, service
contracts, secrets wiring, activation behavior, rollback, and idempotent
redeploys.

Nix is a good forcing function because it punishes vague ops reasoning. A
strong model should be able to move from product/system spec -> declarative
infra -> runnable system -> passing tests, with minimal human repair.

## What Changed

- Captured the intuition that declarative ops should have its own benchmark,
  not just be absorbed into generic "agentic DevOps" evaluation.
- Framed the eval around system outcomes, not syntax fluency.
- Noted the difference between a model that is verbally fluent in Nix and a
  model that is operationally native in NixOS deployment work.

## What To Do Next

1. Define a task format with a natural-language brief plus hidden acceptance
   checks.
2. Keep scoring mostly deterministic: build, activate, health, e2e, rollback,
   idempotence.
3. Avoid an LLM-only judge in the critical scoring path.
4. Start with realistic single-node and two-node NixOS tasks before exploring
   harder multi-tenant or promotion workflows.

---

## Core Idea

Two roles:

- **Builder model**: generates Nix code, configuration, and any required
  deployment wiring.
- **Judge harness**: runs deterministic checks and hidden end-to-end tests
  against the produced system.

Optional third role:

- **Task generator model**: creates benchmark instances from a hidden task
  family, but does not directly score the outcome.

The benchmark should prefer machine-checkable success conditions over
open-ended rubric grading.

## What It Should Measure

- Can the model write valid Nix or NixOS modules?
- Can it compose systemd, networking, storage, and service dependencies
  correctly?
- Can it wire secrets and runtime configuration without cheating?
- Can it deploy to a fresh machine or VM from a pinned input set?
- Can it survive a second apply cleanly?
- Can it recover from a bad assumption with a bounded repair loop?
- Can it preserve rollback and explain what changed?

## Task Shape

Each task bundle should include:

- Natural-language product or system brief.
- Infra constraints and allowed package sources.
- Fixed `nixpkgs` pin and runtime image baseline.
- Public smoke checks.
- Hidden acceptance and adversarial checks.

Example task classes:

- Single-node web app + database + reverse proxy.
- Two-node split app and database with private networking.
- Secret rotation and zero-downtime redeploy.
- Timers, backups, health checks, and rollback.
- Migration from one topology to another under uptime constraints.

## Scoring

Primary signals:

- `nix build` succeeds.
- Activation succeeds.
- Services become healthy.
- Hidden end-to-end checks pass.
- Re-apply is idempotent.
- Rollback works after an injected bad change.

Secondary signals:

- Repair iterations.
- Time to green.
- Token and cost budget.
- Size and clarity of the produced diff.

## Why This Matters

Kubernetes-heavy benchmarks mostly reward local patching of a running system.
`nix-bench` would instead reward whole-system declarative reasoning.

That makes it a useful forcing function for:

- models that need stronger real ops capability,
- infra teams that care about reproducibility,
- evaluation suites that currently underweight deployment correctness.

## Design Rules

- Prefer black-box system checks over style judgments.
- Pin inputs so runs are comparable.
- Hide some tests to reduce benchmark gaming.
- Separate task generation from scoring.
- Treat rollback and idempotence as first-class, not bonus points.
- Favor realistic tasks over toy syntax exercises.
