<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { onMount } from 'svelte';

  import { ApiError } from '$lib/api/client';
  import { csrfStore } from '$lib/stores/csrf.svelte';
  import { sessionStore } from '$lib/stores/session.svelte';
  import Brand from '$lib/components/shell/Brand.svelte';

  let password = $state('');
  let submitting = $state(false);
  let restoring = $state(true);
  let errorMessage = $state('');
  let traceId = $state<string | null>(null);

  onMount(() => {
    let disposed = false;
    void sessionStore
      .restore()
      .then((session) => {
        if (!disposed && session) {
          void goto(resolve('/overview'), { replaceState: true });
        }
      })
      .catch(() => undefined)
      .finally(() => {
        if (!disposed) restoring = false;
      });
    return () => {
      disposed = true;
    };
  });

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (restoring || submitting) return;
    submitting = true;
    errorMessage = '';
    traceId = null;
    try {
      await sessionStore.signIn({ password });
      csrfStore.refresh();
      await goto(resolve('/overview'));
    } catch (error) {
      if (error instanceof ApiError) {
        errorMessage =
          error.code === 'invalid_credentials'
            ? '密码不正确'
            : error.code === 'rate_limited'
              ? '登录尝试过于频繁，请稍后再试'
              : '登录服务暂时不可用';
        traceId = error.traceId || null;
      } else {
        errorMessage = '无法连接到PixivArchive';
      }
    } finally {
      submitting = false;
    }
  }
</script>

<svelte:head>
  <title>登录 · PixivArchive</title>
</svelte:head>

