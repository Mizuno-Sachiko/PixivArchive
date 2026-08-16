<script lang="ts">
  import StatusPill from './StatusPill.svelte';

  type StatusTone = 'neutral' | 'success' | 'warning' | 'error' | 'primary';

  interface StatusValue {
    kind: 'status';
    label: string;
    tone?: StatusTone;
  }

  interface Metric {
    label: string;
    value: string | StatusValue;
    title?: string;
    loading?: boolean;
    valueSize?: 'headline' | 'standard' | 'compact';
  }

  interface Props {
    items: readonly Metric[];
    appearance?: 'default' | 'overview';
    class?: string;
  }

  let {
    items,
    appearance = 'default',
    class: className = ''
  }: Props = $props();
</script>

<div
  class={`metric-strip metric-strip-${appearance} metric-count-${items.length} ${className}`.trim()}
  style:--metric-columns={items.length}
>
  {#each items as item (item.label)}
    <div class:metric-status={typeof item.value !== 'string'}>
      <span>{item.label}</span>
      {#if item.loading}
        <strong class="metric-loading" aria-label="正在读取"></strong>
      {:else if typeof item.value !== 'string'}
        <StatusPill label={item.value.label} tone={item.value.tone} />
      {:else}
        <strong
          class={`metric-value-${item.valueSize ?? 'headline'}`}
          title={item.title}>{item.value}</strong
        >
      {/if}
    </div>
  {/each}
</div>

<style>
  .metric-strip {
    display: grid;
    grid-template-columns: repeat(var(--metric-columns), minmax(0, 1fr));
    overflow: hidden;
    border: 1px solid var(--color-border);
    border-top: 3px solid var(--color-primary);
    border-radius: var(--radius-md);
    background: var(--color-surface-1);
  }

  .metric-strip > div {
    display: grid;
    min-height: 88px;
    align-content: center;
    gap: 0.15rem;
    padding: 1rem 1.2rem;
    border-right: 1px solid var(--color-border);
  }

  .metric-strip > div:last-child {
    border-right: 0;
  }

  .metric-strip span {
    color: var(--color-text-3);
    font-size: 0.72rem;
  }

  .metric-status {
    gap: 0.75rem;
    justify-items: start;
  }

  .metric-value-headline {
    font-size: 1.65rem;
    letter-spacing: -0.04em;
  }

  .metric-value-standard {
    font-size: 1.15rem;
    letter-spacing: 0;
  }

  .metric-value-compact {
    font-size: 0.92rem;
    letter-spacing: 0;
  }

  .metric-loading {
    display: block;
    width: min(5.5rem, 72%);
    height: 1.55rem;
    border-radius: 6px;
    background: var(--color-surface-3);
  }

  .metric-strip-overview > div {
    min-height: 105px;
    padding: 1.1rem 1.35rem;
  }

  .metric-strip-overview > div > span:first-child {
    font-size: 0.75rem;
    font-weight: 630;
  }

  .metric-strip-overview .metric-value-headline {
    margin-top: 0.2rem;
    font-size: 1.9rem;
    font-weight: 720;
  }

  @media (max-width: 980px) {
    .metric-strip {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }

    .metric-strip > div {
      border-bottom: 1px solid var(--color-border);
    }

    .metric-strip > div:nth-child(2n) {
      border-right: 0;
    }

    .metric-strip.metric-count-3 > div:last-child {
      grid-column: 1 / -1;
      border-top: 1px solid var(--color-border);
      border-right: 0;
      border-bottom: 0;
    }

    .metric-strip.metric-count-3 > div:nth-child(-n + 2) {
      border-bottom: 0;
    }

    .metric-strip.metric-count-4 > div:nth-last-child(-n + 2) {
      border-bottom: 0;
    }
  }

  @media (max-width: 620px) {
    .metric-strip-overview > div {
      min-height: 90px;
      padding: 0.95rem;
    }
  }
</style>
