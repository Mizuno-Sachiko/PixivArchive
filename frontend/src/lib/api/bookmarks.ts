import { apiRequest, type ApiRequest } from './client';
import type { components } from './schema';

export type BookmarkCommandResult = components['schemas']['BookmarkCommandDto'];

export function addBookmark(
  input: components['schemas']['AddBookmarkBody'],
  request: ApiRequest = apiRequest
): Promise<BookmarkCommandResult> {
  return request('/api/bookmarks', { method: 'POST', json: input });
}

export function removeBookmark(
  workId: number,
  accountId: string,
  request: ApiRequest = apiRequest
): Promise<BookmarkCommandResult> {
  return request(`/api/bookmarks/works/${workId}`, {
    method: 'DELETE',
    json: { account_id: accountId }
  });
}
