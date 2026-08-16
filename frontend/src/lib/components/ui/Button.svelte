<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';

  interface Props extends HTMLButtonAttributes {
    variant?: 'primary' | 'secondary' | 'danger';
    size?: 'standard' | 'compact';
    busy?: boolean;
    children: Snippet;
  }

  let {
    variant = 'secondary',
    size = 'standard',
    busy = false,
    disabled = false,
    type = 'button',
    class: className = '',
    children,
    ...attributes
  }: Props = $props();
</script>

<button
  class={`${variant}-button${size === 'compact' ? ' compact-button' : ''} ${className}`.trim()}
  {type}
  disabled={disabled || busy}
  {...attributes}
>
  {@render children()}
</button>
