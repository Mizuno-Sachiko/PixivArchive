import { SvelteSet } from 'svelte/reactivity';

export class FollowingSelectionSession {
  mode = $state(false);
  readonly ids = new SvelteSet<number>();

  get count(): number {
    return this.ids.size;
  }

  enter(): void {
    this.mode = true;
    this.ids.clear();
  }

  exit(): void {
    this.mode = false;
    this.ids.clear();
  }

  toggle(pixivArtistId: number): void {
    if (!this.mode) return;
    if (this.ids.has(pixivArtistId)) this.ids.delete(pixivArtistId);
    else this.ids.add(pixivArtistId);
  }

  selectAll(pixivArtistIds: readonly number[]): void {
    if (!this.mode) return;
    for (const pixivArtistId of pixivArtistIds) this.ids.add(pixivArtistId);
  }

  invert(pixivArtistIds: readonly number[]): void {
    if (!this.mode) return;
    for (const pixivArtistId of pixivArtistIds) this.toggle(pixivArtistId);
  }

  clear(): void {
    this.ids.clear();
  }

  retain(pixivArtistIds: readonly number[]): void {
    const available = new SvelteSet(pixivArtistIds);
    for (const pixivArtistId of this.ids) {
      if (!available.has(pixivArtistId)) this.ids.delete(pixivArtistId);
    }
  }
}
