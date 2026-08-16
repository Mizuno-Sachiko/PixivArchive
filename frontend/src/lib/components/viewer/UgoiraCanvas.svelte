<script lang="ts">
  import { onMount } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';

  import type { UgoiraManifest } from '$lib/api/gallery';
  import { fetchSourceMedia } from '$lib/api/media';
  import { decodeFrameOnMainThread } from '$lib/workers/image-bitmap';
  import {
    createUgoiraLoadRequest,
    DecodedFrameCache,
    maximizeContain,
    UgoiraTimeline,
    type UgoiraDecodeLimits,
    type UgoiraWorkerRequest,
    type UgoiraWorkerResponse
  } from '$lib/workers/ugoira-protocol';

  interface Props {
    mediaRevisionId: string;
    manifest: UgoiraManifest;
    playing: boolean;
    limits?: UgoiraDecodeLimits;
  }

  let {
    mediaRevisionId,
    manifest,
    playing,
    limits = {
      maximumCompressedBytes: 512 * 1024 * 1024,
      maximumFrameCount: 3_000,
      maximumEntryBytes: 256 * 1024 * 1024,
      maximumExpandedBytes: 8 * 1024 * 1024 * 1024,
      maximumPixelsPerFrame: 64_000_000,
      decodedCacheBytes: 512 * 1024 * 1024
    }
  }: Props = $props();
  let stage = $state<HTMLDivElement>();
  let canvas = $state<HTMLCanvasElement>();
  let stageWidth = $state(0);
  let stageHeight = $state(0);
  let frameWidth = $state(0);
  let frameHeight = $state(0);
  let status = $state('正在读取动图…');
  let failed = $state(false);
  let displaySize = $derived(
    maximizeContain(frameWidth, frameHeight, stageWidth, stageHeight)
  );

  onMount(() => {
    const frames = manifest.frames.map((frame) => ({
      file: frame.file,
      delay_ms: frame.delay_ms
    }));
    const decodeLimits = {
      maximumFrameCount: limits.maximumFrameCount,
      maximumEntryBytes: limits.maximumEntryBytes,
      maximumExpandedBytes: limits.maximumExpandedBytes,
      maximumCompressedBytes: limits.maximumCompressedBytes,
      maximumPixelsPerFrame: limits.maximumPixelsPerFrame,
      decodedCacheBytes: limits.decodedCacheBytes
    };
    const timeline = new UgoiraTimeline(frames.map((frame) => frame.delay_ms));
    const controller = new AbortController();
    const worker = new Worker(
      new URL('../../workers/ugoira.worker.ts', import.meta.url),
      { type: 'module' }
    );
    const cache = new DecodedFrameCache<ImageBitmap>(
      decodeLimits.decodedCacheBytes
    );
    const pending = new SvelteSet<number>();
    let ready = false;
    let disposed = false;
    let elapsedMs = 0;
    let lastTick = performance.now();
    let animationFrame = 0;
    const resizeObserver = new ResizeObserver(([entry]) => {
      stageWidth = entry.contentRect.width;
      stageHeight = entry.contentRect.height;
    });
    if (stage) {
      const bounds = stage.getBoundingClientRect();
      stageWidth = bounds.width;
      stageHeight = bounds.height;
      resizeObserver.observe(stage);
    }

    worker.onmessage = (event: MessageEvent<UgoiraWorkerResponse>) => {
      void handleWorkerMessage(event.data);
    };

    void fetchSourceMedia(mediaRevisionId, controller.signal)
      .then((archive) => {
        if (disposed) return;
        const request = createUgoiraLoadRequest(
          archive,
          manifest.frame_mime_type,
          frames,
          decodeLimits
        );
        worker.postMessage(request, [archive]);
      })
      .catch((error) => {
        if (controller.signal.aborted) return;
        showError(error);
      });

    animationFrame = requestAnimationFrame(tick);

    async function handleWorkerMessage(
      message: UgoiraWorkerResponse
    ): Promise<void> {
      if (disposed) return;
      if (message.type === 'error') {
        showError(new Error(message.message));
        return;
      }
      if (message.type === 'ready') {
        ready = true;
        status = '';
        requestFrame(0);
        requestFrame(1 % message.frameCount);
        return;
      }
      if (message.type !== 'frame') return;

      try {
        const bitmap =
          message.frame.kind === 'bitmap'
            ? message.frame.bitmap
            : await decodeFrameOnMainThread(
                message.frame.bytes,
                message.frame.mimeType
              );
        if (disposed) {
          bitmap.close();
          return;
        }
        if (bitmap.width * bitmap.height > decodeLimits.maximumPixelsPerFrame) {
          bitmap.close();
          throw new Error('动图帧超过像素限制');
        }
        if (
          !cache.set(
            message.frame.index,
            bitmap,
            bitmap.width * bitmap.height * 4
          )
        ) {
          throw new Error('动图单帧超过解码缓存限制');
        }
      } catch (error) {
        showError(error);
      } finally {
        pending.delete(message.frame.index);
      }
    }

    function tick(now: number): void {
      if (disposed || failed) return;
      if (playing) elapsedMs += now - lastTick;
      lastTick = now;
      if (ready) {
        const frameIndex = timeline.frameAt(elapsedMs);
        const bitmap = cache.get(frameIndex);
        if (bitmap) draw(bitmap);
        else requestFrame(frameIndex);
        requestFrame((frameIndex + 1) % frames.length);
      }
      animationFrame = requestAnimationFrame(tick);
    }

    function requestFrame(index: number): void {
      if (
        !ready ||
        failed ||
        cache.has(index) ||
        pending.has(index) ||
        index < 0 ||
        index >= frames.length
      ) {
        return;
      }
      pending.add(index);
      const request: UgoiraWorkerRequest = {
        type: 'decode_frame',
        index
      };
      worker.postMessage(request);
    }

    function draw(bitmap: ImageBitmap): void {
      if (!canvas) return;
      const context = canvas.getContext('2d', { alpha: false });
      if (!context) return;
      if (canvas.width !== bitmap.width || canvas.height !== bitmap.height) {
        canvas.width = bitmap.width;
        canvas.height = bitmap.height;
      }
      frameWidth = bitmap.width;
      frameHeight = bitmap.height;
      context.clearRect(0, 0, canvas.width, canvas.height);
      context.drawImage(bitmap, 0, 0);
    }

    function showError(error: unknown): void {
      if (disposed || failed) return;
      failed = true;
      status = error instanceof Error ? error.message : '动图解码失败';
    }

    return () => {
      disposed = true;
      controller.abort();
      cancelAnimationFrame(animationFrame);
      resizeObserver.disconnect();
      cache.dispose();
      worker.postMessage({ type: 'dispose' } satisfies UgoiraWorkerRequest);
      worker.terminate();
    };
  });
</script>

<div class="ugoira-stage" aria-label="动图画面" bind:this={stage}>
  <canvas
    bind:this={canvas}
    style:width={`${displaySize.width}px`}
    style:height={`${displaySize.height}px`}
  ></canvas>
  {#if status}
    <p class:error={failed}>{status}</p>
  {/if}
</div>

<style>
  .ugoira-stage {
    position: relative;
    display: grid;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    place-items: center;
    overflow: hidden;
  }

  canvas {
    display: block;
    max-width: 100%;
    max-height: 100%;
  }

  p {
    position: absolute;
    bottom: 1rem;
    padding: 0.45rem 0.7rem;
    margin: 0;
    border-radius: var(--radius-pill);
    background: rgba(0, 0, 0, 0.62);
    color: rgba(255, 255, 255, 0.8);
    font-size: 0.72rem;
  }

  p.error {
    color: #ff9ba3;
  }
</style>
