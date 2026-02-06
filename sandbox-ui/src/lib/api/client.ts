import { ApiError, NetworkError, TimeoutError, HttpError, ErrorType } from './errors';

const DEFAULT_BASE_URL = 'http://localhost:8080';
const DEFAULT_TIMEOUT = 30000;

export interface RequestOptions extends RequestInit {
  timeout?: number;
  params?: Record<string, string>;
}

class ApiClient {
  private baseUrl: string;
  private defaultTimeout: number;

  constructor(baseUrl: string = DEFAULT_BASE_URL, timeout: number = DEFAULT_TIMEOUT) {
    this.baseUrl = baseUrl;
    this.defaultTimeout = timeout;
  }

  async request<T>(endpoint: string, options: RequestOptions = {}): Promise<T> {
    const url = this.buildUrl(endpoint, options.params);
    const timeout = options.timeout || this.defaultTimeout;
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), timeout);

    try {
      const response = await fetch(url, {
        ...options,
        headers: {
          'Content-Type': 'application/json',
          ...options.headers,
        },
        signal: controller.signal,
      });

      clearTimeout(timeoutId);

      if (!response.ok) {
        const errorData = await response.json().catch(() => ({}));
        throw new HttpError(response.status, errorData.error || `HTTP ${response.status}`, errorData);
      }

      return await response.json() as T;
    } catch (error) {
      if (error instanceof ApiError) throw error;
      if (error instanceof TypeError) throw new NetworkError(error.message, error);
      if (error instanceof DOMException && error.name === 'AbortError') {
        throw new TimeoutError(timeout, error);
      }
      throw new ApiError(ErrorType.NETWORK_ERROR, 0, 'Request failed', error);
    }
  }

  async get<T>(endpoint: string, options?: RequestOptions): Promise<T> {
    return this.request<T>(endpoint, { ...options, method: 'GET' });
  }

  async post<T>(endpoint: string, body: unknown, options?: RequestOptions): Promise<T> {
    return this.request<T>(endpoint, { ...options, method: 'POST', body: JSON.stringify(body) });
  }

  async patch<T>(endpoint: string, body: unknown, options?: RequestOptions): Promise<T> {
    return this.request<T>(endpoint, { ...options, method: 'PATCH', body: JSON.stringify(body) });
  }

  async delete<T>(endpoint: string, options?: RequestOptions): Promise<T> {
    return this.request<T>(endpoint, { ...options, method: 'DELETE' });
  }

  private buildUrl(endpoint: string, params?: Record<string, string>): string {
    let url = `${this.baseUrl}${endpoint}`;
    if (params) {
      const query = new URLSearchParams(params).toString();
      if (query) url += `?${query}`;
    }
    return url;
  }
}

export const apiClient = new ApiClient(
  import.meta.env.VITE_API_URL || DEFAULT_BASE_URL
);
export default apiClient;
