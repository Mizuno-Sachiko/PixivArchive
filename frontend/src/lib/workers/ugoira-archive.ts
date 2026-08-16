import { unzipSync } from 'fflate';

import {
  UgoiraArchivePreflight,
  type UgoiraArchiveLimits,
  type UgoiraManifestFrame,
  type ValidatedUgoiraFrame
} from './ugoira-protocol';

export function inspectUgoiraArchive(
  archive: Uint8Array,
  manifest: UgoiraManifestFrame[],
  limits: UgoiraArchiveLimits
): ValidatedUgoiraFrame[] {
  const preflight = new UgoiraArchivePreflight(manifest, limits);
  unzipSync(archive, {
    filter: (entry) => {
      preflight.accept(entry.name, entry.originalSize);
      return false;
    }
  });
  return preflight.finish();
}

export function extractUgoiraFrame(
  archive: Uint8Array,
  frame: ValidatedUgoiraFrame
): Uint8Array {
  const extracted = unzipSync(archive, {
    filter: (entry) =>
      entry.name === frame.file && entry.originalSize === frame.byteSize
  })[frame.file];
  if (!extracted || extracted.byteLength !== frame.byteSize) {
    throw new Error(`Ugoira frame ${frame.file} is not available`);
  }
  return extracted;
}
