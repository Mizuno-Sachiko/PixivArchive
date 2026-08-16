const defaultConcurrency = 6;

export async function settleWithConcurrency<Input, Output>(
  items: readonly Input[],
  operation: (item: Input, index: number) => Promise<Output>,
  concurrency = defaultConcurrency
): Promise<PromiseSettledResult<Output>[]> {
  if (!Number.isInteger(concurrency) || concurrency < 1) {
    throw new RangeError('concurrency must be a positive integer');
  }
  const results = new Array<PromiseSettledResult<Output>>(items.length);
  let nextIndex = 0;

  async function worker(): Promise<void> {
    while (nextIndex < items.length) {
      const index = nextIndex++;
      try {
        results[index] = {
          status: 'fulfilled',
          value: await operation(items[index], index)
        };
      } catch (reason) {
        results[index] = { status: 'rejected', reason };
      }
    }
  }

  await Promise.all(
    Array.from({ length: Math.min(concurrency, items.length) }, () => worker())
  );
  return results;
}
