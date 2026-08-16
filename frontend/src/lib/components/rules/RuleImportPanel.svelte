<script lang="ts">
  import type { RuleWorkbenchStore } from '$lib/stores/rule-workbench.svelte';

  import RuleOverlayPanel from './RuleOverlayPanel.svelte';

  interface Props {
    store: RuleWorkbenchStore;
    initialSource: string;
    onClose: () => void;
  }

  let { store, initialSource, onClose }: Props = $props();
  let importSource = $derived(initialSource);

  async function importRule(): Promise<void> {
    if (await store.importJson(importSource)) {
      importSource = JSON.stringify(store.document, null, 2);
    }
  }
</script>

<RuleOverlayPanel title="导入规则JSON" closeLabel="关闭导入" {onClose}>
  <label>
    <span>规则JSON</span>
    <textarea
      bind:value={importSource}
      aria-label="规则JSON"
      rows="14"
      disabled={store.importLoading}></textarea>
  </label>
  {#if store.importError}
    <p class="notice error" role="alert">{store.importError}</p>
  {/if}
  {#if store.importNotice}
    <p class="notice">{store.importNotice}</p>
  {/if}
  <button
    class="primary"
    type="button"
    disabled={store.importLoading}
    onclick={importRule}
  >
    {store.importLoading ? '正在导入' : '验证并导入'}
  </button>
</RuleOverlayPanel>

<style>
  label {
    display: grid;
    gap: 0.4rem;
  }

  label span {
    color: var(--color-text-2);
    font-size: 0.72rem;
    font-weight: 650;
  }

  textarea {
    width: 100%;
    min-height: 290px;
    padding: 0.7rem;
    resize: vertical;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    outline: none;
    background: var(--color-surface-2);
    color: var(--color-text-1);
    font: 0.68rem/1.5 var(--font-mono);
  }

  textarea:focus {
    border-color: var(--color-primary);
    box-shadow: 0 0 0 2px var(--color-primary-soft);
  }

  .notice {
    margin: 0;
    padding: 0.65rem;
    border-radius: 7px;
    background: var(--color-primary-soft);
    color: var(--color-primary);
    font-size: 0.72rem;
  }

  .notice.error {
    background: var(--color-error-soft);
    color: var(--color-error);
  }

  .primary {
    height: 38px;
    border-radius: 8px;
    background: var(--color-primary);
    color: #fff;
    font-size: 0.76rem;
    font-weight: 680;
  }
</style>
