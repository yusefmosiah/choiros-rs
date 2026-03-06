# Mind Map + Checklist UI: Comprehensive Research Document

**Research Date:** 2026-02-08
**Context:** ChoirOS directive visualization system
**Goal:** Select architecture for hierarchical mind map with checklist execution states and dependency visualization

---

## Executive Summary

After extensive research into mind map libraries, graph visualization tools, canvas architectures, word cloud visualization, multi-view integration patterns, and modern productivity application patterns, the recommendation for ChoirOS is:

### Multi-View Architecture

**Three interconnected visualization modes:**

1. **Word Cloud (Zoom 0.5-1.5x)** - "At-a-glance" density view
   - Shows directive frequency, importance, and patterns
   - Click to drill down; double-click to zoom to mind map view
   - Library: d3-cloud (2.9KB, proven spiral layout)

2. **Mind Map (Zoom 1.5-4x)** - Structural relationship view
   - True radial layout with D3.js d3-hierarchy
   - Checklist states, dependencies, expand/collapse
   - Primary workspace for navigation and planning

3. **Detail Panel (Zoom 4x+)** - Full information view
   - Complete directive metadata, actions, history
   - Edit capabilities and execution controls

**View Transitions:** FLIP animations with shared element transitions; 300ms ease-in-out; hysteresis thresholds prevent flickering.

### Key Insight

Multiple visualization pathways increase cognitive bandwidth:
- **Word cloud** for pattern recognition and discovery
- **Mind map** for understanding relationships and planning
- **Detail panel** for execution and deep work

Smooth animated transitions maintain context while switching modes.

### Technology Stack

| View | Technology | Bundle | Rationale |
|------|------------|--------|-----------|
| Word Cloud | d3-cloud | 2.9KB | Mature spiral algorithm, D3 ecosystem |
| Mind Map | D3.js d3-hierarchy | ~150KB | True radial, custom nodes, EventStore integration |
| Detail Panel | Dioxus components | Native | Rich interactivity, type-safe |
| Transitions | FLIP + View Transitions API | Native | 60fps morphing, GPU-accelerated |

**Why not dedicated mind map libraries:** They optimize for content authoring, not reactive state visualization. Their impedance mismatch with event-sourced architectures creates more work than building precisely what we need.

---

## Part 1: Dedicated Mind Map Library Analysis

### 1.1 Library Comparison Matrix

| Library | Bundle Size | Dependencies | True Radial | Two-Way Binding | Checklist Support | Performance | Maintenance |
|---------|-------------|--------------|-------------|-----------------|-------------------|-------------|-------------|
| **Mind Elixir** | 33KB | 0 | No (L/R/Side) | Excellent | Custom CSS | 500-1000 nodes | Moderate |
| **KityMinder** | 61KB | Kity SVG | Yes (Tianpan) | Good | Progress renderer | 500-1000 nodes | Low (Baidu) |
| **markmap** | 200KB | D3 submodules | Yes (360°) | Limited | None | ~500 nodes | Moderate |
| **jsMind** | Compact | Minimal | No | Poor | None | 1000+ (Canvas) | Low |
| **simple-mind-map** | Moderate | Vue optional | Yes | Good | Possible | 1000+ (Canvas) | Active |

### 1.2 Detailed Library Profiles

#### Mind Elixir
**Best for:** Fast integration with external state control

```javascript
// Initialization
const mind = new MindElixir({
  el: container,
  direction: MindElixir.SIDE, // LEFT, RIGHT, SIDE, BOTTOM
  data: MindElixir.new('Root')
});
mind.init();

// Two-way binding via event bus
mind.bus.addListener('operation', (operation) => {
  // operation.name: 'insertSibling' | 'addChild' | 'removeNode' | 'finishEdit'
  // operation.obj: operation-specific data
  syncToRustBackend(mind.getData());
});

// External control
mind.addChild(targetNode, { topic: 'New Task', checked: false });
mind.selectNode(node);
mind.scale(0.8);
```

**Limitations:**
- No true radial layout (only left/right/side)
- No built-in checklist semantics (requires custom node rendering)
- Single maintainer project

#### KityMinder
**Best for:** Multiple layout types in one tool

```javascript
// 5 built-in layouts: 'mind' | 'btree' | 'filetree' | 'tianpan' | 'fishbone'
minder.execCommand('template', 'tianpan'); // True radial

// Rich renderer ecosystem
KityMinder.registerRenderer('progress', {
  create: function() {
    return new kity.ProgressPie();
  },
  shouldRender: function(node) {
    return node.getData('progress') !== undefined;
  }
});
```

**Limitations:**
- Documentation primarily in Chinese
- Older codebase (Baidu FEX project)
- Heavy SVG rendering (performance ceiling ~1000 nodes)

#### markmap
**Best for:** Markdown-native workflows

```javascript
// One-way flow: Markdown → Mind map
import { Markmap } from 'markmap-view';

const mm = Markmap.create(svg, { autoFit: true }, markdown);
mm.setData('# New Root\n## Child 1\n## Child 2');

// Limited interactivity
mm.setHighlight(nodeId);
mm.expand(node);
mm.collapse(node);
```

**Limitations:**
- Designed for static documentation, not interactive apps
- Getting changes back requires DOM mutation observers
- Full D3.js dependency chain (~200KB)

### 1.3 Why Dedicated Libraries Fall Short for ChoirOS

| ChoirOS Requirement | Mind Map Library Reality |
|--------------------|--------------------------|
| EventStore as source of truth | Libraries assume they own state |
| Checklist state machine (planned/active/blocked/done) | Text/content nodes only |
| Dependency edges between any nodes | Tree structure only |
| 2000+ node performance | Most cap at 500-1000 |
| Keyboard-first workflow | Mouse/touch optimized |
| Real-time collaboration | Single-user design |

**The Translation Tax:** Every mind map library requires a translation layer:

```rust
// ChoirOS EventStore events
GraphEvent::ChecklistStateChanged { node_id, from, to }

// Must translate to...

// Mind Elixir
mind.bus.addListener('operation', |op| {
    if op.name == "updateNode" {
        // Reverse-engineer what changed
        // Emit GraphEvent
    }
});

// This bidirectional translation is error-prone and laggy
```

---

## Part 2: General Graph Library Comparison

### 2.1 Option Matrix

| Library | Rendering | Bundle | 2000 Nodes | Tree Layout | Custom Nodes | License | Dioxus Fit |
|---------|-----------|--------|------------|-------------|--------------|---------|------------|
| **Cytoscape.js** | Canvas/WebGL | 300KB | 60 FPS | dagre/cose | CSS + SVG | MIT | WebView |
| **D3.js** | SVG/Canvas | 150KB | 45-60 FPS | d3-hierarchy | Full control | ISC | WebView |
| **Sigma.js v3** | WebGL | 200KB | 60+ FPS | Force-directed | Custom shaders | MIT | WebView |
| **vis-network** | Canvas | 350KB | 45-60 FPS | Hierarchical | Templates | MIT/Apache | WebView |
| **Pixi.js + custom** | WebGL | 250KB | 60+ FPS | Manual | Unlimited | MIT | WebView |

### 2.2 D3.js Deep Dive

