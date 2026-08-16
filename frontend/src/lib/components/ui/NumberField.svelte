<script lang="ts">
  import type { HTMLInputAttributes } from 'svelte/elements';

  import Field from './Field.svelte';

  interface Props extends Omit<HTMLInputAttributes, 'type' | 'value'> {
    label: string;
    value: number;
    description?: string;
    wide?: boolean;
  }

  let {
    label,
    value = $bindable(),
    description,
    wide = false,
    ...attributes
  }: Props = $props();
</script>

<Field {label} {description} {wide}>
  <input type="number" bind:value {...attributes} />
</Field>
