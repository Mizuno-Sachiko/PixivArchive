<script lang="ts">
  type IconName =
    | 'archive'
    | 'search'
    | 'command'
    | 'theme'
    | 'user'
    | 'menu'
    | 'close'
    | 'chevron'
    | 'alert'
    | 'check'
    | 'queue'
    | 'database'
    | 'storage'
    | 'tag'
    | 'plus'
    | 'more-horizontal'
    | 'grip-vertical'
    | 'calendar'
    | 'arrow-left'
    | 'arrow-right'
    | 'external-link'
    | 'github';

  interface Props {
    name: IconName;
    size?: number;
  }

  let { name, size = 20 }: Props = $props();
</script>

<svg
  aria-hidden="true"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.9"
  stroke-linecap="round"
  stroke-linejoin="round"
>
  {#if name === 'archive'}
    <path d="M4 7.5h16v11a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2z" />
    <path d="M3 4h18v3.5H3zM9 11h6" />
  {:else if name === 'search'}
    <circle cx="10.8" cy="10.8" r="6.8" />
    <path d="m16 16 4.2 4.2" />
  {:else if name === 'command'}
    <path
      d="M9 6H7.4a3.4 3.4 0 1 0 0 6H9V6Zm6 0h1.6a3.4 3.4 0 1 1 0 6H15V6ZM9 12H7.4a3.4 3.4 0 1 0 3.4 3.4V6M15 12h1.6a3.4 3.4 0 1 1-3.4 3.4V6"
    />
  {:else if name === 'theme'}
    <path
      d="M12 3v2M12 19v2M3 12h2M19 12h2M5.64 5.64l1.42 1.42M16.94 16.94l1.42 1.42M18.36 5.64l-1.42 1.42M7.06 16.94l-1.42 1.42"
    />
    <circle cx="12" cy="12" r="4" />
  {:else if name === 'user'}
    <circle cx="12" cy="8" r="3.5" />
    <path d="M5.5 20c.5-4 2.7-6 6.5-6s6 2 6.5 6" />
  {:else if name === 'menu'}
    <path d="M4 7h16M4 12h16M4 17h16" />
  {:else if name === 'close'}
    <path d="m6 6 12 12M18 6 6 18" />
  {:else if name === 'chevron'}
    <path d="m8.5 10 3.5 3.5 3.5-3.5" />
  {:else if name === 'alert'}
    <path
      d="M10.3 4.1 2.7 17.2A1.9 1.9 0 0 0 4.3 20h15.4a1.9 1.9 0 0 0 1.6-2.8L13.7 4.1a2 2 0 0 0-3.4 0Z"
    />
    <path d="M12 9v4M12 16.5h.01" />
  {:else if name === 'check'}
    <path d="m5 12.5 4.2 4.2L19 7" />
  {:else if name === 'queue'}
    <path d="M4 6h11M4 12h16M4 18h8" />
    <circle cx="19" cy="6" r="1.5" />
  {:else if name === 'database'}
    <ellipse cx="12" cy="5.5" rx="7.5" ry="3" />
    <path
      d="M4.5 5.5v6c0 1.7 3.4 3 7.5 3s7.5-1.3 7.5-3v-6M4.5 11.5v6c0 1.7 3.4 3 7.5 3s7.5-1.3 7.5-3v-6"
    />
  {:else if name === 'storage'}
    <path d="M5 4h14l2 5H3zM4 9v10a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1V9M8 14h8" />
  {:else if name === 'tag'}
    <path d="M20 12.5 12.5 20 4 11.5V4h7.5z" />
    <circle cx="8.5" cy="8.5" r="1.25" />
  {:else if name === 'plus'}
    <path d="M12 5v14M5 12h14" />
  {:else if name === 'more-horizontal'}
    <circle cx="5" cy="12" r="1" fill="currentColor" stroke="none" />
    <circle cx="12" cy="12" r="1" fill="currentColor" stroke="none" />
    <circle cx="19" cy="12" r="1" fill="currentColor" stroke="none" />
  {:else if name === 'grip-vertical'}
    <circle cx="9" cy="5" r="1" fill="currentColor" stroke="none" />
    <circle cx="15" cy="5" r="1" fill="currentColor" stroke="none" />
    <circle cx="9" cy="12" r="1" fill="currentColor" stroke="none" />
    <circle cx="15" cy="12" r="1" fill="currentColor" stroke="none" />
    <circle cx="9" cy="19" r="1" fill="currentColor" stroke="none" />
    <circle cx="15" cy="19" r="1" fill="currentColor" stroke="none" />
  {:else if name === 'calendar'}
    <rect x="3.5" y="5.5" width="17" height="15" rx="2" />
    <path d="M7.5 3.5v4M16.5 3.5v4M3.5 10h17" />
  {:else if name === 'arrow-left'}
    <path d="m10 6-6 6 6 6M4 12h16" />
  {:else if name === 'arrow-right'}
    <path d="m14 6 6 6-6 6M20 12H4" />
  {:else if name === 'external-link'}
    <path d="M14 4h6v6M20 4l-9 9" />
    <path d="M18 13v5a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h5" />
  {:else if name === 'github'}
    <path
      d="M15 22v-4a4.8 4.8 0 0 0-1-3.5c3.3-.4 6.7-1.6 6.7-7.2A5.7 5.7 0 0 0 19.2 3.3 5.4 5.4 0 0 0 19.1 1S17.9.6 15 2.5a13.4 13.4 0 0 0-7 0C5.1.6 3.9 1 3.9 1a5.4 5.4 0 0 0-.1 2.3 5.7 5.7 0 0 0-1.5 4c0 5.6 3.4 6.8 6.7 7.2A4.8 4.8 0 0 0 8 18v4M8 19c-3 .9-3-1.5-4-2"
    />
  {/if}
</svg>
