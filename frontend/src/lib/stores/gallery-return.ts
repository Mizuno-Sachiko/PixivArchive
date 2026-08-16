import type { GallerySearch, GalleryWork } from '$lib/api/gallery';
import type { GalleryQueryStore } from '$lib/stores/gallery-query.svelte';

const STORAGE_KEY = 'pixivarchive.gallery-return';
const CONTEXT_STORAGE_KEY = 'pixivarchive.gallery-context-return';
const MAX_MEMORY_SNAPSHOTS = 8;

export interface GalleryViewportSnapshot {
  scrollY: number;
  anchorId?: string;
  anchorOffset?: number;
}

export interface GalleryContextReturnSnapshot {
  route: string;
  query: string;
  viewport: GalleryViewportSnapshot;
  loaded?: Record<string, number>;
}

export interface GalleryReturnSnapshot {
  route: string;
  scrollY: number;
  viewport?: GalleryViewportSnapshot;
  items: GalleryWork[];
  cursor?: GallerySearch['cursor'];
  totalCount: number;
  loadedDepth?: number;
  workRevision?: number;
  pixivBookmarkRevision?: number;
  pixivAccountRevision?: number;
  snapshotRevision?: number;
  query: GalleryQueryStore;
  appliedQuery: GallerySearch;
}

const snapshots = new Map<string, GalleryReturnSnapshot>();
const contextSnapshots = new Map<string, GalleryContextReturnSnapshot>();
const contextData = new Map<string, unknown>();

export function saveGalleryReturn(next: GalleryReturnSnapshot): void {
  snapshots.delete(next.route);
  snapshots.set(next.route, next);
  while (snapshots.size > MAX_MEMORY_SNAPSHOTS) {
    const oldest = snapshots.keys().next().value as string | undefined;
    if (!oldest) break;
    snapshots.delete(oldest);
  }
  removeLegacyStoredSnapshots();
}

export function takeGalleryReturn(route: string): GalleryReturnSnapshot | null {
  const current = snapshots.get(route);
  if (!current) return null;
  snapshots.delete(route);
  snapshots.set(route, current);
  return current;
}

export function clearGalleryReturn(route: string): void {
  snapshots.delete(route);
  removeLegacyStoredSnapshots();
}

export function saveGalleryContextReturn(
  snapshot: GalleryContextReturnSnapshot
): void {
  contextSnapshots.set(snapshot.route, snapshot);
  if (typeof sessionStorage === 'undefined') return;
  const stored = storedContextSnapshots();
  stored[snapshot.route] = snapshot;
  try {
    sessionStorage.setItem(CONTEXT_STORAGE_KEY, JSON.stringify(stored));
  } catch {
    // 内存快照仍可支持当前页面会话内返回。
  }
}

export function loadGalleryContextReturn(
  route: string
): GalleryContextReturnSnapshot | null {
  const current = contextSnapshots.get(route);
  if (current) return current;
  const restored = storedContextSnapshots()[route];
  if (!restored) return null;
  contextSnapshots.set(route, restored);
  return restored;
}

export function saveGalleryContextData<T>(route: string, data: T): void {
  contextData.set(route, data);
}

export function loadGalleryContextData<T>(route: string): T | null {
  return (contextData.get(route) as T | undefined) ?? null;
}

export function captureGalleryViewport(
  anchorSelector = '[data-gallery-anchor]'
): GalleryViewportSnapshot {
  const viewport: GalleryViewportSnapshot = { scrollY: window.scrollY };
  const anchor = [...document.querySelectorAll<HTMLElement>(anchorSelector)]
    .map((element) => ({ element, bounds: element.getBoundingClientRect() }))
    .filter(
      ({ bounds }) => bounds.bottom > 0 && bounds.top < window.innerHeight
    )
    .sort((left, right) => left.bounds.top - right.bounds.top)[0];
  const anchorId = anchor?.element.dataset.galleryAnchor;
  if (anchor && anchorId) {
    viewport.anchorId = anchorId;
    viewport.anchorOffset = anchor.bounds.top;
  }
  return viewport;
}

export async function restoreGalleryViewport(
  viewport: GalleryViewportSnapshot,
  anchorSelector = '[data-gallery-anchor]'
): Promise<void> {
  await nextFrame();
  window.scrollTo({ top: viewport.scrollY, behavior: 'instant' });
  if (!viewport.anchorId || viewport.anchorOffset === undefined) return;
  correctGalleryAnchor(viewport, anchorSelector);
  await nextFrame();
  correctGalleryAnchor(viewport, anchorSelector);
}

function correctGalleryAnchor(
  viewport: GalleryViewportSnapshot,
  anchorSelector: string
): void {
  if (!viewport.anchorId || viewport.anchorOffset === undefined) return;
  const anchor = [
    ...document.querySelectorAll<HTMLElement>(anchorSelector)
  ].find((element) => element.dataset.galleryAnchor === viewport.anchorId);
  if (!anchor) return;
  const correction = anchor.getBoundingClientRect().top - viewport.anchorOffset;
  if (Math.abs(correction) > 0.5) {
    window.scrollBy({ top: correction, behavior: 'instant' });
  }
}

function nextFrame(): Promise<void> {
  return new Promise((resolve) => requestAnimationFrame(() => resolve()));
}

function storedContextSnapshots(): Record<
  string,
  GalleryContextReturnSnapshot
> {
  if (typeof sessionStorage === 'undefined') return {};
  try {
    const value = sessionStorage.getItem(CONTEXT_STORAGE_KEY);
    if (!value) return {};
    return JSON.parse(value) as Record<string, GalleryContextReturnSnapshot>;
  } catch {
    return {};
  }
}

function removeLegacyStoredSnapshots(): void {
  if (typeof sessionStorage === 'undefined') return;
  try {
    sessionStorage.removeItem(STORAGE_KEY);
  } catch {
    // 浏览器拒绝存储访问时，内存中的返回位置仍然可用。
  }
}
