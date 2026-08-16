<script lang="ts">
  import type { ConditionValue, RuleFieldDescriptor } from '$lib/api/rules';
  import DateTimeField from '$lib/components/ui/DateTimeField.svelte';
  import MultiSelectField from '$lib/components/ui/MultiSelectField.svelte';
  import SelectField from '$lib/components/ui/SelectField.svelte';

  interface Props {
    descriptor: RuleFieldDescriptor;
    value: ConditionValue;
    ariaLabel: string;
    onChange: (value: ConditionValue) => void;
  }

  let { descriptor, value, ariaLabel, onChange }: Props = $props();
  let options = $derived(descriptor.options ?? []);

  function updateScalar(source: string): void {
    let next: ConditionValue;
    switch (value.type) {
      case 'number':
        next = { type: 'number', value: Number(source) };
        break;
      case 'duration_hours':
        next = { type: 'duration_hours', value: Number(source) };
        break;
      case 'duration_days':
        next = { type: 'duration_days', value: Number(source) };
        break;
      case 'text_list':
        next = {
          type: 'text_list',
          value: source
            .split(/[,，]/)
            .map((item) => item.trim())
            .filter(Boolean)
        };
        break;
      case 'text':
        next = { type: 'text', value: source };
        break;
      default:
        return;
    }
    onChange(next);
  }

  function scalarValue(): string | number {
    switch (value.type) {
      case 'number':
      case 'duration_hours':
      case 'duration_days':
        return value.value;
      case 'text':
        return value.value;
      case 'text_list':
        return value.value.join(', ');
      default:
        return '';
    }
  }
</script>

<div
  class:range={value.type === 'number_range' || value.type === 'date_range'}
  class="condition-value"
>
  {#if value.type === 'number_range'}
    <input
      aria-label={`${ariaLabel}最小值`}
      type="number"
      placeholder="最小值"
      value={value.value.min}
      oninput={(event) =>
        onChange({
          type: 'number_range',
          value: {
            min: Number(event.currentTarget.value),
            max: value.value.max
          }
        })}
    />
    <span>至</span>
    <input
      aria-label={`${ariaLabel}最大值`}
      type="number"
      placeholder="最大值"
      value={value.value.max}
      oninput={(event) =>
        onChange({
          type: 'number_range',
          value: {
            min: value.value.min,
            max: Number(event.currentTarget.value)
          }
        })}
    />
  {:else if value.type === 'date_range'}
    <DateTimeField
      ariaLabel={`${ariaLabel}开始时间`}
      value={value.value.start}
      fullWidth
      compact
      onChange={(start) =>
        onChange({
          type: 'date_range',
          value: { start, end: value.value.end }
        })}
    />
    <span>至</span>
    <DateTimeField
      ariaLabel={`${ariaLabel}结束时间`}
      value={value.value.end}
      fullWidth
      compact
      onChange={(end) =>
        onChange({
          type: 'date_range',
          value: { start: value.value.start, end }
        })}
    />
  {:else if value.type === 'date'}
    <DateTimeField
      {ariaLabel}
      value={value.value}
      fullWidth
      compact
      onChange={(next) => onChange({ type: 'date', value: next })}
    />
  {:else if options.length > 0 && value.type === 'text'}
    <SelectField
      {ariaLabel}
      value={value.value}
      {options}
      fullWidth
      onChange={(next) => onChange({ type: 'text', value: next })}
    />
  {:else if options.length > 0 && value.type === 'text_list'}
    <MultiSelectField
      {ariaLabel}
      value={value.value}
      {options}
      fullWidth
      onChange={(next) => onChange({ type: 'text_list', value: next })}
    />
  {:else}
    <input
      aria-label={ariaLabel}
      type={value.type === 'number' ||
      value.type === 'duration_hours' ||
      value.type === 'duration_days'
        ? 'number'
        : 'text'}
      value={scalarValue()}
      placeholder={`例如：${descriptor.value_example}`}
      oninput={(event) => updateScalar(event.currentTarget.value)}
    />
  {/if}
</div>

<style>
  .condition-value {
    display: flex;
    width: 100%;
    min-width: 0;
    gap: 0.35rem;
    align-items: center;
  }

  input {
    width: 100%;
    min-width: 0;
    height: 34px;
    padding: 0 0.58rem;
    border: 1px solid var(--color-border);
    border-radius: 7px;
    outline: none;
    background: var(--color-surface-1);
    font-size: 0.72rem;
  }

  input:focus {
    border-color: var(--color-primary);
    box-shadow: 0 0 0 2px var(--color-primary-soft);
  }

  span {
    flex: 0 0 auto;
    color: var(--color-text-3);
    font-size: 0.66rem;
  }

  .condition-value :global(.pa-select-trigger),
  .condition-value :global(.pa-date-time) {
    width: 100%;
    min-width: 0;
    height: 34px;
  }

  @media (max-width: 1180px) {
    .condition-value.range {
      display: grid;
      grid-template-columns: 1fr;
    }

    .condition-value.range > span {
      display: none;
    }
  }
</style>
