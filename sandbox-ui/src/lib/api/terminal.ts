import apiClient from './client';

// Types matching backend structures
export interface Terminal {
  id: string;
  name: string;
  status: 'running' | 'stopped' | 'error';
  created_at: string;
  last_activity?: string;
}

export interface TerminalInfo {
  id: string;
  name: string;
  status: string;
  process_count?: number;
  current_directory?: string;
}

// Terminal API functions
export async function createTerminal(terminalId: string): Promise<Terminal> {
  return apiClient.get<Terminal>(`/api/terminals/${terminalId}`);
}

export async function getTerminalInfo(terminalId: string): Promise<TerminalInfo> {
  return apiClient.get<TerminalInfo>(`/api/terminals/${terminalId}/info`);
}

export async function stopTerminal(terminalId: string): Promise<void> {
  return apiClient.get<void>(`/api/terminals/${terminalId}/stop`);
}

// WebSocket URL helper
export function getTerminalWebSocketUrl(terminalId: string): string {
  const baseUrl = import.meta.env.VITE_API_URL || 'http://localhost:8080';
  const wsBaseUrl = baseUrl.replace(/^http/, 'ws');
  return `${wsBaseUrl}/ws/terminal/${terminalId}`;
}
