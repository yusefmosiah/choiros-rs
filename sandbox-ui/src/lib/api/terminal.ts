import apiClient from './client';
import { httpToWsUrl } from '@/lib/ws/client';

interface CreateTerminalResponse {
  terminal_id: string;
  status: string;
}

export type TerminalInfo = Record<string, unknown>;

export async function createTerminal(terminalId: string): Promise<CreateTerminalResponse> {
  return apiClient.get<CreateTerminalResponse>(`/api/terminals/${terminalId}`);
}

export async function getTerminalInfo(terminalId: string): Promise<TerminalInfo> {
  return apiClient.get<TerminalInfo>(`/api/terminals/${terminalId}/info`);
}

export async function stopTerminal(terminalId: string): Promise<void> {
  await apiClient.get<Record<string, unknown>>(`/api/terminals/${terminalId}/stop`);
}

export function getTerminalWebSocketUrl(terminalId: string, userId: string = 'user-1'): string {
  const explicitWsUrl = import.meta.env.VITE_WS_URL;
  if (explicitWsUrl) {
    const encoded = encodeURIComponent(userId);
    return `${explicitWsUrl}/terminal/${terminalId}?user_id=${encoded}`;
  }

  const baseUrl = import.meta.env.VITE_API_URL || 'http://localhost:8080';
  const wsBaseUrl = httpToWsUrl(baseUrl);
  const encoded = encodeURIComponent(userId);
  return `${wsBaseUrl}/ws/terminal/${terminalId}?user_id=${encoded}`;
}
