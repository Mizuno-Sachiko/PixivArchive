<script lang="ts">
  import { untrack } from 'svelte';

  import {
    systemApi,
    type EffectiveSettings,
    type SavedSetting
  } from '$lib/api/system';
  import Button from '$lib/components/ui/Button.svelte';
  import NumberField from '$lib/components/ui/NumberField.svelte';
  import { QueueSettingsDraft } from '$lib/settings/settings-drafts.svelte';

  import SettingsFeedback from './SettingsFeedback.svelte';
  import SettingsCard from './SettingsCard.svelte';

  let {
    value,
    revision,
    onsaved
  }: {
    value: EffectiveSettings['queue'];
    revision?: number;
    onsaved: (saved: SavedSetting) => void;
  } = $props();
  const draft = untrack(
    () => new QueueSettingsDraft(value, revision, systemApi, onsaved)
  );
</script>

<SettingsCard title="任务队列配额">
  <div class="field-grid">
    <NumberField
      label="即时操作配额"
      min="1"
      bind:value={draft.immediate}
      disabled={draft.busy}
    />
    <NumberField
      label="手动导入配额"
      min="1"
      bind:value={draft.manualImport}
      disabled={draft.busy}
    />
    <NumberField
      label="定时采集配额"
      min="1"
      bind:value={draft.scheduledCollection}
      disabled={draft.busy}
    />
    <NumberField
      label="后台维护配额"
      min="1"
      bind:value={draft.backgroundMaintenance}
      disabled={draft.busy}
    />
  </div>
  {#snippet actions()}
    <Button busy={draft.busy} onclick={() => void draft.save()}>
      {draft.busy ? '正在保存' : '保存队列设置'}
    </Button>
  {/snippet}
  {#snippet help()}新建任务使用新的类型映射；Worker队列配额重启后生效{/snippet}
  {#snippet feedback()}
    <SettingsFeedback message={draft.message} error={draft.error} />
  {/snippet}
</SettingsCard>
