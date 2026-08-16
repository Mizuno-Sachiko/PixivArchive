<script lang="ts">
  import { listTags } from '$lib/api/gallery';
  import ContextDirectory from '$lib/components/gallery/ContextDirectory.svelte';
  import { galleryTagPath } from '$lib/gallery-routes';

  async function loadPage(query: string, cursor: string | null, limit: number) {
    const page = await listTags(query, cursor, limit);
    return {
      items: page.items.map((detail) => ({
        id: detail.tag.id,
        href: galleryTagPath(detail.tag.original),
        anchor: `tag:${detail.tag.id}`,
        title: detail.tag.translation ?? detail.tag.original,
        secondary: detail.tag.translation ? detail.tag.original : undefined,
        workCount: detail.work_count,
        coverUrl: detail.cover_url,
        coverAgeRating: detail.cover_age_rating
      })),
      total: page.total,
      nextCursor: page.next_cursor
    };
  }
</script>

<ContextDirectory
  title="标签"
  kind="tag"
  unit="个标签"
  searchPlaceholder="搜索标签"
  loadingText="正在读取标签…"
  emptyText="还没有标签数据"
  emptySearchText="没有找到匹配的标签"
  readErrorText="标签列表暂时无法读取"
  {loadPage}
/>
