<script lang="ts">
  import type { TrashPurgeState } from '$lib/api/trash';
  import Button from '$lib/components/ui/Button.svelte';
  import Field from '$lib/components/ui/Field.svelte';
  import SelectField from '$lib/components/ui/SelectField.svelte';
  import SearchField from '$lib/components/ui/SearchField.svelte';

  interface Props {
    query: string;
    purgeState: TrashPurgeState | '';
    disabled: boolean;
    onApply: () => void;
    onPurgeStateChange: (value: TrashPurgeState | '') => void;
  }

  let {
    query = $bindable(),
    purgeState,
    disabled,
    onApply,
    onPurgeStateChange
  }: Props = $props();
</script>

<form
  class="trash-filters"
  onsubmit={(event) => {
    event.preventDefault();
    onApply();
  }}
>
  <SearchField
    class="trash-search"
    label="作品筛选"
    labelHidden
    placeholder="标题、作者或Pixiv ID"
    bind:value={query}
    {disabled}
  />
  <Field label="清理状态" labelHidden class="purge-state-filter">
    <SelectField
      value={purgeState}
      ariaLabel="清理状态"
      placeholder="全部状态"
      {disabled}
      options={[
        { value: 'pending', label: '等待清理' },
        { value: 'running', label: '正在清理' },
        { value: 'failed', label: '清理失败' }
      ]}
      onChange={(value) => onPurgeStateChange(value as TrashPurgeState | '')}
    />
  </Field>
  <Button type="submit" {disabled}>筛选</Button>
</form>

<style>
  .trash-filters {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 0.55rem;
  }

  .trash-filters :global(.trash-search input) {
    width: min(260px, 28vw);
    min-width: 180px;
    min-height: var(--control-height-md);
  }

  .trash-filters :global(.purge-state-filter .pa-select-trigger) {
    width: min(170px, 20vw);
    min-width: 140px;
    min-height: var(--control-height-md);
  }

  @media (max-width: 680px) {
    .trash-filters {
      align-items: stretch;
      flex-direction: column;
    }

    .trash-filters :global(.trash-search input) {
      min-width: 0;
      width: 100%;
    }

    .trash-filters :global(.purge-state-filter .pa-select-trigger) {
      width: 100%;
      min-width: 0;
    }
  }
</style>
