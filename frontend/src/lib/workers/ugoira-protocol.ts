export interface UgoiraManifestFrame {
  file: string;
  delay_ms: number;
}

export interface ValidatedUgoiraFrame {
  file: string;
  delayMs: number;
  byteSize: number;
}

export interface UgoiraArchiveLimits {
  maximumFrameCount: number;
  maximumEntryBytes: number;
  maximumExpandedBytes: number;
}

export interface UgoiraDecodeLimits extends UgoiraArchiveLimits {
  maximumCompressedBytes: number;
  maximumPixelsPerFrame: number;
  decodedCacheBytes: number;
}

export type UgoiraWorkerRequest =
  | {
      type: 'load';
      archive: ArrayBuffer;
      mimeType: string;
      frames: UgoiraManifestFrame[];
      limits: UgoiraDecodeLimits;
    }
  | { type: 'decode_frame'; index: number }
  | { type: 'dispose' };

export type UgoiraDecodedFrame =
  | {
      kind: 'bitmap';
      index: number;
      delayMs: number;
      byteSize: number;
      bitmap: ImageBitmap;
    }
  | {
      kind: 'bytes';
      index: number;
      delayMs: number;
      byteSize: number;
      bytes: ArrayBuffer;
      mimeType: string;
    };

export type UgoiraWorkerResponse =
  | { type: 'ready'; frameCount: number; totalDurationMs: number }
  | { type: 'frame'; frame: UgoiraDecodedFrame }
  | { type: 'complete' }
  | { type: 'error'; message: string };

export function maximizeContain(
  mediaWidth: number,
  mediaHeight: number,
  containerWidth: number,
  containerHeight: number
): { width: number; height: number } {
  if (
    ![mediaWidth, mediaHeight, containerWidth, containerHeight].every(
      (value) => Number.isFinite(value) && value > 0
    )
  ) {
    return { width: 0, height: 0 };
  }
  const scale = Math.min(
    containerWidth / mediaWidth,
    containerHeight / mediaHeight
  );
  return {
    width: Math.round(mediaWidth * scale),
    height: Math.round(mediaHeight * scale)
  };
}

export function createUgoiraLoadRequest(
  archive: ArrayBuffer,
  mimeType: string,
  frames: UgoiraManifestFrame[],
  limits: UgoiraDecodeLimits
): Extract<UgoiraWorkerRequest, { type: 'load' }> {
  return {
    type: 'load',
    archive,
    mimeType,
    frames: frames.map((frame) => ({
      file: frame.file,
      delay_ms: frame.delay_ms
    })),
    limits: {
      maximumFrameCount: limits.maximumFrameCount,
      maximumEntryBytes: limits.maximumEntryBytes,
      maximumExpandedBytes: limits.maximumExpandedBytes,
      maximumCompressedBytes: limits.maximumCompressedBytes,
      maximumPixelsPerFrame: limits.maximumPixelsPerFrame,
      decodedCacheBytes: limits.decodedCacheBytes
    }
  };
}

export function validateUgoiraEntries(
  manifest: UgoiraManifestFrame[],
  entries: ReadonlyMap<string, number>,
  limits: UgoiraArchiveLimits
): ValidatedUgoiraFrame[] {
  if (manifest.length === 0) {
    throw new Error('Ugoira manifest has no frames');
  }
  if (manifest.length > limits.maximumFrameCount) {
    throw new Error('Ugoira frame count limit exceeded');
  }

  let expandedBytes = 0;
  const names = new Set<string>();
  return manifest.map((frame) => {
    if (!safeArchiveName(frame.file) || names.has(frame.file)) {
      throw new Error(`Invalid Ugoira frame entry: ${frame.file}`);
    }
    names.add(frame.file);
    if (!Number.isInteger(frame.delay_ms) || frame.delay_ms <= 0) {
      throw new Error(`Invalid Ugoira frame delay: ${frame.file}`);
    }
    const byteSize = entries.get(frame.file);
    if (byteSize === undefined) {
      throw new Error(`Ugoira archive is missing ${frame.file}`);
    }
    if (byteSize <= 0 || byteSize > limits.maximumEntryBytes) {
      throw new Error(`Ugoira frame entry limit exceeded: ${frame.file}`);
    }
    expandedBytes += byteSize;
    if (expandedBytes > limits.maximumExpandedBytes) {
      throw new Error('Ugoira expanded byte limit exceeded');
    }
    return { file: frame.file, delayMs: frame.delay_ms, byteSize };
  });
}

export class UgoiraArchivePreflight {
  readonly #manifest: UgoiraManifestFrame[];
  readonly #limits: UgoiraArchiveLimits;
  readonly #expectedNames: Set<string>;
  readonly #entries = new Map<string, number>();
  #expandedBytes = 0;

