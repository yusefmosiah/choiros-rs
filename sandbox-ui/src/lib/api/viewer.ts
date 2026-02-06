import apiClient from './client';
import type { ViewerRevision, ViewerDescriptor } from '@/types/generated';

interface ApiEnvelope {
  success: boolean;
  error?: string;
}

// GET /viewer/content?uri={uri}
interface FetchViewerContentResponse extends ApiEnvelope {
  uri: string;
  mime: string;
  content: string;
  revision: ViewerRevision;
  readonly: boolean;
}

export interface FetchViewerContentResult {
  content: string;
  revision: ViewerRevision;
  descriptor: ViewerDescriptor;
}

export async function fetchViewerContent(uri: string): Promise<FetchViewerContentResult> {
  const response = await apiClient.get<FetchViewerContentResponse>('/viewer/content', {
    params: { uri }
  });

  if (!response.success) {
    throw new Error(response.error || 'Failed to fetch viewer content');
  }

  // Construct ViewerDescriptor from response fields
  const descriptor: ViewerDescriptor = {
    kind: inferViewerKind(response.mime),
    resource: {
      uri: response.uri,
      mime: response.mime,
    },
    capabilities: {
      readonly: response.readonly,
    },
  };

  return {
    content: response.content,
    revision: response.revision,
    descriptor,
  };
}

// PATCH /viewer/content
export interface PatchViewerContentRequest {
  uri: string;
  content: string;
  base_revision: bigint;
}

interface ConflictLatest {
  content: string;
  revision: ViewerRevision;
}

interface PatchViewerContentApiResponse extends ApiEnvelope {
  revision?: ViewerRevision;
  latest?: ConflictLatest;
}

export interface PatchViewerContentResponse {
  success: boolean;
  revision: ViewerRevision;
  conflict?: boolean;
}

export async function patchViewerContent(
  request: PatchViewerContentRequest
): Promise<PatchViewerContentResponse> {
  // Map base_revision to base_rev for backend compatibility
  const backendRequest = {
    uri: request.uri,
    content: request.content,
    base_rev: Number(request.base_revision),
  };

  try {
    const response = await apiClient.patch<PatchViewerContentApiResponse>('/viewer/content', backendRequest);

    if (!response.success) {
      // Check if this is a conflict response (409 status)
      if (response.latest) {
        return {
          success: false,
          revision: response.latest.revision,
          conflict: true,
        };
      }
      throw new Error(response.error || 'Failed to patch viewer content');
    }

    if (!response.revision) {
      throw new Error('Invalid response: missing revision');
    }

    return {
      success: true,
      revision: response.revision,
    };
  } catch (error) {
    // Handle 409 Conflict errors from the HTTP layer
    if (error && typeof error === 'object' && 'status' in error && error.status === 409) {
      const errorData = (error as { data?: PatchViewerContentApiResponse }).data;
      if (errorData?.latest) {
        return {
          success: false,
          revision: errorData.latest.revision,
          conflict: true,
        };
      }
    }
    throw error;
  }
}

// Helper function to infer ViewerKind from MIME type
function inferViewerKind(mime: string): 'text' | 'image' {
  if (mime.startsWith('image/')) {
    return 'image';
  }
  return 'text';
}
