/**
 * Dashboard Application
 * Main entry point - coordinates views and data
 */

const API_BASE = 'http://localhost:8765';
const REFRESH_INTERVAL = 5000;

let currentView = 'list';
let currentFilter = 'all';
let allFindings = [];
let allSessions = [];
let currentSessionId = null;

// View modules
const views = {
    list: null,
    network: null,
    timeline: null,
    hierarchy: null
};

// Initialize
async function init() {
    await loadData();
    setInterval(loadData, REFRESH_INTERVAL);
    
    // Listen for messages
    window.addEventListener('message', (event) => {
        if (event.data.type === 'findings-update') {
            loadData();
        }
    });
    
    // Keyboard shortcuts
    document.addEventListener('keydown', (e) => {
        if (e.key === 'Escape') closeModal();
        if (e.key === '1') switchView('list');
        if (e.key === '2') switchView('network');
        if (e.key === '3') switchView('timeline');
        if (e.key === '4') switchView('hierarchy');
    });
    
    // Modal click outside
    document.getElementById('modal').addEventListener('click', (e) => {
        if (e.target.id === 'modal') closeModal();
    });
}

// Load data from API
async function loadData() {
    try {
        const response = await fetch(`${API_BASE}/api/all`);
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        
        const data = await response.json();
        allFindings = data.findings || [];
        allSessions = data.sessions || [];
        
        updateStats(data.stats);
        renderCurrentView();
        renderFindings(allFindings);
    } catch (error) {
        console.error('Failed to load data:', error);
        showError('Failed to load data: ' + error.message);
    }
}

// Update stats
function updateStats(stats) {
    const active = allSessions.filter(s => 
        s.status === 'running' || s.status === 'spawned'
    ).length;
    const completed = allSessions.filter(s => 
        s.status === 'completed'
    ).length;
    
    document.getElementById('active-count').textContent = active;
    document.getElementById('completed-count').textContent = completed;
    document.getElementById('findings-count').textContent = stats.total || 0;
    document.getElementById('categories-count').textContent = 
        Object.keys(stats.byCategory || {}).length;
}

// Switch view
function switchView(viewName) {
    currentView = viewName;
    
    // Update buttons
    document.querySelectorAll('.view-btn').forEach(btn => {
        btn.classList.toggle('active', btn.dataset.view === viewName);
    });
    
    // Update sections
    document.querySelectorAll('.view-section').forEach(section => {
        section.classList.remove('active');
    });
    document.getElementById(`view-${viewName}`).classList.add('active');
    
    // Render view
    renderCurrentView();
}

// Render current view
function renderCurrentView() {
    switch (currentView) {
        case 'list':
            renderListView();
            break;
        case 'network':
            renderNetworkView();
            break;
        case 'timeline':
            renderTimelineView();
            break;
        case 'hierarchy':
            renderHierarchyView();
            break;
    }
}

// List View
function renderListView() {
    const container = document.getElementById('list-content');
    
    if (!allSessions || allSessions.length === 0) {
        container.innerHTML = `
            <div class="empty-state">
                <div class="empty-state-icon">📭</div>
                <p>No active research sessions</p>
            </div>
        `;
        return;
    }
    
    const sorted = [...allSessions].sort((a, b) => {
        const aActive = (a.status === 'running' || a.status === 'spawned') ? 0 : 1;
        const bActive = (b.status === 'running' || b.status === 'spawned') ? 0 : 1;
        if (aActive !== bActive) return aActive - bActive;
        return new Date(b.createdAt || 0) - new Date(a.createdAt || 0);
    });
    
    container.innerHTML = `
        <div class="session-list">
            ${sorted.map(session => `
                <div class="session-item">
                    <div class="session-status ${session.status}"></div>
                    <div class="session-info">
                        <h4>${escapeHtml(session.title || 'Untitled')}</h4>
                        <div class="session-meta">
                            ${session.sessionId?.slice(-8) || 'unknown'} • 
                            ${session.agent || 'no agent'} • 
                            ${session.tier || ''} • 
                            ${formatTime(session.lastEventAt || session.createdAt)}
                        </div>
                    </div>
                    <div class="session-controls">
                        <button class="view-log-btn" onclick="viewLog('${session.sessionId}')">Log</button>
                        <button class="view-summary-btn" onclick="viewSummary('${session.sessionId}')">Summary</button>
                    </div>
                </div>
            `).join('')}
        </div>
    `;
}

