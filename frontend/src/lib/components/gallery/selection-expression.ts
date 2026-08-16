export interface SelectionState {
  base_selected: boolean;
  exception_ids: string[];
}

export class SelectionProjectionCoordinator {
  private revision = 0;
  private latest: Promise<boolean> = Promise.resolve(true);

  project<Result>(
    request: () => Promise<Result>,
    onSuccess: (result: Result) => void,
    onFailure: () => void
  ): Promise<boolean> {
    const revision = ++this.revision;
    const operation = (async () => {
      try {
        const result = await request();
        if (revision !== this.revision) return false;
        onSuccess(result);
        return true;
      } catch {
        if (revision !== this.revision) return false;
        onFailure();
        return false;
      }
    })();
    this.latest = operation;
    return operation;
  }

  async waitForLatest(): Promise<boolean> {
    while (true) {
      const operation = this.latest;
      const succeeded = await operation;
      if (operation === this.latest) return succeeded;
    }
  }

  invalidate(): void {
    this.revision += 1;
    this.latest = Promise.resolve(false);
  }
}

export function isSelected(state: SelectionState, id: string): boolean {
  return state.base_selected !== state.exception_ids.includes(id);
}

export function hasSelection(state: SelectionState): boolean {
  return state.base_selected || state.exception_ids.length > 0;
}

export function setSelected(
  state: SelectionState,
  id: string,
  selected: boolean
): SelectionState {
  const exceptions = new Set(state.exception_ids);
  if (selected === state.base_selected) exceptions.delete(id);
  else exceptions.add(id);
  return {
    base_selected: state.base_selected,
    exception_ids: [...exceptions]
  };
}

export function selectAll(): SelectionState {
  return { base_selected: true, exception_ids: [] };
}

export function clearSelection(): SelectionState {
  return { base_selected: false, exception_ids: [] };
}

export function invertSelection(state: SelectionState): SelectionState {
  return {
    base_selected: !state.base_selected,
    exception_ids: [...state.exception_ids]
  };
}
