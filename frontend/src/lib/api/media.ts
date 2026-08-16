export function sourceMediaUrl(mediaRevisionId: string): string {
  return `/api/media/${mediaRevisionId}/source`;
}

export async function fetchSourceMedia(
  mediaRevisionId: string,
  signal?: AbortSignal
): Promise<ArrayBuffer> {
  const response = await fetch(sourceMediaUrl(mediaRevisionId), {
    credentials: 'same-origin',
    signal
  });
  if (!response.ok) {
    throw new Error(`来源媒体读取失败（HTTP ${response.status}）`);
  }
  return response.arrayBuffer();
}
