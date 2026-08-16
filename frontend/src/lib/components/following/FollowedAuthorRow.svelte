<script lang="ts">
  import type { FollowingAuthor } from '$lib/api/following';
  import IdentityCell from '$lib/components/ui/IdentityCell.svelte';
  import PixivSourceLink from '$lib/components/ui/PixivSourceLink.svelte';
  import ReadableTime from '$lib/components/ui/ReadableTime.svelte';
  import { selectableRow } from '$lib/components/ui/row-selection';

  interface Props {
    author: FollowingAuthor;
    selectionMode: boolean;
    selected: boolean;
    disabled: boolean;
    onToggleSelection: () => void;
    onEnabledChange: (enabled: boolean) => void;
  }

  let {
    author,
    selectionMode,
    selected,
    disabled,
    onToggleSelection,
    onEnabledChange
  }: Props = $props();
</script>

<li
  class:selectable={selectionMode}
  class:selected
  class="author-row"
  use:selectableRow={{
    enabled: selectionMode,
    onToggle: onToggleSelection
  }}
>
  {#if selectionMode}
    <label class="author-selection">
      <input
        type="checkbox"
        aria-label={`选择${author.display_name}`}
        checked={selected}
        onchange={onToggleSelection}
      />
    </label>
  {/if}
  <IdentityCell
    class="author-identity"
    src={author.avatar_url}
    title={author.display_name}
    subtitle={`Pixiv ID ${author.pixiv_artist_id}`}
    size={42}
    alt={author.display_name}
    fallbackText={author.display_name}
    layout="stacked"
  />
  <span class="visibility">
    {author.visibility === 'private' ? '私密关注' : '公开关注'}
  </span>
  <label class="author-toggle">
    <input
      type="checkbox"
      aria-label={`采集${author.display_name}`}
      checked={author.enabled}
      {disabled}
      onchange={(event) => onEnabledChange(event.currentTarget.checked)}
    />
    <span>采集</span>
  </label>
  <div class="author-time">
    <PixivSourceLink
      href={`https://www.pixiv.net/users/${author.pixiv_artist_id}`}
      label={`在Pixiv打开${author.display_name}`}
    />
    <span class="author-collected-time">
      <ReadableTime value={author.last_collected_at} empty="尚未抓取" />
    </span>
  </div>
</li>

<style>
  .author-row {
    display: grid;
    grid-template-columns:
      minmax(220px, 1fr) 92px minmax(64px, auto)
      minmax(124px, auto);
    align-items: center;
    gap: 1rem;
    min-width: 0;
    padding: 0.8rem 1rem;
    border-bottom: 1px solid var(--color-border);
  }

  .author-row.selectable {
    grid-template-columns:
      auto minmax(220px, 1fr) 92px minmax(64px, auto)
      minmax(124px, auto);
    cursor: pointer;
  }

  .author-row.selected {
    background: var(--color-primary-soft);
  }

  .author-selection {
    display: grid;
    place-items: center;
  }

  .author-row:last-child {
    overflow: hidden;
    border-radius: 0 0 var(--radius-md) var(--radius-md);
    border-bottom: 0;
  }

  .author-toggle {
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    color: var(--color-text-2);
    font-size: 0.76rem;
    font-weight: 680;
  }

  input[type='checkbox'] {
    width: 16px;
    height: 16px;
    margin: 0;
    accent-color: var(--color-primary);
  }

  .visibility,
  .author-collected-time {
    color: var(--color-text-3);
    font-size: 0.72rem;
  }

  .author-time {
    display: inline-flex;
    align-items: center;
    gap: 0.55rem;
    justify-self: end;
  }

  .author-collected-time {
    white-space: nowrap;
  }

  @media (max-width: 720px) {
    .author-row {
      grid-template-columns: minmax(0, 1fr) auto;
      gap: 0.65rem 1rem;
    }

    .author-row.selectable {
      grid-template-columns: auto minmax(0, 1fr) auto;
    }

    .author-row.selectable :global(.author-identity) {
      grid-column: 2;
    }

    .author-row.selectable .visibility {
      grid-column: 3;
    }

    .visibility {
      justify-self: end;
    }

    .author-time {
      justify-self: start;
      grid-column: 1;
      padding-left: 54px;
    }

    .author-row.selectable .author-time {
      grid-column: 2;
    }

    .author-toggle {
      grid-column: 2;
      grid-row: 2;
      justify-self: end;
    }

    .author-row.selectable .author-toggle {
      grid-column: 3;
    }
  }
</style>
