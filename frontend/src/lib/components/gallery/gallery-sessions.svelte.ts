import { SvelteSet } from 'svelte/reactivity';

import {
  countGallery,
  projectGalleryContextSelection,
  projectGallerySelection,
  searchGallery,
  type GallerySearch,
  type GallerySearchPage,
  type GalleryContextKind,
  type GalleryContextSelectionExpression,
  type GalleryContextSelectionProjection,
  type GallerySelectionExpression,
  type GallerySelectionProjection,
  type GalleryWork
} from '$lib/api/gallery';
import { moveGalleryContextsToTrash, moveGalleryToTrash } from '$lib/api/trash';

import { refreshVisibleItems } from './gallery-refresh';
import {
  clearSelection,
  isSelected,
  SelectionProjectionCoordinator,
  invertSelection,
  selectAll,
  setSelected,
  type SelectionState
} from './selection-expression';

interface GallerySearchGateway {
  search(query: GallerySearch): Promise<GallerySearchPage>;
  count(query: GallerySearch): Promise<number>;
}

const defaultSearchGateway: GallerySearchGateway = {
  search: searchGallery,
  count: countGallery
};

export interface GallerySearchInitialState {
  items?: GalleryWork[];
  cursor?: GallerySearch['cursor'];
  totalCount?: number;
  loadedDepth?: number;
  appliedQuery: GallerySearch;
}

export class GallerySearchSession {
  items = $state<GalleryWork[]>([]);
  cursor = $state<GallerySearch['cursor']>();
  totalCount = $state(0);
  appliedQuery = $state<GallerySearch>({
    group_mode: 'all',
    groups: [],
    sort_field: 'pixiv_id',
    sort_direction: 'descending',
    limit: 48
  });
  loading = $state(false);
  error = $state('');
  paginationError = $state('');
  refreshing = $state(false);

  private requestRevision = 0;
  private loadedDepth = 0;

  constructor(
    initial: GallerySearchInitialState,
    private readonly gateway: GallerySearchGateway = defaultSearchGateway
  ) {
    this.items = initial.items ?? [];
    this.cursor = initial.cursor;
    this.totalCount = initial.totalCount ?? initial.items?.length ?? 0;
    this.loadedDepth = initial.loadedDepth ?? this.items.length;
    this.appliedQuery = initial.appliedQuery;
  }

  async reset(request: GallerySearch): Promise<boolean> {
    const revision = ++this.requestRevision;
    const countQuery = withoutCursor(request);
    this.refreshing = false;
    this.loading = true;
    this.error = '';
    this.paginationError = '';
    try {
      const [page, count] = await Promise.all([
        this.gateway.search(request),
        this.gateway.count(countQuery)
      ]);
      if (revision !== this.requestRevision) return false;
      this.items = page.items;
      this.cursor = page.next_cursor ?? undefined;
      this.appliedQuery = countQuery;
      this.totalCount = Math.max(count, page.items.length);
      this.loadedDepth = this.items.length;
      return true;
    } catch {
      if (revision === this.requestRevision) {
        this.error = '图库数据暂时无法读取';
      }
      return false;
    } finally {
      if (revision === this.requestRevision) this.loading = false;
    }
  }

  async loadNext(): Promise<boolean> {
    if (this.loading || this.refreshing || !this.cursor) return false;
    const revision = this.requestRevision;
    const request = withCursor(this.appliedQuery, this.cursor);
    this.loading = true;
    this.paginationError = '';
    try {
      const page = await this.gateway.search(request);
      if (revision !== this.requestRevision) return false;
      this.items = [...this.items, ...page.items];
      this.cursor = page.next_cursor ?? undefined;
      this.loadedDepth = this.items.length;
      return true;
    } catch {
      if (revision === this.requestRevision) {
        this.paginationError = '后续作品暂时无法读取';
      }
      return false;
    } finally {
      if (revision === this.requestRevision) this.loading = false;
    }
  }

