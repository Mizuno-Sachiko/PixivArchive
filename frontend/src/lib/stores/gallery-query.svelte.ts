import type {
  GalleryFilter,
  GalleryFilterGroup,
  GallerySearch
} from '$lib/api/gallery';
import { createGalleryQuery, galleryTextFilter } from '$lib/gallery-search';

export { createGalleryQuery } from '$lib/gallery-search';

export function serializeGalleryQuery(
  query: GallerySearch,
  cursor?: GallerySearch['cursor']
): GallerySearch {
  const groups = query.groups
    .map(cleanGroup)
    .filter((group) => group.filters.length > 0);
  return {
    group_mode: query.group_mode,
    groups,
    sort_field: query.sort_field,
    sort_direction: query.sort_direction,
    ...(cursor ? { cursor } : {}),
    limit: query.limit
  };
}

export class GalleryQueryStore {
  query = $state<GallerySearch>(createGalleryQuery());
  searchText = $state('');
  tagText = $state('');
  tagOperator = $state<'any' | 'all' | 'exclude_any' | 'not_all' | 'exact_set'>(
    'any'
  );
  tagScope = $state<'original' | 'original_and_translation'>(
    'original_and_translation'
  );
  minimumBookmarks = $state<number | null | undefined>(null);
  maximumBookmarks = $state<number | null | undefined>(null);
  workKinds = $state<string[]>([]);
  ageRatings = $state<string[]>([]);
  aiGenerated = $state<'any' | 'yes' | 'no'>('any');

  reset(query: GallerySearch = createGalleryQuery()) {
    this.query = structuredClone(query);
    this.searchText = '';
    this.tagText = '';
    this.tagOperator = 'any';
    this.tagScope = 'original_and_translation';
    this.minimumBookmarks = null;
    this.maximumBookmarks = null;
    this.workKinds = [];
    this.ageRatings = [];
    this.aiGenerated = 'any';
  }

  get validationError(): string {
    const bounds = [this.minimumBookmarks, this.maximumBookmarks].filter(
      (value): value is number => value !== null && value !== undefined
    );
    if (bounds.some((value) => !Number.isSafeInteger(value) || value < 0)) {
      return '收藏数需要填写非负整数';
    }
    if (
      this.minimumBookmarks !== null &&
      this.minimumBookmarks !== undefined &&
      this.maximumBookmarks !== null &&
      this.maximumBookmarks !== undefined &&
      this.minimumBookmarks > this.maximumBookmarks
    ) {
      return '收藏数下限不能大于上限';
    }
    return '';
  }

  build(): GallerySearch {
    const filters: GalleryFilter[] = [];
    const searchFilter = galleryTextFilter(this.searchText);
    if (searchFilter) filters.push(searchFilter);
    const tags = splitTags(this.tagText);
    if (tags.length > 0) {
      filters.push({
        type: 'tags',
        operator: this.tagOperator,
        names: tags,
        scope: this.tagScope
      });
    }
    const minimumBookmarks = this.minimumBookmarks ?? null;
    const maximumBookmarks = this.maximumBookmarks ?? null;
    let bookmarkComparison:
      Extract<GalleryFilter, { type: 'number' }>['comparison'] | null = null;
    if (minimumBookmarks !== null && maximumBookmarks !== null) {
      bookmarkComparison = {
        operator: 'between',
        value: { min: minimumBookmarks, max: maximumBookmarks }
      };
    } else if (minimumBookmarks !== null) {
      bookmarkComparison = {
        operator: 'greater_than_or_equal',
        value: minimumBookmarks
      };
    } else if (maximumBookmarks !== null) {
      bookmarkComparison = {
        operator: 'less_than_or_equal',
        value: maximumBookmarks
      };
    }
    if (bookmarkComparison) {
      filters.push({
        type: 'number',
        field: 'bookmark_count',
        comparison: bookmarkComparison
      });
    }
    if (this.workKinds.length > 0) {
      filters.push({
        type: 'category',
        field: 'work_kind',
        include: [...this.workKinds],
        exclude: []
      });
    }
    if (this.ageRatings.length > 0) {
      filters.push({
        type: 'category',
        field: 'age_rating',
        include: [...this.ageRatings],
        exclude: []
      });
    }
    if (this.aiGenerated !== 'any') {
      filters.push({
        type: 'boolean',
        field: 'ai_generated',
        value: this.aiGenerated === 'yes'
      });
    }

    return serializeGalleryQuery({
      ...this.query,
      groups: filters.length > 0 ? [{ mode: 'all', filters }] : []
    });
  }
}

export function createGalleryQueryStore(): GalleryQueryStore {
  return new GalleryQueryStore();
}

function cleanGroup(group: GalleryFilterGroup): GalleryFilterGroup {
  return {
    mode: group.mode,
    filters: group.filters
      .map(cleanFilter)
      .filter((filter): filter is GalleryFilter => filter !== null)
  };
}

function cleanFilter(filter: GalleryFilter): GalleryFilter | null {
  if (filter.type === 'text') {
    const value = filter.value.trim();
    return value ? { ...filter, value } : null;
  }
  if (filter.type === 'tags') {
    const names = filter.names.map((name) => name.trim()).filter(Boolean);
    return names.length > 0 ? { ...filter, names: [...new Set(names)] } : null;
  }
  return structuredClone(filter);
}

function splitTags(value: string): string[] {
  return [
    ...new Set(
      value
        .split(/[,，\n]/)
        .map((tag) => tag.trim())
        .filter(Boolean)
    )
  ];
}
