(() => {
  const terminals = new Map();
  let nextId = 1;

  window.createTerminal = function createTerminal(container) {
    const TerminalCtor = window.Terminal?.Terminal || window.Terminal;
    const FitAddonCtor = window.FitAddon?.FitAddon || window.FitAddon;
    if (!TerminalCtor || !FitAddonCtor) {
      throw new Error("xterm.js or fit addon not loaded");
    }

    const term = new TerminalCtor({
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

    const fitAddon = new FitAddonCtor();
    term.loadAddon(fitAddon);
    term.open(container);
    fitAddon.fit();

    const id = nextId++;
    terminals.set(id, {
      term,
      fitAddon,
      dataCb: null,
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
      entry.term.write(data);
    }
  };

  window.fitTerminal = function fitTerminal(id) {
    const entry = terminals.get(id);
    if (!entry) {
      return [0, 0];
    }
    entry.fitAddon.fit();
    return [entry.term.rows, entry.term.cols];
  };

  window.resizeTerminal = function resizeTerminal(id, rows, cols) {
    const entry = terminals.get(id);
    if (entry) {
      entry.term.resize(cols, rows);
    }
  };

  window.disposeTerminal = function disposeTerminal(id) {
    const entry = terminals.get(id);
    if (entry) {
      entry.term.dispose();
      terminals.delete(id);
    }
  };
})();
