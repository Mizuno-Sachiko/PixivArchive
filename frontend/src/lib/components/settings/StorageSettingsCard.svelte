<script lang="ts">
  import { untrack } from 'svelte';

  import {
    systemApi,
    type EffectiveSettings,
    type SavedSetting
  } from '$lib/api/system';
  import Button from '$lib/components/ui/Button.svelte';
  import NumberField from '$lib/components/ui/NumberField.svelte';
  import TextField from '$lib/components/ui/TextField.svelte';
  import { StorageSettingsDraft } from '$lib/settings/settings-drafts.svelte';

  import SettingsFeedback from './SettingsFeedback.svelte';
  import SettingsCard from './SettingsCard.svelte';

  let {
    value,
    activeMediaRoot,
    revision,
    onsaved
  }: {
    value: EffectiveSettings['storage'];
    activeMediaRoot: string;
    revision?: number;
    onsaved: (saved: SavedSetting) => void;
  } = $props();
  const draft = untrack(
    () =>
      new StorageSettingsDraft(
        value,
        activeMediaRoot,
        revision,
        systemApi,
        onsaved
      )
  );
</script>

<SettingsCard title="存储与回收站">
  <TextField
    label="图片存储目录"
    description="填写当前部署系统中的绝对路径"
    bind:value={draft.mediaRoot}
    placeholder="/srv/pixivarchive/media"
    autocomplete="off"
    spellcheck="false"
    disabled={draft.busy}
    inputClass="media-root-input"
  />
  <div class="field-grid">
    <NumberField
      label="空间预警阈值（GiB）"
      min="1"
      bind:value={draft.warningGiB}
      disabled={draft.busy}
    />
    <NumberField
      label="停止写入阈值（GiB）"
      min="1"
      bind:value={draft.writeStopGiB}
      disabled={draft.busy}
    />
    <NumberField
      label="新删除作品保留天数"
      min="1"
      max="365"
      bind:value={draft.trashDays}
      disabled={draft.busy}
    />
  </div>
  {#snippet actions()}
    <Button busy={draft.busy} onclick={() => void draft.save()}>
      {draft.busy ? '正在保存' : '保存存储设置'}
    </Button>
  {/snippet}
  {#snippet help()}
    修改图片目录后，请自行搬迁已有图片并重启Web和Worker；应用不会移动现有文件。停止写入阈值需重启Worker。
  {/snippet}
  {#snippet feedback()}
    <SettingsFeedback message={draft.message} error={draft.error} />
  {/snippet}
</SettingsCard>

<style>
  :global(.media-root-input) {
    font-family: var(--font-mono);
  }
</style>