  async refreshFromAppliedQuery(): Promise<boolean> {
    const revision = ++this.requestRevision;
    const appliedQuery = withoutCursor(this.appliedQuery);
    delete appliedQuery.restrict_work_ids;
    const targetDepth = Math.max(
      this.loadedDepth,
      this.items.length,
      appliedQuery.limit
    );
    this.loading = false;
    this.error = '';
    this.paginationError = '';
    this.refreshing = true;
    try {
      const [result, count] = await Promise.all([
        searchToDepth(appliedQuery, targetDepth, this.gateway),
        this.gateway.count(structuredClone(appliedQuery))
      ]);
      if (revision !== this.requestRevision) return false;
      this.items = result.items;
      this.cursor = result.cursor;
      this.totalCount = Math.max(count, result.items.length);
      this.loadedDepth = result.items.length;
      return true;
    } catch {
      if (revision === this.requestRevision) {
        this.error = '图库数据暂时无法刷新';
      }
      return false;
    } finally {
      if (revision === this.requestRevision) this.refreshing = false;
    }
  }

  async refreshLoadedItems(): Promise<boolean> {
    const revision = ++this.requestRevision;
    const chunks = Array.from(
      { length: Math.ceil(this.items.length / 200) },
      (_, index) => this.items.slice(index * 200, (index + 1) * 200)
    );
    if (chunks.length === 0) return false;

    const appliedQuery = withoutCursor(this.appliedQuery);
    delete appliedQuery.restrict_work_ids;
    this.loading = false;
    this.error = '';
    this.paginationError = '';
    this.refreshing = true;
    try {
      const [pages, count] = await Promise.all([
        refreshChunks(chunks, appliedQuery, this.gateway),
        this.gateway.count(structuredClone(appliedQuery))
      ]);
      if (revision !== this.requestRevision) return false;
      this.items = refreshVisibleItems(
        this.items,
        pages.flatMap((page) => page.items),
        preserveGalleryCardGeometry
      );
      this.totalCount = Math.max(count, this.items.length);
      this.loadedDepth = this.items.length;
      return true;
    } catch {
      if (revision === this.requestRevision) {
        this.error = '图库数据暂时无法刷新';
      }
      return false;
    } finally {
      if (revision === this.requestRevision) this.refreshing = false;
    }
  }

  removeItems(ids: ReadonlySet<string>): void {
    this.items = this.items.filter((work) => !ids.has(work.id));
    this.totalCount = Math.max(0, this.totalCount - ids.size);
    this.loadedDepth = this.items.length;
  }

  snapshot(): GallerySearchInitialState {
    return {
      items: [...this.items],
      cursor: this.cursor ? $state.snapshot(this.cursor) : undefined,
      totalCount: this.totalCount,
      loadedDepth: this.loadedDepth,
      appliedQuery: $state.snapshot(this.appliedQuery)
    };
  }

  invalidate(): void {
    this.requestRevision += 1;
    this.loading = false;
    this.refreshing = false;
  }
}

function preserveGalleryCardGeometry(
  current: GalleryWork,
  refreshed: GalleryWork
): GalleryWork {
  return {
    ...refreshed,
    cover_width: current.cover_width,
    cover_height: current.cover_height,
    tags: current.tags
  };
}

async function searchToDepth(
  query: GallerySearch,
  depth: number,
  gateway: GallerySearchGateway
): Promise<{ items: GalleryWork[]; cursor?: GallerySearch['cursor'] }> {
  const items: GalleryWork[] = [];
  let cursor: GallerySearch['cursor'];
  do {
    const request = cursor ? withCursor(query, cursor) : structuredClone(query);
    const page = await gateway.search(request);
    items.push(...page.items);
    cursor = page.next_cursor ?? undefined;
  } while (items.length < depth && cursor);
  return {
    items,
    cursor
  };
}

async function refreshChunks(
  chunks: GalleryWork[][],
  appliedQuery: GallerySearch,
  gateway: GallerySearchGateway
): Promise<GallerySearchPage[]> {
  const pages: GallerySearchPage[] = [];
  for (let index = 0; index < chunks.length; index += 3) {
    const batch = await Promise.all(
      chunks.slice(index, index + 3).map((chunk) =>
        gateway.search({
          ...structuredClone(appliedQuery),
          restrict_work_ids: chunk.map((work) => work.id),
          limit: chunk.length
        })
      )
    );
    pages.push(...batch);
  }
  return pages;
}

