<script lang="ts">
  import {
    formatDateTime,
    formatDateTimeTitle,
    formatExactDateTime
  } from '$lib/format';

  interface Props {
    value: string | null;
    exact?: boolean;
    empty?: string;
  }

  let { value, exact = false, empty = '—' }: Props = $props();
  let text = $derived(
    value ? (exact ? formatExactDateTime(value) : formatDateTime(value)) : empty
  );
</script>

<time datetime={value ?? undefined} title={formatDateTimeTitle(value)}
  >{text}</time
>
