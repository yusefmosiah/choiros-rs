import type { AppDefinition } from '@/types/generated';

export const CORE_APPS: AppDefinition[] = [
  {
    id: 'chat',
    name: 'Chat',
    icon: '💬',
    component_code: 'ChatApp',
    default_width: 600,
    default_height: 500,
  },
  {
    id: 'writer',
    name: 'Writer',
    icon: '📝',
    component_code: 'WriterApp',
    default_width: 800,
    default_height: 600,
  },
  {
    id: 'terminal',
    name: 'Terminal',
    icon: '🖥️',
    component_code: 'TerminalApp',
    default_width: 700,
    default_height: 450,
  },
  {
    id: 'files',
    name: 'Files',
    icon: '📁',
    component_code: 'FilesApp',
    default_width: 700,
    default_height: 500,
  },
];

export function getAppIcon(appId: string): string {
  return CORE_APPS.find((app) => app.id === appId)?.icon ?? '📱';
}
