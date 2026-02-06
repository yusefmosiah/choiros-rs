import type { WindowState } from '@/types/generated';
import { getAppIcon } from '@/lib/apps';

interface TaskbarProps {
  windows: WindowState[];
  activeWindowId: string | null;
  onFocusWindow: (windowId: string) => void;
}

export function Taskbar({ windows, activeWindowId, onFocusWindow }: TaskbarProps) {
  if (windows.length === 0) {
    return <div className="taskbar taskbar--empty">No open windows</div>;
  }

  return (
    <div className="taskbar">
      {windows.map((window) => (
        <button
          key={window.id}
          type="button"
          className={`taskbar-item ${activeWindowId === window.id ? 'taskbar-item--active' : ''}`}
          onClick={() => onFocusWindow(window.id)}
          title={window.title}
        >
          <span>{getAppIcon(window.app_id)}</span>
          <span>{window.title}</span>
        </button>
      ))}
    </div>
  );
}
