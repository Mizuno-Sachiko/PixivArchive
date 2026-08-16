<script lang="ts">
  interface Props {
    label: string;
    tone?: 'neutral' | 'success' | 'warning' | 'error' | 'primary';
  }

  let { label, tone = 'neutral' }: Props = $props();
</script>

<span class="status-pill {tone}">
  <span class="dot"></span>
  {label}
</span>

<style>
  .status-pill {
    display: inline-flex;
    align-items: center;
    gap: 0.42rem;
    min-height: 26px;
    padding: 0.2rem 0.65rem;
    border-radius: var(--radius-pill);
    background: var(--color-surface-3);
    color: var(--color-text-2);
    font-size: 0.78rem;
    font-weight: 650;
    white-space: nowrap;
  }

  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
  }

  .success {
    color: var(--color-success);
  }

  .warning {
    color: var(--color-warning);
  }

  .error {
    color: var(--color-error);
  }

  .primary {
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }
</style>
