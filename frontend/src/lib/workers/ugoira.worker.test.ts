import { describe, expect, it, vi } from 'vitest';
import { strToU8, zipSync } from 'fflate';

import {
  createUgoiraLoadRequest,
  DecodedFrameCache,
  maximizeContain,
  UgoiraArchivePreflight,
  UgoiraTimeline,
  validateUgoiraEntries
} from './ugoira-protocol';
import { extractUgoiraFrame, inspectUgoiraArchive } from './ugoira-archive';

describe('Ugoira worker protocol', () => {
  it('maximizes a frame inside the viewer without cropping either axis', () => {
    expect(maximizeContain(1_000, 2_000, 900, 700)).toEqual({
      width: 350,
      height: 700
    });
    expect(maximizeContain(2_000, 1_000, 900, 700)).toEqual({
      width: 900,
      height: 450
    });
  });

  it('creates a structured-cloneable load request from reactive-shaped input', () => {
    const archive = new ArrayBuffer(16);
    const request = createUgoiraLoadRequest(
      archive,
      'image/jpeg',
      [{ file: '000000.jpg', delay_ms: 80 }],
      {
        maximumCompressedBytes: 512,
        maximumFrameCount: 3,
        maximumEntryBytes: 256,
        maximumExpandedBytes: 1024,
        maximumPixelsPerFrame: 64,
        decodedCacheBytes: 512
      }
    );

    expect(() => structuredClone(request)).not.toThrow();
    expect(request.frames).toEqual([{ file: '000000.jpg', delay_ms: 80 }]);
  });

  it('preserves manifest order and source delays', () => {
    const frames = validateUgoiraEntries(
      [
        { file: '000002.jpg', delay_ms: 80 },
        { file: '000001.jpg', delay_ms: 120 }
      ],
      new Map([
        ['000001.jpg', 90],
        ['000002.jpg', 110]
      ]),
      {
        maximumFrameCount: 10,
        maximumEntryBytes: 1_000,
        maximumExpandedBytes: 2_000
      }
    );

    expect(frames).toEqual([
      { file: '000002.jpg', delayMs: 80, byteSize: 110 },
      { file: '000001.jpg', delayMs: 120, byteSize: 90 }
    ]);
    const timeline = new UgoiraTimeline(frames.map((frame) => frame.delayMs));
    expect(timeline.frameAt(0)).toBe(0);
    expect(timeline.frameAt(79)).toBe(0);
    expect(timeline.frameAt(80)).toBe(1);
    expect(timeline.frameAt(200)).toBe(0);
  });

  it('rejects missing frames and expanded archives over the configured budget', () => {
    expect(() =>
      validateUgoiraEntries(
        [{ file: 'missing.jpg', delay_ms: 100 }],
        new Map(),
        {
          maximumFrameCount: 10,
          maximumEntryBytes: 1_000,
          maximumExpandedBytes: 2_000
        }
      )
    ).toThrow('missing.jpg');

    expect(() =>
      validateUgoiraEntries(
        [
          { file: 'a.jpg', delay_ms: 100 },
          { file: 'b.jpg', delay_ms: 100 }
        ],
        new Map([
          ['a.jpg', 1_200],
          ['b.jpg', 1_200]
        ]),
        {
          maximumFrameCount: 10,
          maximumEntryBytes: 1_500,
          maximumExpandedBytes: 2_000
        }
      )
    ).toThrow('expanded byte limit');
  });

  it('checks ZIP entry sizes before frame data is expanded', () => {
    const preflight = new UgoiraArchivePreflight(
      [{ file: '000000.jpg', delay_ms: 80 }],
      {
        maximumFrameCount: 10,
        maximumEntryBytes: 1_000,
        maximumExpandedBytes: 2_000
      }
    );

    expect(() => preflight.accept('000000.jpg', 2_001)).toThrow('entry limit');
    expect(() => preflight.accept('unexpected.txt', 1)).toThrow(
      'Unknown Ugoira archive entry'
    );
  });

  it('inspects the archive without retaining every expanded frame', () => {
    const archive = zipSync({
      '000000.jpg': strToU8('frame-zero'),
      '000001.jpg': strToU8('frame-one')
    });
    const frames = inspectUgoiraArchive(
      archive,
      [
        { file: '000000.jpg', delay_ms: 80 },
        { file: '000001.jpg', delay_ms: 120 }
      ],
      {
        maximumFrameCount: 10,
        maximumEntryBytes: 1_000,
        maximumExpandedBytes: 2_000
      }
    );

    expect(frames).toHaveLength(2);
    expect(
      new TextDecoder().decode(extractUgoiraFrame(archive, frames[1]))
    ).toBe('frame-one');
  });

  it('disposes least-recently-used decoded frames within the byte budget', () => {
    const closeA = vi.fn();
    const closeB = vi.fn();
    const cache = new DecodedFrameCache<{ close: () => void }>(100);

    expect(cache.set(0, { close: closeA }, 60)).toBe(true);
    expect(cache.set(1, { close: closeB }, 60)).toBe(true);

    expect(cache.has(0)).toBe(false);
    expect(cache.has(1)).toBe(true);
    expect(cache.byteSize).toBe(60);
    expect(closeA).toHaveBeenCalledOnce();

    cache.dispose();
    expect(closeB).toHaveBeenCalledOnce();
    expect(cache.byteSize).toBe(0);
  });

  it('rejects and closes a frame larger than the decoded cache', () => {
    const close = vi.fn();
    const cache = new DecodedFrameCache<{ close: () => void }>(100);

    expect(cache.set(0, { close }, 101)).toBe(false);
    expect(cache.has(0)).toBe(false);
    expect(close).toHaveBeenCalledOnce();
  });
});
