<script lang="ts" module>
  import { ruleActionLabel, type RuleSummary } from '$lib/api/rules';

  function lifecycleLabel(rule: RuleSummary): string {
    return {
      draft: '草稿',
      modified: '有未保存修改',
      published: '已保存'
    }[rule.lifecycle];
  }
</script>

<script lang="ts">
  import Icon from '$lib/components/ui/Icon.svelte';

  interface Props {
    rule: RuleSummary;
    index: number;
    active: boolean;
    busy: boolean;
    canDrag: boolean;
    onSelect: (ruleId: string) => Promise<void> | void;
    onRename: (ruleId: string, name: string) => Promise<boolean>;
    onEnabledChange: (ruleId: string, enabled: boolean) => Promise<boolean>;
    onCopy: (ruleId: string) => Promise<boolean>;
    onImport: (ruleId: string) => Promise<void> | void;
    onExport: (ruleId: string) => Promise<void> | void;
    onDelete: (
      ruleId: string,
      returnFocus: HTMLElement
    ) => Promise<void> | void;
    onDragStart: (ruleId: string) => void;
    onDragOver: (ruleId: string, after: boolean) => void;
    onDrop: () => Promise<void> | void;
    onDragEnd: () => void;
  }

  let {
    rule,
    index,
    active,
    busy,
    canDrag,
    onSelect,
    onRename,
    onEnabledChange,
    onCopy,
    onImport,
    onExport,
    onDelete,
    onDragStart,
    onDragOver,
    onDrop,
    onDragEnd
  }: Props = $props();

  let menuOpen = $state(false);
  let menuTrigger = $state<HTMLButtonElement>();
  let renameOpen = $state(false);
  let renameValue = $state('');

  function beginRename(): void {
    renameValue = rule.name;
    menuOpen = false;
    renameOpen = true;
  }

  async function finishRename(): Promise<void> {
    if (await onRename(rule.id, renameValue)) renameOpen = false;
  }

  function runAction(action: () => Promise<unknown> | unknown): void {
    menuOpen = false;
    void action();
  }

  function beginDelete(): void {
    const returnFocus = menuTrigger;
    if (!returnFocus) return;
    runAction(() => onDelete(rule.id, returnFocus));
  }
</script>

<article
  class:active
  class:disabled={!rule.enabled}
  data-rule-id={rule.id}
  ondragover={(event) => {
    if (!canDrag) return;
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'move';
    const bounds = event.currentTarget.getBoundingClientRect();
    onDragOver(rule.id, event.clientY >= bounds.top + bounds.height / 2);
  }}
  ondrop={(event) => {
    if (!canDrag) return;
    event.preventDefault();
    void onDrop();
  }}
