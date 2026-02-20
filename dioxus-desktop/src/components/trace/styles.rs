pub const TRACE_VIEW_STYLES: &str = r#"
.trace-header-actions {
    display: flex;
    align-items: center;
    gap: 0.45rem;
}

.trace-run-toggle {
    background: color-mix(in srgb, var(--bg-primary) 75%, var(--accent-bg) 25%);
    border: 1px solid color-mix(in srgb, var(--border-color) 50%, var(--accent-bg) 50%);
    color: var(--text-primary);
    border-radius: 0.45rem;
    padding: 0.32rem 0.55rem;
    font-size: 0.72rem;
    cursor: pointer;
}

.trace-run-toggle:hover {
    background: color-mix(in srgb, var(--bg-primary) 65%, var(--accent-bg) 35%);
}

.trace-main {
    display: flex;
    flex-direction: column;
    gap: 0.8rem;
}

.trace-graph-card {
    border: 1px solid var(--border-color, #334155);
    border-radius: 10px;
    background: color-mix(in srgb, var(--bg-secondary, #111827) 86%, #0b1225 14%);
    padding: 0.7rem;
}

.trace-graph-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 0.6rem;
    margin-bottom: 0.55rem;
}

.trace-graph-title {
    margin: 0;
    color: var(--text-primary, white);
    font-size: 1rem;
}

.trace-graph-objective {
    margin: 0.15rem 0 0 0;
    font-size: 0.78rem;
    color: var(--text-secondary, #9ca3af);
    line-height: 1.35;
}

.trace-graph-metrics {
    display: flex;
    gap: 0.35rem;
    flex-wrap: wrap;
    justify-content: flex-end;
}

.trace-pill {
    border: 1px solid var(--border-color, #334155);
    background: var(--bg-primary, #0f172a);
    color: var(--text-secondary, #cbd5e1);
    border-radius: 999px;
    font-size: 0.68rem;
    padding: 0.2rem 0.45rem;
    white-space: nowrap;
}

.trace-graph-scroll {
    overflow-x: auto;
    border-radius: 8px;
    border: 1px solid var(--border-color, #1f2a44);
    background: var(--bg-primary, #0b1222);
}

.trace-node-chip-row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    margin-top: 0.55rem;
}

.trace-node-chip {
    border: 1px solid var(--border-color);
    background: color-mix(in srgb, var(--bg-primary) 85%, var(--accent-bg) 15%);
    color: var(--text-secondary);
    border-radius: 0.45rem;
    padding: 0.3rem 0.5rem;
    font-size: 0.72rem;
    cursor: pointer;
}

.trace-node-chip:hover {
    border-color: color-mix(in srgb, var(--border-color) 60%, var(--accent-bg) 40%);
}

.trace-node-chip.active {
    border-color: var(--accent-bg);
    background: color-mix(in srgb, var(--bg-primary) 75%, var(--accent-bg) 25%);
    color: var(--text-primary);
}

.trace-node-panel {
    border: 1px solid var(--border-color, #334155);
    border-radius: 10px;
    background: color-mix(in srgb, var(--bg-secondary, #111827) 88%, #020617 12%);
    padding: 0.6rem;
}

.trace-node-panel-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    margin-bottom: 0.4rem;
}

.trace-node-title {
    margin: 0;
    color: var(--text-primary, #f8fafc);
    font-size: 0.95rem;
}

.trace-node-close {
    display: none;
    background: color-mix(in srgb, var(--bg-primary) 85%, var(--accent-bg) 15%);
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    border-radius: 0.4rem;
    font-size: 0.72rem;
    padding: 0.28rem 0.5rem;
    cursor: pointer;
}

.trace-loop-group {
    border: 1px solid var(--border-color, #1f2a44);
    border-radius: 8px;
    background: var(--bg-primary, #0b1222);
    margin-top: 0.55rem;
    padding: 0.45rem;
}

.trace-loop-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.4rem;
    margin-bottom: 0.35rem;
}

.trace-loop-title {
    margin: 0;
    color: color-mix(in srgb, var(--text-primary) 80%, var(--accent-bg) 20%);
    font-size: 0.78rem;
    font-weight: 600;
}

.trace-call-card {
    border: 1px solid var(--border-color, #1f2a44);
    border-radius: 8px;
    background: color-mix(in srgb, var(--bg-primary) 95%, black 5%);
    padding: 0.42rem;
    margin-top: 0.35rem;
}

.trace-call-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.45rem;
    margin-bottom: 0.35rem;
}

.trace-call-title {
    margin: 0;
    color: var(--text-primary);
    font-size: 0.78rem;
    font-weight: 600;
}

.trace-mobile-backdrop {
    display: none;
}

.trace-run-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.45rem;
    margin-top: 0.35rem;
}

.trace-run-row-left {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    flex-wrap: wrap;
}

.trace-run-status {
    border-radius: 999px;
    font-size: 0.62rem;
    padding: 0.15rem 0.42rem;
    border: 1px solid var(--border-color);
    background: var(--bg-primary);
    color: var(--text-secondary);
}

.trace-run-status--completed {
    border-color: #22c55e;
    color: #86efac;
    background: #052e16;
}

.trace-run-status--failed {
    border-color: #ef4444;
    color: #fca5a5;
    background: #450a0a;
}

.trace-run-status--in-progress {
    border-color: #eab308;
    color: #fde68a;
    background: #422006;
}

.trace-run-sparkline {
    width: 120px;
    height: 16px;
    min-width: 120px;
}

.trace-delegation-wrap {
    margin-top: 0.6rem;
}

.trace-delegation-timeline {
    display: flex;
    gap: 0.45rem;
    overflow-x: auto;
    padding-bottom: 0.2rem;
}

.trace-delegation-band {
    border: 1px solid var(--border-color);
    background: var(--bg-primary);
    color: var(--text-primary);
    border-radius: 8px;
    padding: 0.34rem 0.45rem;
    display: flex;
    align-items: center;
    gap: 0.4rem;
    min-width: 220px;
}

.trace-delegation-band--completed {
    border-color: #14532d;
    background: #052e16;
}

.trace-delegation-band--failed {
    border-color: #7f1d1d;
    background: #450a0a;
}

.trace-delegation-band--blocked {
    border-color: #854d0e;
    background: #422006;
}

.trace-delegation-band--inflight {
    border-color: #1e40af;
    background: #172554;
}

.trace-lifecycle-strip {
    display: flex;
    gap: 0.35rem;
    flex-wrap: wrap;
    margin-bottom: 0.45rem;
}

.trace-lifecycle-chip {
    border: 1px solid var(--border-color);
    border-radius: 7px;
    background: var(--bg-secondary);
    color: var(--text-primary);
    font-size: 0.68rem;
    padding: 0.18rem 0.35rem;
}

.trace-lifecycle-chip summary {
    cursor: pointer;
    list-style: none;
}

.trace-lifecycle-chip summary::-webkit-details-marker {
    display: none;
}

.trace-lifecycle-chip--started {
    border-color: #64748b;
    background: #1e293b;
}

.trace-lifecycle-chip--progress {
    border-color: #2563eb;
    background: #172554;
}

.trace-lifecycle-chip--completed {
    border-color: #16a34a;
    background: #052e16;
}

.trace-lifecycle-chip--failed {
    border-color: #dc2626;
    background: #450a0a;
}

.trace-lifecycle-chip--finding {
    border-color: #d97706;
    background: #422006;
}

.trace-lifecycle-chip--learning {
    border-color: #0f766e;
    background: #042f2e;
}

.trace-traj-grid {
    border: 1px solid var(--border-color);
    border-radius: 8px;
    background: var(--bg-primary);
    padding: 0.45rem;
    overflow: auto;
    margin-top: 0.6rem;
}

.trace-traj-grid-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.45rem;
    margin-bottom: 0.35rem;
}

.trace-traj-cell--completed {
    fill: #22c55e;
}

.trace-traj-cell--failed {
    fill: #ef4444;
}

.trace-traj-cell--inflight {
    fill: #f59e0b;
}

.trace-traj-cell--blocked {
    fill: #f97316;
}

.trace-traj-slow-ring {
    fill: none;
    stroke: #ef4444;
    stroke-width: 1.25;
}

.trace-duration-bar {
    height: 3px;
    border-radius: 2px;
    background: #22c55e;
    margin-top: 4px;
    transition: width 0.2s;
}

.trace-duration-bar--slow {
    background: #ef4444;
}

.trace-token-bar {
    display: flex;
    width: 100%;
    height: 5px;
    border-radius: 999px;
    overflow: hidden;
    margin-top: 0.35rem;
}

.trace-token-segment--cached {
    background: #6366f1;
}

.trace-token-segment--input {
    background: #3b82f6;
}

.trace-token-segment--output {
    background: #22c55e;
}

.trace-worker-node {
    filter: drop-shadow(0 0 6px rgba(56, 189, 248, 0.28));
}

.trace-call-card--selected {
    border-color: #60a5fa;
    box-shadow: 0 0 0 1px #60a5fa;
}

@media (max-width: 1024px) {
    .trace-node-panel {
        position: fixed;
        left: 0;
        right: 0;
        bottom: 0;
        max-height: 76vh;
        overflow: auto;
        border-radius: 12px 12px 0 0;
        border-bottom: none;
        z-index: 48;
        transform: translateY(105%);
        transition: transform 0.18s ease;
        margin: 0;
    }

    .trace-node-panel.open {
        transform: translateY(0);
    }

    .trace-node-close {
        display: inline-flex;
    }

    .trace-mobile-backdrop {
        display: block;
        position: fixed;
        inset: 0;
        background: rgba(2, 6, 23, 0.65);
        z-index: 45;
    }
}

:root[data-theme="light"] .trace-run-status--completed {
    border-color: #16a34a;
    color: #15803d;
    background: #dcfce7;
}

:root[data-theme="light"] .trace-run-status--failed {
    border-color: #dc2626;
    color: #b91c1c;
    background: #fee2e2;
}

:root[data-theme="light"] .trace-run-status--in-progress {
    border-color: #ca8a04;
    color: #92400e;
    background: #fef9c3;
}

:root[data-theme="light"] .trace-lifecycle-chip--started {
    border-color: #64748b;
    background: #f1f5f9;
}

:root[data-theme="light"] .trace-lifecycle-chip--progress {
    border-color: #2563eb;
    background: #eff6ff;
}

:root[data-theme="light"] .trace-lifecycle-chip--completed {
    border-color: #16a34a;
    background: #f0fdf4;
}

:root[data-theme="light"] .trace-lifecycle-chip--failed {
    border-color: #dc2626;
    background: #fef2f2;
}

:root[data-theme="light"] .trace-lifecycle-chip--finding {
    border-color: #d97706;
    background: #fffbeb;
}

:root[data-theme="light"] .trace-lifecycle-chip--learning {
    border-color: #0f766e;
    background: #f0fdfa;
}

:root[data-theme="light"] .trace-delegation-band--completed {
    border-color: #16a34a;
    background: #f0fdf4;
}

:root[data-theme="light"] .trace-delegation-band--failed {
    border-color: #dc2626;
    background: #fef2f2;
}

:root[data-theme="light"] .trace-delegation-band--blocked {
    border-color: #d97706;
    background: #fffbeb;
}

:root[data-theme="light"] .trace-delegation-band--inflight {
    border-color: #2563eb;
    background: #eff6ff;
}

/* ── Overview grid ── */
.trace-overview-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 0.75rem;
    padding: 0.75rem;
    overflow-y: auto;
    flex: 1;
}

.trace-run-card {
    border: 1px solid var(--border-color, #334155);
    border-radius: 10px;
    background: var(--bg-secondary, #1e293b);
    padding: 0.65rem 0.75rem;
    cursor: pointer;
    transition: border-color 0.15s, box-shadow 0.15s;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
}

.trace-run-card:hover {
    border-color: color-mix(in srgb, var(--border-color) 50%, var(--accent-bg) 50%);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.18);
}

.trace-run-card-title {
    font-size: 0.82rem;
    font-weight: 600;
    color: var(--text-primary, #f8fafc);
    line-height: 1.35;
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
}

.trace-run-card-meta {
    font-size: 0.7rem;
    color: var(--text-secondary, #94a3b8);
    display: flex;
    align-items: center;
    gap: 0.35rem;
    flex-wrap: wrap;
}

.trace-run-card-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.4rem;
    margin-top: 0.1rem;
}

.trace-run-card-time {
    font-size: 0.68rem;
    color: var(--text-muted, #64748b);
}

/* Back button in run detail header */
.trace-back-btn {
    background: transparent;
    border: 1px solid var(--border-color, #334155);
    color: var(--text-secondary, #94a3b8);
    border-radius: 0.4rem;
    padding: 0.25rem 0.55rem;
    font-size: 0.72rem;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 0.3rem;
}

.trace-back-btn:hover {
    border-color: color-mix(in srgb, var(--border-color) 60%, var(--accent-bg) 40%);
    color: var(--text-primary);
}
"#;
