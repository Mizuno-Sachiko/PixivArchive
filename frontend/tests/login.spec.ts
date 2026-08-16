import { expect, test } from '@playwright/test';

import { mockApi } from './support';

test('administrator logs in with the environment-owned password', async ({
  page
}) => {
  const api = await mockApi(page, false);
  await page.goto('/login');

  await expect(
    page.getByRole('heading', { name: '进入PixivArchive' })
  ).toBeVisible();
  await page.getByLabel('管理员密码').fill('local-test-password');
  await page.getByRole('button', { name: '登录' }).click();

  await expect(page).toHaveURL(/\/overview$/);
  expect(api.loginBody).toEqual({ password: 'local-test-password' });
});

test('logout returns to the login page', async ({ page }) => {
  await mockApi(page);
  await page.goto('/overview');

  await page.getByRole('button', { name: '管理员菜单' }).click();
  await page.getByRole('button', { name: '退出登录' }).click();

  await expect(page).toHaveURL(/\/login$/);
});
