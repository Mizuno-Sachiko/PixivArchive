export const endpoints = {
  login: '/api/auth/login',
  session: '/api/auth/session',
  logout: '/api/auth/logout',
  events: '/api/events',
  systemStatus: '/api/system/status',
  systemStorageUsage: '/api/system/storage-usage',
  systemSettings: '/api/system/settings',
  pixivAccount: '/api/pixiv/account'
} as const;
