import apiClient from './client';

export interface HealthResponse {
  status: string;
  service: string;
  version: string;
}

export async function checkHealth(): Promise<HealthResponse> {
  return apiClient.get<HealthResponse>('/health');
}
