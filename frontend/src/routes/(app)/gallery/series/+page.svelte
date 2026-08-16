<script lang="ts">
  import { listSeries } from '$lib/api/gallery';
  import ContextDirectory from '$lib/components/gallery/ContextDirectory.svelte';
  import { gallerySeriesPath } from '$lib/gallery-routes';

  async function loadPage(query: string, cursor: string | null, limit: number) {
    const page = await listSeries(query, cursor, limit);
    return {
      items: page.items.map((series) => ({
        id: series.id,
        href: gallerySeriesPath(series.pixiv_series_id),
        anchor: `series:${series.id}`,
        eyebrow: `PIXIV ${series.pixiv_series_id}`,
        title: series.title,
        workCount: series.work_count,
        coverUrl: series.cover_url,
        coverAgeRating: series.cover_age_rating
      })),
      total: page.total,
      nextCursor: page.next_cursor
    };
  }
</script>

<ContextDirectory
  title="系列"
  kind="series"
  unit="个系列"
  searchPlaceholder="搜索系列标题或Pixiv ID"
  loadingText="正在读取系列…"
  emptyText="还没有系列数据"
  emptySearchText="没有找到匹配的系列"
  readErrorText="系列列表暂时无法读取"
  {loadPage}
/>
