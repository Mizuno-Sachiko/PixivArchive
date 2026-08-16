<script lang="ts">
  let { message, error }: { message: string; error: string } = $props();
</script>

{#if message}
  <p class="settings-feedback">{message}</p>
{/if}
{#if error}
  <p class="inline-message error" role="alert">{error}</p>
{/if}

<style>
  .settings-feedback {
    margin: 0;
    color: var(--color-primary);
    font-size: 0.74rem;
    font-weight: 700;
  }
</style>
