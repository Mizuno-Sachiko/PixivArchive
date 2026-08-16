<script lang="ts">
  import type { DateValue } from '@internationalized/date';
  import { DatePicker } from 'bits-ui';

  import Icon from './Icon.svelte';
  import SelectField from './SelectField.svelte';
  import TimeSelectField from './TimeSelectField.svelte';
  import {
    dateTimeClock,
    dateTimeFromIso,
    dateTimeMonthOptions,
    dateTimePlaceholder,
    dateTimeToIso,
    dateTimeYearOptions,
    updateDateTime
  } from './date-time';

  interface Props {
    value?: string;
    ariaLabel: string;
    disabled?: boolean;
    fullWidth?: boolean;
    compact?: boolean;
    onChange?: (value: string) => void;
  }

  let {
    value = '',
    ariaLabel,
    disabled = false,
    fullWidth = false,
    compact = false,
    onChange
  }: Props = $props();
  let selected = $state<DateValue | undefined>();
  let calendarView = $state<DateValue>(dateTimePlaceholder());
  let appliedExternalValue = $state<string | null>(null);
  let open = $state(false);
  const yearOptions = dateTimeYearOptions();
  let clock = $derived(dateTimeClock(selected ?? calendarView));

  $effect(() => {
    if (value === appliedExternalValue) return;
    appliedExternalValue = value;
    const next = dateTimeFromIso(value);
    selected = next;
    if (next) calendarView = next;
  });

  function readSelected(): DateValue | undefined {
    return selected;
  }

  function updateSelected(next: DateValue | undefined): void {
    selected = next;
    if (!next) return;
    calendarView = next;
    onChange?.(dateTimeToIso(next));
  }

  function changeCalendarView(parts: { year?: number; month?: number }): void {
    calendarView = updateDateTime(calendarView, { ...parts, day: 1 });
  }

  function changeTime(next: { hour: number; minute: number }): void {
    updateSelected(updateDateTime(selected ?? calendarView, next));
  }
</script>

<DatePicker.Root
  bind:value={readSelected, updateSelected}
  bind:placeholder={calendarView}
  bind:open
  {disabled}
  locale="zh-CN"
  granularity="minute"
  hourCycle={24}
  weekStartsOn={1}
  calendarLabel={ariaLabel}
  preventDeselect
  closeOnDateSelect={false}
