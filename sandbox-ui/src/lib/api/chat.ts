import apiClient from './client';
import type { ChatMessage } from '@/types/generated';

interface ApiEnvelope {
  success: boolean;
  error?: string;
  message?: string;
}

interface SendMessageResponse extends ApiEnvelope {
  temp_id: string;
}

interface GetMessagesResponse extends ApiEnvelope {
  messages: ChatMessage[];
}

export interface SendMessageRequest {
  text: string;
  user_id: string;
}

function assertSuccess<T extends ApiEnvelope>(response: T): T {
  if (!response.success) {
    throw new Error(response.error ?? response.message ?? 'Chat API request failed');
  }

  return response;
}

export async function sendMessage(actorId: string, request: SendMessageRequest): Promise<string> {
  const response = await apiClient.post<SendMessageResponse>('/chat/send', {
    actor_id: actorId,
    user_id: request.user_id,
    text: request.text,
  });

  return assertSuccess(response).temp_id;
}

export async function getMessages(actorId: string): Promise<ChatMessage[]> {
  const response = await apiClient.get<GetMessagesResponse>(`/chat/${actorId}/messages`);
  return assertSuccess(response).messages;
}
