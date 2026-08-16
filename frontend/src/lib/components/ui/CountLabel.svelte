<script lang="ts">
  import { formatCount, formatExactCount } from '$lib/format';

  interface Props {
    total: number;
    loaded: number;
    unit: string;
    loading?: boolean;
    loadingText?: string;
    variant?: 'status' | 'panel';
  }

  let {
    total,
    loaded,
    unit,
    loading = false,
    loadingText = '正在读取…',
    variant = 'status'
  }: Props = $props();

  let initialLoading = $derived(loading && total === 0 && loaded === 0);
  let title = $derived(
    `${formatExactCount(total)}${unit}${loaded < total ? `，已显示${formatExactCount(loaded)}${unit}` : ''}`
  );
</script>

<span class:panel-count={variant === 'panel'} {title}>
  {#if initialLoading}
    {loadingText}
  {:else}
    {formatCount(total)}{unit}{loaded < total
      ? `，已显示${formatCount(loaded)}${unit}`
      : ''}
  {/if}
</span>
