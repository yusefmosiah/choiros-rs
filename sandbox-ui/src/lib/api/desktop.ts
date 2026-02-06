import apiClient from './client';
import type { AppDefinition, DesktopState, WindowState } from '@/types/generated';

interface ApiEnvelope {
  success: boolean;
  error?: string;
  message?: string;
}

interface DesktopEnvelope extends ApiEnvelope {
  desktop: DesktopState;
}

interface WindowsEnvelope extends ApiEnvelope {
  windows: WindowState[];
}

interface OpenWindowEnvelope extends ApiEnvelope {
  window?: WindowState;
}

interface AppsEnvelope extends ApiEnvelope {
  apps: AppDefinition[];
}

interface MaximizeWindowResponse {
  success: boolean;
  window: WindowState;
  from: 'maximized' | 'restored';
  message: string;
}

interface RestoreWindowResponse {
  success: boolean;
  window: WindowState;
  from: 'maximized' | 'minimized' | 'normal';
  message: string;
}

export interface OpenWindowRequest {
  app_id: string;
  title: string;
  props?: unknown;
}

function assertSuccess<T extends ApiEnvelope>(response: T): T {
  if (!response.success) {
    throw new Error(response.error ?? 'Desktop API request failed');
  }

  return response;
}

export async function getDesktopState(desktopId: string): Promise<DesktopState> {
  const response = await apiClient.get<DesktopEnvelope>(`/desktop/${desktopId}`);
  return assertSuccess(response).desktop;
}

export async function getWindows(desktopId: string): Promise<WindowState[]> {
  const response = await apiClient.get<WindowsEnvelope>(`/desktop/${desktopId}/windows`);
  return assertSuccess(response).windows;
}

export async function openWindow(desktopId: string, request: OpenWindowRequest): Promise<WindowState> {
  const response = await apiClient.post<OpenWindowEnvelope>(`/desktop/${desktopId}/windows`, request);
  const ok = assertSuccess(response);

  if (!ok.window) {
    throw new Error('Desktop API open window response missing window');
  }

  return ok.window;
}

export async function closeWindow(desktopId: string, windowId: string): Promise<void> {
  const response = await apiClient.delete<ApiEnvelope>(`/desktop/${desktopId}/windows/${windowId}`);
  assertSuccess(response);
}

export async function moveWindow(desktopId: string, windowId: string, x: number, y: number): Promise<void> {
  const response = await apiClient.patch<ApiEnvelope>(`/desktop/${desktopId}/windows/${windowId}/position`, {
    x,
    y,
  });
  assertSuccess(response);
}

export async function resizeWindow(
  desktopId: string,
  windowId: string,
  width: number,
  height: number,
): Promise<void> {
  const response = await apiClient.patch<ApiEnvelope>(`/desktop/${desktopId}/windows/${windowId}/size`, {
    width,
    height,
  });
  assertSuccess(response);
}

export async function focusWindow(desktopId: string, windowId: string): Promise<void> {
  const response = await apiClient.post<ApiEnvelope>(`/desktop/${desktopId}/windows/${windowId}/focus`, {});
  assertSuccess(response);
}

export async function minimizeWindow(desktopId: string, windowId: string): Promise<void> {
  const response = await apiClient.post<ApiEnvelope>(
    `/desktop/${desktopId}/windows/${windowId}/minimize`,
    {},
  );
  assertSuccess(response);
}

export async function maximizeWindow(desktopId: string, windowId: string): Promise<MaximizeWindowResponse> {
  return await apiClient.post<MaximizeWindowResponse>(`/desktop/${desktopId}/windows/${windowId}/maximize`, {});
}

export async function restoreWindow(desktopId: string, windowId: string): Promise<RestoreWindowResponse> {
  return await apiClient.post<RestoreWindowResponse>(`/desktop/${desktopId}/windows/${windowId}/restore`, {});
}

export async function getApps(desktopId: string): Promise<AppDefinition[]> {
  const response = await apiClient.get<AppsEnvelope>(`/desktop/${desktopId}/apps`);
  return assertSuccess(response).apps;
}

export async function registerApp(desktopId: string, app: AppDefinition): Promise<void> {
  const response = await apiClient.post<ApiEnvelope>(`/desktop/${desktopId}/apps`, app);
  assertSuccess(response);
}
