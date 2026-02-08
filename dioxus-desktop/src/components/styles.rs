pub const CHAT_STYLES: &str = r#"
/* Chat Container */
.chat-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--chat-bg, #0f172a);
    overflow: hidden;
}

/* Header */
.chat-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.75rem 1rem;
    background: var(--chat-header-bg, #1e293b);
    border-bottom: 1px solid var(--border-color, #334155);
    flex-shrink: 0;
}

.chat-title {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-weight: 600;
    color: var(--text-primary, #f8fafc);
}

.chat-icon {
    font-size: 1.25rem;
}

.chat-status {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    font-size: 0.75rem;
    color: var(--text-secondary, #94a3b8);
}

.status-dot {
    color: var(--success-bg, #10b981);
    font-size: 0.5rem;
}

/* Body Layout */
.chat-body {
    flex: 1;
    min-height: 0;
    display: flex;
    overflow: hidden;
}

.thread-sidebar {
    width: 260px;
    background: #0b1222;
    border-right: 1px solid var(--border-color, #334155);
    display: flex;
    flex-direction: column;
    min-width: 220px;
    max-width: 320px;
    position: relative;
}

.thread-sidebar.collapsed {
    width: 0;
    min-width: 0;
    max-width: 0;
    border-right: none;
    overflow: visible;
}

.thread-sidebar-toggle {
    background: transparent;
    border: none;
    color: var(--text-secondary, #94a3b8);
    cursor: pointer;
    font-size: 0.85rem;
    padding: 0.5rem 0.4rem;
    align-self: flex-end;
}

.thread-sidebar.collapsed .thread-sidebar-toggle {
    position: absolute;
    top: 0.5rem;
    left: 0.35rem;
    z-index: 30;
    border-radius: 999px;
    background: color-mix(in srgb, #0b1222 82%, transparent);
    border: 1px solid #1f2a44;
    padding: 0.35rem 0.45rem;
}

.thread-sidebar-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    color: var(--text-primary, #f8fafc);
    font-size: 0.8rem;
    font-weight: 600;
    padding: 0.25rem 0.6rem 0.5rem 0.6rem;
}

.thread-new-button {
    background: #1e293b;
    border: 1px solid #334155;
    color: #cbd5e1;
    font-size: 0.72rem;
    border-radius: 0.35rem;
    padding: 0.15rem 0.35rem;
    cursor: pointer;
}

.thread-list {
    overflow: auto;
    padding: 0 0.4rem 0.5rem 0.4rem;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
}

.thread-item {
    width: 100%;
    text-align: left;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 0.4rem;
    color: var(--text-secondary, #94a3b8);
    cursor: pointer;
    padding: 0.4rem 0.45rem;
}

.thread-item:hover {
    background: #111b32;
    border-color: #23395d;
}

.thread-item.active {
    background: #13213d;
    border-color: #2f4f7a;
    color: #dbeafe;
}

.thread-title {
    font-size: 0.78rem;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
}

.thread-preview {
    font-size: 0.72rem;
    margin-top: 0.2rem;
    color: #94a3b8;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
}

/* Messages Scroll Area */
.messages-scroll-area {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 1rem;
    scroll-behavior: smooth;
}

.messages-scroll-area::-webkit-scrollbar {
    width: 6px;
}

.messages-scroll-area::-webkit-scrollbar-track {
    background: transparent;
}

.messages-scroll-area::-webkit-scrollbar-thumb {
    background: var(--border-color, #334155);
    border-radius: 3px;
}

.messages-list {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    max-width: 100%;
}

/* Empty State */
.empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 3rem 1rem;
    color: var(--text-muted, #64748b);
    text-align: center;
}

.empty-icon {
    font-size: 3rem;
    margin-bottom: 1rem;
    opacity: 0.5;
}

.empty-state p {
    font-weight: 500;
    color: var(--text-secondary, #94a3b8);
    margin: 0 0 0.25rem 0;
}

.empty-state span {
    font-size: 0.875rem;
}

/* Message Row */
.message-row {
    display: flex;
    gap: 0.75rem;
    max-width: 100%;
}

.user-row {
    flex-direction: row-reverse;
}

.assistant-row {
    flex-direction: row;
}

.system-row {
    flex-direction: row;
}

/* Avatar */
.avatar {
    width: 2rem;
    height: 2rem;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 0.75rem;
    font-weight: 600;
    flex-shrink: 0;
}

.user-avatar {
    background: var(--accent-bg, #3b82f6);
    color: white;
}

.assistant-avatar {
    background: var(--bg-secondary, #1e293b);
    color: var(--text-secondary, #94a3b8);
    border: 1px solid var(--border-color, #334155);
}

.system-avatar {
    background: #115e59;
    color: #f0fdfa;
}

/* Message Content */
.message-content {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    max-width: calc(100% - 3rem);
}

.user-row .message-content {
    align-items: flex-end;
}

.assistant-row .message-content {
    align-items: flex-start;
}

.system-row .message-content {
    align-items: flex-start;
}

/* Message Header */
.message-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.75rem;
}

.user-row .message-header {
    flex-direction: row-reverse;
}

.sender-name {
    font-weight: 500;
    color: var(--text-secondary, #94a3b8);
}

.message-time {
    color: var(--text-muted, #64748b);
}

.pending-badge {
    color: var(--warning-bg, #f59e0b);
    font-style: italic;
}

/* Message Bubble */
.message-bubble {
    padding: 0.75rem 1rem;
    border-radius: 1rem;
    font-size: 0.9375rem;
    line-height: 1.5;
    word-wrap: break-word;
    max-width: 100%;
}

.user-bubble {
    background: var(--accent-bg, #3b82f6);
    color: white;
    border-bottom-right-radius: 0.25rem;
}

.assistant-bubble {
    background: var(--bg-secondary, #1e293b);
    color: var(--text-primary, #f8fafc);
    border: 1px solid var(--border-color, #334155);
    border-bottom-left-radius: 0.25rem;
}

.system-bubble {
    background: #111827;
    color: #e5e7eb;
    border: 1px solid #374151;
    border-bottom-left-radius: 0.25rem;
}

.tool-details {
    width: 100%;
    background: #111827;
    border: 1px solid #374151;
    border-radius: 0.75rem;
    padding: 0.5rem 0.75rem;
}

.tool-summary {
    cursor: pointer;
    color: #93c5fd;
    font-weight: 600;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
}

.tool-activity-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
}

.tool-activity-toggle {
    background: transparent;
    color: #93c5fd;
    border: none;
    font-weight: 600;
    font-size: 1rem;
    cursor: pointer;
    padding: 0;
}

.tool-action-button {
    background: #1f2937;
    color: #cbd5e1;
    border: 1px solid #475569;
    border-radius: 0.4rem;
    font-size: 0.75rem;
    padding: 0.1rem 0.45rem;
    cursor: pointer;
}

.tool-body {
    margin-top: 0.5rem;
}

.tool-label {
    margin: 0.25rem 0;
    color: #cbd5e1;
    font-size: 0.8rem;
    font-weight: 600;
}

.tool-meta {
    margin: 0 0 0.5rem 0;
    color: #cbd5e1;
    font-size: 0.8rem;
}

.tool-pre {
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
    background: #030712;
    border: 1px solid #374151;
    border-radius: 0.5rem;
    padding: 0.5rem;
    color: #e2e8f0;
    font-size: 0.78rem;
    max-height: 260px;
    overflow: auto;
}

/* Typing Indicator */
.typing-indicator {
    display: flex;
    gap: 0.25rem;
    padding: 1rem;
    background: var(--bg-secondary, #1e293b);
    border: 1px solid var(--border-color, #334155);
    border-radius: 1rem;
    border-bottom-left-radius: 0.25rem;
    width: fit-content;
}

.typing-indicator span {
    width: 0.5rem;
    height: 0.5rem;
    background: var(--text-muted, #64748b);
    border-radius: 50%;
    animation: typing-bounce 1.4s infinite ease-in-out both;
}

.typing-indicator span:nth-child(1) { animation-delay: -0.32s; }
.typing-indicator span:nth-child(2) { animation-delay: -0.16s; }

@keyframes typing-bounce {
    0%, 80%, 100% { transform: scale(0); }
    40% { transform: scale(1); }
}

/* Chat Input Area */
.chat-input-area {
    padding: 0.75rem 1rem;
    background: var(--chat-header-bg, #1e293b);
    border-top: 1px solid var(--border-color, #334155);
    flex-shrink: 0;
}

.input-wrapper {
    display: flex;
    gap: 0.5rem;
    align-items: flex-end;
}

.chat-textarea {
    flex: 1;
    padding: 0.75rem 1rem;
    background: var(--input-bg, #0f172a);
    color: var(--text-primary, #f8fafc);
    border: 1px solid var(--border-color, #334155);
    border-radius: 1.5rem;
    font-size: 0.9375rem;
    font-family: inherit;
    resize: none;
    outline: none;
    min-height: 2.75rem;
    max-height: 8rem;
    line-height: 1.25;
    transition: border-color 0.2s, box-shadow 0.2s;
}

.chat-textarea:focus {
    border-color: var(--accent-bg, #3b82f6);
    box-shadow: 0 0 0 2px rgba(59, 130, 246, 0.2);
}

.chat-textarea::placeholder {
    color: var(--text-muted, #64748b);
}

.send-button {
    width: 2.75rem;
    height: 2.75rem;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--accent-bg, #3b82f6);
    color: white;
    border: none;
    border-radius: 50%;
    cursor: pointer;
    font-size: 1.25rem;
    transition: all 0.2s;
    flex-shrink: 0;
}

.send-button:hover:not(:disabled) {
    background: var(--accent-bg-hover, #2563eb);
    transform: scale(1.05);
}

.send-button:disabled {
    background: var(--border-color, #334155);
    color: var(--text-muted, #64748b);
    cursor: not-allowed;
}

.send-button .spinner {
    animation: spin 1s linear infinite;
}

@keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
}

.input-hint {
    margin-top: 0.5rem;
    font-size: 0.75rem;
    color: var(--text-muted, #64748b);
    text-align: center;
}

@media (max-width: 1024px) {
    .chat-body {
        position: relative;
    }

    .thread-sidebar {
        position: absolute;
        top: 0;
        left: 0;
        bottom: 0;
        width: 100%;
        min-width: 100%;
        max-width: none;
        z-index: 35;
        border-right: none;
        border-left: none;
        box-shadow: 0 14px 34px rgba(2, 6, 23, 0.55);
    }

    .thread-sidebar.collapsed {
        width: 0;
        min-width: 0;
        max-width: 0;
        box-shadow: none;
    }

    .thread-sidebar.collapsed .thread-sidebar-toggle {
        left: 0.5rem;
        top: 0.5rem;
    }
}
"#;