  constructor(manifest: UgoiraManifestFrame[], limits: UgoiraArchiveLimits) {
    if (manifest.length === 0) {
      throw new Error('Ugoira manifest has no frames');
    }
    if (manifest.length > limits.maximumFrameCount) {
      throw new Error('Ugoira frame count limit exceeded');
    }
    this.#manifest = manifest;
    this.#limits = limits;
    this.#expectedNames = new Set<string>();
    for (const frame of manifest) {
      if (!safeArchiveName(frame.file) || this.#expectedNames.has(frame.file)) {
        throw new Error(`Invalid Ugoira frame entry: ${frame.file}`);
      }
      if (!Number.isInteger(frame.delay_ms) || frame.delay_ms <= 0) {
        throw new Error(`Invalid Ugoira frame delay: ${frame.file}`);
      }
      this.#expectedNames.add(frame.file);
    }
  }

  accept(name: string, originalSize: number): boolean {
    if (!this.#expectedNames.has(name)) {
      throw new Error(`Unknown Ugoira archive entry: ${name}`);
    }
    if (this.#entries.has(name)) {
      throw new Error(`Duplicate Ugoira archive entry: ${name}`);
    }
    if (
      !Number.isSafeInteger(originalSize) ||
      originalSize <= 0 ||
      originalSize > this.#limits.maximumEntryBytes
    ) {
      throw new Error(`Ugoira frame entry limit exceeded: ${name}`);
    }
    this.#expandedBytes += originalSize;
    if (this.#expandedBytes > this.#limits.maximumExpandedBytes) {
      throw new Error('Ugoira expanded byte limit exceeded');
    }
    this.#entries.set(name, originalSize);
    return true;
  }

  finish(): ValidatedUgoiraFrame[] {
    return validateUgoiraEntries(this.#manifest, this.#entries, this.#limits);
  }
}

export class UgoiraTimeline {
  readonly totalDurationMs: number;
  readonly #ends: number[];

  constructor(delays: number[]) {
    if (
      delays.length === 0 ||
      delays.some((delay) => !Number.isInteger(delay) || delay <= 0)
    ) {
      throw new RangeError('Ugoira delays must be positive integers');
    }
    let total = 0;
    this.#ends = delays.map((delay) => {
      total += delay;
      return total;
    });
    this.totalDurationMs = total;
  }

  frameAt(elapsedMs: number): number {
    const position =
      ((Math.max(0, elapsedMs) % this.totalDurationMs) + this.totalDurationMs) %
      this.totalDurationMs;
    return this.#ends.findIndex((end) => position < end);
  }
}

interface Closable {
  close(): void;
}

interface CacheEntry<T> {
  value: T;
  byteSize: number;
}

export class DecodedFrameCache<T extends Closable> {
  readonly #limit: number;
  readonly #entries = new Map<number, CacheEntry<T>>();
  #byteSize = 0;

  constructor(limit: number) {
    if (!Number.isFinite(limit) || limit <= 0) {
      throw new RangeError('Decoded frame cache limit must be positive');
    }
    this.#limit = limit;
  }

  get byteSize(): number {
    return this.#byteSize;
  }

  has(index: number): boolean {
    return this.#entries.has(index);
  }

  get(index: number): T | undefined {
    const entry = this.#entries.get(index);
    if (!entry) return undefined;
    this.#entries.delete(index);
    this.#entries.set(index, entry);
    return entry.value;
  }

  set(index: number, value: T, byteSize: number): boolean {
    if (byteSize <= 0 || byteSize > this.#limit) {
      value.close();
      return false;
    }
    const previous = this.#entries.get(index);
    if (previous) {
      previous.value.close();
      this.#byteSize -= previous.byteSize;
      this.#entries.delete(index);
    }
    this.#entries.set(index, { value, byteSize });
    this.#byteSize += byteSize;
    while (this.#byteSize > this.#limit) {
      const oldest = this.#entries.entries().next().value as
        [number, CacheEntry<T>] | undefined;
      if (!oldest) break;
      this.#entries.delete(oldest[0]);
      this.#byteSize -= oldest[1].byteSize;
      oldest[1].value.close();
    }
    return true;
  }

  dispose(): void {
    for (const entry of this.#entries.values()) {
      entry.value.close();
    }
    this.#entries.clear();
    this.#byteSize = 0;
  }
}

function safeArchiveName(name: string): boolean {
  return (
    name.length > 0 &&
    !name.startsWith('/') &&
    !name.startsWith('\\') &&
    !name.includes('../') &&
    !name.includes('..\\') &&
    !name.includes('\0')
  );
}
