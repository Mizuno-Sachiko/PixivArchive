const CSRF_COOKIE = 'pa_csrf';

class CsrfStore {
  available = $state(false);

  refresh(): void {
    this.available = document.cookie
      .split(';')
      .some((entry) => entry.trim().startsWith(`${CSRF_COOKIE}=`));
  }

  clear(): void {
    this.available = false;
  }
}

export const csrfStore = new CsrfStore();
