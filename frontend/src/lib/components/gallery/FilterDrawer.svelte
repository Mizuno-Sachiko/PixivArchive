<script lang="ts">
  import Button from '$lib/components/ui/Button.svelte';
  import CheckboxFilterGroup from '$lib/components/ui/CheckboxFilterGroup.svelte';
  import Field from '$lib/components/ui/Field.svelte';
  import SelectField from '$lib/components/ui/SelectField.svelte';
  import TextField from '$lib/components/ui/TextField.svelte';
  import type { GalleryQueryStore } from '$lib/stores/gallery-query.svelte';

  interface Props {
    query: GalleryQueryStore;
    onApply: () => void;
    onClose: () => void;
  }

  let { query, onApply, onClose }: Props = $props();
</script>

<div
  class="drawer-backdrop"
  role="presentation"
  onclick={(event) => {
    if (event.currentTarget === event.target) onClose();
  }}
>
  <div class="filter-drawer glass-surface" role="dialog" aria-label="筛选条件">
    <header>
      <div>
        <span>STRUCTURED FILTERS</span>
        <h2>筛选条件</h2>
      </div>
      <button type="button" aria-label="关闭筛选条件" onclick={onClose}
        >×</button
      >
    </header>

    <div class="drawer-body">
      <TextField
        label="标签"
        bind:value={query.tagText}
        placeholder="多个标签用逗号分隔"
      />
      <Field label="标签匹配">
        <SelectField
          bind:value={query.tagOperator}
          ariaLabel="标签匹配"
          fullWidth
          options={[
            { value: 'any', label: '任意一个标签' },
            { value: 'all', label: '全部标签' },
            { value: 'exclude_any', label: '排除任意标签' },
            { value: 'not_all', label: '不同时包含全部' },
            { value: 'exact_set', label: '标签集合完全相同' }
          ]}
        />
      </Field>
      <Field label="标签范围">
        <SelectField
          bind:value={query.tagScope}
          ariaLabel="标签范围"
          fullWidth
          options={[
            { value: 'original_and_translation', label: '原标签与翻译' },
            { value: 'original', label: '只看原标签' }
          ]}
        />
      </Field>
      <Field label="收藏数">
        <div class="bookmark-range">
          <input
            type="number"
            min="0"
            step="1"
            bind:value={query.minimumBookmarks}
            placeholder="不限制"
            aria-label="最低收藏数"
          />
          <span aria-hidden="true">—</span>
          <input
            type="number"
            min="0"
            step="1"
            bind:value={query.maximumBookmarks}
            placeholder="不限制"
            aria-label="最高收藏数"
          />
        </div>
        {#if query.validationError}
          <small class="range-error" role="alert">{query.validationError}</small
          >
        {/if}
      </Field>

      <CheckboxFilterGroup
        legend="作品类型"
        bind:values={query.workKinds}
        options={[
          { value: 'illustration', label: '插画' },
          { value: 'manga', label: '漫画' },
          { value: 'ugoira', label: '动图' }
        ]}
      />

      <CheckboxFilterGroup
        legend="年龄分级"
        bind:values={query.ageRatings}
        options={[
          { value: 'all_age', label: '全年龄' },
          { value: 'r18', label: 'R-18' },
          { value: 'r18g', label: 'R-18G' }
        ]}
      />

      <Field label="AI作品">
        <SelectField
          bind:value={query.aiGenerated}
          ariaLabel="AI作品"
          fullWidth
          options={[
            { value: 'any', label: '全部作品' },
            { value: 'yes', label: '只显示AI作品' },
            { value: 'no', label: '排除AI作品' }
          ]}
        />
      </Field>
    </div>

    <footer>
      <Button onclick={() => query.reset()}>清空条件</Button>
      <Button
        variant="primary"
        disabled={Boolean(query.validationError)}
        onclick={() => {
          onApply();
          onClose();
        }}>应用筛选</Button
      >
    </footer>
  </div>
</div>

<style>
  .drawer-backdrop {
    position: fixed;
    z-index: 80;
    inset: 0;
    display: flex;
    justify-content: end;
    background: rgba(2, 8, 14, 0.28);
  }

  .filter-drawer {
    display: grid;
    width: min(430px, 100%);
    height: 100%;
    grid-template-rows: auto 1fr auto;
    border-width: 0 0 0 1px;
    border-radius: 0;
  }

  header,
  footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 1rem 1.1rem;
    border-bottom: 1px solid var(--color-border);
  }

  header span {
    color: var(--color-primary);
    font: 0.62rem var(--font-mono);
    font-weight: 750;
    letter-spacing: 0.1em;
  }

  h2 {
    margin: 0.15rem 0 0;
    font-size: 1.15rem;
  }

  header button {
    width: 36px;
    height: 36px;
    border-radius: 50%;
    background: var(--color-surface-2);
    color: var(--color-text-2);
    font-size: 1.2rem;
  }

  .drawer-body {
    display: grid;
    align-content: start;
    gap: 1rem;
    padding: 1.1rem;
    overflow-y: auto;
  }

  .bookmark-range {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
    align-items: center;
    gap: 0.55rem;
  }

  .bookmark-range span {
    color: var(--color-text-3);
  }

  .bookmark-range input {
    min-width: 0;
  }

  .range-error {
    color: var(--color-error);
    font-size: 0.7rem;
    line-height: 1.4;
  }

  footer {
    justify-content: end;
    border-top: 1px solid var(--color-border);
    border-bottom: 0;
  }
</style>