**Architecture:**
```javascript
// Data preparation
d3.hierarchy(data)           // Convert flat data to tree structure
  .sum(d => d.value)         // Aggregate for sizing
  .sort((a, b) => b.value - a.value); // Ordering

// Layout algorithms
d3.tree();                   // Classic tidy tree
d3.cluster();                // Dendrogram (leaves at same depth)
d3.treemap();                // Space-filling rectangles
d3.partition();              // Icicle/sunburst
d3.pack();                   // Circle packing

// Specific to mind maps: Radial tree
const tree = d3.tree()
  .size([2 * Math.PI, radius])  // 360 degrees
  .separation((a, b) => (a.parent === b.parent ? 1 : 2) / a.depth);
```

**Performance Characteristics:**
| Metric | SVG (default) | Canvas | WebGL (regl) |
|--------|---------------|--------|--------------|
| 500 nodes | 60 FPS | 60 FPS | 60 FPS |
| 1000 nodes | 45 FPS | 60 FPS | 60 FPS |
| 2000 nodes | 30 FPS | 45 FPS | 60 FPS |
| Memory | ~35MB | ~25MB | ~20MB |

**ChoirOS Integration Pattern:**
```rust
// Rust: Project events to layout
#[derive(Serialize)]
struct LayoutNode {
    id: String,
    title: String,
    checklist_state: ChecklistState,
    progress_percent: u8,
    children: Vec<LayoutNode>,
}

// D3.js: Render with checklist visualization
function renderChecklistNode(selection) {
  selection.each(function(d) {
    const g = d3.select(this);

    // Background
    g.append('rect')
      .attr('class', 'node-bg')
      .attr('rx', 4);

    // Progress fill based on checklist state
    g.append('rect')
      .attr('class', 'progress-fill')
      .attr('width', d => d.data.progress_percent + '%')
      .attr('fill', d => stateColor(d.data.checklist_state));

    // Text
    g.append('text')
      .attr('class', 'node-title')
      .text(d => d.data.title);

    // State indicator
    g.append('circle')
      .attr('class', 'state-indicator')
      .attr('r', 6)
      .attr('fill', d => stateColor(d.data.checklist_state));
  });
}
```

### 2.3 Cytoscape.js Deep Dive

**Architecture:**
```javascript
// Initialization
cy = cytoscape({
  container: document.getElementById('cy'),
  elements: [
    // Nodes
    { data: { id: 'a', label: 'Root', state: 'active' } },
    { data: { id: 'b', label: 'Child', state: 'planned' } },
    // Edges
    { data: { id: 'ab', source: 'a', target: 'b' } }
  ],
  style: [
    {
      selector: 'node',
      style: {
        'background-color': '#666',
        'label': 'data(label)',
        'width': 40,
        'height': 40
      }
    },
    {
      selector: 'node[state="done"]',
      style: { 'background-color': '#22c55e' }
    }
  ],
  layout: {
    name: 'dagre',  // Hierarchical
    rankDir: 'TB',  // Top-bottom
    nodeSep: 50,
    rankSep: 100
  }
});

// Event handling
cy.on('tap', 'node', evt => {
  const node = evt.target;
  console.log('Selected:', node.id());
});
```

**Performance Optimizations:**
```javascript
// For large graphs
const cy = cytoscape({
  // Viewport culling
  textureOnViewport: true,

  // Motion blur during pan/zoom
  motionBlur: true,

  // Hide edges during gestures
  hideEdgesOnViewport: true,

  // Level of detail
  minZoom: 0.3,
  maxZoom: 3,

  // Wheel sensitivity
  wheelSensitivity: 0.3
});
```

### 2.4 When to Choose What

| Scenario | Recommendation | Rationale |
|----------|----------------|-----------|
| Hierarchy > Dependencies | **D3.js + d3-hierarchy** | Purpose-built for trees, radial layout native |
| Dependencies > Hierarchy | **Cytoscape.js** | Native graph semantics, force-directed |
| 5000+ nodes required | **Sigma.js v3** | WebGL rendering, handles massive graphs |
| Game-like interactions | **Pixi.js + custom** | Maximum control, scene graph architecture |
| Rapid prototyping | **Cytoscape.js** | Rich ecosystem, quick to working demo |

---

## Part 3: Architecture Patterns from Modern Tools

### 3.1 Notion: Block-Based Hierarchy

**Key Insights:**
- Everything is a Block with UUID, type, properties, content array
- Parent pointers separate from content (enables permission traversal)
- Structural indentation = data relationship, not presentation
- Granular sync at block level reduces network traffic

**ChoirOS Application:**
```rust
// Block-like node structure
struct DirectiveNode {
    id: NodeId,
    node_type: NodeType,  // Branch, Leaf, ChecklistItem, Milestone
    content: NodeContent, // Title, description, metadata
    children: Vec<NodeId>, // Ordered content array
    parent: Option<NodeId>, // For ancestry/permissions

    // Visual state (separate from content)
    visual: VisualState,
}
```

### 3.2 Figma: Infinite Canvas

**Key Techniques:**
- **Tile-based rendering:** Screen divided into tiles, only visible tiles rendered
- **Multi-resolution tiles:** Different zoom levels cached separately
- **Dirty rectangle tracking:** Only changed tiles re-render
- **WebAssembly:** C++ compiled to WASM for near-native performance
- **WebGPU migration:** 2-3x performance improvement over WebGL

**ChoirOS Application:**
```rust
// Spatial indexing for viewport culling
struct ViewportManager {
    spatial_index: RTree<BoundingBox>,
    tile_cache: LruCache<TileKey, RenderedTile>,
    zoom_level: ZoomLevel, // Determines which tile resolution to use
}

impl ViewportManager {
    fn get_visible_nodes(&self, viewport: Rect) -> Vec<NodeId> {
        self.spatial_index.query(viewport)
    }

    fn render_tile(&mut self, tile: TileKey) -> &RenderedTile {
        self.tile_cache.get_or_insert(tile, || {
            render_nodes_to_tile(self.get_nodes_in_tile(tile))
        })
    }
}
```

### 3.3 Linear: Streamlined State Management

**Three-Layer Architecture:**
```
useState (Component) → Zustand (Global UI) → TanStack Query (Server)
```

**Key Patterns:**
- Optimistic UI updates (immediate feedback, rollback on failure)
- Event-driven commands (unified menu/shortcut/command palette routing)
- Contextual shortcuts (same key, different action by context)
- Focus management (always restore previous focus)

**ChoirOS Application:**
```rust
// Optimistic update pattern
fn transition_checklist_state(node_id: NodeId, new_state: ChecklistState) {
    // 1. Capture previous state
    let previous = get_current_state(node_id);

    // 2. Optimistically update UI
    update_ui_state(node_id, new_state);

    // 3. Emit event to backend
    let event = GraphEvent::ChecklistStateChanged {
        node_id,
        from: previous,
        to: new_state,
    };

    // 4. Handle failure
    if let Err(e) = emit_event(event).await {
        // Rollback UI
        update_ui_state(node_id, previous);
        show_error(e);
    }
}
```

### 3.4 Obsidian: Local-First + Graph

**Architecture:**
- Plain Markdown files on local filesystem (no vendor lock-in)
- DataScript (in-memory Datalog) for graph queries
- Force-directed graph view using D3.js (original) → Pixi (current)

**Key Insight:** Local-first enables true offline work and data longevity

**ChoirOS Application:**
```rust
// Local-first event store
struct LocalEventStore {
    db: sled::Db, // Embedded KV store
    sync_client: Option<CloudSyncClient>, // Optional cloud backup
}

impl LocalEventStore {
    fn append(&self, event: GraphEvent) -> Result<Seq> {
        let seq = self.db.generate_seq()?;
        self.db.insert(seq, serialize(event))?;

        // Async sync to cloud (if configured)
        if let Some(client) = &self.sync_client {
            spawn(async move { client.sync_event(seq, event).await });
        }

        Ok(seq)
    }
}
```

