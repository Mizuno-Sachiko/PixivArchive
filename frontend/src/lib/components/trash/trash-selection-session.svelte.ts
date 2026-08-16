import { SvelteSet } from 'svelte/reactivity';

import {
  projectTrashSelection,
  type TrashFilter,
  type TrashSelectionExpression,
  type TrashSelectionProjection,
  type TrashSelectionRequest
} from '$lib/api/trash';
import {
  clearSelection,
  hasSelection,
  isSelected,
  invertSelection,
  SelectionProjectionCoordinator,
  selectAll,
  setSelected,
  type SelectionState
} from '$lib/components/gallery/selection-expression';

interface TrashSelectionGateway {
  project(request: TrashSelectionRequest): Promise<TrashSelectionProjection>;
}

const defaultGateway: TrashSelectionGateway = {
  project: projectTrashSelection
};

export class TrashSelectionSession {
  mode = $state(false);
  count = $state(0);
  blockedCount = $state(0);
  error = $state('');

  private expression = $state<TrashSelectionExpression>(emptyExpression());
  private readonly visibleSelected = new SvelteSet<string>();
  private readonly projectionCoordinator = new SelectionProjectionCoordinator();

  constructor(
    private readonly gateway: TrashSelectionGateway = defaultGateway
  ) {}

  enter(filter: TrashFilter): void {
    this.invalidateOperation();
    this.mode = true;
    this.expression = {
      filter: structuredClone($state.snapshot(filter)),
      base_selected: false,
      exception_work_ids: []
    };
    this.resetProjection();
    this.error = '';
  }

  exit(): void {
    this.invalidateOperation();
    this.mode = false;
    this.expression = emptyExpression();
    this.resetProjection();
    this.error = '';
  }

  clear(): void {
    this.projectionCoordinator.invalidate();
    this.expression = withSelection(this.expression, clearSelection());
    this.resetProjection();
    this.error = '';
  }

  idsFor(visibleWorkIds: string[]): SvelteSet<string> {
    return new SvelteSet(
      visibleWorkIds.filter((workId) => this.visibleSelected.has(workId))
    );
  }

  snapshotExpression(): TrashSelectionExpression {
    return structuredClone($state.snapshot(this.expression));
  }

  async setWork(
    workId: string,
    selected: boolean,
    visibleWorkIds: string[]
  ): Promise<void> {
    const current = selectionState(this.expression);
    const wasSelected = isSelected(current, workId);
    const next = setSelected(current, workId, selected);
    await this.project(
      withSelection(this.expression, next),
      visibleWorkIds,
      wasSelected === selected ? this.count : this.count + (selected ? 1 : -1)
    );
  }

  async selectAll(visibleWorkIds: string[]): Promise<void> {
    await this.project(
      withSelection(this.expression, selectAll()),
      visibleWorkIds,
      undefined
    );
  }

  async invert(visibleWorkIds: string[]): Promise<void> {
    await this.project(
      withSelection(
        this.expression,
        invertSelection(selectionState(this.expression))
      ),
      visibleWorkIds,
      undefined
    );
  }

  async refreshVisible(visibleWorkIds: string[]): Promise<void> {
    if (!this.mode) return;
    const latestSucceeded = await this.projectionCoordinator.waitForLatest();
    if (!latestSucceeded || !this.mode) return;
    if (!hasSelection(selectionState(this.expression))) {
      this.resetProjection();
      return;
    }
    await this.project(
      this.snapshotExpression(),
      visibleWorkIds,
      undefined,
      false
    );
  }

  private async project(
    expression: TrashSelectionExpression,
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
      () =>
        this.gateway.project({
          expression,
          visible_work_ids: [...visibleWorkIds]
        }),
      (projection) => {
        if (!this.mode) return;
        this.expression = expression;
        this.count = projection.selected_count;
        this.blockedCount = projection.blocked_count;
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

  private resetProjection(): void {
    this.count = 0;
    this.blockedCount = 0;
    this.visibleSelected.clear();
  }

  private applyVisibleIntent(
    expression: TrashSelectionExpression,
    visibleWorkIds: string[]
  ): void {
    const selection = selectionState(expression);
    this.visibleSelected.clear();
    for (const workId of visibleWorkIds) {
      if (isSelected(selection, workId)) this.visibleSelected.add(workId);
    }
  }

  private projectionSnapshot(): TrashSelectionSnapshot {
    return {
      expression: this.snapshotExpression(),
      count: this.count,
      blockedCount: this.blockedCount,
      visibleWorkIds: [...this.visibleSelected]
    };
  }

  private restoreProjection(snapshot: TrashSelectionSnapshot): void {
    this.expression = structuredClone(snapshot.expression);
    this.count = snapshot.count;
    this.blockedCount = snapshot.blockedCount;
    this.visibleSelected.clear();
    for (const workId of snapshot.visibleWorkIds) {
      this.visibleSelected.add(workId);
    }
  }

  private invalidateOperation(): void {
    this.projectionCoordinator.invalidate();
  }
}

interface TrashSelectionSnapshot {
  expression: TrashSelectionExpression;
  count: number;
  blockedCount: number;
  visibleWorkIds: string[];
}

function emptyExpression(): TrashSelectionExpression {
  return {
    filter: { query: null, purge_states: [] },
    base_selected: false,
    exception_work_ids: []
  };
}

function selectionState(expression: TrashSelectionExpression): SelectionState {
  return {
    base_selected: expression.base_selected ?? false,
    exception_ids: expression.exception_work_ids ?? []
  };
}

function withSelection(
  expression: TrashSelectionExpression,
  selection: SelectionState
): TrashSelectionExpression {
  const snapshot = $state.snapshot(expression);
  return {
    filter: structuredClone(snapshot.filter),
    base_selected: selection.base_selected,
    exception_work_ids: selection.exception_ids
  };
}
