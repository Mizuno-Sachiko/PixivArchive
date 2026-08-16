<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { onDestroy, onMount } from 'svelte';

  import Icon from '$lib/components/ui/Icon.svelte';
  import { commandPaletteStore } from '$lib/stores/command-palette.svelte';

  import GlobalSearchResultRow from './GlobalSearchResultRow.svelte';
  import {
    GlobalSearchSession,
    globalSearchGroupKeys,
    globalSearchKindLabels,
    type GlobalSearchGroupKey,
    type GlobalSearchResult
  } from './global-search.svelte';

  const searchSession = new GlobalSearchSession();

  let dialog: HTMLDialogElement;
  let searchInput: HTMLInputElement;
  let closing = $state(false);
  let closePromise: Promise<void> | null = null;
  let finishClose: (() => void) | null = null;
  let archiveLoading = $derived(
    searchSession.groups.work.status === 'loading' ||
      searchSession.groups.artist.status === 'loading' ||
      searchSession.groups.tag.status === 'loading' ||
      searchSession.groups.series.status === 'loading'
  );
  let settledEmpty = $derived(
    searchSession.query.length > 0 &&
      searchSession.results.length === 0 &&
      searchSession.groups.work.status === 'success' &&
      searchSession.groups.artist.status === 'success' &&
      searchSession.groups.tag.status === 'success' &&
      searchSession.groups.series.status === 'success'
  );

  $effect(() => {
    if (!dialog) return;
    if (commandPaletteStore.opened && !dialog.open) {
      searchSession.search(commandPaletteStore.query);
      dialog.showModal();
      requestAnimationFrame(() => searchInput?.focus());
    } else if (!commandPaletteStore.opened && dialog.open && !closing) {
      void requestClose();
    }
  });

  onMount(() => {
    const handleShortcut = (event: KeyboardEvent) => {
      if (
        (event.ctrlKey || event.metaKey) &&
        event.key.toLocaleLowerCase() === 'k'
      ) {
        event.preventDefault();
        if (commandPaletteStore.opened) {
          searchInput?.focus();
        } else {
          commandPaletteStore.open();
        }
      }
    };
    window.addEventListener('keydown', handleShortcut);
    return () => window.removeEventListener('keydown', handleShortcut);
  });

  onDestroy(() => searchSession.dispose());

  function requestClose(): Promise<void> {
    if (!dialog?.open) {
      completeClose();
      return Promise.resolve();
    }
    if (closePromise) return closePromise;

    closing = true;
    closePromise = new Promise<void>((resolve) => {
      finishClose = resolve;
    });
    return closePromise;
  }

  function handleAnimationEnd(event: AnimationEvent): void {
    if (
      closing &&
      event.target === dialog &&
      event.animationName.endsWith('global-search-out')
    ) {
      dialog.close();
    }
  }

  function handleCancel(event: Event): void {
    event.preventDefault();
    void requestClose();
  }

  function completeClose(): void {
    searchSession.search('');
    commandPaletteStore.close();
    closing = false;
    finishClose?.();
    finishClose = null;
    closePromise = null;
  }

  function updateQuery(event: Event): void {
    const value = (event.currentTarget as HTMLInputElement).value;
    commandPaletteStore.query = value;
    searchSession.search(value);
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.isComposing) return;
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      searchSession.moveSelection(1);
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      searchSession.moveSelection(-1);
    } else if (event.key === 'Enter' && searchSession.selectedResult) {
      event.preventDefault();
      void choose(searchSession.selectedResult);
    }
  }

  async function choose(result: GlobalSearchResult): Promise<void> {
    await requestClose();
    await goto(resolve(result.href));
  }

  function showsGroup(key: GlobalSearchGroupKey): boolean {
    const group = searchSession.groups[key];
    return (
      group.items.length > 0 ||
      group.status === 'loading' ||
      group.status === 'error'
    );
  }
</script>

<dialog
  bind:this={dialog}
  aria-label="全局搜索"
  class="command-dialog glass-surface"
  class:closing
  onanimationend={handleAnimationEnd}
  oncancel={handleCancel}
  onclose={completeClose}
  onclick={(event) => {
    if (event.target === dialog) void requestClose();
  }}
