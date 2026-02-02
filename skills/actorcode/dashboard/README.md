# Actorcode Dashboard

Multi-view dashboard for visualizing the actor network.

## Views

### 1. List View (Default)
Traditional list of all sessions with status indicators.
- Shows: title, agent, tier, last activity
- Click: Log or Summary buttons

### 2. Network View 🕸️
Force-directed graph showing actor relationships.
- Nodes: actors (size = importance)
- Edges: parent-child relationships
- Colors: status (green=running, blue=completed, red=error)
- Drag nodes to rearrange
- Click node to view details

### 3. Timeline View 📊
Horizontal timeline showing actor lifespans.
- X-axis: time
- Y-axis: actors
- Bar length: duration
- Overlapping bars: parallel execution
- Click bar to view details

### 4. Hierarchy View 🌳
Collapsible tree showing supervisor → worker relationships.
- Icons: tier (○ ◐ ◑ ◒) and status (▶ ✓ ✗)
- Indentation: depth in tree
- Click to expand/collapse or view details

## Usage

```bash
# Start the findings server
just findings-server

# Open dashboard
open skills/actorcode/dashboard/index.html

# Or serve via HTTP
python -m http.server 8766 --directory skills/actorcode/dashboard
```

## Keyboard Shortcuts

- `1` - List view
- `2` - Network view
- `3` - Timeline view
- `4` - Hierarchy view
- `Escape` - Close modal

## Files

```
dashboard/
├── index.html      # Main entry point
├── styles.css      # All styles
├── app.js          # Main application logic
├── views/          # (Future) Individual view modules
└── components/     # (Future) Reusable components
```

## Future Enhancements

- [ ] WebSocket for real-time updates
- [ ] Sound notifications
- [ ] Export views as PNG/SVG
- [ ] Filter by time range
- [ ] Search functionality
- [ ] Actor details panel

---

*Part of the actorcode skill suite*