interface GallerySelectionGateway {
  project(
    expression: GallerySelectionExpression,
    visibleWorkIds: string[]
  ): Promise<GallerySelectionProjection>;
  move(
    expression: GallerySelectionExpression,
    retentionDays: number
  ): Promise<number>;
}

const defaultSelectionGateway: GallerySelectionGateway = {
  project: projectGallerySelection,
  move: moveGalleryToTrash
};

export interface GalleryTrashResult {
  movedCount: number;
}

export class GallerySelectionSession {
  mode = $state(false);
  count = $state(0);
  busy = $state(false);
  error = $state('');
  notice = $state('');

  private expression = $state<GallerySelectionExpression>(
    emptyGallerySelectionExpression()
  );
  private readonly visibleSelected = new SvelteSet<string>();
  private readonly projectionCoordinator = new SelectionProjectionCoordinator();
  private commandRevision = 0;

  constructor(
    private readonly gateway: GallerySelectionGateway = defaultSelectionGateway
  ) {}

  idsFor(items: GalleryWork[]): SvelteSet<string> {
    return new SvelteSet(
      items
        .filter((work) => this.visibleSelected.has(work.id))
        .map((work) => work.id)
    );
  }

  enter(query: GallerySearch): void {
    this.invalidateOperation();
    this.mode = true;
    this.expression = {
      search: selectionQuery(query),
      base_selected: false,
      exception_work_ids: []
    };
    this.resetProjection();
    this.clearFeedback();
  }

  exit(): void {
    this.invalidateOperation();
    this.mode = false;
    this.expression = emptyGallerySelectionExpression();
    this.resetProjection();
  }

  clear(): void {
    this.projectionCoordinator.invalidate();
    this.expression = withGallerySelection(this.expression, clearSelection());
    this.resetProjection();
    this.error = '';
  }

  snapshotExpression(): GallerySelectionExpression {
    return structuredClone($state.snapshot(this.expression));
  }

  async setWork(
    workId: string,
    selected: boolean,
    visibleWorkIds: string[]
  ): Promise<void> {
    const current = gallerySelectionState(this.expression);
    const wasSelected = isSelected(current, workId);
    await this.projectExpression(
      withGallerySelection(
        this.expression,
        setSelected(current, workId, selected)
      ),
      visibleWorkIds,
      wasSelected === selected ? this.count : this.count + (selected ? 1 : -1)
    );
  }

  async selectAll(visibleWorkIds: string[]): Promise<void> {
    await this.projectExpression(
      withGallerySelection(this.expression, selectAll()),
      visibleWorkIds,
      undefined
    );
  }

  async invert(visibleWorkIds: string[]): Promise<void> {
    await this.projectExpression(
      withGallerySelection(
        this.expression,
        invertSelection(gallerySelectionState(this.expression))
      ),
      visibleWorkIds,
      undefined
    );
  }

  async refreshVisible(visibleWorkIds: string[]): Promise<void> {
    if (!this.mode) return;
    const latestSucceeded = await this.projectionCoordinator.waitForLatest();
    if (!latestSucceeded || !this.mode) return;
    if (!hasGallerySelection(this.expression)) {
      this.visibleSelected.clear();
      this.count = 0;
      return;
    }
    await this.projectExpression(
      this.snapshotExpression(),
      visibleWorkIds,
      undefined,
      false
    );
  }

  async trash(
    retentionDays: number,
    searchBusy: boolean
  ): Promise<GalleryTrashResult | null> {
    if (this.busy || searchBusy || !this.mode) return null;
    const latestSucceeded = await this.projectionCoordinator.waitForLatest();
    if (!latestSucceeded || this.busy || !this.mode || this.count === 0) {
      return null;
    }

    const revision = ++this.commandRevision;
    this.busy = true;
    this.error = '';
    this.notice = '';
    try {
      const moved = await this.gateway.move(
        this.snapshotExpression(),
        retentionDays
      );
      if (revision !== this.commandRevision) return null;
      this.finishSelection();
      this.notice = `${moved}件作品已移入回收站`;
      return { movedCount: moved };
    } catch {
      if (revision === this.commandRevision) {
        this.error = '批量移入回收站失败';
      }
      return null;
    } finally {
      if (revision === this.commandRevision) this.busy = false;
    }
  }