### 3.5 Heptabase: Spatial Hybrids

**Three-Layer Model:**
1. **Card Layer:** Atomic knowledge units
2. **Contextual Layer:** Whiteboards with spatial positioning
3. **Descriptive Layer:** Tags, properties, database views

**Key Innovation:** Cards can appear on multiple whiteboards simultaneously (like synced blocks)

**ChoirOS Application:**
```rust
// Multi-context directives
struct Directive {
    id: DirectiveId,
    content: DirectiveContent, // Single source of truth
}

struct Whiteboard {
    id: WhiteboardId,
    cards: Vec<CardPlacement>,
}

struct CardPlacement {
    directive_id: DirectiveId, // Reference, not duplication
    position: Position,
    size: Size,
    collapsed: bool, // Visual state per context
}

// Same directive can exist on Roadmap whiteboard AND Sprint whiteboard
// with different positions and collapse states
```

---

## Part 4: Rendering Technology Comparison

### 4.1 Technology Matrix

| Technology | Best For | Max Nodes (60fps) | Pros | Cons |
|------------|----------|-------------------|------|------|
| **SVG** | Interactivity, accessibility | ~2,000 | DOM events, CSS styling, accessible | DOM overhead |
| **Canvas 2D** | Balanced performance | ~10,000 | Good performance, pixel control | Manual events, CPU-bound |
| **WebGL** | Massive datasets | ~100,000 | GPU acceleration, shaders | Complex, limited browser support |
| **WebGPU** | Future-proof compute | ~1,000,000 | Compute shaders, modern API | Limited support (2024) |
| **DOM/CSS** | Rapid prototyping | ~500 | Easy styling, native events | Slowest, layout thrashing |

### 4.2 Hybrid Approaches

**Tldraw Pattern (React + Canvas/SVG):**
- React components output HTML/SVG (NOT Canvas API)
- Shape system with TypeScript interfaces
- Native accessibility and CSS styling
- Automatic hit-testing

**Performance ceiling:** ~1,000-5,000 objects

**ChoirOS Fit:** Good for rapid iteration, may hit limits at scale

**Excalidraw Pattern (Canvas 2D + Rough.js):**
- HTML5 Canvas 2D API
- Rough.js for hand-drawn aesthetic
- Scene graph with flat element array
- Viewport culling + element caching

**Performance:** 1,000-10,000 elements

**ChoirOS Fit:** Good for custom aesthetic, moderate scale

**Figma Pattern (C++ → WASM → WebGL):**
- C++ rendering engine compiled to WebAssembly
- WebGL for GPU acceleration
- Tile-based infinite canvas
- React only for UI chrome

**Performance:** 10,000+ objects

**ChoirOS Fit:** Overkill for current scope, consider for v2

### 4.3 Level of Detail (LOD) Strategies

| Zoom Level | Detail Level | Elements Rendered |
|------------|--------------|-------------------|
| < 0.2x | Cluster representatives | ~5% of nodes |
| 0.2-0.5x | Simplified nodes (circles only) | ~25% of nodes |
| 0.5-1x | Full nodes, no labels | ~50% of nodes |
| > 1x | Full nodes + labels + details | 100% of visible |

**Implementation:**
```rust
fn get_lod_level(zoom: f64) -> LodLevel {
    match zoom {
        z if z < 0.2 => LodLevel::Clusters,
        z if z < 0.5 => LodLevel::Simplified,
        z if z < 1.0 => LodLevel::FullNoLabels,
        _ => LodLevel::FullDetail,
    }
}

fn render_node(node: &Node, lod: LodLevel) -> RenderElement {
    match lod {
        LodLevel::Clusters => RenderElement::Dot { color: node.color },
        LodLevel::Simplified => RenderElement::Circle {
            radius: node.size,
            color: node.color
        },
        LodLevel::FullNoLabels => RenderElement::Rect {
            width: node.width,
            height: node.height,
            color: node.color,
            progress: node.progress,
        },
        LodLevel::FullDetail => RenderElement::FullNode {
            node: node.clone()
        },
    }
}
```

---

## Part 5: Hybrid Tree-Graph Architecture

### 5.1 The Problem

Mind maps are fundamentally **trees** (hierarchical), but real-world directives have **cross-cutting concerns** (dependencies, references, blockers).

**Pure Tree:**
```
Root
├── Epic A
│   ├── Task 1
│   └── Task 2
└── Epic B
    ├── Task 3
    └── Task 4
```

**Tree + Dependencies:**
```
Root
├── Epic A
│   ├── Task 1 ───────┐
│   └── Task 2         │ (dependency)
└── Epic B             ▼
    ├── Task 3 ◄───────┘
    └── Task 4
```

### 5.2 Data Model Patterns

**Option A: Tree-First with Cross-Links (Recommended)**
```rust
struct GraphState {
    // Primary: Tree structure
    nodes: HashMap<NodeId, Node>,
    root_id: NodeId,

    // Tree edges: parent_id → [child_ids]
    tree_edges: HashMap<NodeId, Vec<NodeId>>,

    // Secondary: Cross-links (dependencies, references)
    cross_edges: Vec<CrossEdge>,
}

struct CrossEdge {
    id: EdgeId,
    source_id: NodeId,
    target_id: NodeId,
    edge_type: EdgeType, // Dependency | Reference | Blocker
}
```

**Layout Strategy:**
1. Compute tree layout (radial or horizontal) using tree_edges
2. Draw cross_edges as curved lines above tree edges
3. Apply force-directed adjustment to reduce cross-edge crossings

**Option B: Dual Representation**
```rust
struct GraphState {
    // Storage: Tree structure (editing)
    tree: TreeStructure,

    // Analysis: Graph structure (dependencies)
    graph: GraphStructure, // petgraph or similar
}

// Sync on changes
fn on_tree_change(&mut self) {
    self.graph = self.tree.to_graph();
}
```

### 5.3 Handling Collapse with Cross-Links

**Problem:** When node A depends on node B, and B is collapsed under parent P, what happens to the dependency visualization?

**Solutions:**

1. **Ghost Indicators** (XMind pattern)
   - Show dashed line from A to P (collapsed parent)
   - Badge on P showing hidden dependency count
   - Tooltip listing hidden dependencies

2. **Promote to Visible** (Freeplane pattern)
   - If A is visible and B is collapsed, temporarily show B as ghost node
   - Ghost nodes fade out when no longer needed

3. **Hide and Track** (Linear pattern)
   - Hide dependency line when either endpoint collapsed
   - Track in background, show on expand
   - Show "hidden dependencies" badge

**Implementation:**
```rust
fn get_visible_edges(&self, expanded: &HashSet<NodeId>) -> Vec<&CrossEdge> {
    self.cross_edges
        .iter()
        .filter(|edge| {
            let source_visible = expanded.contains(&edge.source_id) ||
                                self.is_ancestor_expanded(&edge.source_id, expanded);
            let target_visible = expanded.contains(&edge.target_id) ||
                                self.is_ancestor_expanded(&edge.target_id, expanded);
            source_visible && target_visible
        })
        .collect()
}
```

### 5.4 Layout Algorithm Pipeline

