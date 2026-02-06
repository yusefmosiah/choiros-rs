import { useState } from 'react';
import type { WindowState } from '@/types/generated';
import { getAppIcon } from '@/lib/apps';

interface PromptBarProps {
  connected: boolean;
  windows: WindowState[];
  activeWindowId: string | null;
  onSubmit: (text: string) => void;
  onFocusWindow: (windowId: string) => void;
}

export function PromptBar({ connected, windows, activeWindowId, onSubmit, onFocusWindow }: PromptBarProps) {
  const [value, setValue] = useState('');

  return (
    <div className="prompt-bar">
      <button className="prompt-help" type="button" aria-label="Help">
        ?
      </button>
      <input
        className="prompt-input"
        placeholder="Ask anything, paste URL, or type ? for commands..."
        value={value}
        onChange={(event) => setValue(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Enter' && value.trim().length > 0) {
            const text = value.trim();
            setValue('');
            onSubmit(text);
          }
        }}
      />
      {windows.length > 0 && (
        <div className="prompt-running-apps">
          {windows.map((window) => (
            <button
              key={window.id}
              type="button"
              className={`prompt-running-app ${activeWindowId === window.id ? 'prompt-running-app--active' : ''}`}
              onClick={() => onFocusWindow(window.id)}
              title={window.title}
            >
              {getAppIcon(window.app_id)}
            </button>
          ))}
        </div>
      )}
      <span className={`prompt-status ${connected ? 'prompt-status--connected' : ''}`}>
        {connected ? 'Connected' : 'Connecting...'}
      </span>
    </div>
  );
}