// Network View (D3.js force-directed graph)
function renderNetworkView() {
    const container = document.getElementById('network-content');
    container.innerHTML = '';
    
    if (!allSessions || allSessions.length === 0) {
        container.innerHTML = `
            <div class="empty-state">
                <div class="empty-state-icon">🕸️</div>
                <p>No sessions to visualize</p>
            </div>
        `;
        return;
    }
    
    const width = container.clientWidth;
    const height = 500;
    
    // Prepare data
    const nodes = allSessions.map(s => ({
        id: s.sessionId,
        title: s.title || 'Untitled',
        status: s.status,
        tier: s.tier,
        radius: s.status === 'running' ? 25 : 20
    }));
    
    const links = [];
    allSessions.forEach(session => {
        if (session.parentId) {
            links.push({
                source: session.parentId,
                target: session.sessionId
            });
        }
    });
    
    // Create SVG
    const svg = d3.select('#network-content')
        .append('svg')
        .attr('width', width)
        .attr('height', height);
    
    // Color scale
    const colorScale = {
        running: '#3fb950',
        completed: '#58a6ff',
        error: '#f85149',
        unknown: '#8b949e'
    };
    
    // Force simulation
    const simulation = d3.forceSimulation(nodes)
        .force('link', d3.forceLink(links).id(d => d.id).distance(100))
        .force('charge', d3.forceManyBody().strength(-300))
        .force('center', d3.forceCenter(width / 2, height / 2))
        .force('collision', d3.forceCollide().radius(d => d.radius + 10));
    
    // Draw links
    const link = svg.append('g')
        .selectAll('line')
        .data(links)
        .enter()
        .append('line')
        .attr('class', 'network-link');
    
    // Draw nodes
    const node = svg.append('g')
        .selectAll('g')
        .data(nodes)
        .enter()
        .append('g')
        .attr('class', 'network-node')
        .call(d3.drag()
            .on('start', dragstarted)
            .on('drag', dragged)
            .on('end', dragended));
    
    // Node circles
    node.append('circle')
        .attr('r', d => d.radius)
        .attr('fill', d => colorScale[d.status] || colorScale.unknown)
        .attr('stroke', '#30363d')
        .attr('stroke-width', 2);
    
    // Node labels
    node.append('text')
        .text(d => d.title.slice(0, 15))
        .attr('x', 0)
        .attr('y', d => d.radius + 15)
        .attr('text-anchor', 'middle')
        .style('font-size', '11px')
        .style('fill', '#c9d1d9');
    
    // Click handler
    node.on('click', (event, d) => {
        viewLog(d.id);
    });
    
    // Update positions
    simulation.on('tick', () => {
        link
            .attr('x1', d => d.source.x)
            .attr('y1', d => d.source.y)
            .attr('x2', d => d.target.x)
            .attr('y2', d => d.target.y);
        
        node.attr('transform', d => `translate(${d.x},${d.y})`);
    });
    
    function dragstarted(event, d) {
        if (!event.active) simulation.alphaTarget(0.3).restart();
        d.fx = d.x;
        d.fy = d.y;
    }
    
    function dragged(event, d) {
        d.fx = event.x;
        d.fy = event.y;
    }
    
    function dragended(event, d) {
        if (!event.active) simulation.alphaTarget(0);
        d.fx = null;
        d.fy = null;
    }
}

// Timeline View
function renderTimelineView() {
    const container = document.getElementById('timeline-content');
    
    if (!allSessions || allSessions.length === 0) {
        container.innerHTML = `
            <div class="empty-state">
                <div class="empty-state-icon">📊</div>
                <p>No sessions to display</p>
            </div>
        `;
        return;
    }
    
    const now = Date.now();
    const sorted = [...allSessions].sort((a, b) => 
        new Date(a.createdAt || 0) - new Date(b.createdAt || 0)
    );
    
    const startTime = new Date(sorted[0].createdAt || now).getTime();
    const endTime = now;
    const totalDuration = endTime - startTime;
    
    const height = 500;
    const rowHeight = 40;
    const padding = 60;
    
    let html = `
        <div class="timeline-container" style="min-width: ${Math.max(800, totalDuration / 1000)}px;">
            <svg width="100%" height="${height}">
    `;
    
    // Time axis
    const timeSteps = 10;
    for (let i = 0; i <= timeSteps; i++) {
        const x = padding + (i / timeSteps) * (100 - padding * 2 / totalDuration * 100) + '%';
        const time = new Date(startTime + (i / timeSteps) * totalDuration);
        html += `
            <line x1="${x}" y1="30" x2="${x}" y2="${height - 30}" 
                  stroke="#30363d" stroke-width="1" stroke-dasharray="4"/>
            <text x="${x}" y="20" text-anchor="middle" fill="#8b949e" font-size="11">
                ${time.toLocaleTimeString()}
            </text>
        `;
    }
    
    // Session bars
    const colorScale = {
        running: '#3fb950',
        completed: '#58a6ff',
        error: '#f85149',
        unknown: '#8b949e'
    };
    
    sorted.forEach((session, index) => {
        const created = new Date(session.createdAt || now).getTime();
        const lastEvent = new Date(session.lastEventAt || session.createdAt || now).getTime();
        const duration = lastEvent - created;
        
        const x = padding + ((created - startTime) / totalDuration) * (100 - padding * 2 / totalDuration * 100) + '%';
        const width = Math.max(2, (duration / totalDuration) * (100 - padding * 2 / totalDuration * 100)) + '%';
        const y = 50 + index * rowHeight;
        
        html += `
            <g class="timeline-bar" onclick="viewLog('${session.sessionId}')" style="cursor: pointer;">
                <rect x="${x}" y="${y}" width="${width}" height="25" 
                      fill="${colorScale[session.status] || colorScale.unknown}" 
                      rx="4" opacity="0.8"/>
                <text x="${x}" y="${y + 17}" dx="5" fill="#fff" font-size="10">
                    ${escapeHtml((session.title || 'Untitled').slice(0, 20))}
                </text>
            </g>
        `;
    });
    
    html += '</svg></div>';
    container.innerHTML = html;
}

