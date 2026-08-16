<script lang="ts">
  import type { Snippet } from 'svelte';

  import Button from '$lib/components/ui/Button.svelte';
  import Field from '$lib/components/ui/Field.svelte';
  import ReadableTime from '$lib/components/ui/ReadableTime.svelte';
  import SelectField from '$lib/components/ui/SelectField.svelte';

  const intervalOptions = [
    { value: '15', label: '15分钟' },
    { value: '30', label: '30分钟' },
    { value: '60', label: '1小时' },
    { value: '180', label: '3小时' },
    { value: '360', label: '6小时' },
    { value: '720', label: '12小时' },
    { value: '1440', label: '24小时' }
  ] as const;

  interface Props {
    intervalMinutes?: string;
    intervalAriaLabel: string;
    disabled?: boolean;
    runBusy?: boolean;
    lastFullReconciledAt?: string | null;
    primary: Snippet;
    feedback?: Snippet;
    trailing?: Snippet;
    onIntervalChange: () => void;
    onRunFull: () => void;
  }

  let {
    intervalMinutes = $bindable('15'),
    intervalAriaLabel,
    disabled = false,
    runBusy = false,
    lastFullReconciledAt = null,
    primary,
    feedback,
    trailing,
    onIntervalChange,
    onRunFull
  }: Props = $props();
</script>

<div class="subscription-sync-controls">
  <div class="sync-primary">
    {@render primary()}
    {#if feedback}{@render feedback()}{/if}
  </div>
  <Field class="sync-interval" label="自动同步间隔">
    <SelectField
      bind:value={intervalMinutes}
      ariaLabel={intervalAriaLabel}
      {disabled}
      options={intervalOptions}
      onChange={onIntervalChange}
    />
  </Field>
  <div class="sync-command">
    <Button size="compact" disabled={disabled || runBusy} onclick={onRunFull}
      >完整同步</Button
    >
    {#if trailing}{@render trailing()}{/if}
    {#if lastFullReconciledAt}
      <small>上次完整核对：<ReadableTime value={lastFullReconciledAt} /></small>
    {/if}
  </div>
</div>

<style>
  .subscription-sync-controls {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(150px, 180px) auto;
    align-items: end;
    gap: 0.8rem;
    min-width: 0;
    width: 100%;
  }

  .sync-primary,
  .sync-command {
    display: flex;
    min-width: 0;
    min-height: var(--control-height-md);
    align-items: center;
  }

  .sync-primary {
    flex-wrap: nowrap;
    gap: 0.55rem;
  }

  .sync-primary :global(.settings-feedback),
  .sync-primary :global(.inline-message) {
    min-width: 0;
  }

  .sync-command {
    flex-wrap: nowrap;
    justify-content: flex-start;
    gap: 0.55rem 0.75rem;
  }

  .sync-command small {
    flex: 0 0 auto;
  }

  small {
    color: var(--color-text-3);
    font-size: 0.72rem;
    line-height: 1.5;
    white-space: nowrap;
  }

  @media (max-width: 900px) {
    .subscription-sync-controls {
      grid-template-columns: 1fr;
    }

    .sync-command {
      flex-wrap: wrap;
      justify-content: flex-start;
    }

    .sync-command small {
      flex: 1 0 100%;
    }
  }

  @media (max-width: 560px) {
    .subscription-sync-controls {
      grid-template-columns: 1fr;
    }

    .sync-primary {
      flex-wrap: wrap;
    }

    .sync-command :global(button) {
      flex: 1 1 auto;
    }
  }
</style>
