import type { Page, Route } from '@playwright/test';

import type { EffectiveSettings, PixivAccount } from '$lib/api/system';

export const PIXIV_ACCOUNT_ID = '0198f652-0000-7000-8000-000000000002';

export function mockPixivAccount<Overrides extends Partial<PixivAccount>>(
  overrides: Overrides = {} as Overrides
): PixivAccount & Overrides {
  return {
    account_id: PIXIV_ACCOUNT_ID,
    pixiv_user_id: 90001,
    display_name: 'Pixiv Test Account',
    avatar_url: null,
    state: 'normal',
    bookmark_writeback_enabled: false,
    last_validated_at: '2026-07-31T12:00:00Z',
    revision: 1,
    ...overrides
  };
}

export interface MockApiState {
  authenticated: boolean;
  loginBody?: unknown;
  eventConnections: number;
}

export interface MockApiOptions {
  initialSnapshot?: boolean;
}

export async function fulfillJson(
  route: Route,
  status: number,
  body: unknown
): Promise<void> {
  await route.fulfill({
    status,
    contentType: 'application/json',
    body: JSON.stringify(body)
  });
}

export async function chooseSelectOption(
  page: Page,
  label: string,
  option: string
): Promise<void> {
  await page.getByLabel(label).click();
  await page.getByRole('option', { name: option, exact: true }).click();
}

export async function mockApi(
  page: Page,
  authenticated = true,
  options: MockApiOptions = {}
): Promise<MockApiState> {
  const state: MockApiState = { authenticated, eventConnections: 0 };

  await page.route('**/api/auth/session', async (route) => {
    if (!state.authenticated) {
      await fulfillJson(
        route,
        401,
        apiError('authentication_required', 'Authentication is required')
      );
      return;
    }
    await fulfillJson(route, 200, session());
  });

  await page.route('**/api/auth/login', async (route) => {
    state.loginBody = route.request().postDataJSON();
    state.authenticated = true;
    await fulfillJson(route, 200, session());
  });

  await page.route('**/api/auth/logout', async (route) => {
    state.authenticated = false;
    await route.fulfill({ status: 204 });
  });

  await page.route('**/api/system/status', async (route) => {
    await fulfillJson(route, 200, {
      version: '0.1.0',
      git_commit: 'daf3069',
      migration_version: 14,
      database: { status: 'healthy', message: null },
      worker: { status: 'healthy', message: null },
      media: { status: 'warning', message: '媒体盘剩余88 GiB' },
      queue: {
        immediate: { queued: 1, running: 1 },
        manual_import: { queued: 3, running: 0 },
        scheduled_collection: { queued: 12, running: 1 },
        maintenance: { queued: 2, running: 0 }
      },
      setting_revisions: {}
    });
  });

  await page.route('**/api/pixiv/account', async (route) => {
    await fulfillJson(route, 200, mockPixivAccount());
  });

  await page.route('**/api/gallery/count', async (route) => {
    await fulfillJson(route, 200, { count: 0 });
  });

  await page.route('**/api/gallery/overview-decorations**', async (route) => {
    await fulfillJson(route, 200, { items: [null, null, null] });
  });

  await page.route('**/api/system/settings', async (route) => {
    await fulfillJson(route, 200, { value: mockEffectiveSettings() });
  });

  await page.route('**/api/events', async (route) => {
    state.eventConnections += 1;
    await route.fulfill({
      status: 200,
      headers: { 'Content-Type': 'text/event-stream' },
      body: options.initialSnapshot
        ? 'retry: 60000\nevent: snapshot_refresh\ndata: {"latest_event_id":12}\n\n'
        : 'retry: 60000\n\n'
    });
  });

  return state;
}

export function mockEffectiveSettings(
  content: Partial<EffectiveSettings['content']> = {}
): EffectiveSettings {
  return {
    security: {
      session_idle_timeout_seconds: 1800,
      session_absolute_timeout_seconds: 28_800,
      last_activity_persist_interval_seconds: 60,
      password_failures: {
        threshold: 8,
        window_seconds: 900,
        cooldown_seconds: 900
      },
      shared_account_failures: {
        threshold: 12,
        window_seconds: 900,
        cooldown_seconds: 900
      },
      entry_source_failures: {
        threshold: 20,
        window_seconds: 600,
        cooldown_seconds: 600
      }
    },
    storage: {
      media_root: null,
      warning_threshold_bytes: 107_374_182_400,
      media_write_stop_threshold_bytes: 34_359_738_368,
      trash_retention_days: 30
    },
    retry: { network_backoff_seconds: [60, 300, 1200, 3600] },
    queue: {
      quota_weights: {
        immediate: 4,
        manual_import: 8,
        scheduled_collection: 2,
        background_maintenance: 1
      },
      job_priorities: []
    },
    processing: {
      pixiv_request_concurrency: 4,
      pixiv_request_rate: { requests: 60, per_seconds: 60 },
      media_download_concurrency: 3,
      media_download_rate: { requests: 20, per_seconds: 60 },
      media_cpu_concurrency: 1
    },
    derivative: {
      format: 'webp',
      max_width: 768,
      webp_quality: 80,
      avif_quality: 50
    },
    pixiv: { default_private_bookmark: false },
    ugoira: {
      max_zip_bytes: 536_870_912,
      max_frames: 1500,
      max_pixels_per_frame: 80_000_000,
      decoded_frame_cache_bytes: 536_870_912
    },
    content: {
      overview_allow_nsfw: false,
      mask_non_all_age_thumbnails: false,
      ...content
    }
  };
}

function session() {
  return {
    administrator_id: '0198f64c-42a2-7374-bace-9f1c3b317fb0',
    session_id: '0198f64c-7a3e-7c87-bdbb-96298037fa3e',
    expires_at: '2026-07-31T18:00:00Z'
  };
}

function apiError(code: string, message: string) {
  return {
    code,
    message,
    details: {},
    trace_id: '0198f64d-477c-7d1e-aa2d-c74bb29ea4d7'
  };
}
