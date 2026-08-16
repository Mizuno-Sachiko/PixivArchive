export type ThemeMode = 'system' | 'light' | 'dark';
export type ResolvedTheme = Exclude<ThemeMode, 'system'>;

const STORAGE_KEY = 'pixivarchive.theme';

class ThemeStore {
  mode = $state<ThemeMode>('system');
  resolved = $state<ResolvedTheme>('light');
  private mediaQuery: MediaQueryList | null = null;

  initialize(): void {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === 'system' || stored === 'light' || stored === 'dark') {
      this.mode = stored;
    }
    this.mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    this.mediaQuery.addEventListener('change', this.handleSystemChange);
    this.apply();
  }

  setMode(mode: ThemeMode): void {
    this.mode = mode;
    localStorage.setItem(STORAGE_KEY, mode);
    this.apply();
  }

  private readonly handleSystemChange = (): void => {
    if (this.mode === 'system') {
      this.apply();
    }
  };

  private apply(): void {
    const systemTheme = this.mediaQuery?.matches ? 'dark' : 'light';
    this.resolved = this.mode === 'system' ? systemTheme : this.mode;
    document.documentElement.dataset.theme = this.resolved;
    document.documentElement.dataset.themeMode = this.mode;
    document.documentElement.style.colorScheme = this.resolved;
  }
}

export const themeStore = new ThemeStore();