  private async projectExpression(
    expression: GallerySelectionExpression,
    visibleWorkIds: string[],
    optimisticCount?: number,
    applyIntent = true
  ): Promise<boolean> {
    if (!this.mode) return false;
    const previous = this.projectionSnapshot();
    if (applyIntent) {
      this.expression = expression;
      this.applyVisibleIntent(expression, visibleWorkIds);
      if (optimisticCount !== undefined) {
        this.count = Math.max(0, optimisticCount);
      }
    }
    this.error = '';
    return this.projectionCoordinator.project(
      () => this.gateway.project(expression, visibleWorkIds),
      (projection) => {
        if (!this.mode) return;
        this.expression = expression;
        this.count = projection.selected_count;
        this.visibleSelected.clear();
        for (const workId of projection.selected_visible_work_ids) {
          this.visibleSelected.add(workId);
        }
      },
      () => {
        if (!this.mode) return;
        this.restoreProjection(previous);
        this.error = '无法更新当前选择';
      }
    );
  }

  private invalidateOperation(): void {
    this.commandRevision += 1;
    this.projectionCoordinator.invalidate();
    this.busy = false;
  }

  private resetProjection(): void {
    this.visibleSelected.clear();
    this.count = 0;
  }

  private applyVisibleIntent(
    expression: GallerySelectionExpression,
    visibleWorkIds: string[]
  ): void {
    const selection = gallerySelectionState(expression);
    this.visibleSelected.clear();
    for (const workId of visibleWorkIds) {
      if (isSelected(selection, workId)) this.visibleSelected.add(workId);
    }
  }

  private projectionSnapshot(): GallerySelectionSnapshot {
    return {
      expression: this.snapshotExpression(),
      count: this.count,
      visibleWorkIds: [...this.visibleSelected]
    };
  }

  private restoreProjection(snapshot: GallerySelectionSnapshot): void {
    this.expression = structuredClone(snapshot.expression);
    this.count = snapshot.count;
    this.visibleSelected.clear();
    for (const workId of snapshot.visibleWorkIds) {
      this.visibleSelected.add(workId);
    }
  }

  private clearFeedback(): void {
    this.error = '';
    this.notice = '';
  }

  private finishSelection(): void {
    this.projectionCoordinator.invalidate();
    this.mode = false;
    this.expression = emptyGallerySelectionExpression();
    this.resetProjection();
  }
}

interface GalleryContextSelectionGateway {
  project(
    expression: GalleryContextSelectionExpression,
    visibleContextIds: string[]
  ): Promise<GalleryContextSelectionProjection>;
  move(
    expression: GalleryContextSelectionExpression,
    retentionDays: number
  ): Promise<number>;
}

const defaultContextSelectionGateway: GalleryContextSelectionGateway = {
  project: projectGalleryContextSelection,
  move: moveGalleryContextsToTrash
};

interface ContextSelectionItem {
  id: string;
}

export class GalleryContextSelectionSession {
  mode = $state(false);
  contextCount = $state(0);
  workCount = $state(0);
  busy = $state(false);
  error = $state('');
  notice = $state('');

  private expression: GalleryContextSelectionExpression;
  private readonly visibleSelected = new SvelteSet<string>();
  private readonly projectionCoordinator = new SelectionProjectionCoordinator();
  private commandRevision = 0;

  constructor(
    private readonly readKind: () => GalleryContextKind,
    private readonly gateway: GalleryContextSelectionGateway = defaultContextSelectionGateway
  ) {
    this.expression = emptyContextSelectionExpression(readKind());
  }

  idsFor(items: ContextSelectionItem[]): SvelteSet<string> {
    return new SvelteSet(
      items
        .filter((item) => this.visibleSelected.has(item.id))
        .map((item) => item.id)
    );
  }

  enter(query: string): void {
    this.invalidateOperation();
    this.mode = true;
    this.expression = {
      kind: this.readKind(),
      query: query.trim(),
      base_selected: false,
      exception_context_ids: []
    };
    this.resetProjection();
    this.clearFeedback();
  }

