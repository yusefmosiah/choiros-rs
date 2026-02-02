# System Monitor

Visualize the actor network as ASCII diagrams for chat context or save as evidence/reports.

## Purpose

The system monitor provides visibility into the actor network:
- See all running/completed/failed actors at a glance
- View hierarchy (parent-child relationships)
- Check runtime and log activity
- Export reports for evidence

## Usage

### Quick View (Terminal)

```bash
# Full ASCII diagram
node skills/system-monitor/scripts/system-monitor.js

# Compact view (for chat context)
node skills/system-monitor/scripts/system-monitor.js --compact

# Save report to file
node skills/system-monitor/scripts/system-monitor.js --save
```

### From Justfile

```bash
# View actor network
just monitor

# Save report
just monitor-save
```

### Programmatic

```javascript
const { loadRegistry, generateNetworkDiagram } = require('./skills/system-monitor/scripts/system-monitor.js');

const registry = loadRegistry();
const diagram = generateNetworkDiagram(registry);
console.log(diagram);
```

## Output Formats

### Full Diagram

```
╔════════════════════════════════════════════════════════════════╗
║              ACTOR NETWORK - System Monitor                    ║
╠════════════════════════════════════════════════════════════════╣
║  Generated: 2026-02-01T20:00:00.000Z                           ║
║  Total Actors: 6                                               ║
╚════════════════════════════════════════════════════════════════╝

┌─ Status Summary ──────────────────────────────────────────────┐
│ ▶ running   :   3 actors                                       │
│ ✓ completed :   2 actors                                       │
│ ✗ failed    :   1 actor                                        │
└────────────────────────────────────────────────────────────────┘

┌─ Actor Details ───────────────────────────────────────────────┐
│
│  ○ abc123...  ▶ running
│     Title: Doc analysis - ARCHITECTURE_SPEC
│     Tier:  pico       Runtime: 2m 15s
│     Log:   45 lines, 3.2 KB
│     Last:  [2026-02-01T20:00:00Z] Analyzing file structure
│     ────────────────────────────────────────
│  ◐ def456...  ✓ completed
│     Title: Doc analysis - TESTING_STRATEGY
│     Tier:  nano       Runtime: 5m 30s
│     Log:   120 lines, 8.5 KB
│     Last:  [2026-02-01T20:05:00Z] Report generated
│
└────────────────────────────────────────────────────────────────┘
```

### Compact View (Markdown)

```markdown
**Actor Network**

Status: running: 3 | completed: 2 | failed: 1

- **abc123** (pico): running - "Doc analysis - ARCH..." (2m 15s)
- **def456** (nano): completed - "Doc analysis - TEST..." (5m 30s)
- **ghi789** (micro): failed - "Code generation" (30s)
```

## Files

- Registry: `.actorcode/registry.json`
- Logs: `logs/actorcode/<session_id>.log`
- Reports: `reports/actor-network-<timestamp>.md`

## Integration

### With Actorcode

The monitor reads from the actorcode registry, so it works seamlessly with:
- `actorcode spawn` - Shows new actors immediately
- `actorcode status` - Same data, different view
- `actorcode logs` - Deep dive into specific actors

### With Dashboard

ASCII output is for chat/terminal. For graphical view, see:
- Web dashboard: `just research-web`
- Tmux dashboard: `just research-dashboard`

## Future Enhancements

- [ ] Real-time updates (watch mode)
- [ ] Historical trends (actor lifecycle over time)
- [ ] Resource usage (CPU, memory per actor)
- [ ] Network topology graph (DOT format for Graphviz)
- [ ] Integration with findings database

---

*Part of the ChoirOS automatic computer vision*
