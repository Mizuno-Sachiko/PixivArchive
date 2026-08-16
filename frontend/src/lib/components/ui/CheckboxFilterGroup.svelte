<script lang="ts" generics="Value extends string">
  import { SvelteSet } from 'svelte/reactivity';

  interface Option {
    value: Value;
    label: string;
  }

  interface Props {
    legend: string;
    values: Value[];
    options: readonly Option[];
    disabled?: boolean;
  }

  let {
    legend,
    values = $bindable(),
    options,
    disabled = false
  }: Props = $props();

  function toggle(value: Value, checked: boolean): void {
    const selected = new SvelteSet(values);
    if (checked) selected.add(value);
    else selected.delete(value);
    values = options
      .filter((option) => selected.has(option.value))
      .map((option) => option.value);
  }
</script>

<fieldset>
  <legend>{legend}</legend>
  {#each options as option (option.value)}
    <label>
      <input
        type="checkbox"
        checked={values.includes(option.value)}
        {disabled}
        onchange={(event) => toggle(option.value, event.currentTarget.checked)}
      />
      {option.label}
    </label>
  {/each}
</fieldset>

<style>
  fieldset {
    display: flex;
    flex-wrap: wrap;
    gap: 0.55rem;
    padding: 0;
    border: 0;
  }

  legend {
    width: 100%;
    margin-bottom: 0.4rem;
    color: var(--color-text-2);
    font-size: 0.74rem;
    font-weight: 650;
  }

  label {
    display: flex;
    align-items: center;
    gap: 0.38rem;
    padding: 0.42rem 0.62rem;
    border-radius: var(--radius-pill);
    background: var(--color-surface-2);
    color: var(--color-text-2);
    font-size: 0.7rem;
  }

  input {
    accent-color: var(--color-primary);
  }

  label:has(input:disabled) {
    opacity: 0.58;
  }
</style>
