import { goto } from '$app/navigation';
import { resolve } from '$app/paths';
import type { Pathname } from '$app/types';

export type DetailSourceKind = 'gallery' | 'trash';

export interface DetailSourceState {
  key: string;
  kind: DetailSourceKind;
  route: Pathname;
}

const activeSources = new Map<string, DetailSourceState>();

export function openWorkDetail(
  target: Pathname,
  source: Omit<DetailSourceState, 'key'>
): Promise<void> {
  const detailSource = registerDetailSource(source);
  return goto(resolve(target), { state: { detailSource } });
}

export function registerDetailSource(
  source: Omit<DetailSourceState, 'key'>
): DetailSourceState {
  const detailSource = { ...source, key: crypto.randomUUID() };
  activeSources.set(detailSource.key, detailSource);
  return detailSource;
}

export function currentDetailSource(
  state: App.PageState | undefined
): DetailSourceState | null {
  const candidate = state?.detailSource;
  if (!candidate) return null;
  const active = activeSources.get(candidate.key);
  if (
    !active ||
    active.kind !== candidate.kind ||
    active.route !== candidate.route
  ) {
    return null;
  }
  return active;
}

export function detailReturnRoute(state: App.PageState | undefined): Pathname {
  return currentDetailSource(state)?.route ?? '/gallery';
}

export function clearDetailSource(key: string): void {
  activeSources.delete(key);
}
