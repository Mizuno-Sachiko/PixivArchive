<script lang="ts">
  import { listArtists } from '$lib/api/gallery';
  import ContextDirectory from '$lib/components/gallery/ContextDirectory.svelte';
  import { galleryArtistPath } from '$lib/gallery-routes';

  async function loadPage(query: string, cursor: string | null, limit: number) {
    const page = await listArtists(query, cursor, limit);
    return {
      items: page.items.map((artist) => ({
        id: artist.id,
        href: galleryArtistPath(artist.pixiv_artist_id),
        anchor: `artist:${artist.id}`,
        eyebrow: `PIXIV ${artist.pixiv_artist_id}`,
        title: artist.name,
        workCount: artist.work_count,
        coverUrl: artist.cover_url,
        coverAgeRating: artist.cover_age_rating
      })),
      total: page.total,
      nextCursor: page.next_cursor
    };
  }
</script>

<ContextDirectory
  title="作者"
  kind="artist"
  unit="位作者"
  searchPlaceholder="搜索作者名称或Pixiv ID"
  loadingText="正在读取作者…"
  emptyText="还没有作者数据"
  emptySearchText="没有找到匹配的作者"
  readErrorText="作者列表暂时无法读取"
  {loadPage}
/>