**Phase 1: Tree Layout**
```rust
fn compute_tree_layout(&self) -> HashMap<NodeId, Position> {
    let root = self.get_root();
    let hierarchy = build_d3_hierarchy(root, &self.tree_edges);

    // Radial layout
    let tree_layout = d3_tree()
        .size(2.0 * PI, self.radius)
        .separation(|a, b| (if a.parent == b.parent { 1.0 } else { 2.0 }) / a.depth);

    tree_layout(hierarchy)
}
```

**Phase 2: Cross-Link Optimization**
```rust
fn optimize_cross_links(&self, positions: &mut HashMap<NodeId, Position>) {
    // Force-directed refinement for nodes with cross-links
    for _ in 0..ITERATIONS {
        for edge in &self.cross_edges {
            let source_pos = positions[&edge.source_id];
            let target_pos = positions[&edge.target_id];

            // Spring force toward ideal length
            let delta = target_pos - source_pos;
            let distance = delta.magnitude();
            let force = (distance - IDEAL_LENGTH) * SPRING_CONSTANT;

            // Apply to both nodes (but weighted by their "mass")
            positions[&edge.source_id] += delta.normalize() * force * 0.5;
            positions[&edge.target_id] -= delta.normalize() * force * 0.5;
        }

        // Constraint: Keep close to tree position
        for (id, pos) in positions.iter_mut() {
            let tree_pos = self.tree_positions[id];
            *pos = tree_pos * CONSTRAINT_STRENGTH + *pos * (1.0 - CONSTRAINT_STRENGTH);
        }
    }
}
```

---

## Part 6: State Management Patterns

### 6.1 Flattened Tree State (Recommended)

**Problem:** Nested tree structures are hard to update immutably and slow to traverse.

**Solution:** Flatten with parent references
```rust
struct FlattenedTree {
    // O(1) lookup by ID
    nodes: HashMap<NodeId, FlatNode>,

    // Ordered root IDs (supports multiple roots)
    roots: Vec<NodeId>,

    // Pre-computed derived data
    depth_cache: HashMap<NodeId, u32>,
    path_cache: HashMap<NodeId, Vec<NodeId>>,
}

struct FlatNode {
    data: Node,
    parent_id: Option<NodeId>,
    child_ids: Vec<NodeId>, // Ordered
    depth: u32, // Computed
    expanded: bool,
}

impl FlattenedTree {
    fn get_visible_nodes(&self) -> Vec<&FlatNode> {
        let mut visible = Vec::new();
        let mut stack: Vec<&NodeId> = self.roots.iter().collect();

        while let Some(id) = stack.pop() {
            let node = &self.nodes[id];
            visible.push(node);

            if node.expanded {
                for child_id in &node.child_ids {
                    stack.push(child_id);
                }
            }
        }

        visible
    }

    fn move_node(&mut self, node_id: NodeId, new_parent_id: Option<NodeId>) {
        // O(1) operations
        let node = self.nodes.get(&node_id).unwrap();
        let old_parent_id = node.parent_id;

        // Remove from old parent
        if let Some(old_parent) = old_parent_id {
            let parent = self.nodes.get_mut(&old_parent).unwrap();
            parent.child_ids.retain(|id| id != &node_id);
        } else {
            self.roots.retain(|id| id != &node_id);
        }

        // Add to new parent
        if let Some(new_parent) = new_parent_id {
            let parent = self.nodes.get_mut(&new_parent).unwrap();
            parent.child_ids.push(node_id);
        } else {
            self.roots.push(node_id);
        }

        // Update node
        self.nodes.get_mut(&node_id).unwrap().parent_id = new_parent_id;

        // Recompute depths for moved subtree
        self.recompute_depths(node_id);
    }
}
```

### 6.2 Optimistic Updates

**Pattern for ChoirOS:**
```rust
struct OptimisticState<T> {
    committed: T,           // Last confirmed from server
    pending: Vec<Op>,       // Operations sent, awaiting confirmation
    optimistic: T,          // committed + pending applied
}

impl OptimisticState<GraphState> {
    fn apply_local(&mut self, op: Op) {
        // Apply immediately to optimistic view
        apply_op(&mut self.optimistic, op.clone());

        // Send to server
        self.pending.push(op);
        self.flush_pending().await;
    }

    fn receive_confirmation(&mut self, seq: Seq) {
        // Move from pending to committed
        let confirmed = self.pending.drain(..).take_while(|op| op.seq <= seq);
        for op in confirmed {
            apply_op(&mut self.committed, op);
        }

        // If any pending remaining, recompute optimistic
        if !self.pending.is_empty() {
            self.optimistic = self.committed.clone();
            for op in &self.pending {
                apply_op(&mut self.optimistic, op.clone());
            }
        }
    }

    fn receive_rejection(&mut self, rejected_seq: Seq) {
        // Rollback rejected op and all after it
        self.pending.retain(|op| op.seq < rejected_seq);

        // Recompute optimistic from committed
        self.optimistic = self.committed.clone();
        for op in &self.pending {
            apply_op(&mut self.optimistic, op.clone());
        }
    }
}
```

### 6.3 Batch Updates

**Problem:** Frequent small updates (drag, collapse, state change) cause re-render thrashing.

**Solution:** RAF-batch updates
```rust
struct UpdateBatcher {
    pending: Vec<GraphUpdate>,
    scheduled: bool,
}

impl UpdateBatcher {
    fn push(&mut self, update: GraphUpdate) {
        self.pending.push(update);

        if !self.scheduled {
            self.scheduled = true;
            // Schedule for next animation frame
            request_animation_frame(|| self.flush());
        }
    }

    fn flush(&mut self) {
        if self.pending.is_empty() { return; }

        // Deduplicate: keep only last update per node
        let mut by_node: HashMap<NodeId, GraphUpdate> = HashMap::new();
        for update in self.pending.drain(..) {
            by_node.insert(update.node_id, update);
        }

        // Apply batch
        apply_updates(by_node.into_values().collect());
        self.scheduled = false;
    }
}
```

---

## Part 7: Dioxus Integration Patterns

### 7.1 WebView Interop Architecture

ChoirOS already uses a bridge pattern for JS integration:

```rust
// Rust side: Define JS functions
#[wasm_bindgen(js_namespace = window)]
extern "C" {
    #[wasm_bindgen(js_name = createMindMap)]
    fn create_mind_map(container: web_sys::Element, data: &JsValue) -> u32;

    #[wasm_bindgen(js_name = updateMindMap)]
    fn update_mind_map(handle: u32, updates: &JsValue);

    #[wasm_bindgen(js_name = onMindMapEvent)]
    fn on_mind_map_event(handle: u32, cb: &Closure<dyn FnMut(String)>);

    #[wasm_bindgen(js_name = disposeMindMap)]
    fn dispose_mind_map(handle: u32);
}
```

```javascript
// JavaScript bridge: mind-map-bridge.js
(function() {
  const instances = new Map();
  let nextId = 1;

  window.createMindMap = function(container, data) {
    // Initialize D3.js visualization
    const svg = d3.select(container).append('svg');
    const root = d3.hierarchy(data);
    const tree = d3.tree().size([2 * Math.PI, 500]);

    const id = nextId++;
    instances.set(id, { svg, root, tree });

    render(id);
    return id;
  };

  window.updateMindMap = function(handle, updates) {
    const instance = instances.get(handle);
    if (!instance) return;

    // Apply updates to data
    applyUpdates(instance.root, updates);

    // Re-render with D3 join pattern
    render(handle);
  };

  window.onMindMapEvent = function(handle, callback) {
    const instance = instances.get(handle);
    if (!instance) return;

    instance.svg.on('click', (event, d) => {
      callback(JSON.stringify({
        type: 'nodeClick',
        nodeId: d.data.id
      }));
    });
  };

  function render(handle) {
    const { svg, root, tree } = instances.get(handle);
    tree(root);

    // D3 join pattern for efficient updates
    const nodes = svg.selectAll('.node')
      .data(root.descendants(), d => d.data.id);

    nodes.enter()
      .append('g')
      .attr('class', 'node')
      .merge(nodes)
      .attr('transform', d => `
        rotate(${d.x * 180 / Math.PI - 90})
        translate(${d.y}, 0)
      `);

    nodes.exit().remove();
  }
})();
```