>
  <button
    class="drag-handle"
    type="button"
    aria-label={`拖动规则 ${rule.name}`}
    title={canDrag ? '拖动排序' : '清除搜索后可排序'}
    draggable={canDrag}
    disabled={!canDrag}
    ondragstart={(event) => {
      event.dataTransfer?.setData('text/plain', rule.id);
      if (event.dataTransfer) event.dataTransfer.effectAllowed = 'move';
      onDragStart(rule.id);
    }}
    ondragend={onDragEnd}
  >
    <Icon name="grip-vertical" size={16} />
  </button>

  {#if renameOpen}
    <div class="rule-select">
      <span class="order">{String(index + 1).padStart(2, '0')}</span>
      <span class="rule-copy">
        <input
          bind:value={renameValue}
          aria-label={`重命名规则 ${rule.name}`}
          autocomplete="off"
          disabled={busy}
          onkeydown={(event) => {
            if (event.key === 'Enter') void finishRename();
            if (event.key === 'Escape') renameOpen = false;
          }}
        />
        <small>
          {rule.enabled ? ruleActionLabel(rule.action) : '已停用'} · {lifecycleLabel(
            rule
          )}
        </small>
      </span>
    </div>
  {:else}
    <button
      class="rule-select"
      type="button"
      aria-label={`选择规则 ${rule.name}`}
      onclick={() => void onSelect(rule.id)}
    >
      <span class="order">{String(index + 1).padStart(2, '0')}</span>
      <span class="rule-copy">
        <strong>{rule.name}</strong>
        <small>
          {rule.enabled ? ruleActionLabel(rule.action) : '已停用'} · {lifecycleLabel(
            rule
          )}
        </small>
      </span>
    </button>
  {/if}

  <label class="rule-switch">
    <input
      type="checkbox"
      role="switch"
      aria-label={`启用规则 ${rule.name}`}
      checked={rule.enabled}
      disabled={busy}
      onchange={(event) =>
        void onEnabledChange(rule.id, event.currentTarget.checked)}
    />
    <span aria-hidden="true"></span>
  </label>

  <div class="row-actions">
    <button
      bind:this={menuTrigger}
      class="menu-trigger"
      type="button"
      aria-label={`规则操作 ${rule.name}`}
      aria-expanded={menuOpen}
      disabled={busy}
      onclick={() => (menuOpen = !menuOpen)}
    >
      <Icon name="more-horizontal" size={17} />
    </button>
    {#if menuOpen}
      <div class="action-menu">
        <button type="button" onclick={beginRename}>重命名</button>
        <button type="button" onclick={() => runAction(() => onCopy(rule.id))}
          >复制</button
        >
        <button type="button" onclick={() => runAction(() => onImport(rule.id))}
          >导入JSON</button
        >
        <button type="button" onclick={() => runAction(() => onExport(rule.id))}
          >导出JSON</button
        >
        <button class="danger" type="button" onclick={beginDelete}
          >删除规则</button
        >
      </div>
    {/if}
  </div>

  <i class="active-marker" aria-hidden="true"></i>
</article>

<style>
  article {
    position: relative;
    display: grid;
    min-height: 64px;
    grid-template-columns: 28px minmax(0, 1fr) 32px 32px 3px;
    gap: 0.35rem;
    align-items: center;
    padding: 0.45rem 0.55rem 0.45rem 0.45rem;
  }

  article:hover {
    background: var(--color-surface-2);
  }

  article.active {
    background: var(--color-primary-soft);
  }

  article.disabled .rule-select {
    opacity: 0.58;
  }

  .drag-handle,
  .menu-trigger {
    display: grid;
    width: 30px;
    height: 30px;
    place-items: center;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-3);
  }

  .drag-handle:not(:disabled) {
    cursor: grab;
  }

  .drag-handle:not(:disabled):active {
    cursor: grabbing;
  }

  .drag-handle:disabled {
    cursor: not-allowed;
    opacity: 0.34;
  }

  .menu-trigger:hover,
  .menu-trigger[aria-expanded='true'] {
    background: var(--color-surface-1);
    color: var(--color-text-1);
  }

  .rule-select {
    display: grid;
    min-width: 0;
    grid-template-columns: 27px minmax(0, 1fr);
    gap: 0.45rem;
    align-items: center;
    padding: 0;
    background: transparent;
    text-align: left;
  }

  .order {
    color: var(--color-text-3);
    font: 0.65rem var(--font-mono);
  }

  .rule-copy {
    min-width: 0;
  }

  .rule-copy strong,
  .rule-copy small {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .rule-copy strong {
    font-size: 0.8rem;
  }

  .rule-copy small {
    margin-top: 0.22rem;
    color: var(--color-text-3);
    font-size: 0.65rem;
  }

  .rule-copy input {
    width: 100%;
    height: 28px;
    padding: 0 0.45rem;
    border: 1px solid var(--color-primary);
    border-radius: 6px;
    outline: none;
    background: var(--color-surface-1);
    color: var(--color-text-1);
    font-size: 0.75rem;
  }

  .rule-switch {
    position: relative;
    display: block;
    width: 30px;
    height: 18px;
  }

  .rule-switch input {
    position: absolute;
    width: 1px;
    height: 1px;
    opacity: 0;
  }

  .rule-switch span {
    display: block;
    width: 30px;
    height: 18px;
    border-radius: var(--radius-pill);
    background: var(--color-border-strong);
    cursor: pointer;
    transition: background 120ms ease;
  }

  .rule-switch span::after {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 3px rgba(2, 8, 14, 0.22);
    content: '';
    transition: transform 120ms ease;
  }

  .rule-switch input:checked + span {
    background: var(--color-primary);
  }

  .rule-switch input:checked + span::after {
    transform: translateX(12px);
  }

  .rule-switch input:focus-visible + span {
    outline: 2px solid var(--color-primary);
    outline-offset: 2px;
  }

  .rule-switch input:disabled + span {
    cursor: not-allowed;
    opacity: 0.48;
  }

  .row-actions {
    position: relative;
  }

  .action-menu {
    position: absolute;
    z-index: 8;
    top: 34px;
    right: 0;
    display: grid;
    width: 138px;
    padding: 0.35rem;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-glass-strong);
    box-shadow: var(--shadow-float);
    backdrop-filter: blur(18px);
  }

  .action-menu button {
    padding: 0.58rem 0.65rem;
    border-radius: 6px;
    background: transparent;
    text-align: left;
    font-size: 0.72rem;
  }

  .action-menu button:hover {
    background: var(--color-primary-soft);
    color: var(--color-primary);
  }

  .action-menu .danger {
    color: var(--color-error);
  }

  .active-marker {
    width: 3px;
    height: 28px;
    border-radius: 3px;
  }

  article.active .active-marker {
    background: var(--color-primary);
  }

  @media (prefers-reduced-motion: reduce) {
    .rule-switch span,
    .rule-switch span::after {
      transition: none;
    }
  }
</style>
