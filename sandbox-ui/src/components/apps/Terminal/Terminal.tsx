import { useEffect, useRef, useState } from 'react';
import { Terminal as XTerm } from 'xterm';
import { FitAddon } from 'xterm-addon-fit';
import { createTerminal, getTerminalInfo, getTerminalWebSocketUrl, stopTerminal } from '@/lib/api/terminal';
import { parseTerminalWsMessage, reconnectDelayMs } from './ws';
import 'xterm/css/xterm.css';
import './Terminal.css';

interface TerminalProps {
  terminalId: string;
  userId?: string;
}

export function Terminal({ terminalId, userId = 'user-1' }: TerminalProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const reconnectAttemptsRef = useRef(0);
  const [status, setStatus] = useState('Connecting...');
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }

    let cancelled = false;
    let shouldReconnect = true;

    const term = new XTerm({
      cursorBlink: true,
      convertEol: true,
      fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
      fontSize: 13,
      theme: {
        background: '#020617',
        foreground: '#e2e8f0',
      },
    });
    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(container);
    fitAddon.fit();

    const sendResize = () => {
      const ws = wsRef.current;
      if (!ws || ws.readyState !== WebSocket.OPEN) {
        return;
      }

      fitAddon.fit();
      ws.send(
        JSON.stringify({
          type: 'resize',
          rows: term.rows,
          cols: term.cols,
        }),
      );
    };

    const clearReconnectTimer = () => {
      if (reconnectTimerRef.current) {
        clearTimeout(reconnectTimerRef.current);
        reconnectTimerRef.current = null;
      }
    };

    const scheduleReconnect = () => {
      if (cancelled || !shouldReconnect) {
        return;
      }

      clearReconnectTimer();

      const delay = reconnectDelayMs(reconnectAttemptsRef.current);
      reconnectAttemptsRef.current += 1;
      setStatus(`Reconnecting in ${Math.round(delay / 1000)}s...`);

      reconnectTimerRef.current = setTimeout(() => {
        if (!cancelled && shouldReconnect) {
          void connect();
        }
      }, delay);
    };

    const ensureTerminalSession = async (): Promise<boolean> => {
      try {
        // First, try to get terminal info to check if session exists and is valid
        const info = await getTerminalInfo(terminalId);

        // If terminal exists but is not running, stop it to clean up and create fresh
        if (info && typeof info === 'object' && 'is_running' in info && !info.is_running) {
          console.log(`Terminal ${terminalId} exists but is not running, restarting...`);
          try {
            await stopTerminal(terminalId);
          } catch {
            // Ignore stop errors - terminal might already be stopped
          }
        }
      } catch {
        // Terminal doesn't exist or error getting info - that's fine, we'll create it
        console.log(`Terminal ${terminalId} not found or error getting info, creating new session...`);
      }

      // Now create/get the terminal session
      try {
        await createTerminal(terminalId);
        return true;
      } catch (err) {
        console.error('Failed to create terminal:', err);
        return false;
      }
    };

    const connect = async () => {
      try {
        setStatus('Connecting...');
        setError(null);

        // Ensure we have a valid terminal session
        const sessionReady = await ensureTerminalSession();
        if (!sessionReady) {
          throw new Error('Failed to initialize terminal session');
        }

        if (cancelled) {
          return;
        }

        const ws = new WebSocket(getTerminalWebSocketUrl(terminalId, userId));
        wsRef.current = ws;

        ws.onopen = () => {
          if (cancelled) {
            return;
          }

          reconnectAttemptsRef.current = 0;
          setStatus('Connected');
          setError(null);
          sendResize();
        };

        ws.onmessage = (event) => {
          if (typeof event.data !== 'string') {
            return;
          }

          const message = parseTerminalWsMessage(event.data);
          if (!message) {
            return;
          }

          if (message.type === 'output') {
            term.write(message.data);
            return;
          }

          if (message.type === 'info') {
            setStatus(message.is_running ? 'Connected' : 'Stopped');
            return;
          }

          if (message.type === 'error') {
            setError(message.message);
            // If we get a terminal error, schedule a reconnect with a fresh session
            if (message.message.includes('not found') || message.message.includes('not running')) {
              ws.close();
            }
          }
        };

        ws.onerror = () => {
          if (!cancelled) {
            setError('Terminal connection error');
          }
        };

        ws.onclose = () => {
          if (!cancelled && shouldReconnect) {
            setStatus('Disconnected');
            scheduleReconnect();
          }
        };
      } catch (err) {
        if (!cancelled) {
          setStatus('Failed');
          setError(err instanceof Error ? err.message : 'Failed to initialize terminal');
          scheduleReconnect();
        }
      }
    };

    const resizeObserver = new ResizeObserver(() => {
      sendResize();
    });
    resizeObserver.observe(container);

    const onDataDisposable = term.onData((data) => {
      const ws = wsRef.current;
      if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ type: 'input', data }));
      }
    });

    void connect();

    return () => {
      cancelled = true;
      shouldReconnect = false;
      clearReconnectTimer();
      resizeObserver.disconnect();
      onDataDisposable.dispose();

      if (wsRef.current) {
        wsRef.current.close();
        wsRef.current = null;
      }

      term.dispose();
      void stopTerminal(terminalId).catch(() => {
        // Best-effort cleanup for backend process.
      });
    };
  }, [terminalId, userId]);

  return (
    <div className="terminal-app">
      <div className="terminal-app__status">{status}</div>
      {error && <div className="terminal-app__error">{error}</div>}
      <div ref={containerRef} className="terminal-app__container" />
    </div>
  );
}