### 7.2 State Synchronization Strategy

**Initial Load:**
```rust
#[component]
fn MindMapViewer(graph_id: String) -> Element {
    let container_id = use_signal(|| format!("mindmap-{}", uuid()));
    let runtime = use_signal(|| None::<MindMapRuntime>);

    // Mount: Initialize D3.js
    use_effect(move || {
        spawn(async move {
            // 1. Load JS bridge
            load_script("/mind-map-bridge.js").await;

            // 2. Fetch initial data
            let data = fetch_graph_data(&graph_id).await;

            // 3. Get container element
            let container = document::get_element_by_id(&container_id);

            // 4. Create D3.js instance
            let js_data = serde_wasm_bindgen::to_value(&data).unwrap();
            let handle = create_mind_map(container, &js_data);

            // 5. Set up event callback
            let on_event = Closure::wrap(Box::new(move |json: String| {
                let event: MindMapEvent = serde_json::from_str(&json).unwrap();
                handle_mind_map_event(event);
            }) as Box<dyn FnMut(String)>);
            on_mind_map_event(handle, &on_event);

            runtime.set(Some(MindMapRuntime {
                handle,
                _callback: on_event,
            }));
        });
    });

    // Sync: WebSocket updates
    use_effect(move || {
        let mut ws = connect_graph_ws(&graph_id);

        spawn(async move {
            while let Some(msg) = ws.next().await {
                if let Some(rt) = runtime.read().as_ref() {
                    let updates = parse_ws_update(msg);
                    let js_updates = serde_wasm_bindgen::to_value(&updates).unwrap();
                    update_mind_map(rt.handle, &js_updates);
                }
            }
        });
    });

    rsx! {
        div {
            id: "{container_id}",
            class: "mindmap-container",
            style: "width: 100%; height: 100%;"
        }
    }
}
```

### 7.3 Alternative: Custom Dioxus SVG

For simpler cases or maximum Rust control:

```rust
#[component]
fn MindMapSvg(nodes: Signal<Vec<Node>>, edges: Signal<Vec<Edge>>) -> Element {
    let viewport = use_signal(|| Viewport {
        x: 0.0, y: 0.0, zoom: 1.0
    });

    let visible_nodes = use_memo(move || {
        let vp = viewport.read();
        nodes.read()
            .iter()
            .filter(|n| vp.contains(n.position))
            .cloned()
            .collect::<Vec<_>>()
    });

    rsx! {
        svg {
            width: "100%",
            height: "100%",
            view_box: "0 0 1000 800",

            // Render edges first (behind nodes)
            for edge in edges.read().iter() {
                MindMapEdge { edge: edge.clone() }
            }

            // Render nodes
            for node in visible_nodes.read().iter() {
                MindMapNode {
                    key: "{node.id}",
                    node: node.clone(),
                    on_collapse: move |_| toggle_collapse(node.id),
                }
            }
        }
    }
}

#[component]
fn MindMapNode(node: Node, on_collapse: EventHandler<()>) -> Element {
    rsx! {
        g {
            transform: "translate({node.x}, {node.y})",

            // Checklist progress background
            rect {
                x: "-60",
                y: "-15",
                width: "120",
                height: "30",
                rx: "4",
                fill: "#e5e7eb",
            }

            // Progress fill
            rect {
                x: "-60",
                y: "-15",
                width: "{node.progress_percent * 1.2}",
                height: "30",
                rx: "4",
                fill: checklist_color(node.state),
            }

            // Title
            text {
                x: "0",
                y: "5",
                "text-anchor": "middle",
                "{node.title}"
            }

            // Collapse button (if has children)
            if !node.children.is_empty() {
                circle {
                    r: "8",
                    cx: "60",
                    cy: "0",
                    fill: "white",
                    stroke: "#6b7280",
                    onclick: move |e| {
                        e.stop_propagation();
                        on_collapse.call(());
                    },
                    "{if node.collapsed { '+' } else { '-' }}"
                }
            }
        }
    }
}
```

**Pros of pure Dioxus:**
- No JS bridge overhead
- Native Rust state management
- Type-safe event handling

**Cons:**
- D3.js algorithms must be ported to Rust (or use d3-wasm)
- Performance ceiling lower than optimized JS
- More development effort

---

## Part 8: Performance Benchmarks & Targets

### 8.1 Library Performance Comparison

| Library | 500 nodes | 1000 nodes | 2000 nodes | 5000 nodes | Technology |
|---------|-----------|------------|------------|------------|------------|
| **D3.js SVG** | 60 FPS | 45 FPS | 30 FPS | 15 FPS | SVG |
| **D3.js Canvas** | 60 FPS | 60 FPS | 45 FPS | 25 FPS | Canvas 2D |
| **Cytoscape.js** | 60 FPS | 60 FPS | 60 FPS | 45 FPS | Canvas/WebGL |
| **Sigma.js v3** | 60 FPS | 60 FPS | 60 FPS | 60 FPS | WebGL |
| **Mind Elixir** | 60 FPS | 45 FPS | 30 FPS | 15 FPS | DOM |
| **jsMind Canvas** | 60 FPS | 60 FPS | 45 FPS | 30 FPS | Canvas |

### 8.2 ChoirOS Performance Targets

| Metric | Target | Measurement |
|--------|--------|-------------|
| Initial render (1000 nodes) | < 2 seconds | From data to interactive |
| Pan/zoom | 60 FPS | No dropped frames |
| Node selection | < 50ms | Click to highlight |
| State transition | < 100ms | Click to visual update |
| Event propagation | < 500ms | Local change to all clients |
| Memory (1000 nodes) | < 100MB | Total JS + Rust heap |

### 8.3 Optimization Checklist

**Must implement:**
- [ ] Viewport culling (only render visible nodes)
- [ ] Spatial indexing (R-tree or quadtree) for hit testing
- [ ] Debounced updates (batch rapid changes)
- [ ] LOD system (simplify distant nodes)

**Should implement:**
- [ ] Offscreen canvas for static backgrounds
- [ ] Node pooling for reduced GC
- [ ] Virtual scrolling for outline view
- [ ] Image/asset lazy loading

**Consider for v2:**
- [ ] WebGL renderer (Sigma.js or custom)
- [ ] Web Workers for layout computation
- [ ] Incremental layout (only recompute affected region)

---

## Part 9: Implementation Recommendations

### 9.1 Final Architecture Decision

**Selected Stack:**
- **Rendering:** D3.js v7 with `d3-hierarchy` (SVG + Canvas hybrid)
- **Layout:** Custom radial tree with dependency overlay
- **State:** Flattened tree structure in Rust, projected to JS
- **Sync:** WebSocket streaming with optimistic updates
- **Integration:** Dioxus WebView with JS bridge

