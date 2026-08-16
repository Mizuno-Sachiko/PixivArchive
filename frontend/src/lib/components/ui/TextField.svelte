<script lang="ts">
  import type { HTMLInputAttributes } from 'svelte/elements';

  import Field from './Field.svelte';

  interface Props extends Omit<HTMLInputAttributes, 'type' | 'value'> {
    label: string;
    value: string;
    description?: string;
    wide?: boolean;
    inputClass?: string;
  }

  let {
    label,
    value = $bindable(),
    description,
    wide = false,
    inputClass = '',
    ...attributes
  }: Props = $props();
</script>

<Field {label} {description} {wide}>
  <input class={inputClass} type="text" bind:value {...attributes} />
</Field>
