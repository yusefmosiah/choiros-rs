(() => {
  const state =
    window.__choirosTerminalState ||
    (window.__choirosTerminalState = { terminals: new Map(), nextId: 1 });
  const terminals = state.terminals;
  const FALLBACK_ROWS = 24;
  const FALLBACK_COLS = 80;

  function allocateId() {
    const id = state.nextId;
    state.nextId += 1;
    return id;
  }

  function ensureMinimumGeometry(entry) {
    if (!entry || !entry.term) return;
    const rows = entry.term.rows || 0;
    const cols = entry.term.cols || 0;
    if (rows < 2 || cols < 2) {
      entry.term.resize(FALLBACK_COLS, FALLBACK_ROWS);
    }
  }

  function flushPendingWrites(entry) {
    if (!entry || !entry.term) return;
    ensureMinimumGeometry(entry);
    while (entry.pendingWrites.length > 0) {
      const chunk = entry.pendingWrites.shift();
      if (chunk) {
        entry.term.write(chunk);
      }
    }
  }

  window.createTerminal = function createTerminal(container) {
    const TerminalCtor = window.Terminal?.Terminal || window.Terminal;
    const FitAddonCtor = window.FitAddon?.FitAddon || window.FitAddon;
    if (!TerminalCtor || !FitAddonCtor || !container) {
      return 0;
    }

    let term;
    let fitAddon;
    try {
      term = new TerminalCtor({
        cursorBlink: true,
        fontFamily:
          "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', 'Courier New', monospace",
        fontSize: 13,
        theme: {
          background: "#0b1020",
          foreground: "#e2e8f0",
        },
        scrollback: 2000,
      });
      fitAddon = new FitAddonCtor();
    } catch (_err) {
      return 0;
    }

    const existingId = Number(container.getAttribute("data-choiros-term-id") || "0");
    if (existingId && terminals.has(existingId)) {
      try {
        terminals.get(existingId).term.dispose();
      } catch (_disposeErr) {}
      terminals.delete(existingId);
    }

    try {
      container.innerHTML = "";
    } catch (_err) {}

    const id = allocateId();
    const entry = {
      term,
      fitAddon,
      dataCb: null,
      pendingWrites: [],
      container,
    };
    terminals.set(id, entry);

    try {
      term.loadAddon(fitAddon);
      term.open(container);
      fitAddon.fit();
    } catch (_err) {
      terminals.delete(id);
      try {
        term.dispose();
      } catch (_disposeErr) {}
      return 0;
    }

    ensureMinimumGeometry(entry);
    term.focus();
    container.setAttribute("data-choiros-term-id", String(id));
    container.addEventListener("click", () => term.focus());

    requestAnimationFrame(() => {
      const liveEntry = terminals.get(id);
      if (!liveEntry) return;
      try {
        liveEntry.fitAddon.fit();
      } catch (_err) {}
      ensureMinimumGeometry(liveEntry);
      flushPendingWrites(liveEntry);
    });

    term.onData((data) => {
      const entry = terminals.get(id);
      if (entry && entry.dataCb) {
        entry.dataCb(data);
      }
    });

    return id;
  };

  window.onTerminalData = function onTerminalData(id, cb) {
    const entry = terminals.get(id);
    if (entry) {
      entry.dataCb = cb;
    }
  };

  window.writeTerminal = function writeTerminal(id, data) {
    const entry = terminals.get(id);
    if (entry) {
      const rows = entry.term.rows || 0;
      const cols = entry.term.cols || 0;
      if (rows < 2 || cols < 2) {
        entry.pendingWrites.push(data);
        while (entry.pendingWrites.length > 2048) {
          entry.pendingWrites.shift();
        }
        return;
      }
      entry.term.write(data);
    }
  };

  window.fitTerminal = function fitTerminal(id) {
    const entry = terminals.get(id);
    if (!entry) {
      return [0, 0];
    }
    try {
      entry.fitAddon.fit();
    } catch (_err) {}
    ensureMinimumGeometry(entry);
    flushPendingWrites(entry);
    return [entry.term.rows, entry.term.cols];
  };

  window.resizeTerminal = function resizeTerminal(id, rows, cols) {
    const entry = terminals.get(id);
    if (entry) {
      entry.term.resize(cols, rows);
      flushPendingWrites(entry);
    }
  };

  window.disposeTerminal = function disposeTerminal(id) {
    const entry = terminals.get(id);
    if (entry) {
      entry.term.dispose();
      if (entry.container) {
        const active = Number(entry.container.getAttribute("data-choiros-term-id") || "0");
        if (active === id) {
          entry.container.removeAttribute("data-choiros-term-id");
        }
      }
      terminals.delete(id);
    }
  };
})();