**Data Flow:**
```
┌─────────────┐     WebSocket      ┌─────────────┐
│  EventStore │ ◄────────────────► │  GraphActor │
│  (SQLite)   │                    │  (Rust)     │
└─────────────┘                    └──────┬──────┘
                                          │ eval()
                                          ▼
┌─────────────┐     Events       ┌─────────────┐
│  Dioxus UI  │ ◄────────────────│  D3.js      │
│  (Toolbar,  │                  │  (Radial    │
│   panels)   │                  │   mind map) │
└─────────────┘                  └─────────────┘
```

### 9.2 Why This Stack for ChoirOS

| Criterion | D3.js Decision |
|-----------|----------------|
| **Tree semantics** | `d3-hierarchy` purpose-built for trees |
| **Radial layout** | Native support, 10 lines of code |
| **Custom nodes** | Full SVG control for checklists |
| **Dependencies** | Secondary edges as overlay |
| **EventStore sync** | Direct data binding, no translation |
| **Performance** | Good enough for 2000 nodes |
| **Maintenance** | Industry standard, stable API |

### 9.3 Milestone Plan

**M0: D3.js Prototype (Week 1)**
- [ ] Set up D3.js in Dioxus WebView
- [ ] Render sample data (100 nodes) as radial tree
- [ ] Implement collapse/expand
- [ ] Basic pan/zoom

**Acceptance:** Screenshot of working radial mind map

**M1: ChoirOS Integration (Week 2-3)**
- [ ] GraphActor with event projection
- [ ] WebSocket streaming to frontend
- [ ] Two-way sync (Rust → D3 → Rust)
- [ ] Checklist state visualization

**Acceptance:** Change state in one client, see update in another within 500ms

**M2: Interactive Features (Week 4-5)**
- [ ] Drag to reparent
- [ ] Inline editing
- [ ] Keyboard navigation
- [ ] Dependency edges

**Acceptance:** Full keyboard workflow without mouse

**M3: Performance & Polish (Week 6-7)**
- [ ] Viewport culling
- [ ] LOD system
- [ ] 2000 node stress test
- [ ] Mobile touch support

**Acceptance:** 60 FPS with 2000 nodes

**M4: Advanced Features (Week 8+)**
- [ ] Drill-down panel
- [ ] Event timeline
- [ ] Filter/search
- [ ] Export formats

### 9.4 Risk Mitigation

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| D3.js learning curve | Medium | Team training, example code |
| Performance at 2000+ nodes | Low | Viewport culling from day 1 |
| State sync bugs | Medium | Property-based testing, CRDTs |
| Accessibility gaps | Medium | ARIA labels, keyboard nav first |
| Mobile performance | Medium | Simplified touch interactions |

### 9.5 Migration Path

If D3.js hits limits:

1. **Short term:** Switch to Canvas renderer (`d3-selection` → custom Canvas)
2. **Medium term:** Integrate Sigma.js v3 for WebGL rendering
3. **Long term:** Custom WGPU renderer in Rust (Dioxus Native)

---

## Appendix A: Data Model Reference

```rust
// Complete ChoirOS mind map data model

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeType {
    Root,
    Branch,
    Leaf,
    ChecklistItem,
    Milestone,
    Reference,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChecklistState {
    Planned,
    Active,
    Blocked,
    Done,
    Cancelled,
}

impl ChecklistState {
    pub fn color(&self) -> &'static str {
        match self {
            ChecklistState::Planned => "#6b7280",
            ChecklistState::Active => "#3b82f6",
            ChecklistState::Blocked => "#ef4444",
            ChecklistState::Done => "#22c55e",
            ChecklistState::Cancelled => "#9ca3af",
        }
    }

    pub fn valid_transitions(&self) -> Vec<ChecklistState> {
        match self {
            ChecklistState::Planned =>
                vec![ChecklistState::Active, ChecklistState::Cancelled],
            ChecklistState::Active =>
                vec![ChecklistState::Done, ChecklistState::Blocked, ChecklistState::Cancelled],
            ChecklistState::Blocked =>
                vec![ChecklistState::Active, ChecklistState::Cancelled],
            ChecklistState::Done =>
                vec![ChecklistState::Active],
            ChecklistState::Cancelled =>
                vec![ChecklistState::Planned],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub node_type: NodeType,
    pub title: String,
    pub description: Option<String>,
    pub checklist_state: Option<ChecklistState>,
    pub progress_percent: Option<u8>,
    pub parent_id: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub collapsed: bool,
    pub position: Position, // Relative or absolute based on layout
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EdgeType {
    ParentChild, // Implicit in tree structure
    Dependency,  // A blocks B
    Reference,   // Non-owning link
    Blocker,     // Explicit blocker
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: String,
    pub source_id: NodeId,
    pub target_id: NodeId,
    pub edge_type: EdgeType,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphState {
    pub nodes: HashMap<NodeId, Node>,
    pub edges: Vec<Edge>, // Cross-links only; tree in node.parent/children
    pub root_ids: Vec<NodeId>, // Support multiple roots
    pub viewport: Viewport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Viewport {
    pub center_x: f64,
    pub center_y: f64,
    pub zoom: f64,
}
```

---

## Appendix B: References

### Libraries
- D3.js: https://d3js.org/
- Cytoscape.js: https://js.cytoscape.org/
- Mind Elixir: https://github.com/ssshooter/mind-elixir-core
- markmap: https://markmap.js.org/
- Sigma.js: https://www.sigmajs.org/

### Research Papers
- Reingold-Tilford tree layout algorithm
- Target Netgrams: Radial layout for large graphs
- SIPA: Streaming graph layout algorithm
- Incremental FM³ layout method

### Tool Architectures
- Notion: Block-based hierarchy research
- Figma: WebAssembly rendering architecture
- Linear: Optimistic UI patterns
- Obsidian: Local-first graph visualization, multi-view switching
- Heptabase: Spatial hybrid whiteboards, card-centric transitions

### Word Cloud & Multi-View
- d3-cloud: https://github.com/jasondavies/d3-cloud
- wordcloud2.js: https://github.com/timdream/wordcloud2.js
- FLIP Animation: https://css-tricks.com/animating-layouts-with-the-flip-technique
- View Transitions API: https://developer.mozilla.org/en-US/docs/Web/API/View_Transitions_API
- Semantic Zoom: https://github.com/prathyvsh/semantic-zoom
- Focus+Context Survey: https://worrydream.com/refs/Cockburn_2007_-_A_Review_of_Overview+Detail,_Zooming,_and_Focus+Context_Interfaces.pdf

---

---

## Part 10: Word Cloud Visualization

### 10.1 Word Cloud Library Comparison

| Library | Bundle Size | Performance (500-2000 words) | Layout Algorithm | Interactivity | Best For |
|---------|-------------|------------------------------|------------------|---------------|----------|
| **d3-cloud** | 2.9 KB | Excellent | Force-directed spiral | Full D3 ecosystem | Custom visualizations, D3 apps |
| **wordcloud2.js** | 8-10 KB | Good for <1000 | Spiral + collision | Hover/click callbacks | Lightweight, Canvas rendering |
| **React-Wordcloud** | 45 KB | Good, async available | Spiral (d3-cloud) | Built-in tooltips, animations | React applications |
| **@isoterik/react-word-cloud** | Lightweight | Excellent | Force-based with animations | Advanced: gradients, hooks | Modern React 18+ apps |
| **araea-wordcloud (Rust)** | N/A | Excellent | Collision detection | SVG output | Pre-computed layouts |

### 10.2 Layout Algorithms

