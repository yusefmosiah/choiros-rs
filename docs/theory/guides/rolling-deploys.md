# Rolling Deploys: Staging -> E2E -> Promote

Date: 2026-03-16
Kind: Guide
Status: Active
Priority: 1
Requires: [ADR-0016]

## Why This Is P1

Rolling deploys are the meta-enabler for autonomous development. Without them,
every meaningful change still needs a human SSH session or an implicit trust jump.
With them, the loop becomes:

```text
agent codes -> push -> CI checks -> deploy to staging (Node B)
  -> post-deploy E2E with artifacts -> human reviews -> promote exact SHA to prod (Node A)
```

That is enough to unblock autonomous infrastructure and backend work, and it gives
UI changes a review surface instead of requiring direct production deploys.

## Current State (2026-03-16)

What already exists in the repo:

- `ci.yml` deploys `main` to **Node B** with `nixos-rebuild switch --flake .#choiros-b`
- `promote.yml` manually deploys **Node A** with `workflow_dispatch`
- Playwright already records HTML reports, traces, videos, and failure screenshots under
  `tests/artifacts/playwright/`
- Manual Node B E2E evidence already exists:
  `docs/state/2026-03-08-node-b-e2e-report.md`
- The staging target is already conceptually real: Node B is `draft.choir-ip.com`

This means Phase 1 of the original plan is done. The remaining work is not
"invent staging." It is "make staging the trusted gate before production."

## Remaining Gaps

1. No CI job runs Playwright after the staging deploy finishes.
2. No CI artifact upload exposes traces/videos for reviewer approval.
3. Production promotion is not tied to the tested commit SHA. `promote.yml`
   currently deploys whatever `main` points at when the button is pressed.
4. No GitHub environment approval gate is wired around production deploy.
5. `ci.yml` ignores `tests/playwright/**`, so E2E-only changes do not exercise CI.
6. This is staged promotion, not true zero-downtime traffic draining yet.

## Plan

### Phase 1: Keep Node B as the only automatic deploy target

This is already implemented and should remain the baseline:

- Push to `main`
- Run format + Rust tests
- Deploy Node B with `nixos-rebuild switch --flake .#choiros-b`
- Verify local hypervisor readiness on Node B before the job exits

Do not reintroduce direct auto-deploys to Node A.

### Phase 2: Add a post-deploy staging smoke suite

Add an `e2e-staging` job after `deploy-staging` in `.github/workflows/ci.yml`.

The first gate should be a curated smoke suite, not the full `hypervisor` project.
The current full suite includes:

- known failures (`branch` runtime, `/api/events`, snapshot restore, writer prompt flow)
- local-only proofs (`vfkit` specs)
- destructive tests that already run best with `--workers=1`

Start with a reliable staging gate such as:

```bash
cd tests/playwright
npm ci
npx playwright install --with-deps chromium
PLAYWRIGHT_HYPERVISOR_BASE_URL=https://draft.choir-ip.com \
  npx playwright test \
    bios-auth.spec.ts \
    desktop-app-suite-hypervisor.spec.ts \
    vm-lifecycle-report.spec.ts \
    concurrency-load-test.spec.ts \
    --project=hypervisor \
    --workers=1
```

This gives a real post-deploy signal without making the gate permanently red on
known unrelated failures.

### Phase 3: Upload review artifacts

After the staging smoke suite runs, upload:

- `tests/artifacts/playwright/html-report/`
- `tests/artifacts/playwright/test-results/`

Those artifacts are the review surface for UI and runtime changes. Videos and traces
matter more than a green check alone when humans are validating behavior.

### Phase 4: Promote the tested SHA, not floating `main`

This is the most important correctness gap.

Today, staging and production are decoupled in a dangerous way:

- staging deploy tests one commit
- promotion later does `git pull --ff-only origin main`
- a newer commit can land between those events
- production can receive code that was never staged or reviewed

Production promotion must deploy the exact SHA that passed staging. There are two
acceptable shapes:

1. Keep `promote.yml`, but require a `sha` input and deploy that exact commit on Node A.
2. Move promotion into the CI workflow after `e2e-staging`, guarded by a production
   environment approval.

Either way, Node A should fetch and check out the tested SHA before running
`nixos-rebuild` rather than pulling moving `main`.

### Phase 5: Add manual approval around production

Wrap production deploy in a GitHub `environment: production` gate with required
reviewers.

The reviewer decision should happen after looking at:

- staging job result
- Playwright HTML report
- traces/videos for the relevant changed flows

That gives a real "staging -> human review -> promote" loop instead of a blind button.

### Phase 6: Expand from smoke gate to broader staging E2E

Once the current known failures are fixed or cleanly quarantined:

- remove local-only specs from the staging path
- stabilize writer prompt flow and `/api/events`
- decide whether snapshot/restore belongs in the blocking gate
- widen the smoke suite toward the full hypervisor suite

Do not make the first version of the gate depend on every existing Playwright spec.

### Phase 7: True rolling / blue-green traffic switching (future)

Only after staged promotion is reliable should the system move to actual traffic
cutover or active-active routing.

Possible follow-on work:

- Caddy upstream health checks and draining
- Node A / Node B role swap or load-balanced fronting
- eventual "approve -> flip traffic" instead of "approve -> redeploy"

This is not required for the immediate autonomous-development loop.

## CI Changes Required

Concrete repo changes implied by this plan:

1. Update `.github/workflows/ci.yml`
2. Stop ignoring `tests/playwright/**` for the workflows that own the staging gate
3. Add Node/Playwright setup to the E2E job
4. Upload Playwright artifacts
5. Pass the tested SHA into production promotion
6. Add a production environment approval gate

## Acceptance

The plan is complete when this exact flow exists:

1. Push to `main`
2. CI checks pass
3. Node B deploys automatically
4. Playwright smoke suite runs against Node B
5. Artifacts upload automatically
6. Human approves production promotion
7. Node A deploys the same tested SHA
8. `https://choir-ip.com/login` is healthy after promotion

## What Not To Do

- Do not gate production on the full current Playwright suite immediately
- Do not let production promotion deploy floating `main`
- Do not add Caddy blue/green complexity before the SHA-pinned staging gate works
- Do not confuse "manual production deploy exists" with "rolling deploys are done"
