<script lang="ts">
  import Icon from '$lib/components/ui/Icon.svelte';
  import AccountAvatar from '$lib/components/ui/AccountAvatar.svelte';
  import WorkThumbnail from '$lib/components/ui/WorkThumbnail.svelte';

  import {
    globalSearchKindLabels,
    type GlobalSearchResult
  } from './global-search.svelte';

  interface Props {
    result: GlobalSearchResult;
    selected: boolean;
    onSelect: () => void;
    onChoose: () => void;
  }

  let { result, selected, onSelect, onChoose }: Props = $props();
  let row: HTMLButtonElement;

  $effect(() => {
    if (selected) row?.scrollIntoView({ block: 'nearest' });
  });
</script>

<button
  bind:this={row}
  id={`global-search-result-${result.key}`}
  type="button"
  role="option"
  tabindex="-1"
  aria-selected={selected}
  data-search-result-kind={result.kind}
  data-search-result-key={result.key}
  onpointerenter={onSelect}
  onclick={onChoose}
>
  <span
    class:avatar={result.kind === 'artist'}
    class="leading"
    aria-hidden={result.kind === 'artist' ? undefined : 'true'}
  >
    {#if result.kind === 'artist'}
      <AccountAvatar
        src={result.avatarUrl}
        size={42}
        alt={`${result.label}头像`}
        fallbackText={result.label}
      />
    {:else if 'coverUrl' in result}
      <WorkThumbnail
        src={result.coverUrl}
        ageRating={result.ageRating}
        unavailableMessage="无封面"
      />
    {:else}
      <Icon name={result.icon} size={20} />
    {/if}
  </span>
  <span class="copy">
    <strong>{result.label}</strong>
    <small>{result.detail}</small>
  </span>
  <span class="kind">{globalSearchKindLabels[result.kind]}</span>
</button>

<style>
  button {
    display: grid;
    width: 100%;
    min-height: 58px;
    grid-template-columns: 46px minmax(0, 1fr) auto;
    gap: 0.75rem;
    align-items: center;
    padding: 0.38rem 0.6rem;
    border-radius: 7px;
    background: transparent;
    color: var(--color-text-1);
    text-align: left;
  }

  button:hover,
  button[aria-selected='true'] {
    background: var(--color-primary-soft);
  }

  button:focus-visible {
    outline-offset: -2px;
  }

  .leading {
    display: grid;
    width: 46px;
    height: 46px;
    overflow: hidden;
    border: 1px solid var(--color-border);
    border-radius: 7px;
    background: var(--color-surface-2);
    color: var(--color-primary);
    place-items: center;
  }

  .leading.avatar {
    overflow: visible;
    border: 0;
    border-radius: 50%;
    background: transparent;
  }

  .copy {
    min-width: 0;
  }

  strong,
  small {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  strong {
    font-size: 0.88rem;
    font-weight: 660;
  }

  small {
    margin-top: 0.2rem;
    color: var(--color-text-3);
    font-size: 0.73rem;
  }

  .kind {
    color: var(--color-text-3);
    font-size: 0.7rem;
  }

  @media (max-width: 420px) {
    button {
      grid-template-columns: 42px minmax(0, 1fr);
      gap: 0.6rem;
      padding-inline: 0.45rem;
    }

    .leading {
      width: 42px;
      height: 42px;
    }

    .kind {
      display: none;
    }
  }
</style>