// Hierarchy View
function renderHierarchyView() {
    const container = document.getElementById('hierarchy-content');
    
    if (!allSessions || allSessions.length === 0) {
        container.innerHTML = `
            <div class="empty-state">
                <div class="empty-state-icon">🌳</div>
                <p>No sessions to display</p>
            </div>
        `;
        return;
    }
    
    const tierIcons = {
        pico: '○',
        nano: '◐',
        micro: '◑',
        milli: '◒',
        unknown: '◎'
    };
    
    const statusIcons = {
        running: '▶',
        completed: '✓',
        error: '✗',
        unknown: '?'
    };
    
    // Build tree
    const sessionMap = new Map(allSessions.map(s => [s.sessionId, s]));
    const roots = allSessions.filter(s => !s.parentId);
    
    function renderNode(session, depth = 0) {
        const children = allSessions.filter(s => s.parentId === session.sessionId);
        const tierIcon = tierIcons[session.tier] || tierIcons.unknown;
        const statusIcon = statusIcons[session.status] || statusIcons.unknown;
        
        let html = `
            <div class="hierarchy-node" onclick="viewLog('${session.sessionId}')" 
                 style="padding-left: ${depth * 30}px;">
                <span class="tier-icon">${tierIcon}</span>
                <span style="color: ${session.status === 'running' ? '#3fb950' : '#58a6ff'}">
                    ${statusIcon}
                </span>
                <span>${escapeHtml(session.title || 'Untitled').slice(0, 40)}</span>
                <span style="color: #8b949e; font-size: 11px; margin-left: 10px;">
                    ${session.sessionId?.slice(-8)}
                </span>
            </div>
        `;
        
        if (children.length > 0) {
            html += `<div class="hierarchy-children">
                ${children.map(child => renderNode(child, depth + 1)).join('')}
            </div>`;
        }
        
        return html;
    }
    
    container.innerHTML = `
        <div class="hierarchy-tree">
            ${roots.map(root => renderNode(root)).join('')}
        </div>
    `;
}

// Findings
function renderFindings(findings) {
    const container = document.getElementById('findings-list');
    
    const filtered = currentFilter === 'all' 
        ? findings 
        : findings.filter(f => f.category === currentFilter);
    
    if (!filtered || filtered.length === 0) {
        container.innerHTML = `
            <div class="empty-state">
                <div class="empty-state-icon">🔍</div>
                <p>No findings yet</p>
            </div>
        `;
        return;
    }
    
    container.innerHTML = filtered.slice(0, 50).map(finding => `
        <div class="finding-item ${finding.category}">
            <div class="finding-header">
                <span class="finding-category ${finding.category}">${finding.category}</span>
                <span class="finding-time">${formatTime(finding.timestamp)}</span>
            </div>
            <div class="finding-description">${escapeHtml(finding.description)}</div>
        </div>
    `).join('');
}

function filterFindings(category) {
    currentFilter = category;
    document.querySelectorAll('.filter-btn').forEach(btn => {
        btn.classList.toggle('active', 
            btn.textContent.toLowerCase().includes(category.toLowerCase()) ||
            (category === 'all' && btn.textContent === 'All')
        );
    });
    renderFindings(allFindings);
}

// Modal functions
function viewLog(sessionId) {
    currentSessionId = sessionId;
    const session = allSessions.find(s => s.sessionId === sessionId);
    document.getElementById('modal-title').textContent = 
        `${session?.title || 'Untitled'} (${sessionId.slice(-8)})`;
    switchTab('log');
    loadSessionLog(sessionId);
    openModal();
}

