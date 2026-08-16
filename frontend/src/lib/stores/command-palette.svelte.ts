class CommandPaletteStore {
  opened = $state(false);
  query = $state('');

  open(): void {
    this.opened = true;
    this.query = '';
  }

  close(): void {
    this.opened = false;
    this.query = '';
  }
}

export const commandPaletteStore = new CommandPaletteStore();
