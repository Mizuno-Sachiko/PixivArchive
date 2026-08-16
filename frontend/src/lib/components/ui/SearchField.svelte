<script lang="ts">
  import type { HTMLInputAttributes } from 'svelte/elements';

  import Field from './Field.svelte';

  interface Props extends Omit<HTMLInputAttributes, 'type' | 'value'> {
    label: string;
    labelHidden?: boolean;
    value: string;
    class?: string;
  }

  let {
    label,
    labelHidden = false,
    value = $bindable(),
    class: className = '',
    ...attributes
  }: Props = $props();
</script>

<Field {label} {labelHidden} class={className}>
  <input type="search" bind:value {...attributes} />
</Field>