>
  <div class="palette">
    <header>
      <span class="search-icon"><Icon name="search" size={20} /></span>
      <input
        bind:this={searchInput}
        value={commandPaletteStore.query}
        role="combobox"
        aria-label="搜索作品、作者、标签、系列或页面"
        aria-controls="global-search-results"
        aria-expanded="true"
        aria-autocomplete="list"
        aria-activedescendant={searchSession.selectedResult
          ? `global-search-result-${searchSession.selectedResult.key}`
          : undefined}
        placeholder="搜索作品、作者、标签、系列或页面"
        autocomplete="off"
        oninput={updateQuery}
        onkeydown={handleKeydown}
      />
      <kbd>Esc</kbd>
    </header>

    <div
      id="global-search-results"
      class="results"
      role="listbox"
      aria-label="搜索结果"
      aria-busy={archiveLoading}
    >
      {#each globalSearchGroupKeys as key (key)}
        {@const group = searchSession.groups[key]}
        {#if showsGroup(key)}
          <section
            class="result-group"
            role="group"
            aria-labelledby={`global-search-heading-${key}`}
            data-search-group={key}
          >
            <h2 id={`global-search-heading-${key}`}>
              {globalSearchKindLabels[key]}
            </h2>
            {#each group.items as result (result.key)}
              <GlobalSearchResultRow
                {result}
                selected={searchSession.selectedKey === result.key}
                onSelect={() => searchSession.select(result.key)}
                onChoose={() => choose(result)}
              />
            {/each}
            {#if group.status === 'loading'}
              <p class="group-state">正在读取{globalSearchKindLabels[key]}…</p>
            {:else if group.status === 'error'}
              <p class="group-state error" role="status">
                {globalSearchKindLabels[key]}暂时无法读取
              </p>
            {/if}
          </section>
        {/if}
      {/each}

      {#if settledEmpty}
        <p class="empty">没有匹配的馆藏内容或页面</p>
      {/if}
    </div>
  </div>
</dialog>

<style>
  .command-dialog {
    width: min(700px, calc(100vw - 28px));
    max-height: min(700px, calc(100dvh - 52px));
    padding: 0;
    overflow: hidden;
    border-radius: 18px;
    color: var(--color-text-1);
  }

  .command-dialog[open] {
    animation: global-search-in var(--motion-base) var(--ease-standard) both;
  }

  .command-dialog[open].closing {
    pointer-events: none;
    animation: global-search-out var(--motion-fast) ease-in both;
  }

  .command-dialog::backdrop {
    background: rgba(3, 9, 15, 0.55);
    backdrop-filter: blur(4px);
  }

  .command-dialog[open]::backdrop {
    animation: global-search-backdrop-in var(--motion-base) ease-out both;
  }

  .command-dialog[open].closing::backdrop {
    animation: global-search-backdrop-out var(--motion-fast) ease-in both;
  }

  .palette {
    display: flex;
    max-height: min(700px, calc(100dvh - 52px));
    flex-direction: column;
    background: var(--color-glass-strong);
  }

  header {
    display: grid;
    height: 58px;
    grid-template-columns: auto minmax(0, 1fr) auto;
    gap: 0.75rem;
    align-items: center;
    padding: 0 1rem;
    border-bottom: 1px solid var(--color-border);
  }

  .search-icon {
    display: grid;
    color: var(--color-text-3);
  }

  input {
    width: 100%;
    min-width: 0;
    padding: 0.35rem 0;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--color-text-1);
    font-size: 0.96rem;
  }

  kbd {
    padding: 0.24rem 0.48rem;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    background: var(--color-surface-2);
    color: var(--color-text-3);
    font: 0.72rem var(--font-ui);
  }

  .results {
    height: min(420px, calc(100dvh - 110px));
    min-height: 0;
    padding: 0.4rem 0.55rem 0.65rem;
    overflow-y: auto;
    overscroll-behavior: contain;
  }

  .result-group + .result-group {
    padding-top: 0.3rem;
    border-top: 1px solid var(--color-border);
    margin-top: 0.3rem;
  }

  h2 {
    margin: 0.42rem 0.6rem 0.25rem;
    color: var(--color-text-3);
    font-size: 0.7rem;
    font-weight: 700;
  }

  .group-state {
    min-height: 46px;
    padding: 0.8rem 0.6rem;
    margin: 0;
    color: var(--color-text-3);
    font-size: 0.78rem;
  }

  .group-state.error {
    color: var(--color-error);
  }

  .empty {
    padding: 2.2rem 1rem;
    margin: 0;
    color: var(--color-text-3);
    font-size: 0.84rem;
    text-align: center;
  }

  @keyframes global-search-in {
    from {
      opacity: 0;
      transform: translateY(-6px);
    }

    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  @keyframes global-search-out {
    from {
      opacity: 1;
      transform: translateY(0);
    }

    to {
      opacity: 0;
      transform: translateY(-4px);
    }
  }

  @keyframes global-search-backdrop-in {
    from {
      opacity: 0;
    }

    to {
      opacity: 1;
    }
  }

  @keyframes global-search-backdrop-out {
    from {
      opacity: 1;
    }

    to {
      opacity: 0;
    }
  }

  @media (max-width: 420px) {
    .command-dialog {
      width: calc(100vw - 16px);
      max-height: calc(100dvh - 24px);
      border-radius: 14px;
    }

    .palette {
      max-height: calc(100dvh - 24px);
    }

    header {
      grid-template-columns: auto minmax(0, 1fr);
      gap: 0.6rem;
      padding-inline: 0.75rem;
    }

    kbd {
      display: none;
    }

    .results {
      height: min(420px, calc(100dvh - 82px));
      padding-inline: 0.35rem;
    }
  }
</style>
