<script lang="ts">
  import { page } from '$app/state';

  import { getTag, type TagDetail } from '$lib/api/gallery';
  import GalleryWorkspace from '$lib/components/gallery/GalleryWorkspace.svelte';
  import RetryMessage from '$lib/components/ui/RetryMessage.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import { LatestRequest } from '$lib/latest-request';

  let detail = $state<TagDetail | null>(null);
  let error = $state('');
  let reload = $state(0);
  const requests = new LatestRequest();

  $effect(() => {
    const tagName = page.params.tag?.trim() ?? '';
    void reload;
    if (!tagName) {
      detail = null;
      error = '标签名称无效';
      requests.invalidate();
      return;
    }
    const request = requests.begin();
    detail = null;
    error = '';
    void getTag(tagName)
      .then((value) => {
        if (requests.isCurrent(request)) detail = value;
      })
      .catch(() => {
        if (requests.isCurrent(request)) error = '标签信息暂时无法读取';
      });
    return () => {
      if (requests.isCurrent(request)) requests.invalidate();
    };
  });
</script>

{#if detail}
  {#key detail.tag.id}
    <GalleryWorkspace
      title={`#${detail.tag.translation ?? detail.tag.original}`}
      description={`${detail.tag.original} · 本地${detail.work_count}件作品`}
      externalUrl={`https://www.pixiv.net/tags/${encodeURIComponent(detail.tag.original)}/artworks`}
      externalPlacement="description"
      baseGroups={[
        {
          mode: 'all',
          filters: [{ type: 'tag_id', value: detail.tag.id }]
        }
      ]}
    />
  {/key}
{:else if error}
  <RetryMessage message={error} onRetry={() => (reload += 1)} />
{:else}
  <EmptyState message="正在读取标签作品…" loading />
{/if}
