<script lang="ts">
  import SelectField from './SelectField.svelte';
  import { dateTimeHourOptions, dateTimeMinuteOptions } from './date-time';

  interface TimeValue {
    hour: number;
    minute: number;
  }

  interface Props extends TimeValue {
    ariaLabel: string;
    disabled?: boolean;
    onChange: (value: TimeValue) => void;
  }

  let { hour, minute, ariaLabel, disabled = false, onChange }: Props = $props();
</script>

<div class="pa-time-select" role="group" aria-label={`${ariaLabel}时间`}>
  <span>时间</span>
  <SelectField
    value={String(hour)}
    options={dateTimeHourOptions}
    ariaLabel={`${ariaLabel}小时`}
    {disabled}
    portal={false}
    onChange={(value) => onChange({ hour: Number(value), minute })}
  />
  <span aria-hidden="true">:</span>
  <SelectField
    value={String(minute)}
    options={dateTimeMinuteOptions}
    ariaLabel={`${ariaLabel}分钟`}
    {disabled}
    portal={false}
    onChange={(value) => onChange({ hour, minute: Number(value) })}
  />
</div>

<style>
  .pa-time-select {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto minmax(0, 1fr);
    align-items: center;
    gap: 0.4rem;
  }

  .pa-time-select > span {
    color: var(--color-text-3);
    font-size: 0.68rem;
  }

  .pa-time-select :global(.pa-select-trigger) {
    width: 100%;
    min-width: 0;
    min-height: 32px;
    padding: 0 0.55rem;
    font-size: 0.7rem;
  }
</style>
