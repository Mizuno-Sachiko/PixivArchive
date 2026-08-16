<script lang="ts">
  interface Props {
    checked: boolean;
    label: string;
    description?: string;
    ariaLabel?: string;
    disabled?: boolean;
    onchange: (checked: boolean) => void;
  }

  let {
    checked,
    label,
    description,
    ariaLabel,
    disabled = false,
    onchange
  }: Props = $props();
</script>

<label class="switch-field" class:described={Boolean(description)}>
  <input
    type="checkbox"
    {checked}
    {disabled}
    aria-label={ariaLabel}
    onchange={(event) => onchange(event.currentTarget.checked)}
  />
  <span>
    <strong>{label}</strong>
    {#if description}<small>{description}</small>{/if}
  </span>
</label>

<style>
  .switch-field {
    display: flex;
    min-height: 40px;
    align-items: center;
    gap: 0.6rem;
    color: var(--color-text-2);
    font-size: 0.78rem;
  }

  .switch-field.described {
    align-items: flex-start;
  }

  input {
    width: 18px;
    height: 18px;
    flex: 0 0 auto;
    margin: 0;
    accent-color: var(--color-primary);
  }

  .described input {
    margin-top: 1px;
  }

  span {
    display: grid;
    gap: 0.2rem;
  }

  strong {
    font-weight: 650;
  }

  small {
    color: var(--color-text-3);
    font-size: 0.7rem;
    font-weight: 450;
    line-height: 1.5;
  }
</style>