<main class="login-page">
  <section class="login-panel">
    <div class="brand-row"><Brand /></div>
    <div class="form-wrap">
      <div class="intro">
        <h1>进入PixivArchive</h1>
      </div>

      <form onsubmit={submit}>
        <label for="password">管理员密码</label>
        <input
          id="password"
          bind:value={password}
          type="password"
          autocomplete="current-password"
          required
        />

        {#if errorMessage}
          <div class="login-error" role="alert">
            <strong>{errorMessage}</strong>
            {#if traceId}<span>跟踪ID {traceId}</span>{/if}
          </div>
        {/if}

        <button
          class="submit-button"
          type="submit"
          disabled={restoring || submitting || !password}
        >
          {restoring ? '正在验证会话…' : submitting ? '正在登录…' : '登录'}
        </button>
      </form>

      <p class="recovery-note">
        忘记密码时，请修改部署环境文件中的管理员密码并重启。
      </p>
    </div>
  </section>

  <section class="archive-field" aria-label="PixivArchive馆藏索引">
    <div class="contact-sheet" aria-hidden="true">
      <div class="archive-tile tile-a"><span>ILLUST</span><b>1001</b></div>
      <div class="archive-tile tile-b">
        <span>MANGA · 08P</span><b>1002</b>
      </div>
      <div class="archive-tile tile-c"><span>UGOIRA</span><b>1003</b></div>
      <div class="archive-tile tile-d"><span>BOOKMARKS</span><b>12,482</b></div>
      <div class="archive-tile tile-e">
        <span>RULE TRACE</span><b>DOWNLOAD</b>
      </div>
    </div>
  </section>
</main>

<style>
  .login-page {
    display: grid;
    min-height: 100vh;
    grid-template-columns: minmax(390px, 0.82fr) minmax(520px, 1.18fr);
    padding: 0;
  }

  .login-panel {
    position: relative;
    display: grid;
    min-height: 100vh;
    grid-template-rows: auto 1fr;
    padding: 30px clamp(28px, 5vw, 72px);
    background: var(--color-surface-1);
  }

  .brand-row {
    align-self: start;
  }

  .form-wrap {
    width: min(100%, 420px);
    align-self: center;
    justify-self: center;
    padding: 32px 0 84px;
  }

  h1 {
    margin: 0;
    font-size: clamp(2rem, 4vw, 2.75rem);
    font-weight: 760;
    letter-spacing: -0.055em;
    line-height: 1.08;
  }

  form {
    margin-top: 2.4rem;
  }

  label {
    display: block;
    margin-bottom: 0.5rem;
    color: var(--color-text-2);
    font-size: 0.82rem;
    font-weight: 650;
  }

  input {
    width: 100%;
    height: 48px;
    padding: 0 0.9rem;
    border: 1px solid transparent;
    border-radius: 10px;
    outline: 0;
    background: var(--color-surface-2);
  }

  input:focus {
    border-color: var(--color-primary);
    box-shadow: 0 0 0 3px var(--color-primary-soft);
  }

  .login-error {
    display: grid;
    gap: 0.25rem;
    padding: 0.7rem 0.8rem;
    border-radius: 9px;
    margin: 0.4rem 0 0.8rem;
    background: var(--color-error-soft);
    color: var(--color-error);
    font-size: 0.8rem;
  }

  .login-error span {
    font-family: var(--font-mono);
    font-size: 0.65rem;
  }

  .submit-button {
    width: 100%;
    height: 48px;
    border-radius: 10px;
    margin-top: 0.65rem;
    background: var(--color-primary);
    color: #fff;
    font-weight: 720;
  }

  .submit-button:hover:not(:disabled) {
    background: var(--color-primary-hover);
  }

  .submit-button:active:not(:disabled) {
    background: var(--color-primary-pressed);
  }

  .submit-button:disabled {
    cursor: default;
    opacity: 0.48;
  }

  .recovery-note {
    margin: 1rem 0 0;
    color: var(--color-text-3);
    font-size: 0.74rem;
    line-height: 1.55;
  }

  .archive-field {
    position: relative;
    display: grid;
    min-height: 100vh;
    align-content: center;
    padding: clamp(44px, 6vw, 90px);
    overflow: hidden;
    background: #071521;
    color: #eaf6ff;
  }

  .archive-field::before {
    position: absolute;
    inset: 0;
    background:
      linear-gradient(rgba(0, 150, 250, 0.055) 1px, transparent 1px),
      linear-gradient(90deg, rgba(0, 150, 250, 0.055) 1px, transparent 1px);
    background-size: 52px 52px;
    content: '';
    mask-image: linear-gradient(
      to bottom,
      transparent,
      black 18%,
      black 82%,
      transparent
    );
  }

  .contact-sheet {
    position: relative;
    z-index: 1;
  }

  .contact-sheet {
    display: grid;
    height: min(48vh, 460px);
    grid-template-columns: 1.05fr 0.78fr 1.12fr;
    grid-template-rows: 1fr 0.72fr;
    gap: 12px;
    margin-top: clamp(34px, 5vh, 58px);
    transform: rotate(-1.2deg);
  }

  .archive-tile {
    position: relative;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    padding: 16px;
    overflow: hidden;
    border: 1px solid rgba(155, 214, 255, 0.16);
    border-radius: 14px;
    background: #102b3d;
    box-shadow: 0 22px 54px rgba(0, 0, 0, 0.24);
  }

  .archive-tile::before,
  .archive-tile::after {
    position: absolute;
    border-radius: 50%;
    content: '';
    filter: blur(2px);
  }

  .archive-tile::before {
    width: 72%;
    aspect-ratio: 1;
    background: rgba(0, 150, 250, 0.48);
    transform: translate(42%, -32%);
  }

  .archive-tile::after {
    right: 14%;
    bottom: -36%;
    width: 54%;
    aspect-ratio: 0.72;
    background: rgba(169, 98, 218, 0.34);
    transform: rotate(36deg);
  }

  .archive-tile span,
  .archive-tile b {
    position: relative;
    z-index: 1;
    font: 0.64rem var(--font-mono);
    letter-spacing: 0.08em;
  }

  .archive-tile span {
    color: rgba(234, 246, 255, 0.66);
  }

  .archive-tile b {
    color: #ffffff;
  }

  .tile-a {
    grid-row: span 2;
  }

  .tile-b::before {
    background: rgba(87, 196, 166, 0.48);
  }

  .tile-c {
    grid-row: span 2;
  }

  .tile-c::before {
    background: rgba(255, 117, 132, 0.45);
  }

  .tile-d::before {
    background: rgba(246, 181, 71, 0.44);
  }

  .tile-e {
    display: none;
  }

  @media (max-width: 900px) {
    .login-page {
      grid-template-columns: 1fr;
    }

    .login-panel {
      z-index: 2;
      min-height: 100vh;
      background: color-mix(in srgb, var(--color-surface-1) 94%, transparent);
    }

    .archive-field {
      position: fixed;
      z-index: 0;
      inset: 0;
      opacity: 0.16;
    }
  }

  @media (max-width: 520px) {
    .login-panel {
      padding: 22px 20px;
    }

    .form-wrap {
      padding-bottom: 48px;
    }
  }
</style>
