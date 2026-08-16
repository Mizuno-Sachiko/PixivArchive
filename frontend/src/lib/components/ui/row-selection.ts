interface ClosestTarget {
  closest(selector: string): unknown;
}

interface SelectableRowOptions {
  enabled: boolean;
  onToggle: () => void;
}

const ROW_SELECTION_CONTROL =
  "a, button, input, select, textarea, label, [role='button'], [role='link'], [data-row-selection-control]";

export function isRowSelectionControl(target: unknown): boolean {
  if (!hasClosest(target)) return false;
  return Boolean(target.closest(ROW_SELECTION_CONTROL));
}

export function selectableRow(
  node: HTMLElement,
  options: SelectableRowOptions
): { update(next: SelectableRowOptions): void; destroy(): void } {
  let current = options;
  const handleClick = (event: MouseEvent) => {
    if (!current.enabled || isRowSelectionControl(event.target)) return;
    current.onToggle();
  };
  node.addEventListener('click', handleClick);
  return {
    update(next) {
      current = next;
    },
    destroy() {
      node.removeEventListener('click', handleClick);
    }
  };
}

function hasClosest(target: unknown): target is ClosestTarget {
  return Boolean(
    target &&
    typeof target === 'object' &&
    'closest' in target &&
    typeof target.closest === 'function'
  );
}