| Algorithm | Approach | Strengths | Weaknesses | Best For |
|-----------|----------|-----------|------------|----------|
| **Spiral** | Archimedean spiral from center | Fast, compact, aesthetic | No semantic preservation | General word clouds |
| **Force-Directed** | Physics simulation | Semantic coherence | Computationally expensive | Semantic word clouds |
| **Grid-Based** | Fixed row/column | Maximum stability | Rigid, wasted space | Comparative visualization |

**Spiral Algorithm (Primary Recommendation):**
```javascript
// Sort words by size (descending) first
// Place largest words at center
// Move along spiral path until no collision
// Collision detection: Bounding box intersection

function spiralLayout(words) {
  const placed = [];
  let angle = 0;
  let radius = 0;
  const spiralTightness = 0.5;

  for (const word of words.sort((a, b) => b.weight - a.weight)) {
    let placed = false;
    while (!placed) {
      radius = spiralTightness * angle;
      const x = center.x + radius * Math.cos(angle);
      const y = center.y + radius * Math.sin(angle);

      if (!collides(x, y, word, placed)) {
        word.x = x;
        word.y = y;
        placed.push(word);
        placed = true;
      }
      angle += 0.1;
    }
  }
}
```

### 10.3 Hierarchical Word Clouds

**Challenge:** Word clouds are flat; hierarchical data requires drill-down.

**Drill-Down Patterns:**

1. **Replace & Breadcrumb**
   - Click word → new cloud of children
   - Breadcrumb: Root > Level 1 > Level 2
   - Animation: FLIP transition from clicked word

2. **Zoom & Expand**
   - Click word → zoom into region
   - Child words appear around parent
   - Parent remains visible as anchor

3. **Radial Expansion**
   - Click word → children in radial pattern
   - Best for showing 1-2 levels simultaneously

**Implementation:**
```rust
struct HierarchicalWordCloud {
    words: HashMap<WordId, Word>,
    tree_edges: HashMap<WordId, Vec<WordId>>,
    current_root: Option<WordId>,
    breadcrumb: Vec<WordId>,
}

impl HierarchicalWordCloud {
    fn drill_down(&mut self, word_id: WordId) {
        self.breadcrumb.push(word_id);
        self.current_root = Some(word_id);
        self.relayout_with_animation(TransitionType::DrillDown);
    }

    fn drill_up(&mut self) {
        if self.breadcrumb.len() > 1 {
            self.breadcrumb.pop();
            self.current_root = self.breadcrumb.last().copied();
            self.relayout_with_animation(TransitionType::DrillUp);
        }
    }

    fn get_visible_words(&self) -> Vec<&Word> {
        let root = self.current_root.or(self.roots[0]);
        self.collect_descendants(root)
    }
}
```

### 10.4 Weight Computation

**Multi-Metric Scoring:**
```rust
struct WeightFactors {
    frequency: f64,      // Raw occurrence count
    recency: f64,        // 0.0 = old, 1.0 = recent
    importance: f64,     // User-defined or PageRank
    priority: f64,       // From metadata
}

fn compute_weight(factors: &WeightFactors, config: &WeightConfig) -> f64 {
    let time_decay = (-factors.recency / config.half_life).exp();

    config.frequency_weight * factors.frequency +
    config.recency_weight * factors.frequency * time_decay +
    config.importance_weight * factors.importance +
    config.priority_weight * factors.priority
}
```

**Scaling Strategies:**
- Linear: Simple, can have outliers dominate
- Logarithmic: Compresses range, good for skewed data
- SquareRoot: Middle ground
- Rank-based: Equal spacing by rank

### 10.5 Interactivity

```javascript
// D3-cloud with interactivity
cloud()
  .words(words)
  .on('end', draw)
  .start();

function draw(words) {
  d3.select('svg')
    .selectAll('text')
    .data(words)
    .enter()
    .append('text')
    .attr('transform', d => `translate(${d.x},${d.y})`)
    .style('font-size', d => `${d.size}px`)
    .text(d => d.text)
    .on('click', (event, d) => drillDown(d.id))
    .on('mouseover', (event, d) => highlightRelated(d))
    .on('mouseout', clearHighlight);
}
```

---

## Part 11: Multi-View Integration

### 11.1 View Architecture

**Three-Level Semantic Zoom:**

```
Zoom 0.5x-1.5x:   Word Cloud (density/importance overview)
         ↓ [threshold: 1.6x up, 1.4x down]
Zoom 1.5x-4x:     Mind Map (structural relationships)
         ↓ [threshold: 4.2x up, 3.8x down]
Zoom 4x+:         Detail Panel (full information)
```

**Hysteresis Implementation:**
```rust
struct ZoomThresholds {
    word_cloud_to_mind_map: Threshold,
    mind_map_to_detail: Threshold,
}

struct Threshold {
    up: f64,    // Zooming in crosses this
    down: f64,  // Zooming out crosses this
}

impl ZoomThresholds {
    fn new() -> Self {
        Self {
            word_cloud_to_mind_map: Threshold { up: 1.6, down: 1.4 },
            mind_map_to_detail: Threshold { up: 4.2, down: 3.8 },
        }
    }
}

fn determine_view(zoom: f64, current: ViewMode, thresholds: &ZoomThresholds) -> ViewMode {
    match current {
        ViewMode::WordCloud => {
            if zoom > thresholds.word_cloud_to_mind_map.up {
                ViewMode::MindMap
            } else {
                ViewMode::WordCloud
            }
        }
        ViewMode::MindMap => {
            if zoom < thresholds.word_cloud_to_mind_map.down {
                ViewMode::WordCloud
            } else if zoom > thresholds.mind_map_to_detail.up {
                ViewMode::Detail
            } else {
                ViewMode::MindMap
            }
        }
        ViewMode::Detail => {
            if zoom < thresholds.mind_map_to_detail.down {
                ViewMode::MindMap
            } else {
                ViewMode::Detail
            }
        }
    }
}
```

### 11.2 FLIP Animation Pattern

**First, Last, Invert, Play:**
```javascript
class FlipAnimator {
  animate(element, layoutChangeFn) {
    // F - First: Capture initial state
    const first = element.getBoundingClientRect();

    // Apply layout change
    layoutChangeFn();

    // L - Last: Capture final state
    const last = element.getBoundingClientRect();

    // I - Invert: Calculate difference
    const deltaX = first.left - last.left;
    const deltaY = first.top - last.top;

    // Apply inversion (no animation)
    element.style.transform = `translate(${deltaX}px, ${deltaY}px)`;

    // Force reflow
    element.offsetHeight;

    // P - Play: Animate to natural state
    element.style.transition = 'transform 300ms ease-out';
    element.style.transform = '';
  }
}
```

**View Transition API (Modern Browsers):**
```javascript
async function switchView(newViewData) {
  const transition = document.startViewTransition(() => {
    updateDOM(newViewData);
  });

  await transition.ready;

  // Customize animation
  document.documentElement.animate({
    clipPath: ['inset(0 0 100% 0)', 'inset(0 0 0 0)']
  }, {
    duration: 500,
    easing: 'ease-in-out',
    pseudoElement: '::view-transition-new(root)'
  });
}
```

### 11.3 Shared State Synchronization

