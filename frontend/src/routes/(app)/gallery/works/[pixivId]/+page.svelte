<script lang="ts">
  import { page } from '$app/state';

  import WorkDetail from '$lib/components/work/WorkDetail.svelte';
  import { parseSourceId } from '$lib/gallery-routes';

  let pixivWorkId = $derived(parseSourceId(page.params.pixivId) ?? Number.NaN);
</script>

<WorkDetail {pixivWorkId} />
