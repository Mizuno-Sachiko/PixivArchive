import { expect, test } from '@playwright/test';

import { mockApi } from './support';

test('theme supports dark, light and persisted system modes', async ({
  page
}) => {
  await mockApi(page);
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.goto('/overview');

  await page.getByRole('button', { name: '主题' }).click();
  await page.getByRole('button', { name: '浅色' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');

  await page.getByRole('button', { name: '主题' }).click();
  await page.getByRole('button', { name: '深色' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');

  await page.getByRole('button', { name: '主题' }).click();
  await page.getByRole('button', { name: '跟随系统' }).click();
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
});

test('keyboard focus stays visible and reduced motion shortens effects', async ({
  page
}) => {
  await mockApi(page);
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.goto('/overview');
  await expect(page.getByRole('link', { name: '打开图库' })).toBeVisible();

  await page.keyboard.press('Tab');
  const focused = page.locator(':focus-visible');
  await expect(focused).toBeVisible();
  await expect
    .poll(() =>
      focused.evaluate((element) => getComputedStyle(element).outlineStyle)
    )
    .not.toBe('none');

  const motionDuration = await page
    .locator('html')
    .evaluate((element) =>
      getComputedStyle(element).getPropertyValue('--motion-base').trim()
    );
  expect(motionDuration).toBe('1ms');
});