**Selection Brushing:**
```rust
struct SelectionManager {
    selected: HashSet<NodeId>,
    highlighted: HashSet<NodeId>,
    focus: Option<NodeId>,
}

impl SelectionManager {
    fn select(&mut self, id: NodeId, source_view: ViewType) {
        self.selected.insert(id);

        // Notify all views except source
        for view in self.views.iter().filter(|v| v.type != source_view) {
            view.on_external_select(id);
        }
    }

    fn highlight(&mut self, ids: Vec<NodeId>, source_view: ViewType) {
        self.highlighted = ids.iter().cloned().collect();

        for view in self.views.iter().filter(|v| v.type != source_view) {
            view.on_external_highlight(&ids);
        }
    }
}
```

**Cross-View Coordinate Transformation:**
```rust
fn transform_coordinates(
    point: Vec2,
    from: ViewType,
    to: ViewType,
    viewport: &Viewport,
) -> Vec2 {
    match (from, to) {
        (ViewType::WordCloud, ViewType::MindMap) => {
            // Word cloud: spiral layout
            // Mind map: radial tree
            // Convert via normalized space
            let normalized = normalize_to_spiral(point, viewport);
            denormalize_from_radial(normalized, viewport)
        }
        (ViewType::MindMap, ViewType::WordCloud) => {
            let normalized = normalize_to_radial(point, viewport);
            denormalize_from_spiral(normalized, viewport)
        }
        _ => point,
    }
}
```

### 11.4 Morphing Between Views

**Word Cloud → Mind Map Transition:**
```javascript
async function morphToMindMap() {
  // 1. Capture word positions
  const wordPositions = captureWordPositions();

  // 2. Compute tree layout (hidden)
  const treeLayout = computeTreeLayout(data);

  // 3. Create mapping between words and nodes
  const mapping = createNodeMapping(wordPositions, treeLayout);

  // 4. FLIP animate each element
  for (const [word, node] of mapping) {
    const startRect = word.getBoundingClientRect();
    const endRect = calculateNodeRect(node);

    // Morph word into node
    await animateMorph(word, startRect, endRect, {
      duration: 500,
      easing: 'cubic-bezier(0.4, 0, 0.2, 1)'
    });
  }

  // 5. Fade in edges
  fadeInEdges(treeLayout.edges);
}
```

**Animation Timing:**
| Context | Duration | Rationale |
|---------|----------|-----------|
| Micro-zoom (button) | 100-150ms | Immediate feedback |
| Standard transition | 250-300ms | Balance speed/context |
| Large modal | 300-400ms | More distance to cover |
| Full-page | 400-500ms | Complex layout changes |

### 11.5 Focus+Context Techniques

**Fisheye Distortion:**
```javascript
function fisheyeLayout(nodes, focusPoint, distortion) {
  return nodes.map(node => {
    const distance = dist(node.position, focusPoint);
    const doe = degreeOfInterest(distance, distortion);

    return {
      ...node,
      scale: 1 + doe * 0.5,
      opacity: 0.5 + doe * 0.5,
    };
  });
}

function degreeOfInterest(distance, distortion) {
  return 1 / (1 + distance * distortion);
}
```

**Hyperbolic Trees:**
- Layout on hyperbolic plane mapped to circular display
- Can display ~1000 nodes in 600×600px
- Click to re-center with smooth animation

**Minimap Navigation:**
```rust
struct Minimap {
    overview: RenderedOverview,
    viewport_indicator: Rect,
    scale_factor: f64,
}

impl Minimap {
    fn render(&self, ui: &mut Ui) {
        // Show coarse-grained overview
        ui.image(self.overview.texture());

        // Draw viewport rectangle
        ui.painter().rect_stroke(
            self.viewport_indicator,
            0.0,
            Stroke::new(2.0, Color32::YELLOW),
        );
    }

    fn on_click(&self, pos: Pos2) -> ViewportCommand {
        let world_pos = self.minimap_to_world(pos);
        ViewportCommand::PanTo(world_pos)
    }
}
```

---

## Part 12: Integrated Architecture

### 12.1 Multi-View Data Flow

```
┌─────────────────────────────────────────────────────────────┐
│                    ChoirOS EventStore                       │
│         (append-only log of GraphEvents)                    │
└──────────────────────┬──────────────────────────────────────┘
                       │ WebSocket
                       ▼
┌─────────────────────────────────────────────────────────────┐
│                    GraphActor (Rust)                        │
│  - Projects events to GraphState                            │
│  - Maintains flattened tree + spatial index                 │
│  - Computes aggregate metrics                               │
└──────────────────────┬──────────────────────────────────────┘
                       │ eval() / JS interop
                       ▼
┌─────────────────────────────────────────────────────────────┐
│              Multi-View Visualization (D3.js)               │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
│  │  Word Cloud │  │  Mind Map   │  │    Detail Panel     │ │
│  │  (zoom 0.5- │  │  (zoom 1.5- │  │    (zoom 4x+)       │ │
│  │   1.5x)     │  │   4x)       │  │                     │ │
│  │             │  │             │  │                     │ │
│  │  • Density  │  │  • Tree     │  │  • Full directive   │ │
│  │  • Frequency│  │  • Edges    │  │  • Actions          │ │
│  │  • Overview │  │  • Focus    │  │  • History          │ │ │
│  └─────────────┘  └─────────────┘  └─────────────────────┘ │
│           │                │                     │          │
│           └────────────────┼─────────────────────┘          │
│                            │                                │
│                     Shared State:                            │
│                     • Selection                              │
│                     • Focus                                  │
│                     • Filter                                 │
│                     • Viewport                               │
└─────────────────────────────────────────────────────────────┘
```

### 12.2 State Management

```rust
struct VisualizationState {
    // Data layer
    graph: GraphState,

    // View layer
    current_view: ViewMode,
    zoom: f64,
    viewport: Viewport,

    // Interaction layer
    selection: SelectionManager,
    filters: FilterConfig,

    // Animation layer
    transition: Option<ViewTransition>,
}

enum ViewMode {
    WordCloud,
    MindMap,
    Detail,
}

struct ViewTransition {
    from: ViewMode,
    to: ViewMode,
    progress: f64, // 0.0 to 1.0
    elements: Vec<ElementTransform>,
}
```

### 12.3 Implementation Phases

**Phase 1: Mind Map Foundation**
- [ ] D3.js radial tree layout
- [ ] Checklist node rendering
- [ ] Collapse/expand interactions
- [ ] Basic pan/zoom

**Phase 2: Word Cloud View**
- [ ] d3-cloud integration
- [ ] Drill-down navigation
- [ ] Breadcrumb trail
- [ ] Weight computation

**Phase 3: View Transitions**
- [ ] FLIP animations
- [ ] Semantic zoom thresholds
- [ ] Shared element transitions
- [ ] Hysteresis handling

**Phase 4: Detail Panel**
- [ ] Node detail view
- [ ] Edit capabilities
- [ ] Action execution
- [ ] History timeline

**Phase 5: Polish**
- [ ] Minimap navigation
- [ ] Fisheye distortion
- [ ] Keyboard shortcuts
- [ ] Performance optimization

### 12.4 When to Use Each View

| Task | Recommended View | Why |
|------|------------------|-----|
| Initial exploration | Word Cloud | See density/importance at glance |
| Understanding structure | Mind Map | See relationships and hierarchy |
| Finding specific task | Word Cloud + Search | Fast filtering by keyword |
| Analyzing dependencies | Mind Map | Visual edge connections |
| Executing work | Detail Panel | Full context and actions |
| Progress review | Mind Map | Checklist states visible |
| Identifying hotspots | Word Cloud | Size = activity/frequency |

---

*Document generated for ChoirOS planning. Last updated: 2026-02-08*
