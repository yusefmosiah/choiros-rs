import apiClient from './client';

// Types matching backend structures
export interface UserPreferences {
  success: boolean;
  theme: 'light' | 'dark' | 'system';
}

export interface UpdatePreferencesRequest {
  theme?: 'light' | 'dark' | 'system';
  language?: string;
  notifications_enabled?: boolean;
  sidebar_collapsed?: boolean;
  custom_settings?: Record<string, unknown>;
}

// User API functions
export async function getUserPreferences(userId: string): Promise<UserPreferences> {
  return apiClient.get<UserPreferences>(`/user/${userId}/preferences`);
}

export async function updateUserPreferences(userId: string, preferences: UpdatePreferencesRequest): Promise<UserPreferences> {
  return apiClient.patch<UserPreferences>(`/user/${userId}/preferences`, preferences);
}
