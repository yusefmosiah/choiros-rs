import apiClient from './client';

// Types matching backend structures
export interface Window {
  id: string;
  app_id: string;
  title: string;
  x: number;
  y: number;
  width: number;
  height: number;
  minimized: boolean;
  maximized: boolean;
  focused: boolean;
}

export interface App {
  id: string;
  name: string;
  icon?: string;
  component: string;
}

export interface DesktopState {
  id: string;
  windows: Window[];
  apps: App[];
  active_window_id?: string;
}

export interface OpenWindowRequest {
  app_id: string;
  title?: string;
  x?: number;
  y?: number;
  width?: number;
  height?: number;
}

export interface WindowPositionRequest {
  x: number;
  y: number;
}

export interface WindowSizeRequest {
  width: number;
  height: number;
}

// Desktop API functions
export async function getDesktopState(desktopId: string): Promise<DesktopState> {
  return apiClient.get<DesktopState>(`/desktop/${desktopId}`);
}

export async function getWindows(desktopId: string): Promise<Window[]> {
  return apiClient.get<Window[]>(`/desktop/${desktopId}/windows`);
}

export async function openWindow(desktopId: string, request: OpenWindowRequest): Promise<Window> {
  return apiClient.post<Window>(`/desktop/${desktopId}/windows`, request);
}

export async function closeWindow(desktopId: string, windowId: string): Promise<void> {
  return apiClient.delete<void>(`/desktop/${desktopId}/windows/${windowId}`);
}

export async function moveWindow(desktopId: string, windowId: string, x: number, y: number): Promise<Window> {
  return apiClient.patch<Window>(`/desktop/${desktopId}/windows/${windowId}/position`, { x, y });
}

export async function resizeWindow(desktopId: string, windowId: string, width: number, height: number): Promise<Window> {
  return apiClient.patch<Window>(`/desktop/${desktopId}/windows/${windowId}/size`, { width, height });
}

export async function focusWindow(desktopId: string, windowId: string): Promise<Window> {
  return apiClient.post<Window>(`/desktop/${desktopId}/windows/${windowId}/focus`, {});
}

export async function minimizeWindow(desktopId: string, windowId: string): Promise<Window> {
  return apiClient.post<Window>(`/desktop/${desktopId}/windows/${windowId}/minimize`, {});
}

export async function maximizeWindow(desktopId: string, windowId: string): Promise<Window> {
  return apiClient.post<Window>(`/desktop/${desktopId}/windows/${windowId}/maximize`, {});
}

export async function restoreWindow(desktopId: string, windowId: string): Promise<Window> {
  return apiClient.post<Window>(`/desktop/${desktopId}/windows/${windowId}/restore`, {});
}

export async function getApps(desktopId: string): Promise<App[]> {
  return apiClient.get<App[]>(`/desktop/${desktopId}/apps`);
}

export async function registerApp(desktopId: string, app: App): Promise<App> {
  return apiClient.post<App>(`/desktop/${desktopId}/apps`, app);
}
