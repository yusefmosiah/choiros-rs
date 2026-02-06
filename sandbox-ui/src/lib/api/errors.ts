export enum ErrorType {
  NETWORK_ERROR = 'NETWORK_ERROR',
  TIMEOUT_ERROR = 'TIMEOUT_ERROR',
  HTTP_ERROR = 'HTTP_ERROR',
}

export class ApiError extends Error {
  constructor(
    public type: ErrorType,
    public statusCode: number = 0,
    message: string,
    public originalError?: unknown
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

export class NetworkError extends ApiError {
  constructor(message: string = 'Network error', originalError?: unknown) {
    super(ErrorType.NETWORK_ERROR, 0, message, originalError);
  }
}

export class TimeoutError extends ApiError {
  constructor(timeoutMs: number, originalError?: unknown) {
    super(ErrorType.TIMEOUT_ERROR, 0, `Request timed out after ${timeoutMs}ms`, originalError);
  }
}

export class HttpError extends ApiError {
  constructor(statusCode: number, message: string, public responseData?: unknown) {
    super(ErrorType.HTTP_ERROR, statusCode, message);
  }
}
