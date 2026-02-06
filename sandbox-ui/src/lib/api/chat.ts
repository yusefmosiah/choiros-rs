import apiClient from './client';

// Types matching backend structures
export interface ChatMessage {
  id: string;
  actor_id: string;
  user_id?: string;
  content: string;
  role: 'user' | 'assistant' | 'system';
  timestamp: string;
}

export interface SendMessageRequest {
  content: string;
  user_id?: string;
}

export interface SendMessageResponse {
  message_id: string;
  status: string;
}

// Chat API functions
export async function sendMessage(actorId: string, request: SendMessageRequest): Promise<SendMessageResponse> {
  return apiClient.post<SendMessageResponse>('/chat/send', { actor_id: actorId, ...request });
}

export async function getMessages(actorId: string, limit?: number, offset?: number): Promise<ChatMessage[]> {
  const params: Record<string, string> = {};
  if (limit !== undefined) params.limit = String(limit);
  if (offset !== undefined) params.offset = String(offset);
  return apiClient.get<ChatMessage[]>(`/chat/${actorId}/messages`, { params });
}
