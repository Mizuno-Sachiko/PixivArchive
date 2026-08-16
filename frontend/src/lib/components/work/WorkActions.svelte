<script lang="ts">
  import Button from '$lib/components/ui/Button.svelte';

  interface Props {
    trashed: boolean;
    canRestore: boolean;
    canBookmark: boolean;
    bookmarkDisabled: boolean;
    bookmarked: boolean;
    busy: string;
    notice: string;
    error: string;
    onToggleBookmark: () => void;
    onMoveToTrash: () => void;
    onRestore: () => void;
    onPurge: (returnFocus: HTMLElement) => void;
  }

  let {
    trashed,
    canRestore,
    canBookmark,
    bookmarkDisabled,
    bookmarked,
    busy,
    notice,
    error,
    onToggleBookmark,
    onMoveToTrash,
    onRestore,
    onPurge
  }: Props = $props();

  function requestPurge(event: MouseEvent): void {
    onPurge(event.currentTarget as HTMLElement);
  }
</script>

<div class="work-actions">
  {#if canBookmark}
    <Button
      variant="primary"
      disabled={bookmarkDisabled || busy === 'bookmark'}
      onclick={onToggleBookmark}>{bookmarked ? '取消收藏' : '收藏'}</Button
    >
  {/if}
  {#if trashed}
    {#if canRestore}
      <Button disabled={busy === 'restore'} onclick={onRestore}
        >移出回收站</Button
      >
    {/if}
    <Button variant="danger" disabled={busy === 'purge'} onclick={requestPurge}
      >立即清理</Button
    >
  {:else}
    <Button variant="danger" disabled={busy === 'trash'} onclick={onMoveToTrash}
      >移入回收站</Button
    >
  {/if}
</div>

{#if notice}<p class="inline-message success">{notice}</p>{/if}
{#if error}<p class="inline-message error" role="alert">{error}</p>{/if}

<style>
  .work-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.55rem;
  }

  .work-actions :global(button) {
    height: 36px;
  }
</style>
