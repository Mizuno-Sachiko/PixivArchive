<script lang="ts">
  import { untrack } from 'svelte';

  import {
    systemApi,
    type EffectiveSettings,
    type SavedSetting
  } from '$lib/api/system';
  import Button from '$lib/components/ui/Button.svelte';
  import SwitchField from '$lib/components/ui/SwitchField.svelte';
  import { ContentSettingsDraft } from '$lib/settings/settings-drafts.svelte';

  import SettingsFeedback from './SettingsFeedback.svelte';
  import SettingsCard from './SettingsCard.svelte';

  let {
    value,
    revision,
    onsaved,
    onvaluechange,
    onshuffle
  }: {
    value: EffectiveSettings['content'];
    revision?: number;
    onsaved: (saved: SavedSetting) => void;
    onvaluechange: (value: EffectiveSettings['content']) => void;
    onshuffle: () => Promise<void>;
  } = $props();
  const draft = untrack(
    () => new ContentSettingsDraft(value, revision, systemApi, onsaved)
  );
  let shuffling = $state(false);

  async function save(): Promise<void> {
    if (await draft.save()) onvaluechange(draft.value());
  }

  async function shuffle(): Promise<void> {
    if (draft.busy || draft.dirty || shuffling) return;
    shuffling = true;
    draft.message = '';
    draft.error = '';
    try {
      await onshuffle();
      draft.message = '概览装饰图已经重新选择';
    } catch {
      draft.error = '概览装饰图重新选择失败';
    } finally {
      shuffling = false;
    }
  }
</script>

<SettingsCard title="非全年龄内容">
  <SwitchField
    checked={draft.overviewAllowNsfw}
    label="概览装饰图允许R-18内容"
    disabled={draft.busy || draft.maskNonAllAgeThumbnails}
    onchange={(checked) => (draft.overviewAllowNsfw = checked)}
  />
  <SwitchField
    checked={draft.maskNonAllAgeThumbnails}
    label="遮挡非全年龄缩略图"
    description="R-18、R-18G及分级未知作品显示为占位图。作品详情和大图查看不受影响。"
    disabled={draft.busy}
    onchange={(checked) => draft.setThumbnailMasking(checked)}
  />
  {#snippet actions()}
    <Button
      disabled={draft.busy || shuffling || !draft.dirty}
      onclick={() => void save()}
    >
      {draft.busy ? '正在保存' : '保存显示设置'}
    </Button>
    <Button
      disabled={draft.busy || shuffling || draft.dirty}
      onclick={() => void shuffle()}
    >
      {shuffling ? '正在重新选择' : '重新随机概览装饰图'}
    </Button>
  {/snippet}
  {#snippet feedback()}
    <SettingsFeedback message={draft.message} error={draft.error} />
  {/snippet}
</SettingsCard>
