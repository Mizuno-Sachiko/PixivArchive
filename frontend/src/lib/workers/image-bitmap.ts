export async function decodeFrameOnMainThread(
  bytes: ArrayBuffer,
  mimeType: string
): Promise<ImageBitmap> {
  const blob = new Blob([bytes], { type: mimeType });
  return createImageBitmap(blob);
}
