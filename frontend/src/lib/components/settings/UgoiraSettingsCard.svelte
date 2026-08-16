<script lang="ts">
  import { untrack } from 'svelte';

  import {
    systemApi,
    type EffectiveSettings,
    type SavedSetting
  } from '$lib/api/system';
  import Button from '$lib/components/ui/Button.svelte';
  import NumberField from '$lib/components/ui/NumberField.svelte';
  import { UgoiraSettingsDraft } from '$lib/settings/settings-drafts.svelte';

  import SettingsFeedback from './SettingsFeedback.svelte';
  import SettingsCard from './SettingsCard.svelte';

  let {
    value,
    revision,
    onsaved
  }: {
    value: NonNullable<EffectiveSettings['ugoira']>;
    revision?: number;
    onsaved: (saved: SavedSetting) => void;
  } = $props();
  const draft = untrack(
    () => new UgoiraSettingsDraft(value, revision, systemApi, onsaved)
  );
</script>

<SettingsCard title="动图处理">
  <div class="field-grid">
    <NumberField
      label="动图ZIP上限（MiB）"
      min="1"
      bind:value={draft.zipMiB}
      disabled={draft.busy}
    />
    <NumberField
      label="动图帧数上限"
      min="1"
      bind:value={draft.frames}
      disabled={draft.busy}
    />
    <NumberField
      label="单帧像素上限"
      min="1"
      bind:value={draft.pixels}
      disabled={draft.busy}
    />
    <NumberField
      label="解码缓存（MiB）"
      min="1"
      bind:value={draft.cacheMiB}
      disabled={draft.busy}
    />
  </div>
  {#snippet actions()}
    <Button busy={draft.busy} onclick={() => void draft.save()}>
      {draft.busy ? '正在保存' : '保存动图设置'}
    </Button>
  {/snippet}
  {#snippet help()}重启Worker后生效{/snippet}
  {#snippet feedback()}
    <SettingsFeedback message={draft.message} error={draft.error} />
  {/snippet}
</SettingsCard>
