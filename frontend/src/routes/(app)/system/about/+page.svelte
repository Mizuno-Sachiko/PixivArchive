<script lang="ts">
  import { onMount } from 'svelte';

  import { systemApi, type SystemStatus } from '$lib/api/system';
  import Icon from '$lib/components/ui/Icon.svelte';
  import PageHeader from '$lib/components/ui/PageHeader.svelte';

  let status = $state<SystemStatus | null>(null);
  let error = $state('');

  onMount(() => void loadStatus());

  async function loadStatus(): Promise<void> {
    try {
      status = await systemApi.status();
    } catch {
      error = '版本信息暂时无法读取';
    }
  }
</script>

<svelte:head>
  <title>关于 · PixivArchive</title>
</svelte:head>

<section class="workspace-page">
  <PageHeader title="关于" />

  <section class="panel about-panel">
    <dl>
      <div>
        <dt>版本</dt>
        <dd class="version-details">
          {#if error}
            <span class="inline-message error" role="alert">{error}</span>
          {:else if status}
            <span>v{status.version}</span>
          {:else}
            <span class="version-loading" aria-label="正在读取版本"></span>
          {/if}
          <a
            class="repository-link"
            href="https://github.com/Mizuno-Sachiko/PixivArchive"
            target="_blank"
            rel="noreferrer"
          >
            <Icon name="github" size={17} />
            <span>GitHub仓库</span>
          </a>
        </dd>
      </div>
    </dl>
  </section>
</section>

<style>
  .about-panel {
    --metadata-gap: 8rem;

    max-width: 680px;
    padding: 0.5rem 1.1rem;
  }

  dl {
    margin: 0;
  }

  dl > div {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    column-gap: var(--metadata-gap);
    row-gap: 1rem;
    padding: 1rem 0;
    border-bottom: 1px solid var(--color-border);
  }

  dl > div:last-child {
    border-bottom: 0;
  }

  dt {
    color: var(--color-text-3);
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
    font-weight: 700;
  }

  .version-details {
    display: flex;
    align-items: center;
    justify-content: flex-start;
    column-gap: var(--metadata-gap);
    row-gap: 0.75rem;
    flex-wrap: wrap;
  }

  .version-loading {
    display: inline-block;
    width: 3.5rem;
    height: 1px;
    flex: 0 0 auto;
    background: var(--color-text-3);
  }

  .repository-link {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    color: var(--color-primary);
  }

  .repository-link:hover {
    text-decoration: underline;
  }

  @media (max-width: 560px) {
    .about-panel {
      --metadata-gap: 1rem;
    }

    dl > div {
      grid-template-columns: 1fr;
      row-gap: 0.45rem;
    }
  }
</style>
