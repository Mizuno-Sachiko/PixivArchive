/// <reference lib="webworker" />

import {
  UgoiraTimeline,
  type UgoiraDecodedFrame,
  type UgoiraWorkerRequest,
  type UgoiraWorkerResponse
} from './ugoira-protocol';
import { extractUgoiraFrame, inspectUgoiraArchive } from './ugoira-archive';

const scope = self as DedicatedWorkerGlobalScope;
let archive: Uint8Array | null = null;
let frames: ReturnType<typeof inspectUgoiraArchive> = [];
let mimeType = '';
let limits: Extract<UgoiraWorkerRequest, { type: 'load' }>['limits'] | null =
  null;

scope.onmessage = async (event: MessageEvent<UgoiraWorkerRequest>) => {
  if (event.data.type === 'dispose') {
    dispose();
    return;
  }
  try {
    if (event.data.type === 'load') {
      load(event.data);
    } else {
      await decodeFrameAt(event.data.index);
    }
  } catch (error) {
    post({
      type: 'error',
      message: error instanceof Error ? error.message : 'Pixiv动图解码失败'
    });
  }
};

function load(request: Extract<UgoiraWorkerRequest, { type: 'load' }>): void {
  dispose();
  if (request.archive.byteLength > request.limits.maximumCompressedBytes) {
    throw new Error('Ugoira compressed byte limit exceeded');
  }
  archive = new Uint8Array(request.archive);
  frames = inspectUgoiraArchive(archive, request.frames, request.limits);
  mimeType = request.mimeType;
  limits = request.limits;
  const timeline = new UgoiraTimeline(frames.map((frame) => frame.delayMs));
  post({
    type: 'ready',
    frameCount: frames.length,
    totalDurationMs: timeline.totalDurationMs
  });
}

async function decodeFrameAt(index: number): Promise<void> {
  const frame = frames[index];
  if (!frame || !archive || !limits) {
    throw new Error(`Ugoira frame ${index} is not available`);
  }
  const bytes = extractUgoiraFrame(archive, frame);
  const decoded = await decodeFrame(
    bytes,
    mimeType,
    index,
    frame.delayMs,
    limits.maximumPixelsPerFrame
  );
  if (decoded.kind === 'bitmap') {
    post({ type: 'frame', frame: decoded }, [decoded.bitmap]);
  } else {
    post({ type: 'frame', frame: decoded }, [decoded.bytes]);
  }
}

async function decodeFrame(
  bytes: Uint8Array,
  mimeType: string,
  index: number,
  delayMs: number,
  maximumPixels: number
): Promise<UgoiraDecodedFrame> {
  const data = exactBuffer(bytes);
  if (typeof ImageDecoder !== 'undefined') {
    const decoder = new ImageDecoder({
      data,
      type: mimeType
    });
    try {
      const result = await decoder.decode({ frameIndex: 0 });
      const bitmap = await createImageBitmap(result.image).finally(() => {
        result.image.close();
      });
      validatePixels(bitmap, maximumPixels);
      return {
        kind: 'bitmap',
        index,
        delayMs,
        byteSize: bitmap.width * bitmap.height * 4,
        bitmap
      };
    } finally {
      decoder.close();
    }
  }
  if (typeof createImageBitmap === 'function') {
    const bitmap = await createImageBitmap(
      new Blob([data], { type: mimeType })
    );
    validatePixels(bitmap, maximumPixels);
    return {
      kind: 'bitmap',
      index,
      delayMs,
      byteSize: bitmap.width * bitmap.height * 4,
      bitmap
    };
  }
  return {
    kind: 'bytes',
    index,
    delayMs,
    byteSize: data.byteLength,
    bytes: data,
    mimeType
  };
}

function exactBuffer(bytes: Uint8Array): ArrayBuffer {
  if (bytes.byteOffset === 0 && bytes.byteLength === bytes.buffer.byteLength) {
    return bytes.buffer as ArrayBuffer;
  }
  return bytes.slice().buffer;
}

function post(message: UgoiraWorkerResponse, transfer: Transferable[] = []) {
  scope.postMessage(message, transfer);
}

function dispose() {
  archive = null;
  frames = [];
  mimeType = '';
  limits = null;
}

function validatePixels(bitmap: ImageBitmap, maximumPixels: number): void {
  if (bitmap.width * bitmap.height > maximumPixels) {
    bitmap.close();
    throw new Error('Ugoira frame pixel limit exceeded');
  }
}

export {};