>
  <div
    class:full-width={fullWidth}
    class:compact
    class:disabled
    class="pa-date-time"
    role="group"
    aria-label={ariaLabel}
    aria-disabled={disabled}
  >
    <DatePicker.Input class="pa-date-time-input">
      {#snippet children({ segments })}
        {#each segments as segment, index (`${segment.part}-${index}`)}
          <DatePicker.Segment class="pa-date-time-segment" part={segment.part}
            >{segment.value}</DatePicker.Segment
          >
        {/each}
      {/snippet}
    </DatePicker.Input>
    <DatePicker.Trigger
      class="pa-date-time-trigger"
      aria-label={`选择${ariaLabel}`}
    >
      <Icon name="calendar" size={16} />
    </DatePicker.Trigger>
  </div>

  <DatePicker.Portal>
    <DatePicker.Content
      class="pa-date-time-content"
      align="start"
      sideOffset={6}
    >
      <DatePicker.Calendar class="pa-calendar">
        {#snippet children({ months, weekdays })}
          <DatePicker.Header class="pa-calendar-header">
            <DatePicker.PrevButton class="pa-calendar-nav" aria-label="上个月"
              >‹</DatePicker.PrevButton
            >
            <div class="pa-calendar-period">
              <SelectField
                value={String(calendarView.year)}
                options={yearOptions}
                ariaLabel={`${ariaLabel}年份`}
                portal={false}
                onChange={(year) => changeCalendarView({ year: Number(year) })}
              />
              <SelectField
                value={String(calendarView.month)}
                options={dateTimeMonthOptions}
                ariaLabel={`${ariaLabel}月份`}
                portal={false}
                onChange={(month) =>
                  changeCalendarView({ month: Number(month) })}
              />
            </div>
            <DatePicker.NextButton class="pa-calendar-nav" aria-label="下个月"
              >›</DatePicker.NextButton
            >
            <DatePicker.Heading class="sr-only" />
          </DatePicker.Header>
          {#each months as month (month.value.toString())}
            <DatePicker.Grid class="pa-calendar-grid">
              <DatePicker.GridHead>
                <DatePicker.GridRow>
                  {#each weekdays as weekday (weekday)}
                    <DatePicker.HeadCell class="pa-calendar-weekday">
                      {weekday}
                    </DatePicker.HeadCell>
                  {/each}
                </DatePicker.GridRow>
              </DatePicker.GridHead>
              <DatePicker.GridBody>
                {#each month.weeks as weekDates, weekIndex (weekIndex)}
                  <DatePicker.GridRow>
                    {#each weekDates as date (date.toString())}
                      <DatePicker.Cell
                        class="pa-calendar-cell"
                        {date}
                        month={month.value}
                      >
                        <DatePicker.Day class="pa-calendar-day" />
                      </DatePicker.Cell>
                    {/each}
                  </DatePicker.GridRow>
                {/each}
              </DatePicker.GridBody>
            </DatePicker.Grid>
          {/each}
        {/snippet}
      </DatePicker.Calendar>
      <div class="pa-calendar-footer">
        <TimeSelectField
          hour={clock.hour}
          minute={clock.minute}
          {ariaLabel}
          onChange={changeTime}
        />
        <button
          class="pa-calendar-done"
          type="button"
          aria-label="完成日期时间选择"
          onclick={() => (open = false)}>完成</button
        >
      </div>
    </DatePicker.Content>
  </DatePicker.Portal>
</DatePicker.Root>

<style>
  :global(.pa-date-time) {
    display: flex;
    min-width: 210px;
    min-height: var(--control-height-md);
    overflow: hidden;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
    color: var(--color-text-2);
  }

  :global(.pa-date-time.full-width) {
    width: 100%;
  }

  :global(.pa-date-time.compact) {
    min-width: 190px;
    min-height: var(--control-height-sm);
    border-radius: 7px;
  }

  :global(.pa-date-time:focus-within) {
    border-color: var(--color-primary);
    box-shadow: 0 0 0 2px var(--color-primary-soft);
  }

  :global(.pa-date-time.disabled) {
    opacity: 0.55;
  }

  :global(.pa-date-time-input) {
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: center;
    padding: 0 0.58rem;
    font: 0.72rem var(--font-mono);
    white-space: nowrap;
  }

  :global(.pa-date-time-segment) {
    display: inline-flex;
    min-width: 0.7em;
    align-items: center;
    justify-content: center;
    padding: 0.15rem 0.05rem;
    border-radius: 4px;
    outline: none;
  }

  :global(.pa-date-time-segment[data-placeholder]) {
    color: var(--color-text-3);
  }

  :global(.pa-date-time-segment:focus) {
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  :global(.pa-date-time-trigger) {
    display: grid;
    width: var(--control-height-md);
    flex: 0 0 auto;
    place-items: center;
    border-left: 1px solid var(--color-border);
    background: transparent;
    color: var(--color-text-3);
  }

  :global(.pa-date-time-trigger:hover),
  :global(.pa-date-time-trigger[data-state='open']) {
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  :global(.pa-date-time-content) {
    z-index: 190;
    width: 324px;
    padding: 0.7rem;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: color-mix(in srgb, var(--color-surface-1) 94%, transparent);
    box-shadow: var(--shadow-float);
    backdrop-filter: blur(22px) saturate(135%);
  }

  :global(.pa-calendar-header) {
    display: grid;
    grid-template-columns: 32px 1fr 32px;
    gap: 0.4rem;
    align-items: center;
    margin-bottom: 0.45rem;
  }

  :global(.pa-calendar-period) {
    display: grid;
    grid-template-columns: minmax(0, 1.2fr) minmax(0, 0.8fr);
    gap: 0.35rem;
  }

  :global(.pa-calendar-period .pa-select-trigger) {
    width: 100%;
    min-width: 0;
    min-height: 30px;
    padding: 0 0.48rem;
    font-size: 0.68rem;
  }

  :global(.pa-calendar-nav) {
    display: grid;
    width: 30px;
    height: 30px;
    place-items: center;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text-2);
    font-size: 1.15rem;
  }

  :global(.pa-calendar-nav:hover) {
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  :global(.pa-calendar-grid) {
    width: 100%;
    border-spacing: 2px;
    table-layout: fixed;
  }

  :global(.pa-calendar-weekday) {
    height: 28px;
    color: var(--color-text-3);
    font-size: 0.64rem;
    font-weight: 500;
  }

  :global(.pa-calendar-cell) {
    padding: 0;
    text-align: center;
  }

  :global(.pa-calendar-day) {
    display: inline-grid;
    width: 34px;
    height: 32px;
    margin: 0 auto;
    padding: 0;
    place-items: center;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text-2);
    font-size: 0.7rem;
    line-height: 1;
  }

  :global(.pa-calendar-day:hover),
  :global(.pa-calendar-day[data-highlighted]) {
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  :global(.pa-calendar-day[data-selected]) {
    background: var(--color-primary);
    color: #fff;
    font-weight: 700;
  }

  :global(.pa-calendar-day[data-outside-month]) {
    color: var(--color-text-3);
    opacity: 0.48;
  }

  :global(.pa-calendar-day[data-disabled]) {
    cursor: not-allowed;
    opacity: 0.35;
  }

  :global(.pa-calendar-footer) {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 0.65rem;
    align-items: center;
    padding-top: 0.65rem;
    margin-top: 0.55rem;
    border-top: 1px solid var(--color-border);
  }

  :global(.pa-calendar-done) {
    min-height: 32px;
    padding: 0 0.75rem;
    border-radius: var(--radius-sm);
    background: var(--color-primary);
    color: #fff;
    font-size: 0.7rem;
    font-weight: 700;
  }

  :global(.pa-calendar-done:hover) {
    background: var(--color-primary-hover);
  }
</style>
