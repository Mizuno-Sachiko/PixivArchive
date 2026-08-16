<script lang="ts">
  import IconLink from './IconLink.svelte';

  interface Props {
    href: string;
    label: string;
    showText?: boolean;
    toolbar?: boolean;
  }

  let { href, label, showText = false, toolbar = false }: Props = $props();
</script>

<IconLink
  {href}
  {label}
  text={showText ? 'Pixiv' : undefined}
  external
  size="compact"
  {toolbar}
/>
