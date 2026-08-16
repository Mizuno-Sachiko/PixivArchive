<script lang="ts">
  import { untrack } from 'svelte';

  import {
    systemApi,
    type EffectiveSettings,
    type SavedSetting
  } from '$lib/api/system';
  import Button from '$lib/components/ui/Button.svelte';
  import SwitchField from '$lib/components/ui/SwitchField.svelte';
  import { PixivSettingsDraft } from '$lib/settings/settings-drafts.svelte';

  import SettingsFeedback from './SettingsFeedback.svelte';
  import SettingsCard from './SettingsCard.svelte';

  let {
    value,
    revision,
    onsaved
  }: {
    value: EffectiveSettings['pixiv'];
    revision?: number;
    onsaved: (saved: SavedSetting) => void;
  } = $props();
  const draft = untrack(
    () => new PixivSettingsDraft(value, revision, systemApi, onsaved)
  );
</script>

<SettingsCard title="Pixiv收藏">
  <SwitchField
    checked={draft.defaultPrivateBookmark}
    label="默认私密收藏"
    disabled={draft.busy}
    onchange={(checked) => (draft.defaultPrivateBookmark = checked)}
  />
  {#snippet actions()}
    <Button busy={draft.busy} onclick={() => void draft.save()}>
      {draft.busy ? '正在保存' : '保存Pixiv设置'}
    </Button>
  {/snippet}
  {#snippet feedback()}
    <SettingsFeedback message={draft.message} error={draft.error} />
  {/snippet}
</SettingsCard>
