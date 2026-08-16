export interface DirectoryPage<T> {
  items: T[];
  total: number;
  nextCursor: string | null;
}

type DirectoryPageLoader<T> = (
  query: string,
  cursor: string | null,
  limit: number
) => Promise<DirectoryPage<T>>;

export async function loadDirectorySnapshot<T>(
  loadPage: DirectoryPageLoader<T>,
  query: string,
  pageSize: number,
  minimumItems: number
): Promise<DirectoryPage<T>> {
  const items: T[] = [];
  let cursor: string | null = null;
  let total: number;

  do {
    const page = await loadPage(query, cursor, pageSize);
    items.push(...page.items);
    total = page.total;
    cursor = page.nextCursor;
  } while (items.length < minimumItems && cursor !== null);

  return { items, total, nextCursor: cursor };
}
