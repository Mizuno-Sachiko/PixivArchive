<script lang="ts">
  import { untrack } from 'svelte';

  import {
    systemApi,
    type EffectiveSettings,
    type SavedSetting
  } from '$lib/api/system';
  import Button from '$lib/components/ui/Button.svelte';
  import NumberField from '$lib/components/ui/NumberField.svelte';
  import { ProcessingSettingsDraft } from '$lib/settings/settings-drafts.svelte';

  import SettingsFeedback from './SettingsFeedback.svelte';
  import SettingsCard from './SettingsCard.svelte';

  let {
    value,
    revision,
    onsaved
  }: {
    value: NonNullable<EffectiveSettings['processing']>;
    revision?: number;
    onsaved: (saved: SavedSetting) => void;
  } = $props();
  const draft = untrack(
    () => new ProcessingSettingsDraft(value, revision, systemApi, onsaved)
  );
</script>

<SettingsCard title="请求与处理限制">
  <div class="field-grid">
    <NumberField
      label="Pixiv请求并发"
      min="1"
      bind:value={draft.pixivConcurrency}
      disabled={draft.busy}
    />
    <NumberField
      label="Pixiv每分钟请求"
      min="1"
      bind:value={draft.pixivRate}
      disabled={draft.busy}
    />
    <NumberField
      label="文件下载并发"
      min="1"
      bind:value={draft.downloadConcurrency}
      disabled={draft.busy}
    />
    <NumberField
      label="文件每分钟请求"
      min="1"
      bind:value={draft.downloadRate}
      disabled={draft.busy}
    />
    <NumberField
      label="图片转换并发"
      min="1"
      bind:value={draft.cpuConcurrency}
      disabled={draft.busy}
    />
  </div>
  {#snippet actions()}
    <Button busy={draft.busy} onclick={() => void draft.save()}>
      {draft.busy ? '正在保存' : '保存处理限制'}
    </Button>
  {/snippet}
  {#snippet help()}重启Web与Worker后生效{/snippet}
  {#snippet feedback()}
    <SettingsFeedback message={draft.message} error={draft.error} />
  {/snippet}
</SettingsCard>
