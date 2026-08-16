import {
  getOverviewDecorations,
  shuffleOverviewDecorations,
  type OverviewDecoration
} from '$lib/api/gallery';
import { dailyDecorationKey } from '$lib/overview-decorations';

type Decorations = Array<OverviewDecoration | null>;
type DecorationLoader = (date: string) => Promise<Decorations>;

class OverviewDecorationsStore {
  current = $state<Decorations>([]);
  loading = $state(false);
  error = $state('');
  private loadedDate: string | null = null;
  private requestRevision = 0;

  async load(
    date = new Date(),
    loadDecorations: DecorationLoader = getOverviewDecorations
  ): Promise<void> {
    const dateKey = dailyDecorationKey(date);
    if (this.loadedDate === dateKey) return;
    try {
      await this.replace(dateKey, loadDecorations);
    } catch {
      // The overview keeps its existing cards while the shared store exposes the read error.
    }
  }

  async shuffle(
    date = new Date(),
    loadDecorations: DecorationLoader = shuffleOverviewDecorations
  ): Promise<void> {
    await this.replace(dailyDecorationKey(date), loadDecorations);
  }

  reset(): void {
    this.requestRevision += 1;
    this.current = [];
    this.loadedDate = null;
    this.loading = false;
    this.error = '';
  }

  private async replace(
    dateKey: string,
    loadDecorations: DecorationLoader
  ): Promise<void> {
    const revision = ++this.requestRevision;
    this.loading = true;
    this.error = '';
    try {
      const items = await loadDecorations(dateKey);
      if (revision !== this.requestRevision) return;
      this.current = items;
      this.loadedDate = dateKey;
    } catch (error) {
      if (revision === this.requestRevision) {
        this.error = '概览装饰图暂时无法读取';
      }
      throw error;
    } finally {
      if (revision === this.requestRevision) this.loading = false;
    }
  }
}

export const overviewDecorationsStore = new OverviewDecorationsStore();
