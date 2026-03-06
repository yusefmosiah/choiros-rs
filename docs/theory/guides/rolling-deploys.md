# Rolling Deploys: Staging → E2E → Promote

Date: 2026-03-06
Kind: Guide
Status: Active
Priority: 1
Requires: []

## Why This Is P1

Rolling deploys are the meta-enabler for autonomous development. Without them,
every change requires a human SSH session. With them, the loop becomes:

```
agent codes → push → CI tests → deploy to staging (Node B)
  → e2e tests with video → human reviews artifacts → promote to prod (Node A)
```

This unblocks both autonomous (test-only changes) and semi-autonomous (UI changes
where human reviews video before approval) implementation of the full roadmap.

## What Exists Today

- **Node A** (51.81.93.94, choir-ip.com): production, CI deploys here
- **Node B** (147.135.70.196): same hardware, NixOS, not actively used
- **CI**: push → fmt → test → deploy to Node A (hard restart)
- **Caddy**: TLS termination on Node A, could load-balance to both
- **E2E**: Playwright specs exist, video recording configured

## What's Missing

1. Node B not receiving deploys
2. No staging environment concept
3. E2E tests don't run post-deploy in CI
4. No promotion step (staging → prod)
5. No Caddy config for blue/green traffic switching

## Implementation

### Phase 1: Node B as staging

**CI changes (`.github/workflows/ci.yml`):**

```yaml
# After tests pass, deploy to Node B (staging) instead of Node A
deploy-staging:
  needs: test
  runs-on: ubuntu-latest
  steps:
    - name: Deploy to staging (Node B)
      run: |
        ssh root@$NODE_B '
          cd /opt/choiros/workspace &&
          git pull --ff-only origin main &&
          nix build ./sandbox#sandbox -o result-sandbox &&
          nix build ./hypervisor#hypervisor -o result-hypervisor &&
          cp -f result-sandbox/bin/sandbox /opt/choiros/bin/sandbox &&
          cp -f result-hypervisor/bin/hypervisor /opt/choiros/bin/hypervisor &&
          systemctl restart hypervisor sandbox
        '
```

**Verification:** `curl -fsS http://$NODE_B:9090/health`

### Phase 2: Post-deploy E2E on staging

```yaml
e2e-staging:
  needs: deploy-staging
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - name: Run Playwright against staging
      run: |
        cd tests/playwright
        PLAYWRIGHT_BASE_URL=http://$NODE_B:9090 \
          npx playwright test --project=sandbox
    - uses: actions/upload-artifact@v4
      with:
        name: e2e-results
        path: tests/playwright/test-results/
```

E2E artifacts (videos, traces, screenshots) are uploaded as CI artifacts
for human review.

### Phase 3: Manual promotion gate

```yaml
promote-production:
  needs: e2e-staging
  environment: production  # GitHub environment with required reviewers
  runs-on: ubuntu-latest
  steps:
    - name: Deploy to production (Node A)
      run: |
        ssh root@$NODE_A '
          cd /opt/choiros/workspace &&
          git pull --ff-only origin main &&
          nix build ./sandbox#sandbox -o result-sandbox &&
          nix build ./hypervisor#hypervisor -o result-hypervisor &&
          cp -f result-sandbox/bin/sandbox /opt/choiros/bin/sandbox &&
          cp -f result-hypervisor/bin/hypervisor /opt/choiros/bin/hypervisor &&
          systemctl restart hypervisor sandbox
        '
```

The `environment: production` with required reviewers means a human must
click "Approve" in GitHub after reviewing e2e artifacts. For non-UI changes
where all e2e tests pass, this could be auto-approved later.

### Phase 4: Caddy load balancing (future)

Once both nodes are healthy and deployed, Caddy can load-balance:

```
choir-ip.com {
    reverse_proxy node-a:9090 node-b:9090 {
        health_uri /health
        health_interval 10s
    }
}
```

This is not needed for Phase 1-3. Single-node prod with staging is sufficient.

## Test Gates

```bash
# T1: Node B receives deploy from CI
ssh root@$NODE_B 'systemctl is-active hypervisor sandbox'

# T2: E2E tests run against Node B
PLAYWRIGHT_BASE_URL=http://$NODE_B:9090 npx playwright test

# T3: Promotion deploys to Node A
ssh root@$NODE_A 'systemctl is-active hypervisor sandbox'

# T4: choir-ip.com serves from Node A after promotion
curl -fsS https://choir-ip.com/health
```

## Order of Operations

1. Ensure Node B has same NixOS config, secrets, and workspace as Node A
2. Update CI to deploy to Node B after tests pass
3. Add Playwright e2e step targeting Node B
4. Add promotion job with GitHub environment gate
5. Wire up artifact upload for video review

## What NOT to Do

- Don't build fleet-ctl yet — this is CI/CD pipeline work, not new Rust code
- Don't add Caddy load balancing until both nodes are consistently healthy
- Don't auto-approve promotions until the e2e suite has proven reliable
