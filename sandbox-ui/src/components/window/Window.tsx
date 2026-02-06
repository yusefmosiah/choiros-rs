import { Suspense, lazy, useRef, type PointerEventHandler } from 'react';
import type { WindowState } from '@/types/generated';
import { Chat } from '@/components/apps/Chat/Chat';

const Terminal = lazy(() =>
  import('@/components/apps/Terminal/Terminal').then((module) => ({
    default: module.Terminal,
  })),
);

interface WindowProps {
  window: WindowState;
  isActive: boolean;
  onFocus: (windowId: string) => void;
  onClose: (windowId: string) => void;
  onMove: (windowId: string, x: number, y: number) => void;
  onResize: (windowId: string, width: number, height: number) => void;
  onMinimize: (windowId: string) => void;
  onMaximize: (windowId: string) => void;
  onRestore: (windowId: string) => void;
}

const MIN_WIDTH = 200;
const MIN_HEIGHT = 160;

export function Window({
  window: windowState,
  isActive,
  onFocus,
  onClose,
  onMove,
  onResize,
  onMinimize,
  onMaximize,
  onRestore,
}: WindowProps) {
  const dragPointerIdRef = useRef<number | null>(null);
  const dragStartRef = useRef<{ pointerX: number; pointerY: number; startX: number; startY: number } | null>(
    null,
  );

  const resizePointerIdRef = useRef<number | null>(null);
  const resizeStartRef = useRef<{
    pointerX: number;
    pointerY: number;
    startWidth: number;
    startHeight: number;
  } | null>(null);

  const onHeaderPointerDown: PointerEventHandler<HTMLDivElement> = (event) => {
    if (event.button !== 0) {
      return;
    }

    event.preventDefault();
    onFocus(windowState.id);

    dragPointerIdRef.current = event.pointerId;
    dragStartRef.current = {
      pointerX: event.clientX,
      pointerY: event.clientY,
      startX: windowState.x,
      startY: windowState.y,
    };

    const handlePointerMove = (moveEvent: PointerEvent) => {
      if (moveEvent.pointerId !== dragPointerIdRef.current || !dragStartRef.current) {
        return;
      }

      const dx = moveEvent.clientX - dragStartRef.current.pointerX;
      const dy = moveEvent.clientY - dragStartRef.current.pointerY;

      onMove(
        windowState.id,
        Math.round(dragStartRef.current.startX + dx),
        Math.round(dragStartRef.current.startY + dy),
      );
    };

    const handlePointerUp = (upEvent: PointerEvent) => {
      if (upEvent.pointerId !== dragPointerIdRef.current) {
        return;
      }

      dragPointerIdRef.current = null;
      dragStartRef.current = null;
      globalThis.window.removeEventListener('pointermove', handlePointerMove);
      globalThis.window.removeEventListener('pointerup', handlePointerUp);
      globalThis.window.removeEventListener('pointercancel', handlePointerUp);
    };

    globalThis.window.addEventListener('pointermove', handlePointerMove);
    globalThis.window.addEventListener('pointerup', handlePointerUp);
    globalThis.window.addEventListener('pointercancel', handlePointerUp);
  };

  const onResizeHandlePointerDown: PointerEventHandler<HTMLDivElement> = (event) => {
    if (event.button !== 0) {
      return;
    }

    event.preventDefault();
    event.stopPropagation();
    onFocus(windowState.id);

    resizePointerIdRef.current = event.pointerId;
    resizeStartRef.current = {
      pointerX: event.clientX,
      pointerY: event.clientY,
      startWidth: windowState.width,
      startHeight: windowState.height,
    };

    const handlePointerMove = (moveEvent: PointerEvent) => {
      if (moveEvent.pointerId !== resizePointerIdRef.current || !resizeStartRef.current) {
        return;
      }

      const nextWidth = Math.max(
        MIN_WIDTH,
        Math.round(resizeStartRef.current.startWidth + (moveEvent.clientX - resizeStartRef.current.pointerX)),
      );
      const nextHeight = Math.max(
        MIN_HEIGHT,
        Math.round(resizeStartRef.current.startHeight + (moveEvent.clientY - resizeStartRef.current.pointerY)),
      );

      onResize(windowState.id, nextWidth, nextHeight);
    };

    const handlePointerUp = (upEvent: PointerEvent) => {
      if (upEvent.pointerId !== resizePointerIdRef.current) {
        return;
      }

      resizePointerIdRef.current = null;
      resizeStartRef.current = null;
      globalThis.window.removeEventListener('pointermove', handlePointerMove);
      globalThis.window.removeEventListener('pointerup', handlePointerUp);
      globalThis.window.removeEventListener('pointercancel', handlePointerUp);
    };

    globalThis.window.addEventListener('pointermove', handlePointerMove);
    globalThis.window.addEventListener('pointerup', handlePointerUp);
    globalThis.window.addEventListener('pointercancel', handlePointerUp);
  };

  return (
    <section
      className={`window ${isActive ? 'window--active' : ''}`}
      style={{
        left: `${windowState.x}px`,
        top: `${windowState.y}px`,
        width: `${windowState.width}px`,
        height: `${windowState.height}px`,
        zIndex: windowState.z_index,
      }}
      onMouseDown={() => onFocus(windowState.id)}
    >
      <header className="window__header" onPointerDown={onHeaderPointerDown}>
        <span className="window__title">{windowState.title}</span>
        <div className="window__controls">
          <button type="button" onClick={() => onMinimize(windowState.id)} title="Minimize">
            _
          </button>
          <button
            type="button"
            onClick={() =>
              windowState.maximized ? onRestore(windowState.id) : onMaximize(windowState.id)
            }
            title={windowState.maximized ? 'Restore' : 'Maximize'}
          >
            {windowState.maximized ? '❐' : '□'}
          </button>
          <button type="button" onClick={() => onClose(windowState.id)} title="Close">
            x
          </button>
        </div>
      </header>
      <div className="window__body">
        <AppPlaceholder appId={windowState.app_id} windowId={windowState.id} />
      </div>
      <div className="window__resize-handle" onPointerDown={onResizeHandlePointerDown} />
    </section>
  );
}

function AppPlaceholder({ appId, windowId }: { appId: string; windowId: string }) {
  if (appId === 'chat') {
    return <Chat actorId={windowId} />;
  }

  if (appId === 'terminal') {
    return (
      <Suspense fallback={<div className="window__placeholder">Loading terminal...</div>}>
        <Terminal terminalId={windowId} />
      </Suspense>
    );
  }

  if (appId === 'writer') {
    return <div className="window__placeholder">Writer app migration in progress</div>;
  }

  if (appId === 'files') {
    return <div className="window__placeholder">Files app migration in progress</div>;
  }

  return <div className="window__placeholder">Unknown app: {appId}</div>;
}
