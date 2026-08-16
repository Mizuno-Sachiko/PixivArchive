<script lang="ts">
  import type { RuleWorkbenchStore } from '$lib/stores/rule-workbench.svelte';

  import RuleOverlayPanel from './RuleOverlayPanel.svelte';

  interface Props {
    store: RuleWorkbenchStore;
    onClose: () => void;
  }

  let { store, onClose }: Props = $props();
  let ruleName = $state('');

  async function createRule(): Promise<void> {
    if (await store.createRule(ruleName)) onClose();
  }
</script>

<RuleOverlayPanel title="新建规则" closeLabel="关闭新建规则" {onClose}>
  <label class="field">
    <span>规则名称</span>
    <input
      bind:value={ruleName}
      aria-label="规则名称"
      autocomplete="off"
      disabled={store.creatingRule}
      oninput={() => (store.createRuleError = '')}
      onkeydown={(event) => {
        if (event.key === 'Enter') void createRule();
      }}
    />
  </label>
  {#if store.createRuleError}
    <p class="notice error" role="alert">{store.createRuleError}</p>
  {/if}
  <button
    class="primary"
    type="button"
    disabled={store.creatingRule}
    onclick={createRule}
  >
    {store.creatingRule ? '正在创建' : '创建规则'}
  </button>
</RuleOverlayPanel>

<style>
  .notice {
    margin: 0;
    padding: 0.65rem;
    border-radius: 7px;
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
