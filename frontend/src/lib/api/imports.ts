import { apiRequest, type ApiRequest } from './client';
import type { components } from './schema';

export type ImportKind = components['schemas']['ImportKindDto'];
export type ImportStrategy = components['schemas']['ImportStrategyDto'];
export type ImportRequest = components['schemas']['QueueImportBody'];
export type ImportRun = components['schemas']['ImportRunDto'];

export const IMPORT_RUN_LIST_LIMIT = 200;

export interface ImportApi {
  list(): Promise<ImportRun[]>;
  queue(input: ImportRequest): Promise<ImportRun>;
}

export function createImportApi(request: ApiRequest = apiRequest): ImportApi {
  return {
    async list() {
      const response = await request<components['schemas']['ImportRunList']>(
        `/api/imports?limit=${IMPORT_RUN_LIST_LIMIT}`
      );
      return response.items;
    },
    queue(input) {
      return request('/api/imports', { method: 'POST', json: input });
    }
  };
}

export const importApi = createImportApi();
