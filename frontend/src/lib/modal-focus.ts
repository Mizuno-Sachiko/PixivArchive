const focusableSelector = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])'
].join(',');

export function activateModal(
  dialog: HTMLDialogElement,
  initialFocus: HTMLElement = dialog,
  returnFocus?: HTMLElement | null
): () => void {
  const previouslyFocused =
    returnFocus === undefined
      ? document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null
      : returnFocus;
  dialog.showModal();
  initialFocus.focus();

  return () => {
    if (dialog.open) dialog.close();
    queueMicrotask(() => {
      if (previouslyFocused?.isConnected) previouslyFocused.focus();
    });
  };
}

export function trapModalFocus(
  dialog: HTMLDialogElement,
  event: KeyboardEvent
): void {
  if (event.key !== 'Tab') return;
  const focusable = Array.from(
    dialog.querySelectorAll<HTMLElement>(focusableSelector)
  ).filter((element) => element.getClientRects().length > 0);
  const first = focusable[0];
  const last = focusable.at(-1);
  if (!first || !last) {
    event.preventDefault();
    dialog.focus();
    return;
  }

  const active = document.activeElement;
  if (event.shiftKey && (active === first || !dialog.contains(active))) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && (active === last || !dialog.contains(active))) {
    event.preventDefault();
    first.focus();
  }
}
