<script lang="ts">
  import { onMount, tick } from 'svelte';

  import type { GalleryPage, GalleryWorkDetail } from '$lib/api/gallery';
  import Icon from '$lib/components/ui/Icon.svelte';
  import PixivSourceLink from '$lib/components/ui/PixivSourceLink.svelte';
  import { activateModal, trapModalFocus } from '$lib/modal-focus';
  import type { UgoiraDecodeLimits } from '$lib/workers/ugoira-protocol';

  import UgoiraCanvas from './UgoiraCanvas.svelte';
  import {
    accumulateWheelDelta,
    centeredScrollOffset,
    clampZoom,
    dominantColorLuminance,
    mediaAspect,
    normalizedScrollCenter
  } from './viewer-math';

  const MIN_ZOOM = 0.25;
  const MAX_ZOOM = 4;
  const ZOOM_STEP = 0.25;
  const CONTROLS_HIDE_DELAY = 1800;
  const WHEEL_PAGE_THRESHOLD = 80;

  interface Props {
    detail: GalleryWorkDetail;
    initialPageIndex?: number;
    ugoiraLimits?: UgoiraDecodeLimits;
    returnFocus?: HTMLElement | null;
    onClose: () => void;
  }

  let {
    detail,
    initialPageIndex = 0,
    ugoiraLimits,
    returnFocus,
    onClose
  }: Props = $props();
  let dialog = $state<HTMLDialogElement>();
  let fullscreenSurface = $state<HTMLDivElement>();
  let stage = $state<HTMLDivElement>();
  let pageIndex = $state(0);
  let playing = $state(true);
  let zoom = $state(1);
  let dragging = $state(false);
  let controlsVisible = $state(true);
  let fullscreenActive = $state(false);
  let fullscreenChanging = $state(false);
  let dragPointerId: number | null = null;
  let dragOriginX = 0;
  let dragOriginY = 0;
  let dragScrollLeft = 0;
  let dragScrollTop = 0;
  let controlsTimer: ReturnType<typeof setTimeout> | null = null;
  let wheelResetTimer: ReturnType<typeof setTimeout> | null = null;
  let wheelUnlockTimer: ReturnType<typeof setTimeout> | null = null;
  let wheelDelta = 0;
  let wheelLocked = false;
  let currentPage = $derived(detail.pages[pageIndex]);
  let currentAspect = $derived(
    mediaAspect(currentPage?.width, currentPage?.height)
  );
  let currentInverseAspect = $derived(1 / currentAspect);
  let currentDominantColor = $derived(
    currentPage?.current_media?.derivatives[0]?.dominant_color ??
      'var(--color-viewer-bg)'
  );
  let useDarkControls = $derived(
    dominantColorLuminance(currentDominantColor) >= 0.58
  );
  let isUgoira = $derived(
    Boolean(
      detail.ugoira && currentPage?.current_media?.media_kind === 'ugoira_zip'
    )
  );

  onMount(() => {
    pageIndex = initialPageIndex;
    const mountedDialog = dialog;
    if (!mountedDialog) return;
    const releaseModal = activateModal(
      mountedDialog,
      mountedDialog,
      returnFocus
    );
    document.addEventListener('fullscreenchange', syncFullscreenState);
    scheduleControlsHide();
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    return () => {
      document.body.style.overflow = previousOverflow;
      if (controlsTimer) clearTimeout(controlsTimer);
      if (wheelResetTimer) clearTimeout(wheelResetTimer);
      if (wheelUnlockTimer) clearTimeout(wheelUnlockTimer);
      document.removeEventListener('fullscreenchange', syncFullscreenState);
      releaseModal();
    };
  });

  async function selectPage(next: number): Promise<void> {
    if (next < 0 || next >= detail.pages.length) return;
    pageIndex = next;
    zoom = 1;
    await tick();
    stage?.scrollTo({ left: 0, top: 0 });
  }

  function handleKey(event: KeyboardEvent): void {
    revealControls();
    if (dialog) trapModalFocus(dialog, event);
    if (event.defaultPrevented) return;
    if (event.key === 'ArrowLeft') void selectPage(pageIndex - 1);
    else if (event.key === 'ArrowRight') void selectPage(pageIndex + 1);
    else if (event.key === ' ' && isUgoira) {
      event.preventDefault();
      playing = !playing;
    } else if (event.key === '+' || event.key === '=') {
      void changeZoom(zoom + ZOOM_STEP);
    } else if (event.key === '-') {
      void changeZoom(zoom - ZOOM_STEP);
    }
  }

  function revealControls(): void {
    controlsVisible = true;
    scheduleControlsHide();
  }

  function scheduleControlsHide(): void {
    if (controlsTimer) clearTimeout(controlsTimer);
    controlsTimer = setTimeout(() => {
      controlsVisible = false;
    }, CONTROLS_HIDE_DELAY);
  }

  function handlePointerMove(event: PointerEvent): void {
    const revealZone = 112;
    if (
      event.clientY <= revealZone ||
      event.clientY >= window.innerHeight - revealZone
    ) {
      revealControls();
    }
  }

  function handleWheel(event: WheelEvent): void {
    if (detail.pages.length <= 1) return;
    event.preventDefault();
    if (wheelLocked) return;
    const wheel = accumulateWheelDelta(
      wheelDelta,
      event.deltaY,
      WHEEL_PAGE_THRESHOLD
    );
    wheelDelta = wheel.accumulated;
    if (wheelResetTimer) clearTimeout(wheelResetTimer);
    wheelResetTimer = setTimeout(() => {
      wheelDelta = 0;
    }, 160);
    if (wheel.direction === 0) return;
    const target = pageIndex + wheel.direction;
    if (target < 0 || target >= detail.pages.length) return;
    wheelLocked = true;
    void selectPage(target);
    if (wheelUnlockTimer) clearTimeout(wheelUnlockTimer);
    wheelUnlockTimer = setTimeout(() => {
      wheelLocked = false;
      wheelUnlockTimer = null;
    }, 260);
  }

  function handleCancel(event: Event): void {
    event.preventDefault();
    onClose();
  }

  async function changeZoom(next: number): Promise<void> {
    if (!stage) {
      zoom = clampZoom(next, MIN_ZOOM, MAX_ZOOM);
      return;
    }
    const center = normalizedScrollCenter(stage);
    zoom = clampZoom(next, MIN_ZOOM, MAX_ZOOM);
    await tick();
    const offset = centeredScrollOffset(center, stage);
    stage.scrollLeft = offset.x;
    stage.scrollTop = offset.y;
  }

  function beginDrag(event: PointerEvent): void {
    if (!stage || zoom <= 1 || event.button !== 0) return;
    dragging = true;
    dragPointerId = event.pointerId;
    dragOriginX = event.clientX;
    dragOriginY = event.clientY;
    dragScrollLeft = stage.scrollLeft;
    dragScrollTop = stage.scrollTop;
    stage.setPointerCapture(event.pointerId);
    event.preventDefault();
  }

  function moveDrag(event: PointerEvent): void {
    if (!stage || !dragging || dragPointerId !== event.pointerId) return;
    stage.scrollLeft = dragScrollLeft - (event.clientX - dragOriginX);
    stage.scrollTop = dragScrollTop - (event.clientY - dragOriginY);
  }

  function endDrag(event: PointerEvent): void {
    if (!stage || dragPointerId !== event.pointerId) return;
    if (stage.hasPointerCapture(event.pointerId)) {
      stage.releasePointerCapture(event.pointerId);
    }
    dragging = false;
    dragPointerId = null;
  }

  function syncFullscreenState(): void {
    // 浏览器和Esc键都能结束全屏，因此以fullscreenchange后的文档状态为准。
    fullscreenActive = document.fullscreenElement === fullscreenSurface;
  }

  async function toggleFullscreen(): Promise<void> {
    if (!fullscreenSurface || fullscreenChanging) return;
    fullscreenChanging = true;
    try {
      if (document.fullscreenElement === fullscreenSurface) {
        await document.exitFullscreen();
      } else {
        await fullscreenSurface.requestFullscreen();
      }
    } catch {
      syncFullscreenState();
    } finally {
      fullscreenChanging = false;
      syncFullscreenState();
    }
  }

  function sourceUrl(page: GalleryPage): string | null {
    return page.current_media?.source_url ?? null;
  }
