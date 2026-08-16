<script lang="ts">
  import { onMount } from 'svelte';

  import { activateModal, trapModalFocus } from '$lib/modal-focus';

  interface Props {
    title: string;
    description: string;
    confirmLabel?: string;
    cancelLabel?: string;
    busy?: boolean;
    error?: string;
    tone?: 'primary' | 'danger';
    returnFocus?: HTMLElement | null;
    onConfirm: () => void;
    onCancel: () => void;
  }

  let {
    title,
    description,
    confirmLabel = '确认',
    cancelLabel = '取消',
    busy = false,
    error = '',
    tone = 'primary',
    returnFocus,
    onConfirm,
    onCancel
  }: Props = $props();
  let dialog = $state<HTMLDialogElement>();
  let cancelButton = $state<HTMLButtonElement>();

  onMount(() => {
    const mountedDialog = dialog;
    if (!mountedDialog) return;
    return activateModal(
      mountedDialog,
      cancelButton ?? mountedDialog,
      returnFocus
    );
  });

  function handleCancel(event: Event): void {
    event.preventDefault();
    if (busy) return;
    onCancel();
  }

  function handleBackdrop(event: MouseEvent): void {
    if (event.target === event.currentTarget && !busy) onCancel();
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (dialog) trapModalFocus(dialog, event);
  }
</script>

<dialog
  bind:this={dialog}
  class="dialog-backdrop"
  aria-labelledby="confirm-dialog-title"
  aria-describedby="confirm-dialog-description"
  oncancel={handleCancel}
  onkeydown={handleKeydown}
  onclick={handleBackdrop}
>
  <div class="confirm-dialog">
    <div
      class="dialog-mark"
      class:danger={tone === 'danger'}
      aria-hidden="true"
    >
      !
    </div>
    <div class="dialog-copy">
      <h2 id="confirm-dialog-title">{title}</h2>
      <p id="confirm-dialog-description">{description}</p>
      {#if error}
        <p class="dialog-error" role="alert">{error}</p>
      {/if}
    </div>
    <div class="dialog-actions">
      <button
        bind:this={cancelButton}
        class="secondary-button"
        type="button"
        disabled={busy}
        onclick={onCancel}>{cancelLabel}</button
      >
      <button
        class:danger-button={tone === 'danger'}
        class:primary-button={tone === 'primary'}
        type="button"
        disabled={busy}
        onclick={onConfirm}
      >
        {busy ? '正在处理' : confirmLabel}
      </button>
    </div>
  </div>
</dialog>

<style>
  .dialog-backdrop {
    position: fixed;
    z-index: 90;
    inset: 0;
    display: none;
    width: 100vw;
    max-width: none;
    height: 100vh;
    max-height: none;
    margin: 0;
    place-items: center;
    padding: 1rem;
    border: 0;
    background: rgba(3, 9, 16, 0.58);
    backdrop-filter: blur(10px) saturate(0.9);
  }

  .dialog-backdrop[open] {
    display: grid;
  }

  .dialog-backdrop::backdrop {
    background: transparent;
  }

  .confirm-dialog {
    display: grid;
    width: min(430px, 100%);
    grid-template-columns: 42px minmax(0, 1fr);
    gap: 0.85rem 1rem;
    padding: 1.15rem;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-glass-strong);
    box-shadow: var(--shadow-float);
    backdrop-filter: blur(24px) saturate(1.15);
  }

  .dialog-mark {
    display: grid;
    width: 42px;
    height: 42px;
    place-content: center;
    border-radius: 50%;
    background: var(--color-primary-soft);
    color: var(--color-primary);
    font-size: 1rem;
    font-weight: 800;
  }

  .dialog-mark.danger {
    background: var(--color-error-soft);
    color: var(--color-error);
  }

  .dialog-copy {
    min-width: 0;
  }

  h2,
  p {
    margin: 0;
  }

  h2 {
    color: var(--color-text-1);
    font-size: 1rem;
  }

  .dialog-copy > p {
    margin-top: 0.45rem;
    color: var(--color-text-2);
    font-size: 0.78rem;
    line-height: 1.65;
  }

  .dialog-copy .dialog-error {
    padding: 0.55rem 0.65rem;
    border-radius: var(--radius-sm);
    background: var(--color-error-soft);
    color: var(--color-error);
  }

  .dialog-actions {
    display: flex;
    grid-column: 1 / -1;
    justify-content: end;
    gap: 0.55rem;
    padding-top: 0.15rem;
  }

  .dialog-actions button {
    min-width: 88px;
  }
</style>