  exit(): void {
    this.invalidateOperation();
    this.mode = false;
    this.expression = emptyContextSelectionExpression(this.readKind());
    this.resetProjection();
  }

  clear(): void {
    this.projectionCoordinator.invalidate();
    this.expression = withContextSelection(this.expression, clearSelection());
    this.resetProjection();
    this.error = '';
  }

  snapshotExpression(): GalleryContextSelectionExpression {
    return structuredClone(this.expression);
  }

  async setItem(
    contextId: string,
    selected: boolean,
    visibleContextIds: string[]
  ): Promise<void> {
    await this.projectExpression(
      withContextSelection(
        this.expression,
        setSelected(contextSelectionState(this.expression), contextId, selected)
      ),
      visibleContextIds,
      undefined
    );
  }

  async selectAll(visibleContextIds: string[]): Promise<void> {
    await this.projectExpression(
      withContextSelection(this.expression, selectAll()),
      visibleContextIds,
      undefined
    );
  }

  async invert(visibleContextIds: string[]): Promise<void> {
    await this.projectExpression(
      withContextSelection(
        this.expression,
        invertSelection(contextSelectionState(this.expression))
      ),
      visibleContextIds,
      undefined
    );
  }

  async refreshVisible(visibleContextIds: string[]): Promise<void> {
    if (!this.mode) return;
    const latestSucceeded = await this.projectionCoordinator.waitForLatest();
    if (!latestSucceeded || !this.mode) return;
    if (!hasContextSelection(this.expression)) {
      this.visibleSelected.clear();
      this.contextCount = 0;
      this.workCount = 0;
      return;
    }
    await this.projectExpression(
      this.snapshotExpression(),
      visibleContextIds,
      undefined,
      false
    );
  }

  async trash(
    retentionDays: number,
    directoryBusy: boolean
  ): Promise<GalleryTrashResult | null> {
    if (this.busy || directoryBusy || !this.mode) {
      return null;
    }
    const latestSucceeded = await this.projectionCoordinator.waitForLatest();
    if (!latestSucceeded || this.busy || !this.mode || this.workCount === 0) {
      return null;
    }
    const revision = ++this.commandRevision;
    this.busy = true;
    this.error = '';
    this.notice = '';
    try {
      const moved = await this.gateway.move(
        this.snapshotExpression(),
        retentionDays
      );
      if (revision !== this.commandRevision) return null;
      this.finishSelection();
      this.notice = `${moved}件作品已移入回收站`;
      return { movedCount: moved };
    } catch {
      if (revision === this.commandRevision) {
        this.error = '批量移入回收站失败';
      }
      return null;
    } finally {
      if (revision === this.commandRevision) this.busy = false;
    }
  }

  private async projectExpression(
    expression: GalleryContextSelectionExpression,
    visibleContextIds: string[],
    optimisticContextCount?: number,
    applyIntent = true
  ): Promise<boolean> {
    if (!this.mode) return false;
    const previous = this.projectionSnapshot();
    if (applyIntent) {
      this.expression = expression;
      this.applyVisibleIntent(expression, visibleContextIds);
      if (optimisticContextCount !== undefined) {
        this.contextCount = Math.max(0, optimisticContextCount);
      }
    }
    this.error = '';
    return this.projectionCoordinator.project(
      () => this.gateway.project(expression, visibleContextIds),
      (projection) => {
        if (!this.mode) return;
        this.expression = expression;
        this.contextCount = projection.selected_context_count;
        this.workCount = projection.selected_work_count;
        this.visibleSelected.clear();
        for (const contextId of projection.selected_visible_context_ids) {
          this.visibleSelected.add(contextId);
        }
      },
      () => {
        if (!this.mode) return;
        this.restoreProjection(previous);
        this.error = '无法更新当前选择';
      }
    );
  }

  private invalidateOperation(): void {
    this.commandRevision += 1;
    this.projectionCoordinator.invalidate();
    this.busy = false;
  }

  private resetProjection(): void {
    this.visibleSelected.clear();
    this.contextCount = 0;
    this.workCount = 0;
  }

