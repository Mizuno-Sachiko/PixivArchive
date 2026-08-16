<script lang="ts">
  import { untrack } from 'svelte';

  import {
    systemApi,
    type EffectiveSettings,
    type SavedSetting
  } from '$lib/api/system';
  import Button from '$lib/components/ui/Button.svelte';
  import Field from '$lib/components/ui/Field.svelte';
  import NumberField from '$lib/components/ui/NumberField.svelte';
  import SelectField from '$lib/components/ui/SelectField.svelte';
  import TextField from '$lib/components/ui/TextField.svelte';
  import { CollectionSettingsDraft } from '$lib/settings/settings-drafts.svelte';

  import SettingsFeedback from './SettingsFeedback.svelte';
  import SettingsCard from './SettingsCard.svelte';

  let {
    retry,
    derivative,
    avifAvailable,
    revisions,
    onsaved
  }: {
    retry: EffectiveSettings['retry'];
    derivative: EffectiveSettings['derivative'];
    avifAvailable: boolean;
    revisions: Record<'retry' | 'derivative', number | undefined>;
    onsaved: (saved: SavedSetting) => void;
  } = $props();
  const draft = untrack(
    () =>
      new CollectionSettingsDraft(
        retry,
        derivative,
        avifAvailable,
        revisions,
        systemApi,
        onsaved
      )
  );
</script>

<SettingsCard title="采集默认值">
  <div class="field-grid">
    {#if avifAvailable}
      <Field label="默认浏览图格式">
        <SelectField
          bind:value={draft.derivativeFormat}
          ariaLabel="默认浏览图格式"
          fullWidth
          disabled={draft.busy}
          options={[
            { value: 'webp', label: 'WebP' },
            { value: 'avif', label: 'AVIF' }
          ]}
        />
      </Field>
    {/if}
    <TextField
      label="网络错误退避秒数"
      wide
      bind:value={draft.retryBackoff}
      disabled={draft.busy}
    />
    <NumberField
      label="缩略图宽度"
      min="1"
      bind:value={draft.derivativeWidth}
      disabled={draft.busy}
    />
    {#if draft.derivativeFormat === 'webp'}
      <NumberField
        label="WebP质量"
        min="1"
        max="100"
        bind:value={draft.webpQuality}
        disabled={draft.busy}
      />
    {:else}
      <NumberField
        label="AVIF质量"
        min="1"
        max="100"
        bind:value={draft.avifQuality}
        disabled={draft.busy}
      />
    {/if}
  </div>
  {#snippet actions()}
    <Button busy={draft.busy} onclick={() => void draft.save()}>
      {draft.busy ? '正在保存' : '保存采集默认值'}
    </Button>
  {/snippet}
  {#snippet help()}重启Worker后生效{/snippet}
  {#snippet feedback()}
    <SettingsFeedback message={draft.message} error={draft.error} />
  {/snippet}
</SettingsCard>