</script>

<dialog
  class="viewer"
  class:controls-visible={controlsVisible}
  style:--viewer-dominant-color={currentDominantColor}
  aria-label="作品查看器"
  tabindex="-1"
  bind:this={dialog}
  oncancel={handleCancel}
  onkeydown={handleKey}
  onpointermove={handlePointerMove}
  onwheel={handleWheel}
  onclick={(event) => {
    if (event.currentTarget === event.target) onClose();
  }}
>
  <div class="viewer-surface" bind:this={fullscreenSurface}>
    <div
      class:zoomed={zoom > 1}
      class:dragging
      class="viewer-stage"
      role="group"
      aria-label="图像画布"
      bind:this={stage}
      onpointerdown={beginDrag}
      onpointermove={moveDrag}
      onpointerup={endDrag}
      onpointercancel={endDrag}
    >
      <div
        class="viewer-media"
        style:--viewer-zoom={zoom}
        style:--viewer-aspect={currentAspect}
        style:--viewer-inverse-aspect={currentInverseAspect}
      >
        {#if currentPage?.current_media}
          {#if isUgoira && detail.ugoira}
            <UgoiraCanvas
              mediaRevisionId={currentPage.current_media.id}
              manifest={detail.ugoira}
              {playing}
              limits={ugoiraLimits}
            />
          {:else if sourceUrl(currentPage)}
            <img
              src={sourceUrl(currentPage)}
              alt={`${detail.work.title} 第${pageIndex + 1}页`}
              draggable="false"
            />
          {/if}
        {:else}
          <p class="viewer-message">这一页没有可用的原图</p>
        {/if}
      </div>
    </div>

    <header class="viewer-topbar">
      <div>
        <strong>{detail.work.title}</strong>
        <span>{pageIndex + 1} / {detail.pages.length}</span>
        {#if isUgoira}<span class="work-badge">动图</span>{/if}
      </div>
      <button
        type="button"
        aria-label="关闭查看器"
        onpointerenter={revealControls}
        onclick={onClose}><Icon name="close" size={20} /></button
      >
    </header>

    <div
      class:dark-controls={useDarkControls}
      class:light-controls={!useDarkControls}
      class="viewer-toolbar"
      role="group"
      aria-label="查看器操作"
      onpointerenter={revealControls}
    >
      {#if detail.pages.length > 1}
        <button
          type="button"
          aria-label="上一页"
          disabled={pageIndex === 0}
          onclick={() => void selectPage(pageIndex - 1)}>←</button
        >
        <button
          type="button"
          aria-label="下一页"
          disabled={pageIndex === detail.pages.length - 1}
          onclick={() => void selectPage(pageIndex + 1)}>→</button
        >
      {/if}
      {#if isUgoira}
        <button
          type="button"
          aria-label={playing ? '暂停动图' : '播放动图'}
          onclick={() => (playing = !playing)}>{playing ? 'Ⅱ' : '▶'}</button
        >
      {/if}
      <button
        type="button"
        aria-label="缩小"
        disabled={zoom <= MIN_ZOOM}
        onclick={() => void changeZoom(zoom - ZOOM_STEP)}>−</button
      >
      <span>{Math.round(zoom * 100)}%</span>
      <button
        type="button"
        aria-label="放大"
        disabled={zoom >= MAX_ZOOM}
        onclick={() => void changeZoom(zoom + ZOOM_STEP)}>＋</button
      >
      <button
        type="button"
        aria-label={fullscreenActive ? '退出全屏' : '进入全屏'}
        disabled={fullscreenChanging}
        onclick={() => void toggleFullscreen()}
        >{fullscreenActive ? '退出全屏' : '全屏'}</button
      >
      <PixivSourceLink
        href={`https://www.pixiv.net/artworks/${detail.work.pixiv_work_id}`}
        label="在Pixiv打开"
        toolbar
      />
    </div>
  </div>
</dialog>

<style>
  .viewer {
    position: fixed;
    z-index: 120;
    inset: 0;
    width: 100vw;
    max-width: none;
    height: 100vh;
    max-height: none;
    margin: 0;
    padding: 0;
    overflow: hidden;
    border: 0;
    outline: 0;
    background: var(--color-viewer-bg);
    color: #fff;
  }

  .viewer::backdrop {
    background: var(--color-viewer-bg);
  }

  .viewer-surface {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: var(--color-viewer-bg);
    color: #fff;
  }

  .viewer-stage {
    display: grid;
    width: 100%;
    height: 100%;
    place-items: center;
    overflow: hidden;
  }

  .viewer-stage.zoomed {
    display: block;
    overflow: auto;
    cursor: grab;
  }

  .viewer-stage.dragging {
    cursor: grabbing;
  }

  .viewer-media {
    display: grid;
    width: min(
      calc(100vw * var(--viewer-zoom)),
      calc(100vh * var(--viewer-aspect) * var(--viewer-zoom))
    );
    height: min(
      calc(100vh * var(--viewer-zoom)),
      calc(100vw * var(--viewer-inverse-aspect) * var(--viewer-zoom))
    );
    min-width: 0;
    min-height: 0;
    place-items: center;
  }

  .viewer-stage.zoomed .viewer-media {
    margin-inline: auto;
  }

  .viewer-media img {
    display: block;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    object-fit: contain;
    user-select: none;
  }

  :global(.viewer-media .ugoira-stage) {
    width: 100%;
    height: 100%;
  }

  .viewer-topbar {
    position: absolute;
    z-index: 1;
    top: 0;
    right: 0;
    left: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.8rem 1rem;
    background: linear-gradient(rgba(0, 0, 0, 0.72), transparent);
    opacity: 0;
    transform: translateY(-8px);
    pointer-events: none;
    transition:
      opacity var(--motion-slow) var(--ease-standard),
      transform var(--motion-slow) var(--ease-standard);
  }

  .viewer-topbar > div {
    display: flex;
    align-items: center;
    gap: 0.7rem;
  }

  .viewer-topbar strong {
    font-size: 0.82rem;
  }

  .viewer-topbar span {
    color: rgba(255, 255, 255, 0.7);
    font-size: 0.68rem;
  }

  .viewer-topbar button {
    display: grid;
    width: 38px;
    height: 38px;
    place-items: center;
    padding: 0;
    border-radius: 50%;
    background: rgba(0, 0, 0, 0.55);
    color: #fff;
    font-size: 1.2rem;
  }

  .viewer-toolbar {
    position: absolute;
    z-index: 1;
    bottom: 1rem;
    left: 50%;
    display: flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.38rem;
    border: 1px solid var(--viewer-control-border);
    border-radius: var(--radius-pill);
    background: var(--viewer-control-bg);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.24);
    backdrop-filter: blur(18px) saturate(130%);
    transform: translateX(-50%) translateY(8px);
    opacity: 0;
    pointer-events: none;
    transition:
      opacity var(--motion-slow) var(--ease-standard),
      transform var(--motion-slow) var(--ease-standard);
  }

  .viewer-toolbar.dark-controls {
    --viewer-control-bg: rgba(10, 16, 24, 0.8);
    --viewer-control-border: rgba(255, 255, 255, 0.2);
    --viewer-control-text: #fff;
    --viewer-control-muted: rgba(255, 255, 255, 0.72);
    --viewer-control-hover: rgba(255, 255, 255, 0.14);
  }

  .viewer-toolbar.light-controls {
    --viewer-control-bg: rgba(247, 249, 252, 0.84);
    --viewer-control-border: rgba(12, 22, 34, 0.18);
    --viewer-control-text: #111923;
    --viewer-control-muted: rgba(17, 25, 35, 0.7);
    --viewer-control-hover: rgba(12, 22, 34, 0.09);
  }

  .viewer.controls-visible .viewer-toolbar {
    transform: translateX(-50%) translateY(0);
    opacity: 1;
    pointer-events: auto;
  }

  .viewer.controls-visible .viewer-topbar {
    transform: translateY(0);
    opacity: 1;
    pointer-events: auto;
  }

  .viewer-toolbar button {
    display: inline-grid;
    min-width: 34px;
    height: 34px;
    place-items: center;
    padding: 0 0.55rem;
    border-radius: var(--radius-pill);
    background: transparent;
    color: var(--viewer-control-text);
    font-size: 0.72rem;
  }

  .viewer-toolbar button:hover {
    background: var(--viewer-control-hover);
  }

  .viewer-toolbar button:disabled {
    opacity: 0.35;
  }

  .viewer-toolbar span {
    min-width: 44px;
    color: var(--viewer-control-muted);
    font-size: 0.66rem;
    text-align: center;
  }

  .viewer-message {
    color: rgba(255, 255, 255, 0.6);
    font-size: 0.8rem;
  }
</style>