  private applyVisibleIntent(
    expression: GalleryContextSelectionExpression,
    visibleContextIds: string[]
  ): void {
    const selection = contextSelectionState(expression);
    this.visibleSelected.clear();
    for (const contextId of visibleContextIds) {
      if (isSelected(selection, contextId)) {
        this.visibleSelected.add(contextId);
      }
    }
  }

  private projectionSnapshot(): ContextSelectionSnapshot {
    return {
      expression: this.snapshotExpression(),
      contextCount: this.contextCount,
      workCount: this.workCount,
      visibleContextIds: [...this.visibleSelected]
    };
  }

  private restoreProjection(snapshot: ContextSelectionSnapshot): void {
    this.expression = structuredClone(snapshot.expression);
    this.contextCount = snapshot.contextCount;
    this.workCount = snapshot.workCount;
    this.visibleSelected.clear();
    for (const contextId of snapshot.visibleContextIds) {
      this.visibleSelected.add(contextId);
    }
  }

  private clearFeedback(): void {
    this.error = '';
    this.notice = '';
  }

  private finishSelection(): void {
    this.projectionCoordinator.invalidate();
    this.mode = false;
    this.expression = emptyContextSelectionExpression(this.readKind());
    this.resetProjection();
  }
}

interface GallerySelectionSnapshot {
  expression: GallerySelectionExpression;
  count: number;
  visibleWorkIds: string[];
}

interface ContextSelectionSnapshot {
  expression: GalleryContextSelectionExpression;
  contextCount: number;
  workCount: number;
  visibleContextIds: string[];
}

function emptyGallerySelectionExpression(): GallerySelectionExpression {
  return {
    search: {
      group_mode: 'all',
      groups: [],
      sort_field: 'pixiv_id',
      sort_direction: 'descending',
      limit: 48
    },
    base_selected: false,
    exception_work_ids: []
  };
}

function gallerySelectionState(
  expression: GallerySelectionExpression
): SelectionState {
  return {
    base_selected: expression.base_selected ?? false,
    exception_ids: expression.exception_work_ids ?? []
  };
}

function withGallerySelection(
  expression: GallerySelectionExpression,
  selection: SelectionState
): GallerySelectionExpression {
  const snapshot = $state.snapshot(expression);
  return {
    search: structuredClone(snapshot.search),
    base_selected: selection.base_selected,
    exception_work_ids: selection.exception_ids
  };
}

function hasGallerySelection(expression: GallerySelectionExpression): boolean {
  return (
    (expression.base_selected ?? false) ||
    (expression.exception_work_ids?.length ?? 0) > 0
  );
}

function emptyContextSelectionExpression(
  kind: GalleryContextKind
): GalleryContextSelectionExpression {
  return {
    kind,
    query: '',
    base_selected: false,
    exception_context_ids: []
  };
}

function contextSelectionState(
  expression: GalleryContextSelectionExpression
): SelectionState {
  return {
    base_selected: expression.base_selected ?? false,
    exception_ids: expression.exception_context_ids ?? []
  };
}

function withContextSelection(
  expression: GalleryContextSelectionExpression,
  selection: SelectionState
): GalleryContextSelectionExpression {
  return {
    kind: expression.kind,
    query: expression.query ?? '',
    base_selected: selection.base_selected,
    exception_context_ids: selection.exception_ids
  };
}

function hasContextSelection(
  expression: GalleryContextSelectionExpression
): boolean {
  return (
    (expression.base_selected ?? false) ||
    (expression.exception_context_ids?.length ?? 0) > 0
  );
}

function selectionQuery(query: GallerySearch): GallerySearch {
  const selection = withoutCursor(query);
  delete selection.restrict_work_ids;
  return selection;
}

function withoutCursor(query: GallerySearch): GallerySearch {
  const result = structuredClone($state.snapshot(query));
  delete result.cursor;
  return result;
}

function withCursor(
  query: GallerySearch,
  cursor: NonNullable<GallerySearch['cursor']>
): GallerySearch {
  return {
    ...structuredClone($state.snapshot(query)),
    cursor: structuredClone($state.snapshot(cursor))
  };
}