function viewSummary(sessionId) {
    currentSessionId = sessionId;
    const session = allSessions.find(s => s.sessionId === sessionId);
    document.getElementById('modal-title').textContent = 
        `${session?.title || 'Untitled'} Summary (${sessionId.slice(-8)})`;
    switchTab('summary');
    loadSessionSummary(sessionId);
    openModal();
}

async function loadSessionLog(sessionId) {
    const container = document.getElementById('log-view');
    container.innerHTML = '<div class="loading">Loading logs...</div>';
    
    try {
        const response = await fetch(`${API_BASE}/api/messages?sessionId=${sessionId}`);
        const data = await response.json();
        renderLog(data.messages);
    } catch (error) {
        container.innerHTML = `<div class="error">Failed to load logs: ${error.message}</div>
        `;
    }
}

async function loadSessionSummary(sessionId) {
    const container = document.getElementById('summary-view');
    container.innerHTML = `
        <div class="streaming-indicator">
            <span class="pulse"></span> Generating summary...
        </div>
        <div class="summary-content-stream"></div>
    `;
    
    const contentDiv = container.querySelector('.summary-content-stream');
    let fullContent = '';
    
    try {
        const response = await fetch(`${API_BASE}/api/summary?sessionId=${sessionId}&stream=true`);
        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        
        while (true) {
            const { done, value } = await reader.read();
            if (done) break;
            
            const text = decoder.decode(value, { stream: true });
            const lines = text.split('\n');
            
            for (const line of lines) {
                if (line.startsWith('data: ')) {
                    try {
                        const data = JSON.parse(line.slice(6));
                        
                        if (data.error) {
                            contentDiv.innerHTML = `<div class="error">Error: ${data.error}</div>`;
                            return;
                        }
                        
                        if (data.done) {
                            container.querySelector('.streaming-indicator').style.display = 'none';
                            return;
                        }
                        
                        if (data.chunk) {
                            fullContent += data.chunk;
                            contentDiv.innerHTML = marked.parse(fullContent);
                            // NO AUTO-SCROLL - let user control the view
                        }
                    } catch (e) {
                        // Skip invalid JSON
                    }
                }
            }
        }
    } catch (error) {
        container.innerHTML = `<div class="error">Failed: ${error.message}</div>`;
    }
}

function renderLog(messages) {
    const container = document.getElementById('log-view');
    
    if (!messages || messages.length === 0) {
        container.innerHTML = '<div class="empty-state"><p>No messages</p></div>';
        return;
    }
    
    container.innerHTML = messages.map(msg => {
        const content = msg.parts
            ?.filter(p => p.type === 'text' && p.text)
            .map(p => p.text)
            .join('\n') || '[No text content]';
        
        return `
            <div class="message role-${msg.role || 'unknown'}">
                <div class="message-header">
                    <span>${(msg.role || 'unknown').toUpperCase()}</span>
                </div>
                <div class="message-content">${marked.parse(content)}</div>
            </div>
        `;
    }).join('');
}

function renderSummary(data) {
    const container = document.getElementById('summary-view');
    
    if (data.summary || data.markdown) {
        container.innerHTML = `<div class="summary-content">
            ${marked.parse(data.summary || data.markdown)}
        </div>`;
    } else {
        container.innerHTML = '<div class="empty-state"><p>No summary</p></div>';
    }
}

function openModal() {
    document.getElementById('modal').classList.add('active');
    document.body.style.overflow = 'hidden';
}

function closeModal() {
    document.getElementById('modal').classList.remove('active');
    document.body.style.overflow = '';
}

function switchTab(tab) {
    document.querySelectorAll('.tab-btn').forEach(btn => btn.classList.remove('active'));
    document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));
    document.getElementById(`tab-${tab}`).classList.add('active');
    document.getElementById(`tab-content-${tab}`).classList.add('active');
}

// Utilities
function escapeHtml(text) {
    if (!text) return '';
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

function formatTime(timestamp) {
    if (!timestamp) return 'unknown';
    const date = new Date(timestamp);
    const now = new Date();
    const diff = now - date;
    
    const minutes = Math.floor(diff / 60000);
    const hours = Math.floor(minutes / 60);
    const days = Math.floor(hours / 24);
    
    if (days > 0) return `${days}d ago`;
    if (hours > 0) return `${hours}h ago`;
    if (minutes > 0) return `${minutes}m ago`;
    return 'just now';
}

function showError(message) {
    const container = document.querySelector('.container');
    const error = document.createElement('div');
    error.className = 'error';
    error.textContent = message;
    container.insertBefore(error, container.children[1]);
    setTimeout(() => error.remove(), 5000);
}

// Start
init();
