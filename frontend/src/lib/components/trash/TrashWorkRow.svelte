<script lang="ts">
  import type { TrashWork } from '$lib/api/trash';
  import Button from '$lib/components/ui/Button.svelte';
  import DateTimeField from '$lib/components/ui/DateTimeField.svelte';
  import IconLink from '$lib/components/ui/IconLink.svelte';
  import ReadableTime from '$lib/components/ui/ReadableTime.svelte';
  import { selectableRow } from '$lib/components/ui/row-selection';
  import StatusPill from '$lib/components/ui/StatusPill.svelte';
  import { formatBytes } from '$lib/format';
  import { galleryWorkPath } from '$lib/gallery-routes';
  import { openWorkDetail } from '$lib/stores/detail-navigation';

  interface Props {
    item: TrashWork;
    multiSelect: boolean;
    selected: boolean;
    schedule: string;
    busy: boolean;
    onSelectionChange: (checked: boolean) => void;
    onScheduleChange: (value: string) => void;
    onRestore: () => void;
    onSaveSchedule: () => void;
    onPurge: (returnFocus: HTMLElement) => void;
    onOpenDetail?: () => void;
  }

  let {
    item,
    multiSelect,
    selected,
    schedule,
    busy,
    onSelectionChange,
    onScheduleChange,
    onRestore,
    onSaveSchedule,
    onPurge,
    onOpenDetail
  }: Props = $props();
  let selectionDisabled = $derived(busy || item.purge_state === 'running');

  async function openDetail(event: MouseEvent): Promise<void> {
    if (
      event.button !== 0 ||
      event.ctrlKey ||
      event.metaKey ||
      event.shiftKey ||
      event.altKey
    ) {
      return;
    }
    event.preventDefault();
    onOpenDetail?.();
    await openWorkDetail(galleryWorkPath(item.pixiv_work_id), {
      kind: 'trash',
      route: '/system/trash'
    });
  }
</script>

<article
  class:multi-select={multiSelect}
  class:selected={multiSelect && selected}
  class="trash-work"
  data-trash-anchor={item.work_id}
  use:selectableRow={{
    enabled: multiSelect && !selectionDisabled,
    onToggle: () => onSelectionChange(!selected)
  }}
>
  {#if multiSelect}
    <label class="selection">
      <input
        type="checkbox"
        disabled={selectionDisabled}
        checked={selected}
        onchange={(event) => onSelectionChange(event.currentTarget.checked)}
        aria-label={`选择${item.title}`}
      />
    </label>
  {/if}
  <div class="trash-copy">
    <div>
      <strong>{item.title}</strong>
      <span>{item.artist_name} · Pixiv {item.pixiv_work_id}</span>
    </div>
    <div class="trash-meta">
      <span>{item.page_count}页</span>
      <span>{formatBytes(item.estimated_release_bytes)}</span>
      <span>移入 <ReadableTime value={item.trashed_at} /></span>
      {#if item.purge_state === 'running'}
        <StatusPill label="正在清理" tone="primary" />
      {:else if item.purge_state === 'failed'}
        <StatusPill label="清理失败" tone="error" />
      {/if}
      {#if item.failure_message}
        <span class="failure">{item.failure_message}</span>
      {/if}
    </div>
  </div>
  <div class="work-controls">
    <IconLink
      href={galleryWorkPath(item.pixiv_work_id)}
      label={`查看${item.title}详情`}
      icon="arrow-right"
      onclick={(event) => void openDetail(event)}
    />
    <div class="schedule-field">
      <span>计划清理时间</span>
      <DateTimeField
        disabled={busy || !item.capabilities.can_reschedule}
        ariaLabel="计划清理时间"
        value={schedule}
        onChange={onScheduleChange}
      />
    </div>
    <div class="row-actions">
      <Button
        disabled={busy || !item.capabilities.can_restore}
        onclick={onRestore}>恢复</Button
      >
      <Button
        disabled={busy || !item.capabilities.can_reschedule}
        onclick={onSaveSchedule}>保存日期</Button
      >
      <Button
        variant="danger"
        disabled={busy || item.purge_state === 'running'}
        onclick={(event) => onPurge(event.currentTarget)}>立即清理</Button
      >
    </div>
  </div>
</article>

<style>
  .trash-work {
    display: grid;
    grid-template-columns: minmax(220px, 1fr) auto;
    gap: 1rem;
    align-items: center;
    padding: 0.9rem 1rem;
    border-bottom: 1px solid var(--color-border);
  }

  .trash-work.multi-select {
    grid-template-columns: auto minmax(220px, 1fr) auto;
    cursor: pointer;
  }

  .trash-work.selected {
    background: var(--color-primary-soft);
  }

  .trash-work:last-child {
    border-bottom: 0;
  }

  .selection input {
    width: 17px;
    height: 17px;
    accent-color: var(--color-primary);
  }

  .trash-copy,
  .trash-copy > div:first-child {
    display: grid;
    gap: 0.25rem;
  }

  .trash-copy strong {
    font-size: 0.8rem;
  }

  .trash-copy span,
  .trash-meta {
    color: var(--color-text-3);
    font-size: 0.67rem;
  }

  .trash-meta :global(.status-pill) {
    min-height: 22px;
    padding: 0.1rem 0.5rem;
    font-size: 0.67rem;
  }

  .trash-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 0.65rem;
  }

  .trash-meta .failure {
    color: var(--color-error);
  }

  .schedule-field {
    display: grid;
    gap: 0.25rem;
    color: var(--color-text-3);
    font-size: 0.68rem;
  }

  .work-controls {
    display: flex;
    align-items: end;
    gap: 0.45rem;
  }

  .row-actions {
    display: flex;
    gap: 0.45rem;
  }

  .row-actions :global(button) {
    height: var(--control-height-md);
  }

  @media (max-width: 1080px) {
    .trash-work {
      grid-template-columns: 1fr;
    }

    .trash-work.multi-select {
      grid-template-columns: auto 1fr;
    }

    .schedule-field,
    .work-controls,
    .row-actions {
      grid-column: 1;
    }

    .multi-select .schedule-field,
    .multi-select .work-controls,
    .multi-select .row-actions {
      grid-column: 2;
    }
  }

  @media (max-width: 680px) {
    .trash-work {
      grid-template-columns: 1fr;
    }

    .selection,
    .schedule-field,
    .work-controls,
    .row-actions {
      grid-column: auto;
    }

    .row-actions {
      flex-wrap: wrap;
    }

    .work-controls {
      flex-wrap: wrap;
    }
  }
</style>
