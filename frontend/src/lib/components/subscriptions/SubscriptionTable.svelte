<script lang="ts">
  import type { Subscription } from '$lib/api/subscriptions';
  import DataTable from '$lib/components/ui/DataTable.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import IdentityCell from '$lib/components/ui/IdentityCell.svelte';
  import ReadableTime from '$lib/components/ui/ReadableTime.svelte';
  import StatusPill from '$lib/components/ui/StatusPill.svelte';
  import { subscriptionKindLabel } from '$lib/labels';
  import { subscriptionPresentation } from '$lib/subscription-status';

  interface Props {
    items: Subscription[];
    selectedId: string | null;
    onSelect: (id: string) => void;
  }

  let { items, selectedId, onSelect }: Props = $props();

  function handleRowKeydown(event: KeyboardEvent, id: string): void {
    if (event.key !== 'Enter' && event.key !== ' ') return;
    event.preventDefault();
    onSelect(id);
  }
</script>

<DataTable ariaLabel="订阅计划" tableClass="subscription-table">
  <colgroup>
    <col class="subscription-name-column" />
    <col class="subscription-kind-column" />
    <col class="subscription-account-column" />
    <col class="subscription-state-column" />
    <col class="subscription-interval-column" />
    <col class="subscription-next-run-column" />
  </colgroup>
  <thead>
    <tr>
      <th>订阅</th>
      <th>类型</th>
      <th>所属账户</th>
      <th>运行状态</th>
      <th>间隔</th>
      <th>下次运行</th>
    </tr>
  </thead>
  <tbody>
    {#each items as subscription (subscription.id)}
      {@const status = subscriptionPresentation(subscription)}
      <tr
        role="button"
        class:selected={selectedId === subscription.id}
        tabindex="0"
        aria-label={`查看订阅${subscription.name}`}
        onclick={() => onSelect(subscription.id)}
        onkeydown={(event) => handleRowKeydown(event, subscription.id)}
      >
        <td><strong class="row-title">{subscription.name}</strong></td>
        <td>{subscriptionKindLabel(subscription.kind)}</td>
        <td>
          <IdentityCell
            src={subscription.account_avatar_url}
            subtitle={`Pixiv ID ${subscription.account_pixiv_user_id}`}
            size={28}
          />
        </td>
        <td>
          <StatusPill label={status.label} tone={status.tone} />
        </td>
        <td>{subscription.schedule.interval_minutes ?? '—'}分钟</td>
        <td>
          <ReadableTime value={subscription.next_run_at} empty="尚未安排" />
        </td>
      </tr>
    {:else}
      <tr>
        <td colspan="6">
          <EmptyState message="没有符合当前筛选条件的订阅" />
        </td>
      </tr>
    {/each}
  </tbody>
</DataTable>

<style>
  :global(.subscription-table) {
    table-layout: fixed;
  }

  .subscription-name-column,
  .subscription-kind-column {
    width: 13%;
  }

  .subscription-account-column {
    width: 27%;
  }

  .subscription-state-column {
    width: 23%;
  }

  .subscription-interval-column {
    width: 11%;
  }

  .subscription-next-run-column {
    width: 13%;
  }

  tbody tr {
    cursor: pointer;
  }

  tbody tr:focus-visible {
    outline: 2px solid var(--color-primary);
    outline-offset: -2px;
  }

  .row-title {
    color: var(--color-text-1);
    font-size: 0.74rem;
  }
</style>
