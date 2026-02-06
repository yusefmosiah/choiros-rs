// Core exports
export { apiClient, default as ApiClient } from './client';
export type { RequestOptions } from './client';
export { ApiError, NetworkError, TimeoutError, HttpError, ErrorType } from './errors';

// API modules
export * from './health';
export * from './desktop';
export * from './chat';
export * from './terminal';
export * from './user';
