import type { WindowState } from '@/types/generated';
import { Window } from './Window';

interface WindowManagerProps {
  windows: WindowState[];
  activeWindowId: string | null;
  onClose: (windowId: string) => void;
  onFocus: (windowId: string) => void;
  onMove: (windowId: string, x: number, y: number) => void;
  onResize: (windowId: string, width: number, height: number) => void;
  onMinimize: (windowId: string) => void;
  onMaximize: (windowId: string) => void;
  onRestore: (windowId: string) => void;
}

export function WindowManager({
  windows,
  activeWindowId,
  onClose,
  onFocus,
  onMove,
  onResize,
  onMinimize,
  onMaximize,
  onRestore,
}: WindowManagerProps) {
  const visibleWindows = windows
    .filter((window) => !window.minimized)
    .sort((a, b) => a.z_index - b.z_index);

  return (
    <div className="window-canvas">
      {visibleWindows.map((window) => (
        <Window
          key={window.id}
          window={window}
          isActive={activeWindowId === window.id}
          onClose={onClose}
          onFocus={onFocus}
          onMove={onMove}
          onResize={onResize}
          onMinimize={onMinimize}
          onMaximize={onMaximize}
          onRestore={onRestore}
        />
      ))}
    </div>
  );
}
