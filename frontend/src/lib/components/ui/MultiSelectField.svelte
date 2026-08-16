<script lang="ts">
  import { Select } from 'bits-ui';

  import Icon from './Icon.svelte';
  import type { SelectOption } from './SelectField.svelte';
  import './select-field.css';

  interface Props {
    value?: string[];
    options: readonly SelectOption[];
    ariaLabel: string;
    placeholder?: string;
    disabled?: boolean;
    name?: string;
    fullWidth?: boolean;
    onChange?: (value: string[]) => void;
  }

  let {
    value = $bindable([]),
    options,
    ariaLabel,
    placeholder = '请选择',
    disabled = false,
    name,
    fullWidth = false,
    onChange
  }: Props = $props();
  let selected = $derived(
    options.filter((option) => value.includes(option.value))
  );
  let summary = $derived(
    selected.length === 0
      ? placeholder
      : selected.map((option) => option.label).join('、')
  );
</script>

<Select.Root
  type="multiple"
  bind:value
  {disabled}
  {name}
  onValueChange={(next) => onChange?.(next)}
>
  <Select.Trigger
    class={`pa-select-trigger${fullWidth ? ' full-width' : ''}`}
    aria-label={ariaLabel}
  >
    <span class:placeholder={selected.length === 0} data-select-value>
      {summary}
    </span>
    <Icon name="chevron" size={16} />
  </Select.Trigger>
  <Select.Portal>
    <Select.Content class="pa-select-content" align="start" sideOffset={6}>
      <Select.Viewport class="pa-select-viewport">
        {#each options as option (option.value)}
          <Select.Item
            class="pa-select-item"
            value={option.value}
            label={option.label}
            disabled={option.disabled}
          >
            {#snippet children({ selected: itemSelected })}
              <span>{option.label}</span>
              {#if itemSelected}<span aria-hidden="true">✓</span>{/if}
            {/snippet}
          </Select.Item>
        {/each}
      </Select.Viewport>
    </Select.Content>
  </Select.Portal>
</Select.Root>

<style>
  .placeholder {
    color: var(--color-text-3);
  }
</style>
