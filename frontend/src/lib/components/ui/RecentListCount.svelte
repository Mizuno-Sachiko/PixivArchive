<script lang="ts">
  interface Props {
    count: number;
    limit: number;
    loading?: boolean;
  }

  let { count, limit, loading = false }: Props = $props();
  let title = $derived(`列表最多显示最近${limit}项`);
</script>

<span class="panel-count" {title}
  >{loading ? '正在读取…' : `最近${count}项`}</span
>
