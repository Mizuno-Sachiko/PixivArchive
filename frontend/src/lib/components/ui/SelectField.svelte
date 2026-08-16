<script lang="ts">
  import { Select } from 'bits-ui';

  import Icon from './Icon.svelte';
  import './select-field.css';

  export interface SelectOption {
    value: string;
    label: string;
    disabled?: boolean;
  }

  interface Props {
    value?: string;
    options: readonly SelectOption[];
    ariaLabel: string;
    placeholder?: string;
    disabled?: boolean;
    name?: string;
    fullWidth?: boolean;
    portal?: boolean;
    onChange?: (value: string) => void;
  }

  let {
    value = $bindable(''),
    options,
    ariaLabel,
    placeholder = '请选择',
    disabled = false,
    name,
    fullWidth = false,
    portal = true,
    onChange
  }: Props = $props();
  let items = $derived(options.map((option) => ({ ...option })));
</script>

<Select.Root
  type="single"
  bind:value
  {items}
  {disabled}
  {name}
  onValueChange={(next) => onChange?.(next)}
>
  <Select.Trigger
    class={`pa-select-trigger${fullWidth ? ' full-width' : ''}`}
    aria-label={ariaLabel}
  >
    <Select.Value {placeholder} />
    <Icon name="chevron" size={16} />
  </Select.Trigger>
  <Select.Portal disabled={!portal}>
    <Select.Content class="pa-select-content" align="start" sideOffset={6}>
      <Select.Viewport class="pa-select-viewport">
        {#each options as option (option.value)}
          <Select.Item
            class="pa-select-item"
            value={option.value}
            label={option.label}
            disabled={option.disabled}
          >
            {#snippet children({ selected })}
              <span>{option.label}</span>
              {#if selected}<span aria-hidden="true">✓</span>{/if}
            {/snippet}
          </Select.Item>
        {/each}
      </Select.Viewport>
    </Select.Content>
  </Select.Portal>
</Select.Root>
